//! Wall-clock time in the zone the person (and Studio) reads it in.
//!
//! Everything this crate stores is a true instant — unix seconds, or the UTC stamp
//! `iso_timestamp` writes. What a person picks in a calendar, and what Studio's schedule
//! picker takes, is a reading of the local clock instead. [`Zone`] converts between the
//! two, and lets a test pin the offset rather than depend on the machine it runs on.

/// Which clock a wall-clock reading belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The machine's own time zone, daylight saving included. The browser Studio runs in
    /// is started on this machine, so this is also the zone Studio interprets a typed
    /// date and time in.
    Local,
    /// A fixed offset east of UTC, in seconds. For tests, and for nothing else.
    Fixed(i64),
}

impl Zone {
    /// Seconds east of UTC in effect at the instant `unix`.
    pub fn offset_at(self, unix: i64) -> i64 {
        match self {
            Self::Local => local_offset_at(unix),
            Self::Fixed(offset) => offset,
        }
    }

    /// The reading this zone's clock shows at `unix`, expressed as the unix seconds a UTC
    /// clock showing the same digits would have. Feed it to `civil_from_unix` for the
    /// calendar fields.
    pub fn wall_clock(self, unix: i64) -> i64 {
        unix + self.offset_at(unix)
    }

    /// The instant a reading of this zone's clock names. The inverse of [`wall_clock`].
    ///
    /// The offset depends on the instant, which is what is being solved for, so it is
    /// asked for twice: once at a first guess, then at the instant that guess gives. That
    /// lands on the right side of a daylight-saving change for every reading that exists.
    ///
    /// [`wall_clock`]: Zone::wall_clock
    pub fn instant(self, wall: i64) -> i64 {
        let guess = wall - self.offset_at(wall);
        wall - self.offset_at(guess)
    }
}

#[cfg(windows)]
fn local_offset_at(unix: i64) -> i64 {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
        SystemTimeToTzSpecificLocalTimeEx, TIME_ZONE_ID_INVALID,
    };

    let (year, month, day) = crate::civil_from_unix(unix);
    // SYSTEMTIME cannot name anything outside these years.
    if !(1601..=30827).contains(&year) {
        return 0;
    }
    let seconds = unix.rem_euclid(86_400);
    let universal = SYSTEMTIME {
        wYear: year as u16,
        wMonth: month as u16,
        wDayOfWeek: 0,
        wDay: day as u16,
        wHour: (seconds / 3_600) as u16,
        wMinute: (seconds % 3_600 / 60) as u16,
        wSecond: (seconds % 60) as u16,
        wMilliseconds: 0,
    };
    let mut local = SYSTEMTIME::default();

    // The dynamic flavour, because it carries the zone's rules for every year rather than
    // only the current one — a schedule months out can cross a change in them.
    //
    // SAFETY: both structs are plain data the calls fill in; a zeroed zone record is a
    // valid (empty) one and is overwritten before it is read.
    let converted = unsafe {
        let mut zone: DYNAMIC_TIME_ZONE_INFORMATION = std::mem::zeroed();
        GetDynamicTimeZoneInformation(&mut zone) != TIME_ZONE_ID_INVALID
            && SystemTimeToTzSpecificLocalTimeEx(&zone, &universal, &mut local) != 0
    };
    if !converted {
        return 0;
    }

    let wall = crate::unix_from_civil(
        i32::from(local.wYear),
        u32::from(local.wMonth),
        u32::from(local.wDay),
        u32::from(local.wHour),
        u32::from(local.wMinute),
    ) + i64::from(local.wSecond);
    wall - unix
}

#[cfg(unix)]
fn local_offset_at(unix: i64) -> i64 {
    let time = unix as libc::time_t;
    // SAFETY: `tm` is plain data that `localtime_r` fills in; the zeroed value is only
    // read when the call reports success.
    unsafe {
        let mut parts: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut parts).is_null() {
            return 0;
        }
        i64::from(parts.tm_gmtoff)
    }
}

#[cfg(not(any(windows, unix)))]
fn local_offset_at(_unix: i64) -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_zone_reads_the_clock_that_many_hours_ahead_and_back_again() {
        let moscow = Zone::Fixed(3 * 3_600);
        let noon_utc = crate::unix_from_civil(2026, 9, 23, 12, 0);

        assert_eq!(
            crate::iso_timestamp(moscow.wall_clock(noon_utc)),
            "2026-09-23T15:00:00Z",
            "the digits a clock in Moscow shows at 12:00 UTC"
        );
        assert_eq!(moscow.instant(moscow.wall_clock(noon_utc)), noon_utc);
    }

    #[test]
    fn a_zone_west_of_utc_can_put_the_reading_on_the_previous_day() {
        let new_york = Zone::Fixed(-4 * 3_600);
        let early_utc = crate::unix_from_civil(2026, 9, 23, 2, 30);
        assert_eq!(
            crate::iso_timestamp(new_york.wall_clock(early_utc)),
            "2026-09-22T22:30:00Z"
        );
    }

    #[test]
    fn the_local_zone_round_trips_an_instant_through_its_own_clock() {
        // Whatever zone the machine running this is in, reading its clock and asking
        // which instant that reading names must land back where it started. Mid-day in
        // September is well clear of every daylight-saving change.
        let instant = crate::unix_from_civil(2026, 9, 23, 12, 0);
        let wall = Zone::Local.wall_clock(instant);
        assert_eq!(Zone::Local.instant(wall), instant);
        assert!(
            (wall - instant).abs() <= 14 * 3_600,
            "no real zone is more than fourteen hours from UTC"
        );
    }
}

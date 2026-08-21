pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut year = year;
    if month < 3 {
        year -= 1;
    }
    let sunday_first = (year + year / 4 - year / 100
        + year / 400
        + OFFSETS[(month as usize).clamp(1, 12) - 1]
        + day as i32)
        .rem_euclid(7);
    ((sunday_first + 6) % 7) as u32
}

pub fn month_grid(year: i32, month: u32) -> Vec<Option<u32>> {
    let total = days_in_month(year, month);
    let lead = weekday(year, month, 1) as usize;
    let mut cells = vec![None; 42];
    for day in 1..=total {
        cells[lead + day as usize - 1] = Some(day);
    }
    cells
}

pub fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month <= 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

pub fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

pub fn clamp_day(year: i32, month: u32, day: u32) -> u32 {
    day.clamp(1, days_in_month(year, month).max(1))
}

pub fn to_stamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z",
        year = year.clamp(0, 9999),
        month = month.clamp(1, 12),
        day = clamp_day(year, month, day),
        hour = hour.min(23),
        minute = minute.min(59),
    )
}

pub fn month_key(month: u32) -> String {
    format!("calendar.month.{}", month.clamp(1, 12))
}

pub fn weekday_key(index: u32) -> String {
    format!("calendar.weekday.{}", index.min(6))
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn from_stamp(stamp: &str) -> Option<(i32, u32, u32, u32, u32)> {
    let (date, time) = stamp.trim().split_once('T')?;
    let mut date = date.split('-');
    let year: i32 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;
    let mut time = time.trim_end_matches('Z').split(':');
    let hour: u32 = time.next()?.parse().ok()?;
    let minute: u32 = time.next()?.parse().ok()?;

    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((year, month, day, hour, minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn february_knows_about_leap_years() {
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);

        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert!(!is_leap(2100));
    }

    #[test]
    fn the_month_lengths_are_the_ordinary_ones() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 13), 0, "not a month");
    }

    #[test]
    fn weekdays_land_where_a_calendar_says_they_do() {
        assert_eq!(weekday(2026, 8, 17), 0, "17 August 2026 is a Monday");
        assert_eq!(weekday(2026, 8, 23), 6, "and the 23rd is the Sunday");
        assert_eq!(weekday(2000, 1, 1), 5, "1 January 2000 was a Saturday");
        assert_eq!(weekday(2024, 2, 29), 3, "the leap day was a Thursday");
    }

    #[test]
    fn a_month_grid_is_always_six_rows_and_puts_the_first_under_its_weekday() {
        let grid = month_grid(2026, 8);
        assert_eq!(grid.len(), 42, "the dialog must not change height");

        assert_eq!(grid[..5], [None, None, None, None, None]);
        assert_eq!(grid[5], Some(1));
        assert_eq!(grid[35], Some(31));
        assert!(grid[36..].iter().all(Option::is_none));
        assert_eq!(grid.iter().flatten().count(), 31);
    }

    #[test]
    fn a_month_starting_on_a_monday_has_no_blanks_at_all_in_front() {
        let grid = month_grid(2026, 6);
        assert_eq!(grid[0], Some(1));
        assert_eq!(grid.iter().flatten().count(), 30);
    }

    #[test]
    fn paging_wraps_the_year_in_both_directions() {
        assert_eq!(next_month(2026, 12), (2027, 1));
        assert_eq!(previous_month(2026, 1), (2025, 12));
        assert_eq!(next_month(2026, 5), (2026, 6));
        assert_eq!(previous_month(2026, 5), (2026, 4));
    }

    #[test]
    fn a_day_that_does_not_exist_in_the_next_month_is_pulled_back() {
        assert_eq!(clamp_day(2026, 2, 31), 28);
        assert_eq!(clamp_day(2024, 2, 31), 29);
        assert_eq!(clamp_day(2026, 4, 31), 30);
        assert_eq!(clamp_day(2026, 1, 31), 31);
        assert_eq!(clamp_day(2026, 1, 0), 1);
    }

    #[test]
    fn a_stamp_has_the_shape_the_publish_settings_check_for() {
        let stamp = to_stamp(2026, 8, 17, 9, 5);
        assert_eq!(stamp, "2026-08-17T09:05:00Z");
        assert!(youtube::publish::is_rfc3339_utc(&stamp));
    }

    #[test]
    fn a_stamp_cannot_be_built_out_of_impossible_parts() {
        assert_eq!(to_stamp(2026, 2, 31, 25, 99), "2026-02-28T23:59:00Z");
        assert!(youtube::publish::is_rfc3339_utc(&to_stamp(
            2026, 13, 0, 0, 0
        )));
    }

    #[test]
    fn a_stamp_round_trips_so_reopening_the_picker_lands_where_it_was_left() {
        let stamp = to_stamp(2026, 8, 17, 14, 30);
        assert_eq!(from_stamp(&stamp), Some((2026, 8, 17, 14, 30)));
        assert_eq!(
            from_stamp("2024-02-29T00:00:00Z"),
            Some((2024, 2, 29, 0, 0))
        );
    }

    #[test]
    fn a_stamp_that_is_not_one_reads_as_nothing_rather_than_as_a_wrong_date() {
        assert_eq!(from_stamp(""), None);
        assert_eq!(from_stamp("tomorrow"), None);
        assert_eq!(from_stamp("2026-13-01T00:00:00Z"), None, "no such month");
        assert_eq!(from_stamp("2023-02-29T00:00:00Z"), None, "not a leap year");
        assert_eq!(from_stamp("2026-08-17T24:00:00Z"), None, "no such hour");
    }

    #[test]
    fn the_names_are_asked_for_by_key_so_the_picker_reads_in_the_apps_language() {
        assert_eq!(month_key(1), "calendar.month.1");
        assert_eq!(month_key(12), "calendar.month.12");
        assert_eq!(
            month_key(99),
            "calendar.month.12",
            "clamped, never a missing key"
        );
        assert_eq!(weekday_key(0), "calendar.weekday.0");
        assert_eq!(weekday_key(9), "calendar.weekday.6");
    }
}

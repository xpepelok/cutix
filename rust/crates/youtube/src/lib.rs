pub mod accounts;
pub mod cdp;
pub mod chrome;
pub mod devtools;
pub mod failure;
pub mod history;
pub mod login;
pub mod publish;
pub mod queue;
pub mod selectors;
pub mod session;
pub mod studio;

pub use accounts::{Account, Accounts};
pub use failure::{Failure, FailureNote};
pub use history::{History, HistoryEntry, HistoryFilter, UploadStatus};
pub use publish::{Privacy, PublishSettings};
pub use queue::{Queue, Task, TaskState};
pub use session::Session;
pub use studio::{Control, Stage};

pub const CHROME_DOWNLOAD_URL: &str = "https://www.google.com/chrome/";

pub fn config_directory() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("cutix")
}

pub fn data_directory() -> std::path::PathBuf {
    config_directory().join("youtube")
}

pub fn civil_from_unix(seconds: i64) -> (i64, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

pub fn iso_date(seconds: i64) -> String {
    let (year, month, day) = civil_from_unix(seconds);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn iso_timestamp(seconds: i64) -> String {
    let (year, month, day) = civil_from_unix(seconds);
    let remainder = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        remainder / 3_600,
        (remainder % 3_600) / 60,
        remainder % 60
    )
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_directory_sits_under_the_apps_own_config_folder() {
        let directory = data_directory();
        assert!(directory.ends_with("youtube"));
        assert!(directory.to_string_lossy().contains("cutix"));
    }

    #[test]
    fn unix_seconds_convert_to_the_right_calendar_date() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_700_000_000), "2023-11-14");
        assert_eq!(iso_date(951_782_400), "2000-02-29");
        assert_eq!(iso_date(-1), "1969-12-31");
        assert_eq!(iso_timestamp(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn a_scheduled_time_is_compared_against_a_stamp_of_the_same_shape() {
        let stamp = iso_timestamp(1_700_000_000);
        assert_eq!(stamp.len(), 20);
        assert!(publish::is_rfc3339_utc(&stamp));
    }

    #[test]
    fn the_clock_is_a_plausible_unix_time() {
        assert!(now_unix() > 1_700_000_000);
        assert!(now_unix() < 4_000_000_000);
    }

    #[test]
    fn the_crate_carries_no_google_credential_of_any_kind() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let secret_marker = format!("GOCSPX{}", '-');
        let google_suffix = format!(".apps{}", ".googleusercontent.com");
        let mut checked = 0;

        for entry in std::fs::read_dir(source).expect("src") {
            let path = entry.expect("entry").path();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            checked += 1;

            assert!(
                !text.contains(&secret_marker),
                "{} looks like it contains a client secret",
                path.display()
            );
            for line in text.lines() {
                let is_const = line.contains("const ") || line.contains("static ");
                assert!(
                    !(is_const && line.contains(&google_suffix)),
                    "{} hard-codes a client id: {line}",
                    path.display()
                );
            }
        }
        assert!(checked >= 10, "only {checked} files were checked");
    }
}

pub const UNKNOWN_LICENSE: &str = "Unknown licence";

pub fn format_archive_license(license_url: Option<&str>) -> String {
    let Some(url) = license_url else {
        return UNKNOWN_LICENSE.to_string();
    };
    let normalized = url.to_lowercase();

    if normalized.contains("/publicdomain/zero") {
        return "CC0 1.0".to_string();
    }
    if normalized.contains("/publicdomain/mark") {
        return "Public Domain Mark".to_string();
    }
    if normalized.contains("/publicdomain") {
        return "Public Domain".to_string();
    }

    match parse_license_path(&normalized) {
        Some((code, version)) => {
            format!("CC {} {}", code.to_uppercase().replace('-', " "), version)
        }
        None => "Creative Commons".to_string(),
    }
}

fn parse_license_path(normalized: &str) -> Option<(String, String)> {
    let start = normalized.find("/licenses/")? + "/licenses/".len();
    let rest = &normalized[start..];
    let mut parts = rest.split('/');
    let code: String = parts
        .next()?
        .chars()
        .take_while(|value| value.is_ascii_lowercase() || *value == '-')
        .collect();
    if code.is_empty() {
        return None;
    }
    let version: String = parts
        .next()?
        .chars()
        .take_while(|value| value.is_ascii_digit() || *value == '.')
        .collect();
    if version.is_empty() {
        return None;
    }
    Some((code, version))
}

pub fn is_edit_safe_license(license_url: Option<&str>) -> bool {
    let Some(url) = license_url else {
        return false;
    };
    let normalized = url.to_lowercase();
    if normalized.contains("-nc") || normalized.contains("-nd") {
        return false;
    }
    if normalized.contains("/publicdomain/") {
        return true;
    }
    match parse_license_path(&normalized) {
        Some((code, _)) => code == "by" || code == "by-sa",
        None => false,
    }
}

pub fn format_openverse_license(license: &str, version: Option<&str>) -> String {
    let code = license.to_lowercase();
    let suffix = version.map(|value| format!(" {value}")).unwrap_or_default();

    if code == "cc0" {
        let suffix = if suffix.is_empty() {
            " 1.0".to_string()
        } else {
            suffix
        };
        return format!("CC0{suffix}");
    }
    if code == "pdm" {
        return "Public Domain Mark".to_string();
    }
    if code == "sampling+" {
        return format!("CC Sampling+{suffix}");
    }
    format!("CC {}{}", code.to_uppercase().replace('-', " "), suffix)
}

pub fn is_edit_safe_openverse(license: &str) -> bool {
    let code = license.to_lowercase();
    if code.contains("-nc") || code.contains("nc-") || code.contains("-nd") || code.ends_with("nd")
    {
        return false;
    }
    matches!(code.as_str(), "cc0" | "pdm" | "by" | "by-sa" | "sampling+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_licenses_are_formatted_like_the_web() {
        assert_eq!(
            format_archive_license(Some("https://creativecommons.org/licenses/by/4.0/")),
            "CC BY 4.0"
        );
        assert_eq!(
            format_archive_license(Some("http://creativecommons.org/licenses/by-sa/3.0/")),
            "CC BY SA 3.0"
        );
        assert_eq!(
            format_archive_license(Some("https://creativecommons.org/publicdomain/zero/1.0/")),
            "CC0 1.0"
        );
        assert_eq!(
            format_archive_license(Some("https://creativecommons.org/publicdomain/mark/1.0/")),
            "Public Domain Mark"
        );
        assert_eq!(format_archive_license(None), UNKNOWN_LICENSE);
        assert_eq!(
            format_archive_license(Some("https://example.com/nothing")),
            "Creative Commons"
        );
    }

    #[test]
    fn only_edit_safe_archive_licenses_pass() {
        for url in [
            "https://creativecommons.org/licenses/by/4.0/",
            "https://creativecommons.org/licenses/by-sa/3.0/",
            "https://creativecommons.org/publicdomain/zero/1.0/",
        ] {
            assert!(is_edit_safe_license(Some(url)), "{url}");
        }
        for url in [
            "https://creativecommons.org/licenses/by-nc/4.0/",
            "https://creativecommons.org/licenses/by-nd/4.0/",
            "https://creativecommons.org/licenses/by-nc-sa/4.0/",
        ] {
            assert!(!is_edit_safe_license(Some(url)), "{url}");
        }
        assert!(!is_edit_safe_license(None));
    }

    #[test]
    fn openverse_licenses_are_formatted_like_the_web() {
        assert_eq!(format_openverse_license("cc0", None), "CC0 1.0");
        assert_eq!(
            format_openverse_license("by-sa", Some("4.0")),
            "CC BY SA 4.0"
        );
        assert_eq!(format_openverse_license("pdm", None), "Public Domain Mark");
    }

    #[test]
    fn only_edit_safe_openverse_licenses_pass() {
        for code in ["cc0", "pdm", "by", "by-sa"] {
            assert!(is_edit_safe_openverse(code), "{code}");
        }
        for code in ["by-nc", "by-nd", "by-nc-sa", "nc-sampling+"] {
            assert!(!is_edit_safe_openverse(code), "{code}");
        }
    }
}

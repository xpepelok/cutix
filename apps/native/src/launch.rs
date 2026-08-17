use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    Normal,
    OpenMedia(PathBuf),

    BrowseFolder(PathBuf),

    Publish(PathBuf),
    Register,
    Unregister,
    NotifyCheck,
}

pub const REGISTER_FLAG: &str = "--register-file-types";
pub const UNREGISTER_FLAG: &str = "--unregister-file-types";
pub const BROWSE_FLAG: &str = "--browse";
pub const PUBLISH_FLAG: &str = "--publish";
pub const NOTIFY_FLAG: &str = "--notify-check";

fn looks_like_flag(argument: &str) -> bool {
    argument.starts_with('-') || (cfg!(windows) && argument.starts_with('/'))
}

pub fn parse<I>(arguments: I, openable: &dyn Fn(&Path) -> bool) -> Launch
where
    I: IntoIterator<Item = OsString>,
{
    let mut browsing = false;
    let mut publishing = false;

    for argument in arguments.into_iter().skip(1) {
        let Some(text) = argument.to_str() else {
            let path = PathBuf::from(&argument);
            if publishing {
                return Launch::Publish(path);
            }
            if browsing || path.is_dir() {
                return Launch::BrowseFolder(path);
            }
            if openable(&path) {
                return Launch::OpenMedia(path);
            }
            continue;
        };

        if text == BROWSE_FLAG {
            browsing = true;
            continue;
        }
        if text == PUBLISH_FLAG {
            publishing = true;
            continue;
        }
        if text == REGISTER_FLAG {
            return Launch::Register;
        }
        if text == UNREGISTER_FLAG {
            return Launch::Unregister;
        }
        if text == NOTIFY_FLAG {
            return Launch::NotifyCheck;
        }
        if looks_like_flag(text) {
            continue;
        }

        let path = PathBuf::from(text);
        if publishing {
            return Launch::Publish(path);
        }

        if browsing || path.is_dir() {
            return Launch::BrowseFolder(path);
        }
        if openable(&path) {
            return Launch::OpenMedia(path);
        }
    }

    Launch::Normal
}

pub fn openable(path: &Path) -> bool {
    path.is_file() && cutix_project::probe::is_supported(path)
}

pub fn from_env() -> Launch {
    parse(std::env::args_os(), &openable)
}

pub fn project_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| cutix_i18n::t("projects.new"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn any_path(_: &Path) -> bool {
        true
    }

    fn no_path(_: &Path) -> bool {
        false
    }

    #[test]
    fn a_bare_launch_opens_the_project_list() {
        assert_eq!(parse(args(&["cutix.exe"]), &any_path), Launch::Normal);
    }

    #[test]
    fn the_first_openable_argument_is_the_media_to_open() {
        assert_eq!(
            parse(args(&["cutix.exe", "C:\\clips\\raid.mp4"]), &any_path),
            Launch::OpenMedia(PathBuf::from("C:\\clips\\raid.mp4"))
        );
    }

    #[test]
    fn the_executable_path_is_never_mistaken_for_media() {
        assert_eq!(parse(args(&["C:\\cutix.exe"]), &any_path), Launch::Normal);
    }

    #[test]
    fn unopenable_arguments_fall_back_to_a_normal_launch() {
        assert_eq!(
            parse(args(&["cutix.exe", "C:\\notes.docx"]), &no_path),
            Launch::Normal
        );
    }

    #[test]
    fn flags_are_not_treated_as_paths() {
        assert_eq!(
            parse(args(&["cutix.exe", "--verbose", "--silent"]), &any_path),
            Launch::Normal
        );
    }

    #[test]
    fn a_flag_before_a_path_does_not_hide_the_path() {
        assert_eq!(
            parse(args(&["cutix.exe", "--verbose", "C:\\a.mp4"]), &any_path),
            Launch::OpenMedia(PathBuf::from("C:\\a.mp4"))
        );
    }

    #[test]
    fn only_the_first_openable_file_is_taken() {
        assert_eq!(
            parse(args(&["cutix.exe", "a.mp4", "b.mp4"]), &any_path),
            Launch::OpenMedia(PathBuf::from("a.mp4"))
        );
    }

    #[test]
    fn the_registration_flags_are_recognised() {
        assert_eq!(
            parse(args(&["cutix.exe", REGISTER_FLAG]), &any_path),
            Launch::Register
        );
        assert_eq!(
            parse(args(&["cutix.exe", UNREGISTER_FLAG]), &any_path),
            Launch::Unregister
        );
    }

    #[test]
    fn the_publish_flag_takes_the_file_that_follows_it() {
        assert_eq!(
            parse(
                args(&["cutix.exe", PUBLISH_FLAG, r"C:\clips\raid.mp4"]),
                &no_path
            ),
            Launch::Publish(PathBuf::from(r"C:\clips\raid.mp4"))
        );
    }

    #[test]
    fn the_browse_flag_takes_the_folder_that_follows_it() {
        assert_eq!(
            parse(
                args(&["cutix.exe", BROWSE_FLAG, r"C:\Users\me\Videos"]),
                &no_path
            ),
            Launch::BrowseFolder(PathBuf::from(r"C:\Users\me\Videos"))
        );
    }

    #[test]
    fn a_folder_handed_over_without_the_flag_is_still_browsed() {
        let temporary = std::env::temp_dir();
        let path = temporary.to_string_lossy().to_string();
        match parse(args(&["cutix.exe", &path]), &no_path) {
            Launch::BrowseFolder(seen) => assert_eq!(seen, std::path::PathBuf::from(path)),
            other => panic!("a folder should open the browser, got {other:?}"),
        }
    }

    #[test]
    fn a_path_that_begins_with_a_slash_is_a_path_everywhere_but_windows() {
        assert!(looks_like_flag("--publish"));
        assert_eq!(looks_like_flag("/videos/holiday.mp4"), cfg!(windows));
    }

    #[test]
    fn a_project_is_named_after_the_clip() {
        let clip = std::env::temp_dir().join("Boss Fight.mp4");
        assert_eq!(project_name(&clip), "Boss Fight");
    }

    #[test]
    fn a_nameless_file_falls_back_to_the_default_project_name() {
        cutix_i18n::bootstrap();
        assert!(!project_name(&std::env::temp_dir()).is_empty());
    }
}

use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "flv", "wmv", "mpeg", "mpg", "m2ts", "ts", "3gp",
];

const MAX_ENTRIES: usize = 2_000;

pub fn is_video(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&extension.as_str())
}

pub fn default_directory() -> PathBuf {
    dirs::video_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir)
}

pub fn directory(configured: Option<&Path>) -> PathBuf {
    match configured {
        Some(path) if path.is_dir() => path.to_path_buf(),
        _ => default_directory(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,

    pub name: String,
    pub bytes: u64,

    pub created: i64,

    pub duration_seconds: Option<f64>,
}

impl Entry {
    pub fn from_path(path: PathBuf) -> Option<Self> {
        if !is_video(&path) {
            return None;
        }
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        Some(Self {
            name,
            bytes: meta.len(),
            created: unix_seconds(meta.created().or_else(|_| meta.modified())),
            duration_seconds: None,
            path,
        })
    }
}

fn unix_seconds(time: std::io::Result<SystemTime>) -> i64 {
    time.ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

pub fn scan(directory: &Path) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<Entry> = entries
        .flatten()
        .filter_map(|entry| Entry::from_path(entry.path()))
        .take(MAX_ENTRIES)
        .collect();
    sort_newest_first(&mut found);
    found
}

pub fn sort_newest_first(entries: &mut [Entry]) {
    entries.sort_by(|left, right| {
        right
            .created
            .cmp(&left.created)
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.round() as u64;
    let (hours, minutes, secs) = (total / 3_600, (total % 3_600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "cutix-library-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("workspace");
        directory
    }

    #[test]
    fn the_browser_lists_videos_and_nothing_else() {
        for name in ["clip.mp4", "clip.MOV", "clip.mkv", "clip.webm", "clip.ts"] {
            assert!(is_video(Path::new(name)), "{name}");
        }

        for name in [
            "song.mp3",
            "song.wav",
            "song.flac",
            "shot.png",
            "shot.jpg",
            "shot.gif",
            "notes.txt",
            "clip",
            "clip.mp4.part",
        ] {
            assert!(!is_video(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn a_folder_yields_its_videos_newest_first() {
        let directory = workspace("scan");
        for name in ["a.mp4", "b.mkv", "cover.png", "track.mp3", "notes.txt"] {
            std::fs::write(directory.join(name), b"x").expect("write");
        }

        let found = scan(&directory);
        let names: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(found.len(), 2, "only the two videos: {names:?}");
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(found.iter().all(|entry| entry.duration_seconds.is_none()));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_folder_that_is_not_there_is_an_empty_browser_rather_than_a_crash() {
        assert!(scan(Path::new("/nowhere/at/all")).is_empty());
    }

    #[test]
    fn the_newest_comes_first_and_a_tie_is_broken_by_name_so_the_grid_holds_still() {
        let entry = |name: &str, created: i64| Entry {
            path: PathBuf::from(format!("{name}.mp4")),
            name: name.to_string(),
            bytes: 0,
            created,
            duration_seconds: None,
        };
        let mut entries = vec![
            entry("old", 100),
            entry("zebra", 500),
            entry("apple", 500),
            entry("newest", 900),
        ];
        sort_newest_first(&mut entries);
        let order: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(order, ["newest", "apple", "zebra", "old"]);
    }

    #[test]
    fn a_configured_folder_is_used_and_a_missing_one_falls_back() {
        let chosen = workspace("configured");
        assert_eq!(directory(Some(&chosen)), chosen);

        assert_eq!(
            directory(Some(Path::new("/nowhere/at/all"))),
            default_directory()
        );
        assert_eq!(directory(None), default_directory());

        let _ = std::fs::remove_dir_all(&chosen);
    }

    #[test]
    fn a_duration_reads_the_way_a_player_shows_it() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(8.0), "0:08");
        assert_eq!(format_duration(62.0), "1:02");
        assert_eq!(format_duration(599.5), "10:00");
        assert_eq!(format_duration(3_600.0), "1:00:00");
        assert_eq!(format_duration(3_723.0), "1:02:03");
    }

    #[test]
    fn a_duration_that_could_not_be_probed_does_not_print_nonsense() {
        assert_eq!(format_duration(-1.0), "0:00");
        assert_eq!(format_duration(f64::NAN), "0:00");
        assert_eq!(format_duration(f64::INFINITY), "0:00");
    }

    #[test]
    fn the_default_folder_is_the_platforms_own_videos_folder() {
        let directory = default_directory();
        assert!(directory.is_absolute(), "{}", directory.display());
    }
}

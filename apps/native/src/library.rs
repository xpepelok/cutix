use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "flv", "wmv", "mpeg", "mpg", "m2ts", "ts", "3gp",
];

const MAX_ENTRIES: usize = 2_000;

const MAX_DIRECTORIES: usize = 4_000;

const MAX_SCANNED: usize = 20_000;

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

    pub modified: i64,

    pub created: i64,

    pub duration_seconds: Option<f64>,

    pub width: Option<u32>,
    pub height: Option<u32>,
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
            modified: unix_seconds(meta.modified().or_else(|_| meta.created())),
            created: unix_seconds(meta.created().or_else(|_| meta.modified())),
            duration_seconds: None,
            width: None,
            height: None,
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

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

pub fn scan(root: &Path) -> Vec<Entry> {
    let mut found: Vec<Entry> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    let mut opened = 0usize;

    while let Some(directory) = queue.pop_front() {
        if opened >= MAX_DIRECTORIES || found.len() >= MAX_SCANNED {
            break;
        }
        opened += 1;
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if !is_hidden(&path) {
                    queue.push_back(path);
                }
            } else if let Some(one) = Entry::from_path(path) {
                found.push(one);
                if found.len() >= MAX_SCANNED {
                    break;
                }
            }
        }
    }

    sort_newest_first(&mut found);
    found.truncate(MAX_ENTRIES);
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Duration,
    Created,
    Modified,
    Resolution,
}

pub const SORT_KEYS: [SortKey; 6] = [
    SortKey::Name,
    SortKey::Size,
    SortKey::Duration,
    SortKey::Created,
    SortKey::Modified,
    SortKey::Resolution,
];

impl SortKey {
    /// Whether ordering by this key needs the media probed first.
    ///
    /// Duration and resolution only exist once a file has been inspected; the rest come
    /// straight from the directory entry. Only the tests ask this today, so it compiles
    /// for them alone rather than shipping as a method nothing calls.
    #[cfg(test)]
    pub fn needs_probe(self) -> bool {
        matches!(self, SortKey::Duration | SortKey::Resolution)
    }

    pub fn id(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Duration => "duration",
            SortKey::Created => "created",
            SortKey::Modified => "modified",
            SortKey::Resolution => "resolution",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            SortKey::Name => "library.sort.name",
            SortKey::Size => "library.sort.size",
            SortKey::Duration => "library.sort.duration",
            SortKey::Created => "library.sort.created",
            SortKey::Modified => "library.sort.modified",
            SortKey::Resolution => "library.sort.resolution",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping {
    None,
    Folder,
    Day,
}

pub const GROUPINGS: [Grouping; 3] = [Grouping::None, Grouping::Folder, Grouping::Day];

impl Grouping {
    pub fn id(self) -> &'static str {
        match self {
            Grouping::None => "none",
            Grouping::Folder => "folder",
            Grouping::Day => "day",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Grouping::None => "library.group.none",
            Grouping::Folder => "library.group.folder",
            Grouping::Day => "library.group.day",
        }
    }
}

pub fn matches(entry: &Entry, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let name = entry.name.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| name.contains(word))
}

pub fn folder_of(entry: &Entry, root: &Path) -> String {
    let parent = entry.path.parent().unwrap_or(root);
    match parent.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ => parent
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
    }
}

pub fn arrange(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by(|left, right| {
        let ordering = match key {
            SortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            SortKey::Size => left.bytes.cmp(&right.bytes),
            SortKey::Created => left.created.cmp(&right.created),
            SortKey::Modified => left.modified.cmp(&right.modified),
            SortKey::Duration => unknown_last(
                left.duration_seconds,
                right.duration_seconds,
                ascending,
                |a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            ),
            SortKey::Resolution => unknown_last(
                pixels(left),
                pixels(right),
                ascending,
                |a: &u64, b: &u64| a.cmp(b),
            ),
        };
        let ordering = if ascending {
            ordering
        } else {
            ordering.reverse()
        };
        ordering.then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

fn pixels(entry: &Entry) -> Option<u64> {
    match (entry.width, entry.height) {
        (Some(width), Some(height)) => Some(u64::from(width) * u64::from(height)),
        _ => None,
    }
}

fn unknown_last<T>(
    left: Option<T>,
    right: Option<T>,
    ascending: bool,
    compare: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> std::cmp::Ordering {
    let sunk = |ordering: std::cmp::Ordering| {
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    };
    match (left, right) {
        (Some(left), Some(right)) => compare(&left, &right),
        (Some(_), None) => sunk(std::cmp::Ordering::Less),
        (None, Some(_)) => sunk(std::cmp::Ordering::Greater),
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub fn sort_newest_first(entries: &mut [Entry]) {
    entries.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
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

pub const TEMP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub fn temp_directory() -> Option<PathBuf> {
    dirs::data_local_dir().map(|root| root.join("cutix").join("tmp"))
}

pub fn temp_file(source: &Path) -> Option<PathBuf> {
    let directory = temp_directory()?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("clip");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4");
    let ticket = uuid::Uuid::new_v4().simple().to_string();
    Some(directory.join(format!("{stem}-{ticket}.{extension}")))
}

pub fn is_stale(modified: SystemTime, now: SystemTime, max_age: std::time::Duration) -> bool {
    now.duration_since(modified)
        .map(|age| age >= max_age)
        .unwrap_or(false)
}

pub fn prune_temp(directory: &Path, now: SystemTime, max_age: std::time::Duration) -> usize {
    let Ok(listing) = std::fs::read_dir(directory) else {
        return 0;
    };
    let mut removed = 0usize;
    for entry in listing.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if is_stale(modified, now, max_age) && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn trim_span(from: f32, to: f32, duration: f64) -> (f64, f64) {
    if !duration.is_finite() || duration <= 0.0 {
        return (0.0, 0.0);
    }
    let first = f64::from(from.clamp(0.0, 1.0)) * duration;
    let second = f64::from(to.clamp(0.0, 1.0)) * duration;
    let start = first.min(second);
    let end = first.max(second);
    (start, (end - start).max(0.0))
}

pub fn upload_settled(seen_busy: bool, busy: bool) -> bool {
    seen_busy && !busy
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
    fn every_subfolder_is_walked_not_just_the_top_one() {
        let directory = workspace("recursive");
        let nested = directory.join("2026").join("august").join("raw");
        std::fs::create_dir_all(&nested).expect("nested");
        std::fs::write(directory.join("top.mp4"), b"x").expect("write");
        std::fs::write(directory.join("2026").join("year.mkv"), b"x").expect("write");
        std::fs::write(nested.join("deep.mov"), b"x").expect("write");
        std::fs::write(nested.join("cover.png"), b"x").expect("write");

        let found = scan(&directory);
        let mut names: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["deep", "top", "year"]);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_hidden_folder_is_left_alone() {
        let directory = workspace("hidden");
        let hidden = directory.join(".trash");
        std::fs::create_dir_all(&hidden).expect("hidden");
        std::fs::write(hidden.join("deleted.mp4"), b"x").expect("write");
        std::fs::write(directory.join("kept.mp4"), b"x").expect("write");

        let found = scan(&directory);
        let names: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["kept"]);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_grid_is_ordered_by_the_file_write_time_across_folders() {
        let directory = workspace("order");
        let nested = directory.join("inner");
        std::fs::create_dir_all(&nested).expect("nested");
        std::fs::write(directory.join("first.mp4"), b"x").expect("write");
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        std::fs::write(nested.join("second.mp4"), b"x").expect("write");

        let found = scan(&directory);
        let names: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["second", "first"], "the newer write comes first");
        assert!(found[0].modified >= found[1].modified);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_folder_that_is_not_there_is_an_empty_browser_rather_than_a_crash() {
        assert!(scan(Path::new("/nowhere/at/all")).is_empty());
    }

    #[test]
    fn the_newest_comes_first_and_a_tie_is_broken_by_name_so_the_grid_holds_still() {
        let entry = |name: &str, modified: i64| Entry {
            path: PathBuf::from(format!("{name}.mp4")),
            name: name.to_string(),
            bytes: 0,
            modified,
            created: modified,
            duration_seconds: None,
            width: None,
            height: None,
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

    fn sample(name: &str) -> Entry {
        Entry {
            path: PathBuf::from(format!("root/{name}.mp4")),
            name: name.to_string(),
            bytes: 0,
            modified: 0,
            created: 0,
            duration_seconds: None,
            width: None,
            height: None,
        }
    }

    #[test]
    fn a_search_matches_every_word_in_any_order() {
        let entry = sample("Zort 2026.05.04 - 02.01.48");
        assert!(matches(&entry, ""));
        assert!(matches(&entry, "zort"));
        assert!(matches(&entry, "ZORT"));
        assert!(matches(&entry, "02 zort"));
        assert!(!matches(&entry, "valorant"));
        assert!(!matches(&entry, "zort valorant"));
    }

    #[test]
    fn sorting_by_size_runs_both_ways() {
        let mut entries = vec![sample("a"), sample("b"), sample("c")];
        entries[0].bytes = 30;
        entries[1].bytes = 10;
        entries[2].bytes = 20;

        arrange(&mut entries, SortKey::Size, true);
        let order: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(order, ["b", "c", "a"]);

        arrange(&mut entries, SortKey::Size, false);
        let order: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(order, ["a", "c", "b"]);
    }

    #[test]
    fn an_unprobed_clip_sinks_to_the_bottom_whichever_way_the_sort_runs() {
        let mut entries = vec![sample("known"), sample("unknown"), sample("other")];
        entries[0].duration_seconds = Some(10.0);
        entries[2].duration_seconds = Some(20.0);

        arrange(&mut entries, SortKey::Duration, true);
        assert_eq!(
            entries.last().map(|entry| entry.name.as_str()),
            Some("unknown")
        );

        arrange(&mut entries, SortKey::Duration, false);
        assert_eq!(
            entries.last().map(|entry| entry.name.as_str()),
            Some("unknown")
        );
    }

    #[test]
    fn sorting_by_resolution_uses_the_pixel_count() {
        let mut entries = vec![sample("hd"), sample("uhd"), sample("sd")];
        entries[0].width = Some(1920);
        entries[0].height = Some(1080);
        entries[1].width = Some(3840);
        entries[1].height = Some(2160);
        entries[2].width = Some(640);
        entries[2].height = Some(360);

        arrange(&mut entries, SortKey::Resolution, false);
        let order: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(order, ["uhd", "hd", "sd"]);
    }

    #[test]
    fn a_tie_is_broken_by_name_so_the_grid_holds_still() {
        let mut entries = vec![sample("zebra"), sample("apple")];
        arrange(&mut entries, SortKey::Size, false);
        let order: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(order, ["apple", "zebra"]);
    }

    #[test]
    fn a_folder_group_is_the_path_below_the_root() {
        let root = Path::new("C:/videos");
        let mut entry = sample("clip");
        entry.path = PathBuf::from("C:/videos/NVIDIA/Zort/clip.mp4");
        assert_eq!(
            folder_of(&entry, root).replace(std::path::MAIN_SEPARATOR, "/"),
            "NVIDIA/Zort"
        );

        entry.path = PathBuf::from("C:/videos/clip.mp4");
        assert_eq!(folder_of(&entry, root), "videos");
    }

    #[test]
    fn every_sort_key_and_grouping_carries_a_distinct_id() {
        let mut ids: Vec<&str> = SORT_KEYS.iter().map(|key| key.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SORT_KEYS.len());

        assert!(SortKey::Duration.needs_probe());
        assert!(SortKey::Resolution.needs_probe());
        assert!(!SortKey::Name.needs_probe());

        let mut groups: Vec<&str> = GROUPINGS.iter().map(|group| group.id()).collect();
        groups.sort_unstable();
        groups.dedup();
        assert_eq!(groups.len(), GROUPINGS.len());
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

    #[test]
    fn a_trimmed_clip_lands_next_to_its_neighbours_under_a_name_of_its_own() {
        let first = temp_file(Path::new("C:/videos/holiday.mp4")).expect("a local data folder");
        let second = temp_file(Path::new("C:/videos/holiday.mp4")).expect("a local data folder");
        assert_ne!(first, second, "two cuts never fight over one file");
        assert_eq!(first.parent(), second.parent());
        assert_eq!(
            first.extension().and_then(|value| value.to_str()),
            Some("mp4"),
            "the container of the copy matches the source"
        );
        assert!(first
            .file_name()
            .and_then(|value| value.to_str())
            .expect("a name")
            .starts_with("holiday-"));
        assert!(first.starts_with(temp_directory().expect("a local data folder")));
    }

    #[test]
    fn yesterdays_leftovers_are_swept_up_and_todays_are_left_alone() {
        let directory = workspace("temp-sweep");
        let old = directory.join("old.mp4");
        let fresh = directory.join("fresh.mp4");
        std::fs::write(&old, b"old").expect("write");
        std::fs::write(&fresh, b"fresh").expect("write");

        let now = SystemTime::now() + std::time::Duration::from_secs(2 * 60 * 60);
        assert_eq!(prune_temp(&directory, now, TEMP_MAX_AGE), 0);
        assert!(old.exists() && fresh.exists());

        let later = SystemTime::now() + std::time::Duration::from_secs(25 * 60 * 60);
        assert_eq!(prune_temp(&directory, later, TEMP_MAX_AGE), 2);
        assert!(!old.exists() && !fresh.exists());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn sweeping_a_folder_that_was_never_made_is_not_a_failure() {
        let missing = std::env::temp_dir().join("cutix-temp-that-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);
        assert_eq!(prune_temp(&missing, SystemTime::now(), TEMP_MAX_AGE), 0);
    }

    #[test]
    fn the_handles_may_be_dragged_past_each_other_and_still_name_a_piece() {
        let (start, span) = trim_span(0.25, 0.75, 40.0);
        assert!((start - 10.0).abs() < 1e-9);
        assert!((span - 20.0).abs() < 1e-9);

        let (crossed, same) = trim_span(0.75, 0.25, 40.0);
        assert!((crossed - 10.0).abs() < 1e-9, "the earlier handle wins");
        assert!((same - 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_piece_of_a_video_of_unknown_length_is_empty_rather_than_wrong() {
        assert_eq!(trim_span(0.0, 1.0, 0.0), (0.0, 0.0));
        assert_eq!(trim_span(0.0, 1.0, f64::NAN), (0.0, 0.0));
        let (start, span) = trim_span(-3.0, 9.0, 10.0);
        assert_eq!((start, span), (0.0, 10.0), "the handles stay on the bar");
    }

    #[test]
    fn a_handed_over_clip_is_only_swept_once_the_upload_has_come_and_gone() {
        assert!(!upload_settled(false, false), "the upload has not begun");
        assert!(!upload_settled(true, true), "the upload is still running");
        assert!(upload_settled(true, false), "the upload is done with it");
    }
}

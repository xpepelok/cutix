use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Configured,
    DefaultVideosFolder,
}

impl Source {
    pub fn label_key(self) -> &'static str {
        match self {
            Source::Configured => "nvidia.source.configured",
            Source::DefaultVideosFolder => "nvidia.source.default",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub root: PathBuf,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    pub path: PathBuf,
    pub name: String,
    pub group: Option<String>,
}

pub trait Probe {
    fn installed(&self) -> bool;
    fn configured_path(&self) -> Option<PathBuf>;
    fn videos_dir(&self) -> Option<PathBuf>;
    fn is_dir(&self, path: &Path) -> bool;
}

pub const DEFAULT_SUBFOLDER: &str = "NVIDIA";

pub fn resolve(probe: &dyn Probe) -> Option<Library> {
    if !probe.installed() {
        return None;
    }

    if let Some(root) = probe.configured_path() {
        if probe.is_dir(&root) {
            return Some(Library {
                root,
                source: Source::Configured,
            });
        }
        return None;
    }

    let fallback = probe.videos_dir()?.join(DEFAULT_SUBFOLDER);
    probe.is_dir(&fallback).then_some(Library {
        root: fallback,
        source: Source::DefaultVideosFolder,
    })
}

pub fn discover() -> Option<Library> {
    resolve(&SystemProbe)
}

fn is_media(path: &Path) -> bool {
    cutix_project::probe::is_supported(path)
}

pub fn list(root: &Path) -> Vec<Recording> {
    let mut found = Vec::new();
    collect(root, None, &mut found);

    let groups: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    for group in groups {
        if found.len() >= MAX_ENTRIES {
            break;
        }
        let label = group
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string);
        collect(&group, label, &mut found);
    }

    found.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    found.truncate(MAX_ENTRIES);
    found
}

fn collect(directory: &Path, group: Option<String>, into: &mut Vec<Recording>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if into.len() >= MAX_ENTRIES {
            return;
        }
        let path = entry.path();
        if !path.is_file() || !is_media(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        into.push(Recording {
            path: path.clone(),
            name: name.to_string(),
            group: group.clone(),
        });
    }
}

struct SystemProbe;

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;

    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_READ, RRF_RT_REG_BINARY,
    };

    fn key_exists(root: HKEY, path: windows::core::PCWSTR) -> bool {
        let mut handle = HKEY::default();
        let status = unsafe { RegOpenKeyExW(root, path, Some(0), KEY_READ, &mut handle) };
        if status == ERROR_SUCCESS {
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return true;
        }
        false
    }

    pub fn configured_path() -> Option<PathBuf> {
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\NVIDIA Corporation\\Global\\ShadowPlay\\NVSPCAPS"),
                w!("DefaultPathW"),
                RRF_RT_REG_BINARY,
                None,
                None,
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }

        let mut bytes = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\NVIDIA Corporation\\Global\\ShadowPlay\\NVSPCAPS"),
                w!("DefaultPathW"),
                RRF_RT_REG_BINARY,
                None,
                Some(bytes.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        bytes.truncate(size as usize);
        super::decode_utf16le(&bytes)
    }

    pub fn installed() -> bool {
        key_exists(HKEY_CURRENT_USER, w!("Software\\NVIDIA Corporation"))
            || key_exists(HKEY_LOCAL_MACHINE, w!("SOFTWARE\\NVIDIA Corporation\\Global"))
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::PathBuf;

    pub fn configured_path() -> Option<PathBuf> {
        None
    }

    pub fn installed() -> bool {
        false
    }
}

pub fn decode_utf16le(bytes: &[u8]) -> Option<PathBuf> {
    if bytes.len() < 2 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    let text = String::from_utf16(&units).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

impl Probe for SystemProbe {
    fn installed(&self) -> bool {
        platform::installed()
    }

    fn configured_path(&self) -> Option<PathBuf> {
        platform::configured_path()
    }

    fn videos_dir(&self) -> Option<PathBuf> {
        dirs::video_dir()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        installed: bool,
        configured: Option<PathBuf>,
        videos: Option<PathBuf>,
        existing: Vec<PathBuf>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                installed: true,
                configured: None,
                videos: Some(PathBuf::from("C:\\Users\\a\\Videos")),
                existing: Vec::new(),
            }
        }
    }

    impl Probe for Fake {
        fn installed(&self) -> bool {
            self.installed
        }

        fn configured_path(&self) -> Option<PathBuf> {
            self.configured.clone()
        }

        fn videos_dir(&self) -> Option<PathBuf> {
            self.videos.clone()
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.existing.iter().any(|known| known == path)
        }
    }

    #[test]
    fn nothing_is_offered_when_nvidia_is_absent() {
        let probe = Fake {
            installed: false,
            configured: Some(PathBuf::from("C:\\Clips")),
            existing: vec![PathBuf::from("C:\\Clips")],
            ..Fake::default()
        };
        assert_eq!(resolve(&probe), None);
    }

    #[test]
    fn a_configured_folder_that_exists_wins() {
        let probe = Fake {
            configured: Some(PathBuf::from("D:\\Clips")),
            existing: vec![
                PathBuf::from("D:\\Clips"),
                PathBuf::from("C:\\Users\\a\\Videos\\NVIDIA"),
            ],
            ..Fake::default()
        };
        assert_eq!(
            resolve(&probe),
            Some(Library {
                root: PathBuf::from("D:\\Clips"),
                source: Source::Configured,
            })
        );
    }

    #[test]
    fn a_configured_folder_that_vanished_offers_nothing() {
        let probe = Fake {
            configured: Some(PathBuf::from("E:\\Gone")),
            existing: vec![PathBuf::from("C:\\Users\\a\\Videos\\NVIDIA")],
            ..Fake::default()
        };
        assert_eq!(resolve(&probe), None);
    }

    #[test]
    fn the_default_videos_subfolder_is_used_when_no_path_is_recorded() {
        let probe = Fake {
            existing: vec![PathBuf::from("C:\\Users\\a\\Videos\\NVIDIA")],
            ..Fake::default()
        };
        assert_eq!(
            resolve(&probe),
            Some(Library {
                root: PathBuf::from("C:\\Users\\a\\Videos\\NVIDIA"),
                source: Source::DefaultVideosFolder,
            })
        );
    }

    #[test]
    fn an_installed_driver_without_any_capture_folder_offers_nothing() {
        assert_eq!(resolve(&Fake::default()), None);
    }

    #[test]
    fn a_missing_videos_folder_offers_nothing() {
        let probe = Fake {
            videos: None,
            ..Fake::default()
        };
        assert_eq!(resolve(&probe), None);
    }

    #[test]
    fn binary_registry_paths_decode_as_utf16() {
        let mut bytes = Vec::new();
        for unit in "D:\\Clips".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(decode_utf16le(&bytes), Some(PathBuf::from("D:\\Clips")));
    }

    #[test]
    fn empty_or_truncated_registry_values_decode_to_nothing() {
        assert_eq!(decode_utf16le(&[]), None);
        assert_eq!(decode_utf16le(&[0, 0]), None);
        assert_eq!(decode_utf16le(&[0x41]), None);
    }

    #[test]
    fn listing_covers_the_root_and_one_level_of_game_folders() {
        let base = std::env::temp_dir().join(format!("cutix-nvidia-{}", uuid::Uuid::new_v4()));
        let game = base.join("Some Game");
        let deep = game.join("deeper");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(base.join("loose.mp4"), b"x").unwrap();
        std::fs::write(base.join("notes.txt"), b"x").unwrap();
        std::fs::write(game.join("clip.mp4"), b"x").unwrap();
        std::fs::write(deep.join("buried.mp4"), b"x").unwrap();

        let found = list(&base);
        let names: Vec<&str> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["clip.mp4", "loose.mp4"]);
        assert_eq!(
            found[0].group.as_deref(),
            Some("Some Game"),
            "a per-game folder becomes the group label"
        );
        assert_eq!(found[1].group, None);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn listing_a_missing_folder_is_empty_rather_than_an_error() {
        assert!(list(Path::new("Z:\\definitely\\not\\here")).is_empty());
    }
}

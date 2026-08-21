use std::path::Path;

#[cfg(any(windows, test))]
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov"];
#[cfg(any(windows, test))]
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav"];

#[cfg(any(windows, test))]
pub const APPLICATION_KEY: &str = "Software\\cutix";
#[cfg(any(windows, test))]
pub const CAPABILITIES_KEY: &str = "Software\\cutix\\Capabilities";
#[cfg(any(windows, test))]
pub const REGISTERED_APPLICATIONS_KEY: &str = "Software\\RegisteredApplications";
#[cfg(any(windows, test))]
pub const APPLICATION_NAME: &str = "cutix";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Video,
    Audio,
}

impl Group {
    #[cfg(any(windows, test))]
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Group::Video => VIDEO_EXTENSIONS,
            Group::Audio => AUDIO_EXTENSIONS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(windows, test))]
pub enum Op {
    SetValue {
        key: String,
        name: String,
        value: String,
    },

    AddOpenWith {
        key: String,
        name: String,
    },
    DeleteTree {
        key: String,
    },
    DeleteValue {
        key: String,
        name: String,
    },
}

#[cfg(any(windows, test))]
const FOLDER_VERB: &str = "cutix.Browse";

#[cfg(any(windows, test))]
fn folder_verb_keys() -> [String; 3] {
    [
        format!(r"Software\Classes\Directory\shell\{FOLDER_VERB}"),
        format!(r"Software\Classes\Directory\Background\shell\{FOLDER_VERB}"),
        format!(r"Software\Classes\Drive\shell\{FOLDER_VERB}"),
    ]
}

#[cfg(any(windows, test))]
const FILE_VERBS: [(&str, &str); 2] = [("cutix.Edit", ""), ("cutix.Publish", " --publish")];

#[cfg(any(windows, test))]
fn file_verb_key(extension: &str, verb: &str) -> String {
    format!(r"Software\Classes\SystemFileAssociations\.{extension}\shell\{verb}")
}

#[cfg(any(windows, test))]
pub fn plan_register_file_verbs(executable: &Path, labels: [&str; 2]) -> Vec<Op> {
    let exe = executable.display().to_string();
    let mut ops = Vec::new();

    for extension in extensions_for(&[Group::Video]) {
        for ((verb, argument), label) in FILE_VERBS.iter().zip(labels.iter()) {
            let key = file_verb_key(extension, verb);
            ops.push(Op::SetValue {
                key: key.clone(),
                name: String::new(),
                value: (*label).to_string(),
            });
            ops.push(Op::SetValue {
                key: key.clone(),
                name: "Icon".to_string(),
                value: format!("\"{exe}\",0"),
            });
            ops.push(Op::SetValue {
                key: format!(r"{key}\command"),
                name: String::new(),
                value: format!("\"{exe}\"{argument} \"%1\""),
            });
        }
    }

    ops
}

#[cfg(any(windows, test))]
pub fn plan_unregister_file_verbs() -> Vec<Op> {
    let mut ops = Vec::new();
    for extension in all_extensions() {
        for (verb, _) in FILE_VERBS {
            ops.push(Op::DeleteTree {
                key: file_verb_key(extension, verb),
            });
        }
    }
    ops
}

#[cfg(windows)]
pub fn register_file_verbs(labels: [&str; 2]) -> Result<(), String> {
    let exe = executable().ok_or_else(|| "cannot locate the running executable".to_string())?;
    platform::apply(&plan_register_file_verbs(&exe, labels))
}

#[cfg(not(windows))]
pub fn register_file_verbs(_labels: [&str; 2]) -> Result<(), String> {
    Ok(())
}

#[cfg(any(windows, test))]
pub fn plan_register_folder_verb(executable: &Path, label: &str) -> Vec<Op> {
    let exe = executable.display().to_string();
    let mut ops = Vec::new();

    for key in folder_verb_keys() {
        ops.push(Op::SetValue {
            key: key.clone(),
            name: String::new(),
            value: label.to_string(),
        });
        ops.push(Op::SetValue {
            key: key.clone(),
            name: "Icon".to_string(),
            value: format!("\"{exe}\",0"),
        });
        ops.push(Op::SetValue {
            key: format!(r"{key}\command"),
            name: String::new(),
            value: format!("\"{exe}\" --browse \"%V\""),
        });
    }

    ops
}

#[cfg(any(windows, test))]
pub fn plan_unregister_folder_verb() -> Vec<Op> {
    folder_verb_keys()
        .into_iter()
        .map(|key| Op::DeleteTree { key })
        .collect()
}

#[cfg(windows)]
pub fn register_folder_verb(label: &str) -> Result<(), String> {
    let exe = executable().ok_or_else(|| "cannot locate the running executable".to_string())?;
    platform::apply(&plan_register_folder_verb(&exe, label))
}

#[cfg(not(windows))]
pub fn register_folder_verb(_label: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(any(windows, test))]
pub fn progid(extension: &str) -> String {
    format!("cutix.{extension}")
}

#[cfg(any(windows, test))]
fn class_key(extension: &str) -> String {
    format!("Software\\Classes\\{}", progid(extension))
}

#[cfg(any(windows, test))]
fn extension_key(extension: &str) -> String {
    format!("Software\\Classes\\.{extension}")
}

#[cfg(any(windows, test))]
pub fn extensions_for(groups: &[Group]) -> Vec<&'static str> {
    let mut extensions: Vec<&'static str> = groups
        .iter()
        .flat_map(|group| group.extensions().iter().copied())
        .collect();
    extensions.sort_unstable();
    extensions.dedup();
    extensions
}

#[cfg(any(windows, test))]
pub fn all_extensions() -> Vec<&'static str> {
    extensions_for(&[Group::Video, Group::Audio])
}

#[cfg(any(windows, test))]
pub fn plan_register(executable: &Path, groups: &[Group]) -> Vec<Op> {
    let exe = executable.display().to_string();
    let mut ops = plan_forget_previous_name();

    for extension in extensions_for(groups) {
        let class = class_key(extension);
        ops.push(Op::SetValue {
            key: class.clone(),
            name: String::new(),
            value: format!("{APPLICATION_NAME} {} file", extension.to_uppercase()),
        });
        ops.push(Op::SetValue {
            key: format!("{class}\\DefaultIcon"),
            name: String::new(),
            value: format!("\"{exe}\",0"),
        });
        ops.push(Op::SetValue {
            key: format!("{class}\\shell\\open\\command"),
            name: String::new(),
            value: format!("\"{exe}\" \"%1\""),
        });
        ops.push(Op::AddOpenWith {
            key: format!("{}\\OpenWithProgids", extension_key(extension)),
            name: progid(extension),
        });
        ops.push(Op::SetValue {
            key: format!("{CAPABILITIES_KEY}\\FileAssociations"),
            name: format!(".{extension}"),
            value: progid(extension),
        });
    }

    ops.push(Op::SetValue {
        key: CAPABILITIES_KEY.to_string(),
        name: "ApplicationName".to_string(),
        value: APPLICATION_NAME.to_string(),
    });
    ops.push(Op::SetValue {
        key: CAPABILITIES_KEY.to_string(),
        name: "ApplicationDescription".to_string(),
        value: "Open source video editor".to_string(),
    });
    ops.push(Op::SetValue {
        key: REGISTERED_APPLICATIONS_KEY.to_string(),
        name: APPLICATION_NAME.to_string(),
        value: CAPABILITIES_KEY.to_string(),
    });

    ops
}

#[cfg(any(windows, test))]
const PREVIOUS_NAME: &str = "OpenCut";

#[cfg(any(windows, test))]
pub fn plan_forget_previous_name() -> Vec<Op> {
    let mut ops = Vec::new();

    for extension in all_extensions() {
        ops.push(Op::DeleteValue {
            key: format!(r"{}\OpenWithProgids", extension_key(extension)),
            name: format!("{PREVIOUS_NAME}.{extension}"),
        });
        ops.push(Op::DeleteTree {
            key: format!(r"Software\Classes\{PREVIOUS_NAME}.{extension}"),
        });
        for verb in ["Edit", "Publish"] {
            ops.push(Op::DeleteTree {
                key: format!(
                    r"Software\Classes\SystemFileAssociations\.{extension}\shell\{PREVIOUS_NAME}.{verb}"
                ),
            });
        }
    }

    for root in ["Directory", r"Directory\Background", "Drive"] {
        ops.push(Op::DeleteTree {
            key: format!(r"Software\Classes\{root}\shell\{PREVIOUS_NAME}.Browse"),
        });
    }

    ops.push(Op::DeleteValue {
        key: REGISTERED_APPLICATIONS_KEY.to_string(),
        name: PREVIOUS_NAME.to_string(),
    });
    ops.push(Op::DeleteTree {
        key: format!(r"Software\{PREVIOUS_NAME}"),
    });

    ops
}

#[cfg(any(windows, test))]
pub fn plan_unregister() -> Vec<Op> {
    let mut ops = Vec::new();

    for extension in all_extensions() {
        ops.push(Op::DeleteValue {
            key: format!("{}\\OpenWithProgids", extension_key(extension)),
            name: progid(extension),
        });
        ops.push(Op::DeleteTree {
            key: class_key(extension),
        });
    }

    ops.push(Op::DeleteValue {
        key: REGISTERED_APPLICATIONS_KEY.to_string(),
        name: APPLICATION_NAME.to_string(),
    });
    ops.push(Op::DeleteTree {
        key: APPLICATION_KEY.to_string(),
    });
    ops.extend(plan_unregister_folder_verb());
    ops.extend(plan_unregister_file_verbs());

    ops
}

#[cfg(any(windows, test))]
pub fn executable() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(windows)]
pub fn register(groups: &[Group]) -> Result<(), String> {
    let exe = executable().ok_or_else(|| "cannot locate the running executable".to_string())?;
    platform::apply(&plan_register(&exe, groups))
}

#[cfg(windows)]
pub fn unregister() -> Result<(), String> {
    platform::apply(&plan_unregister())
}

#[cfg(not(windows))]
pub fn register(_groups: &[Group]) -> Result<(), String> {
    Err("file type registration is only implemented on Windows".to_string())
}

#[cfg(not(windows))]
pub fn unregister() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
mod platform {
    use super::Op;

    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_NONE, REG_OPTION_NON_VOLATILE,
        REG_SZ,
    };
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    fn wide(text: &str) -> HSTRING {
        HSTRING::from(text)
    }

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn create(key: &str) -> Result<HKEY, String> {
        let mut handle = HKEY::default();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(wide(key).as_ptr()),
                Some(0),
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut handle,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("cannot create HKCU\\{key} ({})", status.0));
        }
        Ok(handle)
    }

    fn set_string(key: &str, name: &str, value: &str) -> Result<(), String> {
        let handle = create(key)?;
        let encoded: Vec<u8> = utf16(value)
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let status = unsafe {
            RegSetValueExW(
                handle,
                PCWSTR(wide(name).as_ptr()),
                Some(0),
                REG_SZ,
                Some(&encoded),
            )
        };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("cannot write HKCU\\{key}\\{name} ({})", status.0));
        }
        Ok(())
    }

    fn set_marker(key: &str, name: &str) -> Result<(), String> {
        let handle = create(key)?;
        let status = unsafe {
            RegSetValueExW(
                handle,
                PCWSTR(wide(name).as_ptr()),
                Some(0),
                REG_NONE,
                Some(&[]),
            )
        };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("cannot write HKCU\\{key}\\{name} ({})", status.0));
        }
        Ok(())
    }

    fn tolerate_missing(status: WIN32_ERROR) -> bool {
        status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND
    }

    fn delete_tree(key: &str) -> Result<(), String> {
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr())) };
        if !tolerate_missing(status) {
            return Err(format!("cannot remove HKCU\\{key} ({})", status.0));
        }
        Ok(())
    }

    fn delete_value(key: &str, name: &str) -> Result<(), String> {
        let mut handle = HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(wide(key).as_ptr()),
                Some(0),
                KEY_WRITE,
                &mut handle,
            )
        };
        if opened == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        if opened != ERROR_SUCCESS {
            return Err(format!("cannot open HKCU\\{key} ({})", opened.0));
        }
        let status = unsafe { RegDeleteValueW(handle, PCWSTR(wide(name).as_ptr())) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if !tolerate_missing(status) {
            return Err(format!("cannot remove HKCU\\{key}\\{name} ({})", status.0));
        }
        Ok(())
    }

    pub fn apply(ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::SetValue { key, name, value } => set_string(key, name, value)?,
                Op::AddOpenWith { key, name } => set_marker(key, name)?,
                Op::DeleteTree { key } => delete_tree(key)?,
                Op::DeleteValue { key, name } => delete_value(key, name)?,
            }
        }
        unsafe {
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plan() -> Vec<Op> {
        plan_register(&PathBuf::from("C:\\Apps\\cutix.exe"), &[Group::Video])
    }

    #[test]
    fn only_dependency_free_formats_are_registered() {
        let registered = all_extensions();
        for extension in video::ffmpeg::CONTAINERS {
            assert!(
                !registered.contains(extension),
                "{extension} needs FFmpeg at runtime and must not be registered"
            );
        }
        assert!(registered.contains(&"mp4"));
        assert!(registered.contains(&"mov"));
    }

    #[test]
    fn every_registered_extension_is_one_the_app_can_actually_open() {
        for extension in all_extensions() {
            assert!(
                cutix_project::probe::supported_extensions()
                    .iter()
                    .any(|known| known == extension),
                "{extension} is registered but not supported"
            );
        }
    }

    #[test]
    fn the_open_command_passes_the_file_path_through() {
        assert!(plan().contains(&Op::SetValue {
            key: "Software\\Classes\\cutix.mp4\\shell\\open\\command".to_string(),
            name: String::new(),
            value: "\"C:\\Apps\\cutix.exe\" \"%1\"".to_string(),
        }));
    }

    #[test]
    fn registration_never_writes_the_extension_default() {
        for op in plan() {
            if let Op::SetValue { key, name, .. } = &op {
                assert!(
                    !(key == "Software\\Classes\\.mp4" && name.is_empty()),
                    "registration must not claim the .mp4 default handler"
                );
            }
        }
        assert!(plan().contains(&Op::AddOpenWith {
            key: "Software\\Classes\\.mp4\\OpenWithProgids".to_string(),
            name: "cutix.mp4".to_string(),
        }));
    }

    #[test]
    fn registration_advertises_itself_to_the_default_apps_page() {
        assert!(plan().contains(&Op::SetValue {
            key: REGISTERED_APPLICATIONS_KEY.to_string(),
            name: APPLICATION_NAME.to_string(),
            value: CAPABILITIES_KEY.to_string(),
        }));
        assert!(plan().contains(&Op::SetValue {
            key: "Software\\cutix\\Capabilities\\FileAssociations".to_string(),
            name: ".mp4".to_string(),
            value: "cutix.mp4".to_string(),
        }));
    }

    #[test]
    fn audio_is_a_separate_opt_in_group() {
        let video_only = plan();
        assert!(!video_only.iter().any(|op| match op {
            Op::SetValue { key, .. } | Op::AddOpenWith { key, .. } => key.contains("mp3"),
            _ => false,
        }));
        let both = plan_register(
            &PathBuf::from("C:\\Apps\\cutix.exe"),
            &[Group::Video, Group::Audio],
        );
        assert!(both.iter().any(|op| match op {
            Op::AddOpenWith { name, .. } => name == "cutix.mp3",
            _ => false,
        }));
    }

    #[test]
    fn unregistering_undoes_everything_registration_creates() {
        let created = plan_register(
            &PathBuf::from("C:\\Apps\\cutix.exe"),
            &[Group::Video, Group::Audio],
        );
        let removed = plan_unregister();

        for op in &created {
            match op {
                Op::SetValue { key, name, .. } => {
                    let covered = removed.iter().any(|undo| match undo {
                        Op::DeleteTree { key: tree } => key.starts_with(tree.as_str()),
                        Op::DeleteValue {
                            key: other,
                            name: value,
                        } => other == key && value == name,
                        _ => false,
                    });
                    assert!(covered, "nothing removes HKCU\\{key}\\{name}");
                }
                Op::AddOpenWith { key, name } => {
                    assert!(
                        removed.iter().any(|undo| matches!(
                            undo,
                            Op::DeleteValue { key: other, name: value }
                                if other == key && value == name
                        )),
                        "nothing removes the Open With entry {key}\\{name}"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn unregistering_never_deletes_a_shared_extension_key() {
        for op in plan_unregister() {
            if let Op::DeleteTree { key } = op {
                assert!(
                    !key.starts_with("Software\\Classes\\."),
                    "{key} is shared with other applications"
                );
            }
        }
    }

    #[test]
    fn progids_are_namespaced_to_the_app() {
        assert_eq!(progid("mp4"), "cutix.mp4");
    }
}

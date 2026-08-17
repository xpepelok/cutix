use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui::Keystroke;
use serde::{Deserialize, Serialize};

pub const CURRENT_VERSION: u32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Playback,
    Navigation,
    Editing,
    Selection,
    History,
    Timeline,
    Controls,
    Assets,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Playback,
        Category::Navigation,
        Category::Editing,
        Category::Selection,
        Category::History,
        Category::Timeline,
        Category::Controls,
        Category::Assets,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Playback => "shortcuts.category.playback",
            Category::Navigation => "shortcuts.category.navigation",
            Category::Editing => "shortcuts.category.editing",
            Category::Selection => "shortcuts.category.selection",
            Category::History => "shortcuts.category.history",
            Category::Timeline => "shortcuts.category.timeline",
            Category::Controls => "shortcuts.category.controls",
            Category::Assets => "shortcuts.category.assets",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    TogglePlay,
    StopPlayback,
    SeekForward,
    SeekBackward,
    FrameStepForward,
    FrameStepBackward,
    JumpForward,
    JumpBackward,
    GotoStart,
    GotoEnd,
    Split,
    SplitLeft,
    SplitRight,
    DeleteSelected,
    CopySelected,
    PasteCopied,
    ToggleSnapping,
    ToggleRippleEditing,
    ToggleSourceAudio,
    SelectAll,
    CancelInteraction,
    DeselectAll,
    DuplicateSelected,
    ToggleElementsMuted,
    ToggleElementsVisibility,
    ToggleBookmark,
    Undo,
    Redo,
    PreviewVolumeUp,
    PreviewVolumeDown,
    PreviewToggleMute,
    PreviewToggleFullscreen,
    RemoveMediaAsset,
    RemoveMediaAssets,
}

impl Action {
    pub const ALL: [Action; 34] = [
        Action::TogglePlay,
        Action::StopPlayback,
        Action::SeekForward,
        Action::SeekBackward,
        Action::FrameStepForward,
        Action::FrameStepBackward,
        Action::JumpForward,
        Action::JumpBackward,
        Action::GotoStart,
        Action::GotoEnd,
        Action::Split,
        Action::SplitLeft,
        Action::SplitRight,
        Action::DeleteSelected,
        Action::CopySelected,
        Action::PasteCopied,
        Action::ToggleSnapping,
        Action::ToggleRippleEditing,
        Action::ToggleSourceAudio,
        Action::SelectAll,
        Action::CancelInteraction,
        Action::DeselectAll,
        Action::DuplicateSelected,
        Action::ToggleElementsMuted,
        Action::ToggleElementsVisibility,
        Action::ToggleBookmark,
        Action::Undo,
        Action::Redo,
        Action::PreviewVolumeUp,
        Action::PreviewVolumeDown,
        Action::PreviewToggleMute,
        Action::PreviewToggleFullscreen,
        Action::RemoveMediaAsset,
        Action::RemoveMediaAssets,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Action::TogglePlay => "toggle-play",
            Action::StopPlayback => "stop-playback",
            Action::SeekForward => "seek-forward",
            Action::SeekBackward => "seek-backward",
            Action::FrameStepForward => "frame-step-forward",
            Action::FrameStepBackward => "frame-step-backward",
            Action::JumpForward => "jump-forward",
            Action::JumpBackward => "jump-backward",
            Action::GotoStart => "goto-start",
            Action::GotoEnd => "goto-end",
            Action::Split => "split",
            Action::SplitLeft => "split-left",
            Action::SplitRight => "split-right",
            Action::DeleteSelected => "delete-selected",
            Action::CopySelected => "copy-selected",
            Action::PasteCopied => "paste-copied",
            Action::ToggleSnapping => "toggle-snapping",
            Action::ToggleRippleEditing => "toggle-ripple-editing",
            Action::ToggleSourceAudio => "toggle-source-audio",
            Action::SelectAll => "select-all",
            Action::CancelInteraction => "cancel-interaction",
            Action::DeselectAll => "deselect-all",
            Action::DuplicateSelected => "duplicate-selected",
            Action::ToggleElementsMuted => "toggle-elements-muted-selected",
            Action::ToggleElementsVisibility => "toggle-elements-visibility-selected",
            Action::ToggleBookmark => "toggle-bookmark",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::PreviewVolumeUp => "preview-volume-up",
            Action::PreviewVolumeDown => "preview-volume-down",
            Action::PreviewToggleMute => "preview-toggle-mute",
            Action::PreviewToggleFullscreen => "preview-toggle-fullscreen",
            Action::RemoveMediaAsset => "remove-media-asset",
            Action::RemoveMediaAssets => "remove-media-assets",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.id() == id)
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Action::TogglePlay => "shortcuts.action.togglePlay",
            Action::StopPlayback => "shortcuts.action.stopPlayback",
            Action::SeekForward => "shortcuts.action.seekForward",
            Action::SeekBackward => "shortcuts.action.seekBackward",
            Action::FrameStepForward => "shortcuts.action.frameStepForward",
            Action::FrameStepBackward => "shortcuts.action.frameStepBackward",
            Action::JumpForward => "shortcuts.action.jumpForward",
            Action::JumpBackward => "shortcuts.action.jumpBackward",
            Action::GotoStart => "shortcuts.action.gotoStart",
            Action::GotoEnd => "shortcuts.action.gotoEnd",
            Action::Split => "shortcuts.action.split",
            Action::SplitLeft => "shortcuts.action.splitLeft",
            Action::SplitRight => "shortcuts.action.splitRight",
            Action::DeleteSelected => "shortcuts.action.deleteSelected",
            Action::CopySelected => "shortcuts.action.copySelected",
            Action::PasteCopied => "shortcuts.action.pasteCopied",
            Action::ToggleSnapping => "shortcuts.action.toggleSnapping",
            Action::ToggleRippleEditing => "shortcuts.action.toggleRippleEditing",
            Action::ToggleSourceAudio => "shortcuts.action.toggleSourceAudio",
            Action::SelectAll => "shortcuts.action.selectAll",
            Action::CancelInteraction => "shortcuts.action.cancelInteraction",
            Action::DeselectAll => "shortcuts.action.deselectAll",
            Action::DuplicateSelected => "shortcuts.action.duplicateSelected",
            Action::ToggleElementsMuted => "shortcuts.action.toggleElementsMuted",
            Action::ToggleElementsVisibility => "shortcuts.action.toggleElementsVisibility",
            Action::ToggleBookmark => "shortcuts.action.toggleBookmark",
            Action::Undo => "shortcuts.action.undo",
            Action::Redo => "shortcuts.action.redo",
            Action::PreviewVolumeUp => "shortcuts.action.previewVolumeUp",
            Action::PreviewVolumeDown => "shortcuts.action.previewVolumeDown",
            Action::PreviewToggleMute => "shortcuts.action.previewToggleMute",
            Action::PreviewToggleFullscreen => "shortcuts.action.previewToggleFullscreen",
            Action::RemoveMediaAsset => "shortcuts.action.removeMediaAsset",
            Action::RemoveMediaAssets => "shortcuts.action.removeMediaAssets",
        }
    }

    pub fn category(self) -> Category {
        match self {
            Action::TogglePlay
            | Action::StopPlayback
            | Action::SeekForward
            | Action::SeekBackward
            | Action::PreviewVolumeUp
            | Action::PreviewVolumeDown
            | Action::PreviewToggleMute => Category::Playback,
            Action::FrameStepForward
            | Action::FrameStepBackward
            | Action::JumpForward
            | Action::JumpBackward
            | Action::GotoStart
            | Action::GotoEnd => Category::Navigation,
            Action::Split
            | Action::SplitLeft
            | Action::SplitRight
            | Action::DeleteSelected
            | Action::CopySelected
            | Action::PasteCopied
            | Action::ToggleSnapping
            | Action::ToggleRippleEditing
            | Action::ToggleSourceAudio => Category::Editing,
            Action::SelectAll
            | Action::DeselectAll
            | Action::DuplicateSelected
            | Action::ToggleElementsMuted
            | Action::ToggleElementsVisibility => Category::Selection,
            Action::CancelInteraction | Action::PreviewToggleFullscreen => Category::Controls,
            Action::ToggleBookmark => Category::Timeline,
            Action::Undo | Action::Redo => Category::History,
            Action::RemoveMediaAsset | Action::RemoveMediaAssets => Category::Assets,
        }
    }

    pub fn is_bindable(self) -> bool {
        !matches!(self, Action::RemoveMediaAsset | Action::RemoveMediaAssets)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

const NAMED_KEYS: &[&str] = &[
    "up",
    "down",
    "left",
    "right",
    "enter",
    "tab",
    "space",
    "escape",
    "esc",
    "backspace",
    "delete",
    "home",
    "end",
    "/",
    "?",
    ".",
];

impl Chord {
    pub fn new(control: bool, alt: bool, shift: bool, key: impl Into<String>) -> Self {
        Self {
            control,
            alt,
            shift,
            key: key.into(),
        }
    }

    pub fn parse(value: &str) -> Option<Chord> {
        let mut chord = Chord::new(false, false, false, "");
        let mut rest = value;
        loop {
            let Some((head, tail)) = rest.split_once('+') else {
                break;
            };
            match head {
                "ctrl" if !chord.control => chord.control = true,
                "alt" if !chord.alt => chord.alt = true,
                "shift" if !chord.shift => chord.shift = true,
                _ => return None,
            }
            rest = tail;
        }
        if !is_key(rest) {
            return None;
        }
        chord.key = normalize_key(rest);
        Some(chord)
    }

    pub fn to_key_string(&self) -> String {
        let mut out = String::new();
        if self.control {
            out.push_str("ctrl+");
        }
        if self.alt {
            out.push_str("alt+");
        }
        if self.shift {
            out.push_str("shift+");
        }
        out.push_str(&self.key);
        out
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.control {
            parts.push(String::from("Ctrl"));
        }
        if self.alt {
            parts.push(String::from("Alt"));
        }
        if self.shift {
            parts.push(String::from("Shift"));
        }
        parts.push(display_key(&self.key));
        parts.join("+")
    }

    pub fn from_keystroke(keystroke: &Keystroke) -> Option<Chord> {
        let key = normalize_key(&keystroke.key.to_lowercase());
        if !is_key(&key) {
            return None;
        }
        let modifiers = keystroke.modifiers;
        Some(Chord::new(
            modifiers.control || modifiers.platform,
            modifiers.alt,
            modifiers.shift,
            key,
        ))
    }
}

fn normalize_key(key: &str) -> String {
    match key {
        "esc" => String::from("escape"),
        "spacebar" | " " => String::from("space"),
        other => other.to_string(),
    }
}

fn is_key(value: &str) -> bool {
    if value.len() == 1 {
        let ch = value.chars().next().unwrap_or('\0');
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            return true;
        }
    }
    NAMED_KEYS.contains(&value)
}

fn display_key(key: &str) -> String {
    match key {
        "left" => String::from("\u{2190}"),
        "right" => String::from("\u{2192}"),
        "up" => String::from("\u{2191}"),
        "down" => String::from("\u{2193}"),
        "space" => String::from("Space"),
        "home" => String::from("Home"),
        "end" => String::from("End"),
        "enter" => String::from("Enter"),
        "tab" => String::from("Tab"),
        "escape" | "esc" => String::from("Esc"),
        "delete" => String::from("Delete"),
        "backspace" => String::from("Backspace"),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

pub const DEFAULT_SHORTCUTS: &[(Action, &[&str])] = &[
    (Action::TogglePlay, &["space", "k"]),
    (Action::SeekForward, &["l"]),
    (Action::SeekBackward, &["j"]),
    (Action::FrameStepForward, &["right"]),
    (Action::FrameStepBackward, &["left"]),
    (Action::JumpForward, &["shift+right"]),
    (Action::JumpBackward, &["shift+left"]),
    (Action::GotoStart, &["home", "enter"]),
    (Action::GotoEnd, &["end"]),
    (Action::Split, &["s"]),
    (Action::SplitLeft, &["q"]),
    (Action::SplitRight, &["w"]),
    (Action::DeleteSelected, &["backspace", "delete"]),
    (Action::CopySelected, &["ctrl+c"]),
    (Action::PasteCopied, &["ctrl+v"]),
    (Action::ToggleSnapping, &["n"]),
    (Action::SelectAll, &["ctrl+a"]),
    (Action::CancelInteraction, &["escape"]),
    (Action::DuplicateSelected, &["ctrl+d"]),
    (Action::Undo, &["ctrl+z"]),
    (Action::Redo, &["ctrl+shift+z", "ctrl+y"]),
];

pub const NATIVE_DEFAULT_SHORTCUTS: &[(Action, &[&str])] = &[
    (Action::PreviewVolumeUp, &["up"]),
    (Action::PreviewVolumeDown, &["down"]),
    (Action::PreviewToggleMute, &["m"]),
    (Action::PreviewToggleFullscreen, &["f"]),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub chord: Chord,
    pub existing: Action,
    pub incoming: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keybindings {
    map: BTreeMap<Chord, Action>,
    pub customized: bool,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Keybindings {
    pub fn defaults() -> Self {
        let mut map = BTreeMap::new();
        for (action, chords) in DEFAULT_SHORTCUTS.iter().chain(NATIVE_DEFAULT_SHORTCUTS) {
            for chord in *chords {
                if let Some(chord) = Chord::parse(chord) {
                    map.insert(chord, *action);
                }
            }
        }
        Self {
            map,
            customized: false,
        }
    }

    pub fn action_for(&self, chord: &Chord) -> Option<Action> {
        self.map.get(chord).copied()
    }

    pub fn chords_for(&self, action: Action) -> Vec<Chord> {
        let defaults: Vec<Chord> = DEFAULT_SHORTCUTS
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, chords)| {
                chords
                    .iter()
                    .filter_map(|chord| Chord::parse(chord))
                    .collect()
            })
            .unwrap_or_default();

        let mut ordered: Vec<Chord> = defaults
            .into_iter()
            .filter(|chord| self.map.get(chord) == Some(&action))
            .collect();
        for (chord, bound) in &self.map {
            if *bound == action && !ordered.contains(chord) {
                ordered.push(chord.clone());
            }
        }
        ordered
    }

    pub fn conflict(&self, chord: &Chord, action: Action) -> Option<Conflict> {
        match self.map.get(chord) {
            Some(existing) if *existing != action => Some(Conflict {
                chord: chord.clone(),
                existing: *existing,
                incoming: action,
            }),
            _ => None,
        }
    }

    pub fn bind(&mut self, chord: Chord, action: Action) {
        self.map.insert(chord, action);
        self.customized = true;
    }

    pub fn unbind(&mut self, chord: &Chord) {
        if self.map.remove(chord).is_some() {
            self.customized = true;
        }
    }

    pub fn rebind(&mut self, action: Action, chord: Chord) {
        for existing in self.chords_for(action) {
            self.map.remove(&existing);
        }
        self.map.insert(chord, action);
        self.customized = true;
    }

    pub fn reset(&mut self) {
        *self = Self::defaults();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredKeybindings {
    version: u32,
    #[serde(default)]
    is_customized: bool,
    keybindings: BTreeMap<String, String>,
}

pub fn keybindings_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("cutix").join("native-keybindings.json"))
}

pub fn serialize(bindings: &Keybindings) -> String {
    let stored = StoredKeybindings {
        version: CURRENT_VERSION,
        is_customized: bindings.customized,
        keybindings: bindings
            .map
            .iter()
            .map(|(chord, action)| (chord.to_key_string(), action.id().to_string()))
            .collect(),
    };
    serde_json::to_string_pretty(&stored).unwrap_or_default()
}

pub fn deserialize(bytes: &[u8]) -> Option<Keybindings> {
    let stored: StoredKeybindings = serde_json::from_slice(bytes).ok()?;
    if stored.version != CURRENT_VERSION {
        return None;
    }
    let mut map = BTreeMap::new();
    for (chord, action) in stored.keybindings {
        if let (Some(chord), Some(action)) = (Chord::parse(&chord), Action::from_id(&action)) {
            if action.is_bindable() {
                map.insert(chord, action);
            }
        }
    }

    if !stored.is_customized {
        for (action, chords) in DEFAULT_SHORTCUTS.iter().chain(NATIVE_DEFAULT_SHORTCUTS) {
            for chord in *chords {
                if let Some(chord) = Chord::parse(chord) {
                    map.entry(chord).or_insert(*action);
                }
            }
        }
    }

    Some(Keybindings {
        map,
        customized: stored.is_customized,
    })
}

pub fn load() -> Keybindings {
    keybindings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| deserialize(&bytes))
        .unwrap_or_else(Keybindings::defaults)
}

pub fn save(bindings: &Keybindings) {
    let Some(path) = keybindings_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serialize(bindings));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    #[test]
    fn every_action_has_a_unique_id_and_a_translated_description() {
        let mut ids: Vec<&str> = Action::ALL.iter().map(|action| action.id()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
        for action in Action::ALL {
            let key = action.description_key();
            assert_ne!(cutix_i18n::t(key), key, "{}", action.id());
        }
    }

    #[test]
    fn every_category_key_resolves() {
        for category in Category::ALL {
            let key = category.key();
            assert_ne!(cutix_i18n::t(key), key);
        }
    }

    #[test]
    fn action_ids_round_trip() {
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
        }
        assert_eq!(Action::from_id("split-element"), None);
    }

    #[test]
    fn the_default_table_matches_the_web_definitions() {
        let expected: &[(&str, &[&str])] = &[
            ("toggle-play", &["space", "k"]),
            ("seek-forward", &["l"]),
            ("seek-backward", &["j"]),
            ("frame-step-forward", &["right"]),
            ("frame-step-backward", &["left"]),
            ("jump-forward", &["shift+right"]),
            ("jump-backward", &["shift+left"]),
            ("goto-start", &["home", "enter"]),
            ("goto-end", &["end"]),
            ("split", &["s"]),
            ("split-left", &["q"]),
            ("split-right", &["w"]),
            ("delete-selected", &["backspace", "delete"]),
            ("copy-selected", &["ctrl+c"]),
            ("paste-copied", &["ctrl+v"]),
            ("toggle-snapping", &["n"]),
            ("select-all", &["ctrl+a"]),
            ("cancel-interaction", &["escape"]),
            ("duplicate-selected", &["ctrl+d"]),
            ("undo", &["ctrl+z"]),
            ("redo", &["ctrl+shift+z", "ctrl+y"]),
        ];
        assert_eq!(DEFAULT_SHORTCUTS.len(), expected.len());
        for ((action, chords), (id, expected_chords)) in DEFAULT_SHORTCUTS.iter().zip(expected) {
            assert_eq!(action.id(), *id);
            assert_eq!(chords, expected_chords, "{id}");
        }
    }

    #[test]
    fn an_untouched_stored_file_gains_shortcuts_added_since_it_was_written() {
        let stored = format!(
            "{{\"version\":{CURRENT_VERSION},\"isCustomized\":false,\"keybindings\":{{\"k\":\"toggle-play\"}}}}"
        );
        let bindings = deserialize(stored.as_bytes()).expect("deserialise");
        assert_eq!(
            bindings.action_for(&Chord::parse("m").unwrap()),
            Some(Action::PreviewToggleMute)
        );
        assert_eq!(
            bindings.action_for(&Chord::parse("f").unwrap()),
            Some(Action::PreviewToggleFullscreen)
        );
    }

    #[test]
    fn a_customised_stored_file_is_left_alone() {
        let stored = format!(
            "{{\"version\":{CURRENT_VERSION},\"isCustomized\":true,\"keybindings\":{{\"k\":\"toggle-play\"}}}}"
        );
        let bindings = deserialize(stored.as_bytes()).expect("deserialise");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.action_for(&Chord::parse("m").unwrap()), None);
    }

    #[test]
    fn the_defaults_bind_every_chord_exactly_once() {
        let bindings = Keybindings::defaults();
        let total: usize = DEFAULT_SHORTCUTS
            .iter()
            .chain(NATIVE_DEFAULT_SHORTCUTS)
            .map(|(_, keys)| keys.len())
            .sum();
        assert_eq!(bindings.len(), total);
    }

    #[test]
    fn only_the_asset_actions_are_unbindable() {
        let unbindable: Vec<&str> = Action::ALL
            .iter()
            .filter(|action| !action.is_bindable())
            .map(|action| action.id())
            .collect();
        assert_eq!(
            unbindable,
            vec!["remove-media-asset", "remove-media-assets"]
        );
    }

    #[test]
    fn chords_parse_and_format_back_to_the_same_string() {
        for value in [
            "space",
            "k",
            "ctrl+z",
            "ctrl+shift+z",
            "shift+right",
            "ctrl+alt+shift+7",
            "backspace",
            "escape",
            ".",
        ] {
            let chord = Chord::parse(value).unwrap_or_else(|| panic!("{value}"));
            assert_eq!(chord.to_key_string(), value);
        }
    }

    #[test]
    fn modifiers_parse_in_any_position_but_serialize_canonically() {
        let chord = Chord::parse("shift+ctrl+z").unwrap();
        assert_eq!(chord.to_key_string(), "ctrl+shift+z");
    }

    #[test]
    fn malformed_chords_are_rejected() {
        for value in [
            "",
            "ctrl+",
            "meta+z",
            "ctrl+ctrl+z",
            "f1",
            "ctrl+space+z",
            "Z",
        ] {
            assert_eq!(Chord::parse(value), None, "{value}");
        }
    }

    #[test]
    fn esc_is_normalized_onto_escape() {
        assert_eq!(Chord::parse("esc").unwrap().key, "escape");
        assert_eq!(Chord::parse("esc"), Chord::parse("escape"));
    }

    #[test]
    fn display_uses_arrows_and_capitalized_names() {
        assert_eq!(
            Chord::parse("shift+right").unwrap().display(),
            "Shift+\u{2192}"
        );
        assert_eq!(
            Chord::parse("ctrl+shift+z").unwrap().display(),
            "Ctrl+Shift+Z"
        );
        assert_eq!(Chord::parse("space").unwrap().display(), "Space");
        assert_eq!(Chord::parse("escape").unwrap().display(), "Esc");
    }

    fn keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        }
    }

    #[test]
    fn keystrokes_become_the_chord_they_are_bound_as() {
        let mut modifiers = Modifiers::default();
        modifiers.control = true;
        modifiers.shift = true;
        let chord = Chord::from_keystroke(&keystroke("z", modifiers)).unwrap();
        assert_eq!(chord.to_key_string(), "ctrl+shift+z");
        assert_eq!(
            Keybindings::defaults().action_for(&chord),
            Some(Action::Redo)
        );
    }

    #[test]
    fn function_keys_are_not_chords() {
        assert_eq!(
            Chord::from_keystroke(&keystroke("f5", Modifiers::default())),
            None
        );
    }

    #[test]
    fn conflicts_are_reported_against_the_action_already_holding_the_chord() {
        let bindings = Keybindings::defaults();
        let chord = Chord::parse("s").unwrap();
        let conflict = bindings.conflict(&chord, Action::GotoEnd).unwrap();
        assert_eq!(conflict.existing, Action::Split);
        assert_eq!(conflict.incoming, Action::GotoEnd);
        assert_eq!(bindings.conflict(&chord, Action::Split), None);
        assert_eq!(
            bindings.conflict(&Chord::parse("x").unwrap(), Action::Split),
            None
        );
    }

    #[test]
    fn rebinding_moves_an_action_off_all_of_its_old_chords() {
        let mut bindings = Keybindings::defaults();
        assert_eq!(bindings.chords_for(Action::TogglePlay).len(), 2);
        bindings.rebind(Action::TogglePlay, Chord::parse("p").unwrap());
        assert_eq!(bindings.action_for(&Chord::parse("space").unwrap()), None);
        assert_eq!(bindings.action_for(&Chord::parse("k").unwrap()), None);
        assert_eq!(
            bindings.action_for(&Chord::parse("p").unwrap()),
            Some(Action::TogglePlay)
        );
        assert!(bindings.customized);
    }

    #[test]
    fn resetting_restores_the_defaults_and_clears_the_customized_flag() {
        let mut bindings = Keybindings::defaults();
        bindings.rebind(Action::Split, Chord::parse("g").unwrap());
        bindings.reset();
        assert_eq!(bindings, Keybindings::defaults());
        assert!(!bindings.customized);
    }

    #[test]
    fn persistence_round_trips_a_customized_table() {
        let mut bindings = Keybindings::defaults();
        bindings.rebind(Action::Split, Chord::parse("ctrl+alt+g").unwrap());
        bindings.unbind(&Chord::parse("k").unwrap());
        let json = serialize(&bindings);
        let restored = deserialize(json.as_bytes()).unwrap();
        assert_eq!(restored, bindings);
        assert!(restored.customized);
    }

    #[test]
    fn a_file_from_another_version_falls_back_to_the_defaults() {
        let json = serialize(&Keybindings::defaults()).replace("\"version\": 7", "\"version\": 6");
        assert_eq!(deserialize(json.as_bytes()), None);
    }

    #[test]
    fn unknown_chords_and_actions_are_dropped_not_fatal() {
        let json = r#"{"version":7,"isCustomized":true,"keybindings":{"ctrl+z":"undo","f9":"redo","s":"split-element"}}"#;
        let restored = deserialize(json.as_bytes()).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(
            restored.action_for(&Chord::parse("ctrl+z").unwrap()),
            Some(Action::Undo)
        );
    }

    #[test]
    fn asset_actions_cannot_be_smuggled_in_through_the_file() {
        let json =
            r#"{"version":7,"isCustomized":true,"keybindings":{"ctrl+r":"remove-media-asset"}}"#;
        assert!(deserialize(json.as_bytes()).unwrap().is_empty());
    }
}

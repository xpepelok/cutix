use std::path::PathBuf;

use cutix_i18n::t;

pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "webm", "mkv", "avi", "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus",
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "avif",
];
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt"];
pub const LUT_EXTENSIONS: &[&str] = &["cube"];
pub const TEMPLATE_EXTENSIONS: &[&str] = &["json"];
pub const CAPCUT_EXTENSIONS: &[&str] = &["json"];
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4"];
pub const PROJECT_EXTENSIONS: &[&str] = &[cutix_export::PACKAGE_EXTENSION];

pub enum Filter {
    Media,
    Subtitles,
    Lut,
    Template,
    CapCut,
    Video,
    Project,
}

impl Filter {
    fn extensions(&self) -> &'static [&'static str] {
        match self {
            Filter::Media => MEDIA_EXTENSIONS,
            Filter::Subtitles => SUBTITLE_EXTENSIONS,
            Filter::Lut => LUT_EXTENSIONS,
            Filter::Template => TEMPLATE_EXTENSIONS,
            Filter::CapCut => CAPCUT_EXTENSIONS,
            Filter::Video => VIDEO_EXTENSIONS,
            Filter::Project => PROJECT_EXTENSIONS,
        }
    }

    fn label(&self) -> String {
        match self {
            Filter::Media => t("dialog.filter.media"),
            Filter::Subtitles => t("dialog.filter.subtitles"),
            Filter::Lut => t("dialog.filter.lut"),
            Filter::Template => t("dialog.filter.template"),
            Filter::CapCut => t("dialog.filter.capcut"),
            Filter::Video => t("dialog.filter.video"),
            Filter::Project => t("dialog.filter.project"),
        }
    }

    fn apply(&self, dialog: rfd::AsyncFileDialog) -> rfd::AsyncFileDialog {
        dialog
            .add_filter(self.label(), self.extensions())
            .add_filter(t("dialog.filter.all"), &["*"])
    }
}

pub async fn open_files(filter: Filter, title: String, multiple: bool) -> Vec<PathBuf> {
    let dialog = filter.apply(rfd::AsyncFileDialog::new().set_title(title));
    let handles = if multiple {
        dialog.pick_files().await
    } else {
        dialog.pick_file().await.map(|handle| vec![handle])
    };
    handles
        .unwrap_or_default()
        .into_iter()
        .map(|handle| handle.path().to_path_buf())
        .collect()
}

pub async fn open_folder(title: String, start: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some(start) = start.filter(|path| path.is_dir()) {
        dialog = dialog.set_directory(start);
    }
    Some(dialog.pick_folder().await?.path().to_path_buf())
}

pub async fn save_file(
    filter: Filter,
    title: String,
    directory: PathBuf,
    file_name: String,
) -> Option<PathBuf> {
    let extension = filter.extensions().first().copied().unwrap_or("");
    let dialog = filter.apply(
        rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_directory(&directory)
            .set_file_name(&file_name),
    );
    let path = dialog.save_file().await?.path().to_path_buf();
    Some(ensure_extension(path, extension))
}

pub fn ensure_extension(path: PathBuf, extension: &str) -> PathBuf {
    if extension.is_empty() {
        return path;
    }
    let matches = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension));
    if matches {
        path
    } else {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("export")
            .to_owned();
        path.with_file_name(format!("{name}.{extension}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_extension_is_appended() {
        let path = ensure_extension(PathBuf::from("C:/tmp/clip"), "mp4");
        assert_eq!(path, PathBuf::from("C:/tmp/clip.mp4"));
    }

    #[test]
    fn a_matching_extension_is_kept() {
        let path = ensure_extension(PathBuf::from("C:/tmp/clip.MP4"), "mp4");
        assert_eq!(path, PathBuf::from("C:/tmp/clip.MP4"));
    }

    #[test]
    fn a_different_extension_is_not_replaced() {
        let path = ensure_extension(PathBuf::from("C:/tmp/clip.v2"), "srt");
        assert_eq!(path, PathBuf::from("C:/tmp/clip.v2.srt"));
    }

    #[test]
    fn an_empty_extension_is_a_no_op() {
        let path = ensure_extension(PathBuf::from("C:/tmp/clip"), "");
        assert_eq!(path, PathBuf::from("C:/tmp/clip"));
    }

    #[test]
    fn subtitles_do_not_offer_video() {
        assert!(!SUBTITLE_EXTENSIONS.contains(&"mp4"));
        assert!(SUBTITLE_EXTENSIONS.contains(&"srt"));
        assert!(SUBTITLE_EXTENSIONS.contains(&"vtt"));
    }

    #[test]
    fn media_covers_the_three_kinds() {
        assert!(MEDIA_EXTENSIONS.contains(&"mp4"));
        assert!(MEDIA_EXTENSIONS.contains(&"wav"));
        assert!(MEDIA_EXTENSIONS.contains(&"png"));
        assert!(!MEDIA_EXTENSIONS.contains(&"srt"));
    }
}

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_TITLE_CHARS: usize = 100;
pub const MAX_DESCRIPTION_CHARS: usize = 5_000;
pub const MAX_TAG_CHARS: usize = 500;
pub const MAX_TAGS: usize = 60;

pub const CATEGORIES: [(&str, &str); 15] = [
    ("1", "Film & Animation"),
    ("2", "Autos & Vehicles"),
    ("10", "Music"),
    ("15", "Pets & Animals"),
    ("17", "Sports"),
    ("19", "Travel & Events"),
    ("20", "Gaming"),
    ("22", "People & Blogs"),
    ("23", "Comedy"),
    ("24", "Entertainment"),
    ("25", "News & Politics"),
    ("26", "Howto & Style"),
    ("27", "Education"),
    ("28", "Science & Technology"),
    ("29", "Nonprofits & Activism"),
];

pub const DEFAULT_CATEGORY: &str = "22";

pub fn category_label(id: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, label)| *label)
}

pub fn category_key(id: &str) -> String {
    format!("youtube.category.{id}")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Privacy {
    Public,
    Unlisted,
    #[default]
    Private,
}

impl Privacy {
    pub const ALL: [Privacy; 3] = [Privacy::Public, Privacy::Unlisted, Privacy::Private];

    pub fn as_api(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Unlisted => "unlisted",
            Self::Private => "private",
        }
    }

    pub fn from_api(value: &str) -> Self {
        match value {
            "public" => Self::Public,
            "unlisted" => Self::Unlisted,
            _ => Self::Private,
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Public => "youtube.privacy.public",
            Self::Unlisted => "youtube.privacy.unlisted",
            Self::Private => "youtube.privacy.private",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum License {
    #[default]
    Standard,
    CreativeCommons,
}

impl License {
    pub const ALL: [License; 2] = [License::Standard, License::CreativeCommons];

    pub fn option_index(&self) -> usize {
        match self {
            Self::Standard => 0,
            Self::CreativeCommons => 1,
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Standard => "youtube.license.standard",
            Self::CreativeCommons => "youtube.license.creativeCommons",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comments {
    #[default]
    On,
    HoldInappropriate,
    HoldInappropriateStrict,
    HoldAll,
    Off,
}

impl Comments {
    pub const ALL: [Comments; 5] = [
        Comments::On,
        Comments::HoldInappropriate,
        Comments::HoldInappropriateStrict,
        Comments::HoldAll,
        Comments::Off,
    ];

    pub fn enabled(&self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn moderation_index(&self) -> Option<usize> {
        match self {
            Self::On => Some(0),
            Self::HoldInappropriate => Some(1),
            Self::HoldInappropriateStrict => Some(2),
            Self::HoldAll => Some(3),
            Self::Off => None,
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::On => "youtube.comments.on",
            Self::HoldInappropriate => "youtube.comments.holdInappropriate",
            Self::HoldInappropriateStrict => "youtube.comments.holdInappropriateStrict",
            Self::HoldAll => "youtube.comments.holdAll",
            Self::Off => "youtube.comments.off",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Remix {
    #[default]
    VideoAndAudio,
    AudioOnly,
    NotAllowed,
}

impl Remix {
    pub const ALL: [Remix; 3] = [Remix::VideoAndAudio, Remix::AudioOnly, Remix::NotAllowed];

    pub fn selector(&self) -> &'static str {
        match self {
            Self::VideoAndAudio => "#opt-in-radio-button",
            Self::AudioOnly => "#visual-opt-out-radio-button",
            Self::NotAllowed => "#opt-out-radio-button",
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::VideoAndAudio => "youtube.remix.videoAndAudio",
            Self::AudioOnly => "youtube.remix.audioOnly",
            Self::NotAllowed => "youtube.remix.notAllowed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishSettings {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category_id: String,
    pub privacy: Privacy,
    pub made_for_kids: bool,
    pub age_restricted: bool,
    pub publish_at: Option<String>,
    pub notify_subscribers: bool,

    #[serde(default)]
    pub category_label: String,

    #[serde(default)]
    pub thumbnail: Option<std::path::PathBuf>,

    #[serde(default)]
    pub playlists: Vec<String>,

    #[serde(default)]
    pub video_language: String,
    #[serde(default)]
    pub license: License,
    #[serde(default = "yes")]
    pub allow_embedding: bool,
    #[serde(default)]
    pub comments: Comments,
    #[serde(default = "yes")]
    pub show_like_count: bool,
    #[serde(default)]
    pub paid_promotion: bool,

    #[serde(default)]
    pub altered_content: bool,
    #[serde(default)]
    pub remix: Remix,
}

fn yes() -> bool {
    true
}

impl Default for PublishSettings {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            tags: Vec::new(),
            category_id: DEFAULT_CATEGORY.to_string(),
            privacy: Privacy::Private,
            made_for_kids: false,
            age_restricted: false,
            publish_at: None,
            notify_subscribers: true,
            category_label: String::new(),
            thumbnail: None,
            playlists: Vec::new(),
            video_language: String::new(),
            license: License::Standard,
            allow_embedding: true,
            comments: Comments::On,
            show_like_count: true,
            paid_promotion: false,
            altered_content: false,
            remix: Remix::VideoAndAudio,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Issue {
    TitleEmpty,
    TitleTooLong,
    TitleHasAngleBrackets,
    DescriptionTooLong,
    TagsTooLong,
    TooManyTags,
    UnknownCategory,
    ScheduleInThePast,
    ScheduleNotIso,
    KidsAndAgeRestricted,
    ThumbnailMissing,

    KidsAndComments,
}

impl Issue {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::TitleEmpty => "youtube.issue.titleEmpty",
            Self::TitleTooLong => "youtube.issue.titleTooLong",
            Self::TitleHasAngleBrackets => "youtube.issue.titleAngleBrackets",
            Self::DescriptionTooLong => "youtube.issue.descriptionTooLong",
            Self::TagsTooLong => "youtube.issue.tagsTooLong",
            Self::TooManyTags => "youtube.issue.tooManyTags",
            Self::UnknownCategory => "youtube.issue.unknownCategory",
            Self::ScheduleInThePast => "youtube.issue.scheduleInThePast",
            Self::ScheduleNotIso => "youtube.issue.scheduleNotIso",
            Self::KidsAndAgeRestricted => "youtube.issue.kidsAndAgeRestricted",
            Self::ThumbnailMissing => "youtube.issue.thumbnailMissing",
            Self::KidsAndComments => "youtube.issue.kidsAndComments",
        }
    }
}

pub fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn tags_length(tags: &[String]) -> usize {
    if tags.is_empty() {
        return 0;
    }
    tags.iter().map(|tag| tag.chars().count()).sum::<usize>() + tags.len() - 1
}

pub fn is_rfc3339_utc(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20 || bytes[19] != b'Z' {
        return false;
    }
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    if !digits.iter().all(|index| bytes[*index].is_ascii_digit()) {
        return false;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return false;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return false;
    }
    let number = |start: usize, end: usize| value[start..end].parse::<u32>().unwrap_or(u32::MAX);
    (1..=12).contains(&number(5, 7))
        && (1..=31).contains(&number(8, 10))
        && number(11, 13) < 24
        && number(14, 16) < 60
        && number(17, 19) < 60
}

impl PublishSettings {
    pub fn is_scheduled(&self) -> bool {
        self.publish_at
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    }

    pub fn effective_privacy(&self) -> Privacy {
        if self.is_scheduled() {
            Privacy::Private
        } else {
            self.privacy
        }
    }

    pub fn validate(&self, now_iso: &str) -> Vec<Issue> {
        let mut issues = Vec::new();
        let title = self.title.trim();

        if title.is_empty() {
            issues.push(Issue::TitleEmpty);
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            issues.push(Issue::TitleTooLong);
        }
        if title.contains('<') || title.contains('>') {
            issues.push(Issue::TitleHasAngleBrackets);
        }
        if self.description.chars().count() > MAX_DESCRIPTION_CHARS {
            issues.push(Issue::DescriptionTooLong);
        }
        if tags_length(&self.tags) > MAX_TAG_CHARS {
            issues.push(Issue::TagsTooLong);
        }
        if self.tags.len() > MAX_TAGS {
            issues.push(Issue::TooManyTags);
        }
        if category_label(&self.category_id).is_none() {
            issues.push(Issue::UnknownCategory);
        }
        if self.made_for_kids && self.age_restricted {
            issues.push(Issue::KidsAndAgeRestricted);
        }
        if self.made_for_kids && self.comments != Comments::Off {
            issues.push(Issue::KidsAndComments);
        }
        if self.thumbnail.as_ref().is_some_and(|path| !path.is_file()) {
            issues.push(Issue::ThumbnailMissing);
        }
        if let Some(publish_at) = self
            .publish_at
            .as_deref()
            .filter(|at| !at.trim().is_empty())
        {
            if !is_rfc3339_utc(publish_at) {
                issues.push(Issue::ScheduleNotIso);
            } else if publish_at <= now_iso {
                issues.push(Issue::ScheduleInThePast);
            }
        }

        issues
    }

    pub fn to_payload(&self) -> Value {
        let mut snippet = json!({
            "title": self.title.trim(),
            "description": self.description,
            "categoryId": self.category_id,
        });
        if !self.tags.is_empty() {
            snippet["tags"] = json!(self.tags);
        }

        let mut status = json!({
            "privacyStatus": self.effective_privacy().as_api(),
            "selfDeclaredMadeForKids": self.made_for_kids,
        });
        if self.is_scheduled() {
            status["publishAt"] = json!(self.publish_at.clone().unwrap_or_default());
        }

        let mut payload = json!({ "snippet": snippet, "status": status });
        if self.age_restricted {
            payload["contentDetails"] = json!({
                "contentRating": { "ytRating": "ytAgeRestricted" }
            });
        }
        payload
    }

    pub fn parts(&self) -> &'static str {
        if self.age_restricted {
            "snippet,status,contentDetails"
        } else {
            "snippet,status"
        }
    }
}

pub fn normalise_label(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn is_category(item_text: &str, id: &str, localized: &str) -> bool {
    let item = normalise_label(item_text);
    if item.is_empty() {
        return false;
    }
    let localized = normalise_label(localized);
    if !localized.is_empty() && item == localized {
        return true;
    }
    category_label(id)
        .map(|english| normalise_label(english) == item)
        .unwrap_or(false)
}

pub fn is_supported_video(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" | "flv" | "wmv" | "mpeg" | "mpg" | "3gp"
    )
}

pub fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

pub fn studio_url(video_id: &str) -> String {
    format!("https://studio.youtube.com/video/{video_id}/edit")
}

#[cfg(test)]
mod category_tests {
    use super::*;

    #[test]
    fn the_english_name_finds_the_item_on_an_english_account() {
        assert!(is_category("People & Blogs", "22", ""));
        assert!(is_category("Science & Technology", "28", ""));
        assert!(!is_category("Music", "22", ""));
    }

    #[test]
    fn the_localised_name_finds_it_on_an_account_in_another_language() {
        assert!(is_category("Люди и блоги", "22", "Люди и блоги"));
        assert!(is_category("Наука и техника", "28", "Наука и техника"));
        assert!(!is_category("Музыка", "22", "Люди и блоги"));
    }

    #[test]
    fn studios_indentation_and_casing_do_not_stop_a_match() {
        assert!(is_category("  people & blogs ", "22", ""));
        assert!(is_category(
            "People &
   Blogs",
            "22",
            ""
        ));
        assert!(is_category("  ЛЮДИ И БЛОГИ  ", "22", "Люди и блоги"));
    }

    #[test]
    fn an_empty_item_or_an_unknown_category_matches_nothing() {
        assert!(!is_category("", "22", "People & Blogs"));
        assert!(!is_category("   ", "22", ""));
        assert!(!is_category("Anything", "9999", ""), "no such category id");
    }

    #[test]
    fn every_category_finds_itself_by_its_english_name() {
        for (id, label) in CATEGORIES {
            assert!(is_category(label, id, ""), "{id} / {label}");
        }
    }

    #[test]
    fn the_setting_carries_the_label_across_a_json_round_trip() {
        let settings = PublishSettings {
            category_id: "28".to_string(),
            category_label: "Наука и техника".to_string(),
            ..Default::default()
        };
        let text = serde_json::to_string(&settings).expect("json");
        let back: PublishSettings = serde_json::from_str(&text).expect("back");
        assert_eq!(back.category_label, "Наука и техника");

        let legacy = r#"{"title":"","description":"","tags":[],"category_id":"28",
            "privacy":"private","made_for_kids":false,"age_restricted":false,
            "publish_at":null,"notify_subscribers":true}"#;
        let old: PublishSettings = serde_json::from_str(legacy).expect("legacy");
        assert_eq!(old.category_label, "");
        assert_eq!(old.category_id, "28");
    }
}

#[cfg(test)]
mod extras_tests {
    use super::*;

    #[test]
    fn the_defaults_are_what_studio_would_have_done_on_its_own() {
        let settings = PublishSettings::default();
        assert!(settings.allow_embedding);
        assert!(settings.show_like_count);
        assert_eq!(settings.comments, Comments::On);
        assert_eq!(settings.license, License::Standard);
        assert_eq!(settings.remix, Remix::VideoAndAudio);
        assert!(!settings.paid_promotion);
        assert!(!settings.altered_content);
        assert!(settings.playlists.is_empty());
        assert_eq!(settings.thumbnail, None);
    }

    #[test]
    fn a_queue_written_before_these_fields_existed_still_loads_with_those_defaults() {
        let legacy = r#"{"title":"t","description":"","tags":[],"category_id":"22",
            "privacy":"private","made_for_kids":false,"age_restricted":false,
            "publish_at":null,"notify_subscribers":true}"#;
        let old: PublishSettings = serde_json::from_str(legacy).expect("legacy");
        assert!(
            old.allow_embedding,
            "an absent field must not read as false"
        );
        assert!(old.show_like_count);
        assert_eq!(old.comments, Comments::On);
        assert_eq!(old.license, License::Standard);
        assert_eq!(old.remix, Remix::VideoAndAudio);
    }

    #[test]
    fn every_choice_survives_a_json_round_trip() {
        let settings = PublishSettings {
            license: License::CreativeCommons,
            comments: Comments::HoldAll,
            remix: Remix::NotAllowed,
            allow_embedding: false,
            show_like_count: false,
            paid_promotion: true,
            altered_content: true,
            playlists: vec!["Клипы".to_string()],
            video_language: "Русский".to_string(),
            ..Default::default()
        };
        let back: PublishSettings =
            serde_json::from_str(&serde_json::to_string(&settings).expect("json")).expect("back");
        assert_eq!(back, settings);
    }

    #[test]
    fn the_comment_dropdowns_are_addressed_by_position_not_by_name() {
        assert!(Comments::On.enabled());
        assert!(Comments::HoldAll.enabled());
        assert!(!Comments::Off.enabled());
        assert_eq!(Comments::Off.moderation_index(), None);

        assert_eq!(Comments::On.moderation_index(), Some(0));
        assert_eq!(Comments::HoldInappropriate.moderation_index(), Some(1));
        assert_eq!(
            Comments::HoldInappropriateStrict.moderation_index(),
            Some(2)
        );
        assert_eq!(Comments::HoldAll.moderation_index(), Some(3));
        let mut seen = Vec::new();
        for comments in Comments::ALL {
            if let Some(index) = comments.moderation_index() {
                assert!(!seen.contains(&index), "{comments:?} shares a place");
                seen.push(index);
            }
        }
        assert_eq!(License::Standard.option_index(), 0);
        assert_eq!(License::CreativeCommons.option_index(), 1);
    }

    #[test]
    fn each_remix_choice_names_a_radio_of_its_own_and_no_two_share_one() {
        let mut seen = Vec::new();
        for remix in Remix::ALL {
            assert!(remix.selector().starts_with('#'));
            assert!(!seen.contains(&remix.selector()), "{:?}", remix);
            seen.push(remix.selector());
        }
    }

    #[test]
    fn a_thumbnail_that_is_not_on_disc_is_caught_before_the_upload_starts() {
        let settings = PublishSettings {
            title: "t".to_string(),
            thumbnail: Some(std::path::PathBuf::from("no-such-cover.png")),
            ..Default::default()
        };
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::ThumbnailMissing)
        );
    }

    #[test]
    fn a_video_for_children_cannot_also_take_comments() {
        let settings = PublishSettings {
            title: "t".to_string(),
            made_for_kids: true,
            comments: Comments::On,
            ..Default::default()
        };
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::KidsAndComments)
        );

        let settings = PublishSettings {
            comments: Comments::Off,
            ..settings
        };
        assert!(
            !settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::KidsAndComments)
        );
    }
}

#[cfg(test)]
mod picker_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_file_picker_takes_the_containers_youtube_takes() {
        for name in ["clip.mp4", "clip.MOV", "clip.webm", "clip.mkv", "clip.3gp"] {
            assert!(is_supported_video(Path::new(name)), "{name}");
        }
        for name in ["clip.txt", "clip.mp3", "clip", "clip.mp4.zip"] {
            assert!(!is_supported_video(Path::new(name)), "{name}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> PublishSettings {
        PublishSettings {
            title: "  My clip  ".to_string(),
            description: "line one\nline two".to_string(),
            tags: vec!["gaming".to_string(), "clip".to_string()],
            category_id: "20".to_string(),
            privacy: Privacy::Public,
            made_for_kids: false,
            age_restricted: false,
            publish_at: None,
            notify_subscribers: true,
            ..Default::default()
        }
    }

    #[test]
    fn the_payload_maps_the_form_onto_the_documented_snippet_and_status_parts() {
        let payload = settings().to_payload();
        assert_eq!(payload["snippet"]["title"], "My clip");
        assert_eq!(payload["snippet"]["description"], "line one\nline two");
        assert_eq!(payload["snippet"]["categoryId"], "20");
        assert_eq!(payload["snippet"]["tags"], json!(["gaming", "clip"]));
        assert_eq!(payload["status"]["privacyStatus"], "public");
        assert_eq!(payload["status"]["selfDeclaredMadeForKids"], false);
        assert!(payload["status"].get("publishAt").is_none());
        assert!(payload.get("contentDetails").is_none());
        assert_eq!(settings().parts(), "snippet,status");
    }

    #[test]
    fn an_empty_tag_list_is_omitted_rather_than_sent_as_an_empty_array() {
        let mut settings = settings();
        settings.tags.clear();
        assert!(settings.to_payload()["snippet"].get("tags").is_none());
    }

    #[test]
    fn scheduling_forces_private_because_youtube_rejects_publish_at_otherwise() {
        let mut settings = settings();
        settings.privacy = Privacy::Public;
        settings.publish_at = Some("2030-01-02T15:04:05Z".to_string());

        assert_eq!(settings.effective_privacy(), Privacy::Private);
        let payload = settings.to_payload();
        assert_eq!(payload["status"]["privacyStatus"], "private");
        assert_eq!(payload["status"]["publishAt"], "2030-01-02T15:04:05Z");
    }

    #[test]
    fn a_blank_schedule_string_counts_as_no_schedule() {
        let mut settings = settings();
        settings.publish_at = Some("   ".to_string());
        assert!(!settings.is_scheduled());
        assert_eq!(settings.effective_privacy(), Privacy::Public);
        assert!(settings.to_payload()["status"].get("publishAt").is_none());
    }

    #[test]
    fn age_restriction_adds_the_content_rating_part() {
        let mut settings = settings();
        settings.age_restricted = true;
        let payload = settings.to_payload();
        assert_eq!(
            payload["contentDetails"]["contentRating"]["ytRating"],
            "ytAgeRestricted"
        );
        assert_eq!(settings.parts(), "snippet,status,contentDetails");
    }

    #[test]
    fn made_for_kids_is_declared_on_the_status_part() {
        let mut settings = settings();
        settings.made_for_kids = true;
        assert_eq!(
            settings.to_payload()["status"]["selfDeclaredMadeForKids"],
            true
        );
    }

    #[test]
    fn privacy_round_trips_through_the_api_spelling() {
        for privacy in Privacy::ALL {
            assert_eq!(Privacy::from_api(privacy.as_api()), privacy);
        }
        assert_eq!(Privacy::from_api("nonsense"), Privacy::Private);
        assert_eq!(Privacy::default(), Privacy::Private);
    }

    #[test]
    fn a_well_formed_form_reports_no_issues() {
        assert_eq!(settings().validate("2026-08-14T00:00:00Z"), Vec::new());
    }

    #[test]
    fn each_field_limit_is_enforced() {
        let mut form = settings();
        form.title = "   ".to_string();
        assert!(
            form.validate("2026-08-14T00:00:00Z")
                .contains(&Issue::TitleEmpty)
        );

        form.title = "a".repeat(MAX_TITLE_CHARS + 1);
        assert!(
            form.validate("2026-08-14T00:00:00Z")
                .contains(&Issue::TitleTooLong)
        );

        form.title = "a <script>".to_string();
        assert!(
            form.validate("2026-08-14T00:00:00Z")
                .contains(&Issue::TitleHasAngleBrackets)
        );

        let mut form = settings();
        form.description = "d".repeat(MAX_DESCRIPTION_CHARS + 1);
        assert!(
            form.validate("2026-08-14T00:00:00Z")
                .contains(&Issue::DescriptionTooLong)
        );

        let mut form = settings();
        form.category_id = "9999".to_string();
        assert!(
            form.validate("2026-08-14T00:00:00Z")
                .contains(&Issue::UnknownCategory)
        );
    }

    #[test]
    fn tag_length_counts_the_separators_youtube_counts() {
        assert_eq!(tags_length(&[]), 0);
        assert_eq!(tags_length(&["ab".to_string()]), 2);
        assert_eq!(tags_length(&["ab".to_string(), "cd".to_string()]), 5);

        let mut settings = settings();
        settings.tags = vec!["x".repeat(MAX_TAG_CHARS)];
        assert!(settings.validate("2026-08-14T00:00:00Z").is_empty());
        settings.tags = vec!["x".repeat(MAX_TAG_CHARS), "y".to_string()];
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::TagsTooLong)
        );

        settings.tags = (0..=MAX_TAGS).map(|index| index.to_string()).collect();
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::TooManyTags)
        );
    }

    #[test]
    fn tags_are_parsed_from_a_comma_separated_field_without_blanks() {
        assert_eq!(
            parse_tags(" gaming , , clip,  "),
            vec!["gaming".to_string(), "clip".to_string()]
        );
        assert_eq!(parse_tags(""), Vec::<String>::new());
        assert_eq!(parse_tags(",,,"), Vec::<String>::new());
    }

    #[test]
    fn only_a_real_utc_timestamp_is_accepted_for_the_schedule() {
        assert!(is_rfc3339_utc("2030-01-02T15:04:05Z"));
        assert!(!is_rfc3339_utc("2030-01-02T15:04:05"));
        assert!(!is_rfc3339_utc("2030-01-02 15:04:05Z"));
        assert!(!is_rfc3339_utc("2030-13-02T15:04:05Z"));
        assert!(!is_rfc3339_utc("2030-01-02T25:04:05Z"));
        assert!(!is_rfc3339_utc("2030-01-02T15:60:05Z"));
        assert!(!is_rfc3339_utc(""));
        assert!(!is_rfc3339_utc("2030-01-02T15:04:05+03:00"));
    }

    #[test]
    fn a_schedule_in_the_past_or_in_the_wrong_shape_is_caught_before_upload() {
        let mut settings = settings();
        settings.publish_at = Some("tomorrow please".to_string());
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::ScheduleNotIso)
        );

        settings.publish_at = Some("2020-01-01T00:00:00Z".to_string());
        let issues = settings.validate("2026-08-14T00:00:00Z");
        assert!(issues.contains(&Issue::ScheduleInThePast));
        assert!(!issues.contains(&Issue::ScheduleNotIso));

        settings.publish_at = Some("2030-01-01T00:00:00Z".to_string());
        assert!(settings.validate("2026-08-14T00:00:00Z").is_empty());
    }

    #[test]
    fn a_video_cannot_be_both_made_for_kids_and_age_restricted() {
        let mut settings = settings();
        settings.made_for_kids = true;
        settings.age_restricted = true;
        assert!(
            settings
                .validate("2026-08-14T00:00:00Z")
                .contains(&Issue::KidsAndAgeRestricted)
        );
    }

    #[test]
    fn every_category_has_a_label_and_a_locale_key_and_the_default_is_real() {
        assert!(category_label(DEFAULT_CATEGORY).is_some());
        assert_eq!(category_key("20"), "youtube.category.20");
        assert_eq!(category_label("nope"), None);
        for (id, label) in CATEGORIES {
            assert!(!label.is_empty());
            assert!(id.parse::<u32>().is_ok());
        }
    }

    #[test]
    fn the_share_link_is_built_from_the_returned_video_id() {
        assert_eq!(
            watch_url("dQw4w9WgXcQ"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            studio_url("dQw4w9WgXcQ"),
            "https://studio.youtube.com/video/dQw4w9WgXcQ/edit"
        );
    }
}

pub mod studio {

    pub const CREATE_BUTTON: &str = "#create-icon, ytcp-button#create-icon";
    pub const UPLOAD_MENU_ITEM: &str = "#text-item-0, tp-yt-paper-item#text-item-0";

    pub const SELECT_FILES: &str =
        "#select-files-button, ytcp-uploads-file-picker #select-files-button";

    pub const TEXT_BOXES: &str = "[id=\"textbox\"]";
    pub const THUMBNAIL_INPUT: &str = "#file-loader";
    pub const SHOW_MORE: &str = "#toggle-button";

    pub const ADVANCED: &str = "ytcp-video-metadata-editor-advanced";

    pub const TAGS: &str =
        "#tags-container input, #tags-container textarea, ytcp-form-input-container#tags-container [contenteditable], [aria-label=\"Tags\"]";

    pub const ALTERED_CONTENT_NO: &str =
        "tp-yt-paper-radio-button[name=\"VIDEO_HAS_ALTERED_CONTENT_NO\"]";
    pub const MADE_FOR_KIDS: &str = "tp-yt-paper-radio-button[name=\"VIDEO_MADE_FOR_KIDS_MFK\"]";
    pub const NOT_FOR_KIDS: &str = "tp-yt-paper-radio-button[name=\"VIDEO_MADE_FOR_KIDS_NOT_MFK\"]";
    pub const AGE_RESTRICTED: &str =
        "tp-yt-paper-radio-button[name=\"VIDEO_AGE_RESTRICTION_SELF\"]";

    pub const CATEGORY_SELECT: &str =
        "ytcp-form-select#category ytcp-text-dropdown-trigger, #category-container ytcp-text-dropdown-trigger";

    pub const CATEGORY_ITEMS: &str = "tp-yt-paper-item[role=\"option\"], ytcp-menuitem";

    pub const NOTIFY_SUBSCRIBERS: &str =
        "#notify-subscribers, ytcp-checkbox-lit#notify-subscribers";

    pub const PLAYLIST_SELECT: &str = "ytcp-video-metadata-playlists ytcp-text-dropdown-trigger";
    pub const PLAYLIST_ITEMS: &str = "ytcp-checkbox-lit, tp-yt-paper-checkbox";
    pub const PLAYLIST_DONE: &str =
        "ytcp-playlist-dialog #done-button, tp-yt-paper-dialog #done-button";
    pub const VIDEO_LANGUAGE: &str =
        "ytcp-form-language-input#language-input ytcp-text-dropdown-trigger";
    pub const LICENSE_SELECT: &str = "ytcp-form-select#license ytcp-text-dropdown-trigger";
    pub const ALLOW_EMBED: &str = "ytcp-form-checkbox#allow-embed";
    pub const COMMENTS_SELECT: &str =
        "ytcp-select#enablement-state-select ytcp-text-dropdown-trigger";
    pub const MODERATION_SELECT: &str =
        "ytcp-select#moderation-type-select ytcp-text-dropdown-trigger";
    pub const PAID_PROMOTION: &str = "ytcp-checkbox-lit#has-ppp";

    pub const DROPDOWN_ITEMS: &str = "tp-yt-paper-item[role=\"option\"], ytcp-menuitem";

    pub const ALTERED_CONTENT_YES: &str =
        "tp-yt-paper-radio-button[name=\"VIDEO_HAS_ALTERED_CONTENT_YES\"]";

    pub const REMIX_SECTION: &str = "#content-remixing-container";

    pub const NEXT: &str = "#next-button";
    pub const DONE: &str = "#done-button";

    pub const PROGRESS_LABEL: &str = "span.progress-label";

    pub const SHARE_URL: &str = "a#video-link, #share-url, .ytcpVideoInfoValue";
    pub const ERROR_SHORT: &str = ".error-area.style-scope.ytcp-uploads-dialog, .error-short";
    pub const DIALOG: &str = "ytcp-uploads-dialog";
    pub const CLOSE_DIALOG: &str = "#close-button";

    pub const CONFIRM_DIALOG: &str =
        "ytcp-confirmation-dialog, ytcp-dialog[role=\"dialog\"], tp-yt-paper-dialog[role=\"dialog\"]";

    pub const CONFIRM_BUTTON: &str = "#secondary-action-button, ytcp-button#confirm-button";

    pub const CONFIRM_LABEL: &str = "publish anyway";

    pub const PUBLISHED_DIALOG: &str =
        "ytcp-uploads-still-processing-dialog, ytcp-video-published-dialog, ytcp-uploads-review-dialog";

    pub fn visibility(state: &str) -> String {
        format!(
            "tp-yt-paper-radio-button[name=\"{}\"]",
            state.to_uppercase()
        )
    }
    pub const SCHEDULE_TOGGLE: &str = "#second-container-expand-button";
    pub const SCHEDULE_DATE: &str = "#datepicker-trigger input";
    pub const SCHEDULE_TIME: &str = "#time-of-day-container input";
}

pub mod channel {
    pub const AVATAR_IMAGE: &str = "#avatar-btn img, ytcp-header img#img";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_visibility_radio_is_named_after_the_state() {
        assert_eq!(
            studio::visibility("public"),
            "tp-yt-paper-radio-button[name=\"PUBLIC\"]"
        );
        assert_eq!(
            studio::visibility("PRIVATE"),
            "tp-yt-paper-radio-button[name=\"PRIVATE\"]"
        );
    }

    #[test]
    fn no_selector_carries_a_quote_that_would_break_the_script_it_lands_in() {
        for selector in [
            studio::DIALOG,
            studio::NEXT,
            studio::DONE,
            studio::TEXT_BOXES,
            studio::SHARE_URL,
            studio::SELECT_FILES,
        ] {
            assert!(!selector.contains('\n'), "{selector}");
            assert!(!selector.trim().is_empty());
        }
    }
}

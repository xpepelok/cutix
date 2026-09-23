use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    NoBrowser,

    BrowserLaunch(String),

    Protocol(String),

    SignInAbandoned,

    NoChannel,

    SignedOut,

    /// A re-authorisation landed on a different channel than the account being fixed.
    ///
    /// Carries the channel that was actually signed in, so the message can name it.
    WrongChannel(String),

    Timeout(String),

    PageChanged(String),

    Rejected(String),

    Cancelled,
    Io(String),

    /// The app went away while the upload was running.
    ///
    /// Studio may already hold the video, as a draft or published, so a retry is offered
    /// but never made on the person's behalf.
    Interrupted,
}

impl Failure {
    pub fn is_auth(&self) -> bool {
        matches!(
            self,
            Self::SignedOut | Self::NoChannel | Self::WrongChannel(_)
        )
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Protocol(_)
                | Self::Timeout(_)
                | Self::BrowserLaunch(_)
                | Self::Io(_)
                | Self::Interrupted
        )
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::NoBrowser => "youtube.error.noBrowser",
            Self::BrowserLaunch(_) => "youtube.error.browserLaunch",
            Self::Protocol(_) => "youtube.error.protocol",
            Self::SignInAbandoned => "youtube.error.signInAbandoned",
            Self::NoChannel => "youtube.error.noChannel",
            Self::SignedOut => "youtube.error.signedOut",
            Self::WrongChannel(_) => "youtube.error.wrongChannel",
            Self::Timeout(_) => "youtube.error.timeout",
            Self::PageChanged(_) => "youtube.error.pageChanged",
            Self::Rejected(_) => "youtube.error.rejected",
            Self::Cancelled => "youtube.error.cancelled",
            Self::Io(_) => "youtube.error.io",
            Self::Interrupted => "youtube.error.interrupted",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::BrowserLaunch(message)
            | Self::Protocol(message)
            | Self::Timeout(message)
            | Self::PageChanged(message)
            | Self::Rejected(message)
            | Self::WrongChannel(message)
            | Self::Io(message) => message.clone(),
            Self::NoBrowser
            | Self::SignInAbandoned
            | Self::NoChannel
            | Self::SignedOut
            | Self::Cancelled
            | Self::Interrupted => String::new(),
        }
    }

    pub fn note(&self) -> FailureNote {
        FailureNote {
            key: self.message_key().to_string(),
            detail: self.detail(),
            auth: self.is_auth(),
            retryable: self.is_retryable(),
            cancelled: self.is_cancelled(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = self.detail();
        if detail.is_empty() {
            write!(formatter, "{}", self.message_key())
        } else {
            write!(formatter, "{}: {detail}", self.message_key())
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureNote {
    pub key: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub auth: bool,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub cancelled: bool,
}

impl FailureNote {
    pub fn worth_retrying(&self) -> bool {
        self.retryable || self.auth || self.cancelled
    }

    /// The note an upload gets when the app went away while it was running.
    pub fn interrupted() -> Self {
        Failure::Interrupted.note()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ended_session_is_an_auth_failure_and_signing_in_again_is_the_offered_fix() {
        let failure = Failure::SignedOut;
        assert!(failure.is_auth());
        assert!(
            !failure.is_retryable(),
            "the same stale cookies would fail again"
        );
        assert!(failure.note().worth_retrying());
        assert_eq!(failure.message_key(), "youtube.error.signedOut");
    }

    #[test]
    fn signing_back_in_as_the_wrong_channel_names_it_and_leaves_the_row_flagged() {
        let failure = Failure::WrongChannel("Second Channel".to_string());
        assert_eq!(failure.message_key(), "youtube.error.wrongChannel");
        assert_eq!(failure.detail(), "Second Channel");
        assert!(
            failure.is_auth(),
            "the account still has no live session of its own"
        );
        assert!(
            !failure.is_retryable(),
            "repeating it lands on the same wrong channel"
        );
    }

    #[test]
    fn a_dropped_devtools_socket_is_the_kind_of_thing_a_retry_fixes() {
        assert!(Failure::Protocol("connection reset".to_string()).is_retryable());
        assert!(Failure::Timeout("upload dialog".to_string()).is_retryable());
        assert!(!Failure::Rejected("duplicate video".to_string()).is_retryable());
        assert!(!Failure::NoBrowser.is_retryable());
    }

    #[test]
    fn an_unfinished_sign_in_is_reported_without_blaming_the_person_or_the_app() {
        let failure = Failure::SignInAbandoned;
        assert_eq!(
            failure.detail(),
            "",
            "there is nothing of Google's to quote"
        );
        assert!(
            !failure.is_auth(),
            "no account was added, so no row needs re-authorising"
        );
        assert!(
            !failure.is_retryable(),
            "the fix is to open the window again, not to repeat this"
        );
    }

    #[test]
    fn a_failure_flattens_into_a_note_that_survives_the_trip_to_disk() {
        let note = Failure::Rejected("This video is a duplicate".to_string()).note();
        assert_eq!(note.key, "youtube.error.rejected");
        assert_eq!(note.detail, "This video is a duplicate");
        assert!(!note.worth_retrying());

        let text = serde_json::to_string(&note).expect("json");
        assert_eq!(
            serde_json::from_str::<FailureNote>(&text).expect("back"),
            note
        );
    }

    #[test]
    fn a_history_row_written_by_an_older_build_still_loads() {
        let legacy = r#"{"key":"youtube.error.quota","detail":"no more today","quota":true,
            "auth":false,"retryable":false,"cancelled":false}"#;
        let note: FailureNote = serde_json::from_str(legacy).expect("legacy note");
        assert_eq!(note.detail, "no more today");
        assert!(!note.worth_retrying());
    }

    #[test]
    fn an_upload_the_app_closed_on_has_its_own_message_and_waits_for_a_retry() {
        let failure = Failure::Interrupted;
        assert_eq!(failure.message_key(), "youtube.error.interrupted");
        assert_eq!(failure.detail(), "", "the message says it all");
        assert!(failure.is_retryable(), "one press puts it back");
        assert!(!failure.is_auth() && !failure.is_cancelled());
        assert_eq!(FailureNote::interrupted(), failure.note());
        assert!(FailureNote::interrupted().worth_retrying());
    }

    #[test]
    fn cancelling_is_reported_as_such_rather_than_as_a_fault() {
        let note = Failure::Cancelled.note();
        assert!(note.cancelled);
        assert!(note.worth_retrying());
        assert_eq!(Failure::Cancelled.to_string(), "youtube.error.cancelled");
    }

    #[test]
    fn every_failure_has_a_key_of_its_own() {
        let all = [
            Failure::NoBrowser,
            Failure::BrowserLaunch(String::new()),
            Failure::Protocol(String::new()),
            Failure::SignInAbandoned,
            Failure::NoChannel,
            Failure::SignedOut,
            Failure::Timeout(String::new()),
            Failure::PageChanged(String::new()),
            Failure::Rejected(String::new()),
            Failure::Cancelled,
            Failure::Io(String::new()),
            Failure::WrongChannel(String::new()),
            Failure::Interrupted,
        ];
        let mut keys: Vec<&str> = all.iter().map(Failure::message_key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), all.len());
        assert!(keys.iter().all(|key| key.starts_with("youtube.error.")));
    }
}

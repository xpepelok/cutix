use crate::publish::{watch_url, Privacy};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const HISTORY_FILE: &str = "youtube-history.json";
const MAX_ENTRIES: usize = 500;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UploadStatus {
    #[default]
    Uploaded,
    Processing,
    Processed,
    Rejected {
        reason: String,
    },
    Failed {
        reason: String,
    },
    Deleted,
}

impl UploadStatus {
    pub fn from_api(upload_status: &str, processing_status: &str, rejection: &str) -> Self {
        match upload_status {
            "processed" => Self::Processed,
            "rejected" => Self::Rejected {
                reason: rejection.to_string(),
            },
            "failed" => Self::Failed {
                reason: rejection.to_string(),
            },
            "uploaded" => match processing_status {
                "succeeded" => Self::Processed,
                "failed" | "terminated" => Self::Failed {
                    reason: rejection.to_string(),
                },
                _ => Self::Processing,
            },
            _ => Self::Uploaded,
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Uploaded => "youtube.status.uploaded",
            Self::Processing => "youtube.status.processing",
            Self::Processed => "youtube.status.processed",
            Self::Rejected { .. } => "youtube.status.rejected",
            Self::Failed { .. } => "youtube.status.failed",
            Self::Deleted => "youtube.status.deleted",
        }
    }

    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            Self::Processed | Self::Rejected { .. } | Self::Failed { .. } | Self::Deleted
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub video_id: String,
    pub account_id: String,
    pub title: String,
    pub uploaded_at: i64,
    pub privacy: Privacy,
    #[serde(default)]
    pub status: UploadStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_for: Option<String>,
    #[serde(default)]
    pub bytes: u64,
}

impl HistoryEntry {
    pub fn url(&self) -> String {
        watch_url(&self.video_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryFilter {
    pub search: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub accounts: Vec<String>,
}

impl HistoryFilter {
    pub fn is_empty(&self) -> bool {
        self.search.trim().is_empty()
            && self.from.is_none()
            && self.to.is_none()
            && self.accounts.is_empty()
    }

    pub fn toggle_account(&mut self, account_id: &str) {
        match self.accounts.iter().position(|id| id == account_id) {
            Some(index) => {
                self.accounts.remove(index);
            }
            None => self.accounts.push(account_id.to_string()),
        }
    }

    pub fn has_account(&self, account_id: &str) -> bool {
        self.accounts.iter().any(|id| id == account_id)
    }

    pub fn matches(&self, entry: &HistoryEntry) -> bool {
        if !self.accounts.is_empty() && !self.has_account(&entry.account_id) {
            return false;
        }
        if self.from.is_some_and(|from| entry.uploaded_at < from) {
            return false;
        }
        if self.to.is_some_and(|to| entry.uploaded_at > to) {
            return false;
        }

        let needle = self.search.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        entry.title.to_lowercase().contains(&needle)
            || entry.video_id.to_lowercase().contains(&needle)
            || entry
                .source_file
                .as_deref()
                .is_some_and(|file| file.to_lowercase().contains(&needle))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub entries: Vec<HistoryEntry>,
}

impl History {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join(HISTORY_FILE);
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut history: Self = serde_json::from_str(&text).unwrap_or_default();
        history.sort();
        history
    }

    pub fn save(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(directory.join(HISTORY_FILE), text)
    }

    fn sort(&mut self) {
        self.entries
            .sort_by(|left, right| right.uploaded_at.cmp(&left.uploaded_at));
    }

    pub fn upsert(&mut self, entry: HistoryEntry) {
        match self
            .entries
            .iter_mut()
            .find(|existing| existing.video_id == entry.video_id)
        {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
        self.sort();
        self.entries.truncate(MAX_ENTRIES);
    }

    pub fn set_status(&mut self, video_id: &str, status: UploadStatus) -> bool {
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.video_id == video_id)
        {
            Some(entry) => {
                entry.status = status;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, video_id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.video_id != video_id);
        before != self.entries.len()
    }

    pub fn filtered(&self, filter: &HistoryFilter) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|entry| filter.matches(entry))
            .collect()
    }

    pub fn account_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for entry in &self.entries {
            if !ids.contains(&entry.account_id) {
                ids.push(entry.account_id.clone());
            }
        }
        ids
    }

    pub fn unsettled_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| !entry.status.is_settled())
            .map(|entry| entry.video_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(video_id: &str, account: &str, title: &str, at: i64) -> HistoryEntry {
        HistoryEntry {
            video_id: video_id.to_string(),
            account_id: account.to_string(),
            title: title.to_string(),
            uploaded_at: at,
            privacy: Privacy::Public,
            status: UploadStatus::Uploaded,
            source_file: Some(format!("{title}.mp4")),
            scheduled_for: None,
            bytes: 1024,
        }
    }

    fn history() -> History {
        let mut history = History::default();
        history.upsert(entry("aaa", "chan-1", "Sunset drive", 1_000));
        history.upsert(entry("bbb", "chan-2", "Boss fight", 2_000));
        history.upsert(entry("ccc", "chan-1", "SUNSET beach", 3_000));
        history
    }

    fn ids(entries: &[&HistoryEntry]) -> Vec<String> {
        entries.iter().map(|entry| entry.video_id.clone()).collect()
    }

    #[test]
    fn entries_are_kept_newest_first() {
        assert_eq!(
            ids(&history().filtered(&HistoryFilter::default())),
            ["ccc", "bbb", "aaa"]
        );
    }

    #[test]
    fn an_empty_filter_returns_everything() {
        let filter = HistoryFilter::default();
        assert!(filter.is_empty());
        assert_eq!(history().filtered(&filter).len(), 3);
    }

    #[test]
    fn the_search_is_case_insensitive_and_covers_title_id_and_source_file() {
        let history = history();
        let search = |needle: &str| {
            ids(&history.filtered(&HistoryFilter {
                search: needle.to_string(),
                ..Default::default()
            }))
        };
        assert_eq!(search("sunset"), ["ccc", "aaa"]);
        assert_eq!(search("  SUNSET  "), ["ccc", "aaa"]);
        assert_eq!(search("bbb"), ["bbb"]);
        assert_eq!(search("Boss fight.mp4"), ["bbb"]);
        assert!(search("nothing here").is_empty());
    }

    #[test]
    fn the_date_range_is_inclusive_at_both_ends() {
        let history = history();
        let between = |from, to| {
            ids(&history.filtered(&HistoryFilter {
                from,
                to,
                ..Default::default()
            }))
        };
        assert_eq!(between(Some(2_000), None), ["ccc", "bbb"]);
        assert_eq!(between(None, Some(2_000)), ["bbb", "aaa"]);
        assert_eq!(between(Some(2_000), Some(2_000)), ["bbb"]);
        assert_eq!(between(Some(1_000), Some(3_000)).len(), 3);
        assert!(between(Some(4_000), None).is_empty());
    }

    #[test]
    fn no_account_checkbox_means_every_account_and_ticking_one_narrows_it() {
        let history = history();
        let mut filter = HistoryFilter::default();
        assert_eq!(history.filtered(&filter).len(), 3);

        filter.toggle_account("chan-1");
        assert!(filter.has_account("chan-1"));
        assert_eq!(ids(&history.filtered(&filter)), ["ccc", "aaa"]);

        filter.toggle_account("chan-2");
        assert_eq!(history.filtered(&filter).len(), 3);

        filter.toggle_account("chan-1");
        assert_eq!(ids(&history.filtered(&filter)), ["bbb"]);

        filter.toggle_account("chan-2");
        assert!(filter.accounts.is_empty());
        assert_eq!(history.filtered(&filter).len(), 3);
    }

    #[test]
    fn the_filters_compose_rather_than_override_one_another() {
        let history = history();
        let matched = history.filtered(&HistoryFilter {
            search: "sunset".to_string(),
            from: Some(2_000),
            to: None,
            accounts: vec!["chan-1".to_string()],
        });
        assert_eq!(ids(&matched), ["ccc"]);

        let none = history.filtered(&HistoryFilter {
            search: "sunset".to_string(),
            from: None,
            to: None,
            accounts: vec!["chan-2".to_string()],
        });
        assert!(none.is_empty());
    }

    #[test]
    fn upserting_the_same_video_replaces_it_instead_of_duplicating() {
        let mut history = history();
        history.upsert(entry("bbb", "chan-2", "Boss fight v2", 4_000));
        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.entries[0].title, "Boss fight v2");
        assert_eq!(history.entries[0].video_id, "bbb");
    }

    #[test]
    fn status_updates_and_removals_target_one_video() {
        let mut history = history();
        assert!(history.set_status("bbb", UploadStatus::Processed));
        assert!(!history.set_status("zzz", UploadStatus::Processed));
        assert_eq!(
            history
                .entries
                .iter()
                .find(|e| e.video_id == "bbb")
                .map(|e| e.status.clone()),
            Some(UploadStatus::Processed)
        );
        assert!(history.remove("bbb"));
        assert!(!history.remove("bbb"));
        assert_eq!(history.entries.len(), 2);
    }

    #[test]
    fn only_unsettled_uploads_are_polled_for_a_status_refresh() {
        let mut history = history();
        history.set_status("aaa", UploadStatus::Processed);
        history.set_status("bbb", UploadStatus::Processing);
        let mut pending = history.unsettled_ids();
        pending.sort();
        assert_eq!(pending, ["bbb", "ccc"]);

        assert!(UploadStatus::Processed.is_settled());
        assert!(UploadStatus::Rejected {
            reason: "copyright".to_string()
        }
        .is_settled());
        assert!(!UploadStatus::Uploaded.is_settled());
        assert!(!UploadStatus::Processing.is_settled());
    }

    #[test]
    fn api_status_fields_map_onto_the_local_status() {
        assert_eq!(
            UploadStatus::from_api("processed", "", ""),
            UploadStatus::Processed
        );
        assert_eq!(
            UploadStatus::from_api("uploaded", "processing", ""),
            UploadStatus::Processing
        );
        assert_eq!(
            UploadStatus::from_api("uploaded", "succeeded", ""),
            UploadStatus::Processed
        );
        assert_eq!(
            UploadStatus::from_api("rejected", "", "copyright"),
            UploadStatus::Rejected {
                reason: "copyright".to_string()
            }
        );
        assert_eq!(
            UploadStatus::from_api("uploaded", "failed", "transcode"),
            UploadStatus::Failed {
                reason: "transcode".to_string()
            }
        );
        assert_eq!(
            UploadStatus::from_api("unknown", "", ""),
            UploadStatus::Uploaded
        );
    }

    #[test]
    fn the_accounts_seen_in_history_are_listed_once_each() {
        assert_eq!(history().account_ids(), ["chan-1", "chan-2"]);
    }

    #[test]
    fn history_round_trips_through_disk_and_a_missing_file_is_empty() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-youtube-history-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);

        assert_eq!(History::load(&directory), History::default());

        let history = history();
        history.save(&directory).expect("save");
        let loaded = History::load(&directory);
        assert_eq!(loaded, history);
        assert_eq!(
            ids(&loaded.filtered(&HistoryFilter::default())),
            ["ccc", "bbb", "aaa"]
        );

        std::fs::write(directory.join(HISTORY_FILE), "{ not json").expect("corrupt");
        assert_eq!(History::load(&directory), History::default());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_entry_link_is_a_watch_url() {
        assert_eq!(
            entry("dQw4w9WgXcQ", "c", "t", 0).url(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn the_stored_history_never_grows_without_bound() {
        let mut history = History::default();
        for index in 0..(MAX_ENTRIES + 40) {
            history.upsert(entry(&format!("v{index}"), "chan-1", "clip", index as i64));
        }
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert_eq!(
            history.entries[0].video_id,
            format!("v{}", MAX_ENTRIES + 39)
        );
    }
}

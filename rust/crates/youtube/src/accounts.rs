use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ACCOUNTS_FILE: &str = "youtube-accounts.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_file: Option<PathBuf>,
    #[serde(default)]
    pub added_at: i64,
    #[serde(default)]
    pub refreshed_at: i64,
    #[serde(default)]
    pub needs_reauth: bool,
}

impl Account {
    pub fn initials(&self) -> String {
        let source = if self.title.trim().is_empty() {
            self.handle.trim()
        } else {
            self.title.trim()
        };
        let letters: String = source
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .collect();
        if letters.is_empty() {
            "?".to_string()
        } else {
            letters.to_uppercase()
        }
    }

    pub fn display_name(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.trim().to_string();
        }
        if !self.handle.trim().is_empty() {
            return self.handle.trim().to_string();
        }
        self.id.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accounts {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

impl Accounts {
    pub fn load(directory: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(directory.join(ACCOUNTS_FILE)) else {
            return Self::default();
        };
        let mut accounts: Self = serde_json::from_str(&text).unwrap_or_default();
        accounts.repair();
        accounts
    }

    pub fn save(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(directory.join(ACCOUNTS_FILE), text)
    }

    fn repair(&mut self) {
        if self
            .active
            .as_deref()
            .is_none_or(|id| !self.accounts.iter().any(|account| account.id == id))
        {
            self.active = self.accounts.first().map(|account| account.id.clone());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|account| account.id == id)
    }

    pub fn active(&self) -> Option<&Account> {
        self.active.as_deref().and_then(|id| self.get(id))
    }

    pub fn upsert(&mut self, account: Account) {
        match self
            .accounts
            .iter_mut()
            .find(|existing| existing.id == account.id)
        {
            Some(existing) => {
                let added_at = if existing.added_at == 0 {
                    account.added_at
                } else {
                    existing.added_at
                };
                let avatar_file = account.avatar_file.clone().or(existing.avatar_file.clone());
                *existing = Account {
                    added_at,
                    avatar_file,
                    ..account
                };
            }
            None => self.accounts.push(account),
        }
        self.repair();
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|account| account.id != id);
        let removed = before != self.accounts.len();
        self.repair();
        removed
    }

    pub fn select(&mut self, id: &str) -> bool {
        if self.get(id).is_some() {
            self.active = Some(id.to_string());
            true
        } else {
            false
        }
    }

    pub fn mark_needs_reauth(&mut self, id: &str) {
        if let Some(account) = self.accounts.iter_mut().find(|account| account.id == id) {
            account.needs_reauth = true;
        }
    }

    pub fn stale(&self, now: i64, max_age: i64) -> Vec<String> {
        self.accounts
            .iter()
            .filter(|account| now - account.refreshed_at >= max_age)
            .map(|account| account.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, title: &str) -> Account {
        Account {
            id: id.to_string(),
            title: title.to_string(),
            handle: format!("@{title}"),
            avatar_url: format!("https://yt3.example/{id}.jpg"),
            avatar_file: None,
            added_at: 100,
            refreshed_at: 100,
            needs_reauth: false,
        }
    }

    fn accounts() -> Accounts {
        let mut accounts = Accounts::default();
        accounts.upsert(account("chan-1", "First Channel"));
        accounts.upsert(account("chan-2", "Second Channel"));
        accounts
    }

    #[test]
    fn the_first_account_added_becomes_the_active_one() {
        let accounts = accounts();
        assert_eq!(
            accounts.active().map(|a| a.id.clone()),
            Some("chan-1".to_string())
        );
        assert!(!accounts.is_empty());
        assert!(Accounts::default().active().is_none());
    }

    #[test]
    fn selecting_switches_the_account_used_by_later_actions() {
        let mut accounts = accounts();
        assert!(accounts.select("chan-2"));
        assert_eq!(
            accounts.active().map(|a| a.title.clone()),
            Some("Second Channel".to_string())
        );
        assert!(!accounts.select("chan-9"));
        assert_eq!(accounts.active.as_deref(), Some("chan-2"));
    }

    #[test]
    fn removing_the_active_account_falls_back_to_another_one() {
        let mut accounts = accounts();
        accounts.select("chan-2");
        assert!(accounts.remove("chan-2"));
        assert_eq!(accounts.active.as_deref(), Some("chan-1"));
        assert!(accounts.remove("chan-1"));
        assert_eq!(accounts.active, None);
        assert!(accounts.is_empty());
        assert!(!accounts.remove("chan-1"));
    }

    #[test]
    fn a_background_refresh_updates_the_name_and_avatar_without_losing_the_added_date() {
        let mut accounts = accounts();
        accounts.upsert(Account {
            id: "chan-1".to_string(),
            title: "Renamed Channel".to_string(),
            handle: "@renamed".to_string(),
            avatar_url: "https://yt3.example/new.jpg".to_string(),
            avatar_file: None,
            added_at: 0,
            refreshed_at: 900,
            needs_reauth: false,
        });

        let account = accounts.get("chan-1").expect("account");
        assert_eq!(accounts.accounts.len(), 2);
        assert_eq!(account.title, "Renamed Channel");
        assert_eq!(account.avatar_url, "https://yt3.example/new.jpg");
        assert_eq!(account.added_at, 100);
        assert_eq!(account.refreshed_at, 900);
    }

    #[test]
    fn a_cached_avatar_file_survives_a_refresh_that_does_not_mention_one() {
        let mut accounts = accounts();
        let mut cached = account("chan-1", "First Channel");
        cached.avatar_file = Some(PathBuf::from("avatars/chan-1.png"));
        accounts.upsert(cached);

        accounts.upsert(account("chan-1", "First Channel"));
        assert_eq!(
            accounts.get("chan-1").and_then(|a| a.avatar_file.clone()),
            Some(PathBuf::from("avatars/chan-1.png"))
        );
    }

    #[test]
    fn only_accounts_older_than_the_max_age_are_refreshed_at_startup() {
        let accounts = accounts();
        assert_eq!(accounts.stale(100, 3_600), Vec::<String>::new());
        assert_eq!(accounts.stale(3_700, 3_600), ["chan-1", "chan-2"]);
    }

    #[test]
    fn a_revoked_account_is_flagged_rather_than_silently_dropped() {
        let mut accounts = accounts();
        accounts.mark_needs_reauth("chan-2");
        assert!(accounts.get("chan-2").expect("account").needs_reauth);
        assert!(!accounts.get("chan-1").expect("account").needs_reauth);
        assert_eq!(accounts.accounts.len(), 2);
    }

    #[test]
    fn initials_and_display_names_cope_with_missing_channel_details() {
        assert_eq!(account("c", "First Channel").initials(), "FC");
        assert_eq!(account("c", "Solo").initials(), "S");
        let mut blank = account("chan-x", "");
        blank.handle = String::new();
        assert_eq!(blank.initials(), "?");
        assert_eq!(blank.display_name(), "chan-x");
        assert_eq!(account("c", "Named").display_name(), "Named");
    }

    #[test]
    fn accounts_round_trip_through_disk_and_never_carry_a_token() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-youtube-accounts-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);

        assert_eq!(Accounts::load(&directory), Accounts::default());

        let mut accounts = accounts();
        accounts.select("chan-2");
        accounts.save(&directory).expect("save");

        let text = std::fs::read_to_string(directory.join(ACCOUNTS_FILE)).expect("read");
        assert!(!text.contains("refresh_token"));
        assert!(!text.contains("access_token"));
        assert!(!text.to_lowercase().contains("secret"));
        assert_eq!(Accounts::load(&directory), accounts);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_stored_active_id_that_no_longer_exists_is_repaired_on_load() {
        let mut accounts = accounts();
        accounts.active = Some("ghost".to_string());
        accounts.repair();
        assert_eq!(accounts.active.as_deref(), Some("chan-1"));
    }
}

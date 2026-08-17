use crate::cdp::{Connection, Page};
use crate::chrome::{profile_directory, Browser};
use crate::failure::Failure;
use crate::login;
use crate::publish::PublishSettings;
use crate::studio::{self, Control, Stage};
use std::path::{Path, PathBuf};

pub struct Session {
    browser: Browser,
    connection: Connection,
    page: Page,
}

impl Session {
    pub fn open(data_directory: &Path, account_id: &str, headless: bool) -> Result<Self, Failure> {
        let profile = profile_directory(data_directory, account_id);
        let browser = Browser::launch(&profile, headless)?;
        let mut connection = Connection::connect(&browser.websocket_url)?;
        let page = connection.open("about:blank")?;
        present_as_a_visible_browser(&mut connection, &page);
        Ok(Self {
            browser,
            connection,
            page,
        })
    }

    pub fn profile(&self) -> &PathBuf {
        &self.browser.profile
    }

    pub fn is_signed_in(&mut self) -> Result<bool, Failure> {
        login::is_session_live(&mut self.connection, &self.page)
    }

    fn channel_id(&mut self) -> Result<String, Failure> {
        login::wait_for_channel(&mut self.connection, &self.page)
    }

    fn decorations(&mut self) -> Decorations {
        Decorations {
            title: login::channel_name(&mut self.connection, &self.page),
            handle: login::channel_handle(&mut self.connection, &self.page),
            avatar_url: login::channel_avatar(&mut self.connection, &self.page),
            avatar: login::channel_avatar_bytes(&mut self.connection, &self.page),
        }
    }

    pub fn upload(
        &mut self,
        settings: &PublishSettings,
        source: &Path,
        report: &mut dyn FnMut(Stage, u32) -> Control,
    ) -> Result<String, Failure> {
        studio::upload(&mut self.connection, &self.page, settings, source, report)
    }

    pub fn sign_out(&mut self) -> Result<(), Failure> {
        login::sign_out(&mut self.connection, &self.page)
    }

    pub fn close(&mut self) {
        self.connection.close_browser();
        self.browser.wait_for_exit(SHUTDOWN_TIMEOUT);
    }
}

pub const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn present_as_a_visible_browser(connection: &mut Connection, page: &Page) {
    let Ok(agent) = connection.user_agent() else {
        return;
    };
    if !crate::cdp::is_headless_agent(&agent) {
        return;
    }
    let _ = page.set_user_agent(connection, &crate::cdp::visible_user_agent(&agent));
}

#[derive(Default)]
struct Decorations {
    title: String,
    handle: String,
    avatar_url: String,
    avatar: Option<Vec<u8>>,
}

pub fn sign_in(
    data_directory: &Path,
    account_id: &str,
    now: i64,
    still_open: &mut dyn FnMut() -> bool,
) -> Result<(crate::Account, Option<Vec<u8>>), Failure> {
    let profile = profile_directory(data_directory, account_id);
    let mut browser = Browser::launch_at(&profile, false, login::SIGN_IN_URL)?;
    let port = browser.port;
    let websocket_url = browser.websocket_url.clone();

    {
        let mut open = || browser.is_running() && still_open();
        if !login::any_tab_signed_in(port) {
            login::watch_for_sign_in(port, &mut open)?;
        }
    }

    let watched_id = {
        let mut open = || browser.is_running() && still_open();
        login::watch_for_channel_id(port, &mut open)
    };

    let mut session = match Connection::connect(&websocket_url) {
        Ok(mut connection) => match connection.open("about:blank") {
            Ok(page) => Some(Session {
                browser,
                connection,
                page,
            }),
            Err(error) => return Err(error),
        },

        Err(error) if watched_id.is_none() => return Err(error),
        Err(_) => None,
    };

    let outcome = match (watched_id, session.as_mut()) {
        (Some(id), Some(session)) => Ok((id, session.decorations())),
        (Some(id), None) => Ok((id, Decorations::default())),
        (None, Some(session)) => session.channel_id().map(|id| {
            let shown = session.decorations();
            (id, shown)
        }),
        (None, None) => Err(Failure::NoChannel),
    };

    if let Some(session) = session.as_mut() {
        session.close();
    }
    drop(session);

    let (id, shown) = outcome?;

    Ok((
        crate::Account {
            id,
            title: shown.title,
            handle: shown.handle,
            avatar_url: shown.avatar_url,
            avatar_file: None,
            added_at: now,
            refreshed_at: now,
            needs_reauth: false,
        },
        shown.avatar,
    ))
}

const DECORATION_ATTEMPTS: usize = 15;
const DECORATION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub fn refresh_decorations(
    data_directory: &Path,
    account_id: &str,
) -> Result<(String, String, String, Option<Vec<u8>>), Failure> {
    let profile = profile_directory(data_directory, account_id);
    let mut browser = Browser::launch_at(&profile, true, login::STUDIO_URL)?;
    let websocket_url = browser.websocket_url.clone();

    let mut connection = match Connection::connect(&websocket_url) {
        Ok(connection) => connection,
        Err(error) => {
            browser.wait_for_exit(SHUTDOWN_TIMEOUT);
            return Err(error);
        }
    };
    let page = match connection.open(login::STUDIO_URL) {
        Ok(page) => page,
        Err(error) => {
            connection.close_browser();
            browser.wait_for_exit(SHUTDOWN_TIMEOUT);
            return Err(error);
        }
    };

    present_as_a_visible_browser(&mut connection, &page);
    let _ = page.navigate(&mut connection, login::STUDIO_URL);

    let mut session = Session {
        browser,
        connection,
        page,
    };

    let mut shown = Decorations::default();
    for _ in 0..DECORATION_ATTEMPTS {
        std::thread::sleep(DECORATION_INTERVAL);
        shown = session.decorations();
        if !shown.title.trim().is_empty() {
            break;
        }
    }
    session.close();

    Ok((shown.title, shown.handle, shown.avatar_url, shown.avatar))
}

pub fn forget_profile(data_directory: &Path, account_id: &str) -> std::io::Result<()> {
    let profile = profile_directory(data_directory, account_id);
    match std::fs::remove_dir_all(&profile) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

pub fn is_supported() -> bool {
    crate::chrome::find().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgetting_a_profile_that_was_never_created_is_not_an_error() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-youtube-forget-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        assert!(forget_profile(&directory, "UCmissing").is_ok());

        let profile = profile_directory(&directory, "UCreal");
        std::fs::create_dir_all(profile.join("Default")).expect("create");
        std::fs::write(profile.join("Default").join("Cookies"), b"session").expect("write");
        assert!(profile.exists());

        forget_profile(&directory, "UCreal").expect("forget");
        assert!(!profile.exists(), "the cookies have to actually go");
        assert!(
            forget_profile(&directory, "UCreal").is_ok(),
            "and again is fine"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn removing_one_account_leaves_the_others_signed_in() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-youtube-profiles-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);

        for account in ["UCfirst", "UCsecond"] {
            std::fs::create_dir_all(profile_directory(&directory, account)).expect("create");
        }
        forget_profile(&directory, "UCfirst").expect("forget");

        assert!(!profile_directory(&directory, "UCfirst").exists());
        assert!(profile_directory(&directory, "UCsecond").exists());

        let _ = std::fs::remove_dir_all(&directory);
    }
}

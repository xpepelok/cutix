use crate::failure::Failure;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const START_TIMEOUT: Duration = Duration::from_secs(30);

const PORT_FILE: &str = "DevToolsActivePort";

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let roots = [
        std::env::var_os("PROGRAMFILES"),
        std::env::var_os("PROGRAMFILES(X86)"),
        std::env::var_os("LOCALAPPDATA"),
    ];
    let suffixes = [
        r"Google\Chrome\Application\chrome.exe",
        r"Chromium\Application\chrome.exe",
        r"Microsoft\Edge\Application\msedge.exe",
    ];
    for suffix in suffixes {
        for root in roots.iter().flatten() {
            paths.push(PathBuf::from(root).join(suffix));
        }
    }
    paths
}

#[cfg(not(windows))]
fn candidates() -> Vec<PathBuf> {
    [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

pub const BROWSER_ENV: &str = "CUTIX_CHROME";

pub fn find() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os(BROWSER_ENV) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    candidates().into_iter().find(|path| path.is_file())
}

pub fn profile_directory(data_directory: &Path, account_id: &str) -> PathBuf {
    let safe: String = account_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() {
        "default".to_string()
    } else {
        safe
    };
    data_directory.join("profiles").join(safe)
}

fn arguments(profile: &Path, headless: bool, open_at: &str) -> Vec<String> {
    let mut arguments = vec![
        "--remote-debugging-port=0".to_string(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-features=Translate,MediaRouter,OptimizationHints".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-popup-blocking".to_string(),
        "--window-size=1280,900".to_string(),
        "--disable-blink-features=AutomationControlled".to_string(),
    ];
    if headless {
        arguments.push("--headless=new".to_string());
    }
    arguments.push(open_at.to_string());
    arguments
}

#[derive(Debug)]
pub struct Browser {
    child: Child,
    pub websocket_url: String,

    pub port: u16,
    pub profile: PathBuf,
}

pub const LOCK_FILES: [&str; 3] = ["SingletonLock", "SingletonSocket", "SingletonCookie"];

pub fn lock_files(profile: &Path) -> Vec<std::path::PathBuf> {
    LOCK_FILES
        .iter()
        .map(|name| profile.join(name))
        .filter(|path| path.exists() || std::fs::symlink_metadata(path).is_ok())
        .collect()
}

fn release_profile(profile: &Path) -> bool {
    let stale = lock_files(profile);
    let mut freed = !stale.is_empty();
    for path in stale {
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(windows)]
    {
        freed |= stop_processes_on(profile);
    }

    freed
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(windows)]
fn stop_processes_on(profile: &Path) -> bool {
    let needle = profile.display().to_string();
    let mut lister = Command::new("wmic");
    lister.args([
        "process",
        "where",
        "name='chrome.exe'",
        "get",
        "ProcessId,CommandLine",
        "/format:csv",
    ]);
    hide_console(&mut lister);
    let listing = lister.output();
    let Ok(listing) = listing else {
        return false;
    };
    let text = String::from_utf8_lossy(&listing.stdout).to_string();

    let mut stopped = false;
    for line in text.lines() {
        if !line.contains(&needle) {
            continue;
        }
        let Some(id) = line.rsplit(',').next().map(str::trim) else {
            continue;
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let mut kill = Command::new("taskkill");
        kill.args(["/PID", id, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut kill);
        let killed = kill
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        stopped |= killed;
    }
    stopped
}

impl Browser {
    pub fn launch(profile: &Path, headless: bool) -> Result<Self, Failure> {
        Self::launch_at(profile, headless, "about:blank")
    }

    pub fn launch_at(profile: &Path, headless: bool, open_at: &str) -> Result<Self, Failure> {
        match Self::start(profile, headless, open_at) {
            Ok(browser) => Ok(browser),
            Err(first) => {
                if !release_profile(profile) {
                    return Err(first);
                }
                Self::start(profile, headless, open_at)
            }
        }
    }

    fn start(profile: &Path, headless: bool, open_at: &str) -> Result<Self, Failure> {
        let binary = find().ok_or(Failure::NoBrowser)?;
        std::fs::create_dir_all(profile).map_err(|error| Failure::Io(error.to_string()))?;

        let _ = std::fs::remove_file(profile.join(PORT_FILE));

        let child = Command::new(&binary)
            .args(arguments(profile, headless, open_at))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|error| Failure::BrowserLaunch(format!("{}: {error}", binary.display())))?;

        let mut browser = Self {
            child,
            websocket_url: String::new(),
            port: 0,
            profile: profile.to_path_buf(),
        };
        let (port, url) = browser.wait_for_endpoint(START_TIMEOUT)?;
        browser.port = port;
        browser.websocket_url = url;
        Ok(browser)
    }

    pub fn wait_for_exit(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {}
                Err(_) => return false,
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn wait_for_endpoint(&mut self, timeout: Duration) -> Result<(u16, String), Failure> {
        let deadline = Instant::now() + timeout;
        let port_file = self.profile.join(PORT_FILE);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|error| Failure::Io(error.to_string()))?
            {
                return Err(Failure::BrowserLaunch(format!(
                    "the browser exited before it was ready ({status})"
                )));
            }
            if let Some(found) = read_endpoint(&port_file) {
                return Ok(found);
            }
            if Instant::now() >= deadline {
                return Err(Failure::Timeout("browser startup".to_string()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn parse_port_file(text: &str) -> Option<(u16, String)> {
    let mut lines = text.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim();
    if !path.starts_with('/') {
        return None;
    }
    Some((port, format!("ws://127.0.0.1:{port}{path}")))
}

fn read_endpoint(port_file: &Path) -> Option<(u16, String)> {
    let mut text = String::new();
    std::fs::File::open(port_file)
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    parse_port_file(&text)
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn a_profile_left_locked_by_a_dead_browser_is_cleared_before_the_retry() {
        let profile = std::env::temp_dir().join("cutix-chrome-lock-test");
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).expect("profile");
        for name in LOCK_FILES {
            std::fs::write(profile.join(name), b"stale").expect("lock");
        }

        assert_eq!(lock_files(&profile).len(), LOCK_FILES.len());
        assert!(release_profile(&profile));
        assert!(
            lock_files(&profile).is_empty(),
            "the locks have to be gone, otherwise the retry hits the same wall"
        );

        let _ = std::fs::remove_dir_all(&profile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_file_yields_a_loopback_websocket_url() {
        assert_eq!(
            parse_port_file("54321\n/devtools/browser/8f2a-4c\n"),
            Some((
                54321,
                "ws://127.0.0.1:54321/devtools/browser/8f2a-4c".to_string()
            ))
        );
    }

    #[test]
    fn a_half_written_port_file_is_waited_on_rather_than_misread() {
        assert_eq!(parse_port_file(""), None);
        assert_eq!(parse_port_file("54321"), None);
        assert_eq!(parse_port_file("54321\n"), None);
        assert_eq!(parse_port_file("not-a-port\n/devtools/browser/x"), None);
        assert_eq!(parse_port_file("54321\ndevtools/browser/x"), None);
    }

    #[test]
    fn every_account_gets_its_own_profile_and_an_odd_id_cannot_escape_the_folder() {
        let root = Path::new("/data");
        let first = profile_directory(root, "UC_x5XG1OV2P6uZZ5FSM9Ttw");
        let second = profile_directory(root, "UCother");
        assert_ne!(first, second);
        assert!(first.starts_with(root.join("profiles")));

        let hostile = profile_directory(root, "../../etc/passwd");
        assert_eq!(hostile, root.join("profiles").join("______etc_passwd"));
        assert_eq!(
            profile_directory(root, ""),
            root.join("profiles").join("default")
        );
    }

    #[test]
    fn the_launch_flags_pin_the_profile_and_let_the_browser_choose_its_own_port() {
        let visible = arguments(Path::new("/tmp/profile"), false, "about:blank");
        assert!(visible.contains(&"--remote-debugging-port=0".to_string()));
        assert!(visible.contains(&"--user-data-dir=/tmp/profile".to_string()));
        assert!(!visible.iter().any(|argument| argument.contains("headless")));
        assert!(
            visible.contains(&"--disable-blink-features=AutomationControlled".to_string()),
            "google's sign-in refuses a browser that admits to being automated"
        );
        assert_eq!(visible.last().map(String::as_str), Some("about:blank"));

        let at_url = arguments(Path::new("/tmp/profile"), false, "https://example.com/x");
        assert_eq!(
            at_url.last().map(String::as_str),
            Some("https://example.com/x")
        );

        let hidden = arguments(Path::new("/tmp/profile"), true, "about:blank");
        assert!(hidden.contains(&"--headless=new".to_string()));
        assert!(
            !hidden.contains(&"--headless".to_string()),
            "the old headless is detectable"
        );
    }

    #[test]
    fn an_explicit_browser_path_that_does_not_exist_is_ignored_rather_than_launched() {
        std::env::set_var(BROWSER_ENV, "/nowhere/chrome-that-is-not-there");
        let found = find();
        std::env::remove_var(BROWSER_ENV);
        assert!(found.is_none_or(|path| path.is_file()));
    }
}

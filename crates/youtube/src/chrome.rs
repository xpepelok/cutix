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
    let freed = !stale.is_empty();
    for path in stale {
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(windows)]
    let freed = freed | stop_processes_on(profile);

    freed
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(windows)]
const PROCESS_LISTING: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
     Get-CimInstance Win32_Process -Filter 'Name=''chrome.exe'' OR Name=''msedge.exe''' | \
     Select-Object ProcessId,CommandLine | ConvertTo-Csv -NoTypeInformation";

#[cfg(windows)]
fn browser_processes() -> Option<Vec<(u32, String)>> {
    let mut lister = Command::new("powershell");
    lister
        .args(["-NoProfile", "-NonInteractive", "-Command", PROCESS_LISTING])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut lister);
    let listing = lister.output().ok()?;
    Some(parse_process_listing(&String::from_utf8_lossy(
        &listing.stdout,
    )))
}

#[cfg(windows)]
fn stop_processes_on(profile: &Path) -> bool {
    let Some(processes) = browser_processes() else {
        return false;
    };
    let profile = profile.display().to_string();

    let mut stopped = false;
    for (id, command_line) in processes {
        if !uses_profile(&command_line, &profile) {
            continue;
        }
        let mut kill = Command::new("taskkill");
        kill.args(["/PID", &id.to_string(), "/T", "/F"])
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

pub fn parse_process_listing(text: &str) -> Vec<(u32, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let mut fields = csv_fields(line.trim_end_matches('\r')).into_iter();
            let id = fields.next()?.trim().parse::<u32>().ok()?;
            Some((id, fields.next().unwrap_or_default()))
        })
        .collect()
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

pub fn uses_profile(command_line: &str, profile: &str) -> bool {
    const FLAG: &str = "--user-data-dir=";
    let line = command_line.to_lowercase();
    let profile = profile.to_lowercase();
    if profile.is_empty() {
        return false;
    }
    line.match_indices(FLAG).any(|(at, _)| {
        let value = &line[at + FLAG.len()..];
        let value = value.strip_prefix('"').unwrap_or(value);
        value
            .strip_prefix(profile.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['"', ' ']))
    })
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    fn job() -> Option<HANDLE> {
        static JOB: OnceLock<usize> = OnceLock::new();
        let raw = *JOB.get_or_init(|| unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return 0;
            }
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set == 0 {
                CloseHandle(handle);
                return 0;
            }
            handle as usize
        });
        (raw != 0).then_some(raw as HANDLE)
    }

    pub fn adopt(child: &std::process::Child) {
        if let Some(job) = job() {
            unsafe {
                AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE);
            }
        }
    }
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
        #[cfg(windows)]
        job::adopt(&child);

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
    fn the_process_listing_is_read_through_its_quoting() {
        let text = "\u{feff}\"ProcessId\",\"CommandLine\"\r\n\
            \"36380\",\"\"\"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe\"\" \
            --type=crashpad-handler \"\"--user-data-dir=C:\\Users\\Иван\\AppData\\cutix\\profiles\\UCa\"\" /prefetch:4\"\r\n\
            \"4242\",\"\"\r\n\
            \"not-a-pid\",\"x\"\r\n";
        let rows = parse_process_listing(text);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].0, 36_380);
        assert!(
            rows[0]
                .1
                .contains("\"--user-data-dir=C:\\Users\\Иван\\AppData\\cutix\\profiles\\UCa\""),
            "{}",
            rows[0].1
        );
        assert_eq!(rows[1], (4_242, String::new()));
    }

    #[test]
    fn only_a_browser_on_exactly_this_profile_is_matched() {
        let profile = r"C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab";
        for line in [
            r"chrome.exe --remote-debugging-port=0 --user-data-dir=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab --no-first-run",
            r#"chrome.exe --type=renderer "--user-data-dir=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab" --lang=en"#,
            r#"chrome.exe --type=gpu --user-data-dir="c:\users\me\appdata\roaming\cutix\youtube\profiles\ucab" --x"#,
            r"chrome.exe --user-data-dir=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab",
        ] {
            assert!(uses_profile(line, profile), "{line}");
        }
        for line in [
            r"chrome.exe --user-data-dir=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCabc --x",
            r"chrome.exe --user-data-dir=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab\sub",
            r"chrome.exe --note=C:\Users\Me\AppData\Roaming\cutix\youtube\profiles\UCab",
            r"chrome.exe --no-first-run",
            "",
        ] {
            assert!(!uses_profile(line, profile), "{line}");
        }
        assert!(
            !uses_profile("--user-data-dir= --x", ""),
            "an empty path is never a match"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_process_listing_runs_on_this_windows_and_comes_back_parseable() {
        let rows = browser_processes().expect("powershell ran");
        assert!(rows.iter().all(|(id, _)| *id > 0), "{rows:?}");
    }

    #[test]
    fn an_explicit_browser_path_that_does_not_exist_is_ignored_rather_than_launched() {
        unsafe { std::env::set_var(BROWSER_ENV, "/nowhere/chrome-that-is-not-there") };
        let found = find();
        unsafe { std::env::remove_var(BROWSER_ENV) };
        assert!(found.is_none_or(|path| path.is_file()));
    }
}

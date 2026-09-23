//! Self-update from GitHub releases.
//!
//! Only an unpacked release copy updates itself: the directory holding the executable
//! must also hold `lang/`, it must not live in the Nix store, and a debug build never
//! replaces itself unless `CUTIX_FORCE_UPDATES` is set. `CUTIX_DISABLE_UPDATES` turns
//! the whole thing off.
//!
//! To exercise the titlebar pill from a debug build, put `lang/` next to the executable
//! and start it with `CUTIX_FORCE_UPDATES=1 CUTIX_FAKE_VERSION=0.0.1`: the build then
//! reports itself as that version, so the latest release looks newer. Release builds
//! ignore `CUTIX_FAKE_VERSION`.
//!
//! Installing swaps files in place. A running executable (or a loaded DLL) cannot be
//! overwritten on Windows but it can be renamed, so every file that is replaced is
//! first moved aside to `<name>.old`, the new one is moved in, and the renamed files
//! are listed in `.cutix-leftovers` for the next start to delete. Any failure halfway
//! puts the renamed files back.
//!
//! While an install runs, `.cutix-update/lock` is held open (exclusively on Windows;
//! with the owner's pid elsewhere), so a second cutix started meanwhile — Explorer's
//! "Publish", a double-clicked video — leaves the work directory alone instead of
//! deleting it from under the download.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPOSITORY: &str = "xpepelok/cutix";
pub const UPDATED_FLAG: &str = "--updated";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(5);

const CHECKSUMS_ASSET: &str = "checksums.txt";
const WORK_DIR: &str = ".cutix-update";
const STAGED_DIR: &str = "staged";
const LOCK_FILE: &str = "lock";
const PROBE_FILE: &str = ".cutix-update-probe";
const LEFTOVERS_FILE: &str = ".cutix-leftovers";
const OLD_SUFFIX: &str = ".old";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;

#[cfg(windows)]
const EXECUTABLE: &str = "cutix.exe";
#[cfg(not(windows))]
const EXECUTABLE: &str = "cutix";

/// The version releases are compared against. Debug builds take `CUTIX_FAKE_VERSION`
/// instead when it is set, so an update can be offered without cutting a release.
pub fn current_version() -> &'static str {
    if cfg!(debug_assertions) {
        static FAKE: OnceLock<Option<String>> = OnceLock::new();
        let fake = FAKE.get_or_init(|| {
            std::env::var("CUTIX_FAKE_VERSION")
                .ok()
                .map(|version| version.trim().to_string())
                .filter(|version| !version.is_empty())
        });
        if let Some(version) = fake {
            return version;
        }
    }
    env!("CARGO_PKG_VERSION")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub asset_name: String,
    pub asset_url: String,
    pub checksums_url: String,
}

impl Release {
    /// The tag as shown to people, always with a leading `v`.
    pub fn label(&self) -> String {
        let bare = self.tag.trim_start_matches(['v', 'V']);
        format!("v{bare}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    Network,
    Checksum,
    Archive,
    Install,
}

impl UpdateError {
    pub fn message_key(self) -> &'static str {
        match self {
            Self::Network => "update.error.network",
            Self::Checksum => "update.error.checksum",
            Self::Archive => "update.error.archive",
            Self::Install => "update.error.install",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    Available(Release),
    Installing {
        release: Release,
        progress: f32,
    },
    Restarting,
    /// The new files are in place, but an export or an upload is still running: the
    /// restart waits for it (or for a click) rather than cutting it off.
    ReadyToRestart {
        release: Release,
        exe: PathBuf,
    },
    Failed {
        release: Release,
        error: UpdateError,
    },
}

impl Phase {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Installing { .. } | Self::Restarting)
    }
}

pub type Shared = Arc<Mutex<Phase>>;

// ---------------------------------------------------------------------------------
// Eligibility

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn in_nix_store(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('\\', "/")
        .starts_with("/nix/store/")
}

/// Whether files can be created in `dir`: a copy unpacked by an administrator under
/// `Program Files` or `/opt` looks like an install but cannot take an update, and
/// offering one would only ever end in "Update failed".
pub fn writable(dir: &Path) -> bool {
    let probe = dir.join(PROBE_FILE);
    let created = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
        .is_ok();
    let _ = fs::remove_file(&probe);
    created
}

/// The directory of the running executable, when this copy may update itself.
pub fn install_dir() -> Option<PathBuf> {
    if env_set("CUTIX_DISABLE_UPDATES") {
        return None;
    }
    if cfg!(debug_assertions) && !env_set("CUTIX_FORCE_UPDATES") {
        return None;
    }
    platform_asset()?;
    let exe = std::env::current_exe().ok()?;
    if in_nix_store(&exe) || fs::canonicalize(&exe).is_ok_and(|real| in_nix_store(&real)) {
        return None;
    }
    let dir = exe.parent()?.to_path_buf();
    (dir.join("lang").is_dir() && writable(&dir)).then_some(dir)
}

/// The release archive for a platform. There is no NixOS variant on purpose: the
/// `cutix-nixos-*` tarball holds a wrapper script pointing into the CI's `/nix/store`,
/// and the only NixOS copies that can update at all are generic builds run outside
/// the store (`install_dir` rules the store out), which need the generic tarball.
pub fn asset_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("cutix-windows-x64.zip"),
        ("windows", "aarch64") => Some("cutix-windows-arm64.zip"),
        ("linux", "x86_64") => Some("cutix-linux-x64.tar.gz"),
        ("linux", "aarch64") => Some("cutix-linux-arm64.tar.gz"),
        _ => None,
    }
}

fn platform_asset() -> Option<&'static str> {
    asset_for(std::env::consts::OS, std::env::consts::ARCH)
}

// ---------------------------------------------------------------------------------
// Versions and release metadata

/// Numeric components of a version such as `v1.2.3-beta`; the suffix is ignored.
pub fn parse_version(text: &str) -> Option<Vec<u64>> {
    let core = text
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?;
    if core.is_empty() {
        return None;
    }
    core.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some(candidate), Some(current)) = (parse_version(candidate), parse_version(current))
    else {
        return false;
    };
    let length = candidate.len().max(current.len());
    let at = |parts: &[u64], index: usize| parts.get(index).copied().unwrap_or(0);
    for index in 0..length {
        let (a, b) = (at(&candidate, index), at(&current, index));
        if a != b {
            return a > b;
        }
    }
    false
}

/// The sha256 listed for `file` in a `sha256sum`-style listing.
pub fn checksum_for(listing: &str, file: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        let name = name.trim_start_matches('*').trim_start_matches("./");
        let valid = hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
        (valid && name == file).then(|| hash.to_ascii_lowercase())
    })
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// The release described by a `releases/latest` payload, when it is newer than
/// `current` and carries both the archive for `asset` and the checksum listing.
pub fn pick_release(payload: &str, current: &str, asset: &str) -> Option<Release> {
    let release: ApiRelease = serde_json::from_str(payload).ok()?;
    if release.draft || release.prerelease || !is_newer(&release.tag_name, current) {
        return None;
    }
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| candidate.browser_download_url.clone())
    };
    Some(Release {
        asset_url: url(asset)?,
        checksums_url: url(CHECKSUMS_ASSET)?,
        asset_name: asset.to_string(),
        tag: release.tag_name,
    })
}

// ---------------------------------------------------------------------------------
// Network

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("cutix/{} (self-update)", current_version()))
        .build()
}

fn fetch_text(agent: &ureq::Agent, url: &str) -> Result<String, UpdateError> {
    let response = agent
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| UpdateError::Network)?;
    let mut text = String::new();
    response
        .into_reader()
        .take(MAX_METADATA_BYTES)
        .read_to_string(&mut text)
        .map_err(|_| UpdateError::Network)?;
    Ok(text)
}

/// Asks GitHub whether a newer release exists for this platform.
pub fn check() -> Result<Option<Release>, UpdateError> {
    let Some(asset) = platform_asset() else {
        return Ok(None);
    };
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let payload = fetch_text(&agent(), &url)?;
    Ok(pick_release(&payload, current_version(), asset))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Streams `url` into `path`, returning the sha256 of what was written.
fn download(
    agent: &ureq::Agent,
    url: &str,
    path: &Path,
    mut progress: impl FnMut(f32),
) -> Result<String, UpdateError> {
    let response = agent.get(url).call().map_err(|_| UpdateError::Network)?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|total| *total > 0);
    let mut reader = response.into_reader();
    let mut file = fs::File::create(path).map_err(|_| UpdateError::Install)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(UpdateError::Network),
        };
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|_| UpdateError::Install)?;
        written += read as u64;
        if let Some(total) = total {
            progress((written as f32 / total as f32).min(1.0));
        }
    }
    file.flush().map_err(|_| UpdateError::Install)?;
    if total.is_some_and(|total| total != written) {
        return Err(UpdateError::Network);
    }
    Ok(hex(&hasher.finalize()))
}

// ---------------------------------------------------------------------------------
// Archives

/// The relative path an archive entry may be unpacked to. `Ok(None)` is an entry that
/// names the archive root itself (`./`); an absolute path, a drive prefix or a `..`
/// component is refused outright.
pub fn entry_path(name: &str) -> Result<Option<PathBuf>, UpdateError> {
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(UpdateError::Archive);
    }
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(UpdateError::Archive),
            _ if part.contains(':') || part.contains('\0') => return Err(UpdateError::Archive),
            _ => path.push(part),
        }
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UpdateError::Archive);
    }
    Ok((!path.as_os_str().is_empty()).then_some(path))
}

fn write_entry(
    dest: &Path,
    relative: &Path,
    reader: &mut dyn Read,
    mode: Option<u32>,
) -> Result<(), UpdateError> {
    let target = dest.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|_| UpdateError::Install)?;
    }
    let mut file = fs::File::create(&target).map_err(|_| UpdateError::Install)?;
    io::copy(reader, &mut file).map_err(|_| UpdateError::Archive)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = relative == Path::new(EXECUTABLE);
        let mode = mode.map(|mode| mode & 0o777).filter(|mode| *mode != 0);
        let mode = if executable {
            mode.unwrap_or(0o755) | 0o755
        } else {
            mode.unwrap_or(0o644)
        };
        fs::set_permissions(&target, fs::Permissions::from_mode(mode))
            .map_err(|_| UpdateError::Install)?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

/// Unpacks a zip into `dest`, returning the files it wrote.
fn unzip(archive: &Path, dest: &Path) -> Result<Vec<PathBuf>, UpdateError> {
    let file = fs::File::open(archive).map_err(|_| UpdateError::Archive)?;
    let mut zip =
        zip::ZipArchive::new(io::BufReader::new(file)).map_err(|_| UpdateError::Archive)?;
    let mut written = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|_| UpdateError::Archive)?;
        let Some(relative) = entry_path(entry.name())? else {
            continue;
        };
        if entry.is_dir() {
            fs::create_dir_all(dest.join(&relative)).map_err(|_| UpdateError::Install)?;
            continue;
        }
        if entry.is_symlink() {
            continue;
        }
        let mode = entry.unix_mode();
        write_entry(dest, &relative, &mut entry, mode)?;
        written.push(relative);
    }
    Ok(written)
}

fn read_block(reader: &mut impl Read, block: &mut [u8; 512]) -> Result<bool, UpdateError> {
    let mut filled = 0;
    while filled < block.len() {
        match reader.read(&mut block[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err(UpdateError::Archive),
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(UpdateError::Archive),
        }
    }
    Ok(true)
}

fn tar_number(field: &[u8]) -> Result<u64, UpdateError> {
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        // GNU base-256: big-endian, the marker bit cleared.
        let mut value = u64::from(field[0] & 0x7f);
        for byte in &field[1..] {
            value = value
                .checked_mul(256)
                .and_then(|value| value.checked_add(u64::from(*byte)))
                .ok_or(UpdateError::Archive)?;
        }
        return Ok(value);
    }
    let text = std::str::from_utf8(field).map_err(|_| UpdateError::Archive)?;
    let text = text.trim_matches(|c: char| c == '\0' || c == ' ');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| UpdateError::Archive)
}

fn tar_text(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn skip(reader: &mut impl Read, count: u64) -> Result<(), UpdateError> {
    let skipped =
        io::copy(&mut reader.take(count), &mut io::sink()).map_err(|_| UpdateError::Archive)?;
    if skipped == count {
        Ok(())
    } else {
        Err(UpdateError::Archive)
    }
}

fn pax_path(records: &[u8]) -> Option<String> {
    String::from_utf8_lossy(records).lines().find_map(|record| {
        let (_, pair) = record.split_once(' ')?;
        pair.strip_prefix("path=").map(str::to_string)
    })
}

/// A small ustar/GNU/pax reader: regular files and directories only; links and
/// devices are skipped. Returns the files it wrote.
fn untar(mut reader: impl Read, dest: &Path) -> Result<Vec<PathBuf>, UpdateError> {
    let mut header = [0u8; 512];
    let mut long_name: Option<String> = None;
    let mut written = Vec::new();
    loop {
        if !read_block(&mut reader, &mut header)? || header.iter().all(|byte| *byte == 0) {
            return Ok(written);
        }
        let stored = tar_number(&header[148..156])?;
        let computed: u64 = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    u64::from(b' ')
                } else {
                    u64::from(*byte)
                }
            })
            .sum();
        if stored != computed {
            return Err(UpdateError::Archive);
        }

        let size = tar_number(&header[124..136])?;
        let padding = size.div_ceil(512) * 512 - size;
        let kind = header[156];
        let name = long_name.take().unwrap_or_else(|| {
            let name = tar_text(&header[0..100]);
            let prefix = tar_text(&header[345..500]);
            // Only POSIX ustar (`ustar\0`) keeps a path prefix at 345. GNU tar's magic
            // is `ustar  \0`, and the same bytes hold its atime/ctime and sparse map.
            if &header[257..263] == b"ustar\0" && !prefix.is_empty() {
                format!("{prefix}/{name}")
            } else {
                name
            }
        });

        match kind {
            b'L' | b'x' => {
                if size > MAX_METADATA_BYTES {
                    return Err(UpdateError::Archive);
                }
                let mut data = Vec::new();
                (&mut reader)
                    .take(size)
                    .read_to_end(&mut data)
                    .map_err(|_| UpdateError::Archive)?;
                skip(&mut reader, padding)?;
                long_name = if kind == b'L' {
                    Some(tar_text(&data))
                } else {
                    pax_path(&data)
                };
            }
            b'0' | b'7' | 0 => {
                let mut body = (&mut reader).take(size);
                match entry_path(&name)? {
                    Some(relative) => {
                        let mode = tar_number(&header[100..108]).ok().map(|mode| mode as u32);
                        write_entry(dest, &relative, &mut body, mode)?;
                        written.push(relative);
                    }
                    None => {
                        io::copy(&mut body, &mut io::sink()).map_err(|_| UpdateError::Archive)?;
                    }
                }
                if body.limit() != 0 {
                    return Err(UpdateError::Archive);
                }
                skip(&mut reader, padding)?;
            }
            b'5' => {
                if let Some(relative) = entry_path(&name)? {
                    fs::create_dir_all(dest.join(relative)).map_err(|_| UpdateError::Install)?;
                }
                skip(&mut reader, size + padding)?;
            }
            _ => skip(&mut reader, size + padding)?,
        }
    }
}

/// Unpacks `archive` into `dest`, returning the files it wrote.
fn extract(archive: &Path, dest: &Path) -> Result<Vec<PathBuf>, UpdateError> {
    let name = archive
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        unzip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = fs::File::open(archive).map_err(|_| UpdateError::Archive)?;
        untar(flate2::read::GzDecoder::new(io::BufReader::new(file)), dest)
    } else {
        Err(UpdateError::Archive)
    }
}

/// The files under `staged`, checked against what the archive `written`: anything
/// missing means something else emptied the work directory between unpacking and
/// swapping, and a half-staged set must never be swapped in (the new executable with
/// the old `lang/` would look like a clean install).
fn verify_staged(staged: &Path, written: &[PathBuf]) -> Result<Vec<PathBuf>, UpdateError> {
    let mut expected = written.to_vec();
    expected.sort();
    expected.dedup();
    let files = staged_files(staged).map_err(|_| UpdateError::Install)?;
    if files != expected {
        return Err(UpdateError::Install);
    }
    if !files.iter().any(|file| file == Path::new(EXECUTABLE)) {
        return Err(UpdateError::Archive);
    }
    Ok(files)
}

fn staged_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(root.join(&relative))? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

// ---------------------------------------------------------------------------------
// Swapping files and leftovers

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// A free `<name>.old` (or `.old1`…) next to `target`.
fn aside_name(target: &Path) -> Option<PathBuf> {
    (0..16).find_map(|index| {
        let suffix = if index == 0 {
            OLD_SUFFIX.to_string()
        } else {
            format!("{OLD_SUFFIX}{index}")
        };
        let candidate = with_suffix(target, &suffix);
        (!candidate.exists() || fs::remove_file(&candidate).is_ok()).then_some(candidate)
    })
}

fn is_leftover_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(at) = name.rfind(OLD_SUFFIX) else {
        return false;
    };
    at > 0
        && name[at + OLD_SUFFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit())
}

fn leftover_line(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Leftovers recorded in `dir`; lines that are not a safe relative `*.old` path are
/// dropped so the list can never point outside the install directory.
pub fn read_leftovers(dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(dir.join(LEFTOVERS_FILE)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| entry_path(line.trim()).ok().flatten())
        .filter(|path| is_leftover_name(path))
        .collect()
}

pub fn write_leftovers(dir: &Path, leftovers: &[PathBuf]) -> io::Result<()> {
    let file = dir.join(LEFTOVERS_FILE);
    if leftovers.is_empty() {
        return match fs::remove_file(&file) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let mut text = String::new();
    for path in leftovers {
        text.push_str(&leftover_line(path));
        text.push('\n');
    }
    fs::write(file, text)
}

/// Deletes the recorded leftovers, keeping the ones that are still locked on the
/// list. Returns how many remain.
pub fn remove_leftovers(dir: &Path) -> usize {
    let remaining: Vec<PathBuf> = read_leftovers(dir)
        .into_iter()
        .filter(|relative| {
            let path = dir.join(relative);
            match fs::remove_file(&path) {
                Ok(()) => false,
                Err(error) => error.kind() != io::ErrorKind::NotFound && path.exists(),
            }
        })
        .collect();
    let _ = write_leftovers(dir, &remaining);
    remaining.len()
}

struct Swap {
    target: PathBuf,
    aside: Option<PathBuf>,
    placed: bool,
}

fn roll_back(swaps: &[Swap]) {
    for swap in swaps.iter().rev() {
        if swap.placed {
            let _ = fs::remove_file(&swap.target);
        }
        if let Some(aside) = &swap.aside {
            let _ = fs::rename(aside, &swap.target);
        }
    }
}

/// Moves every staged file over its counterpart in `dir`. On failure everything is
/// put back and nothing is recorded.
fn swap_in(staged: &Path, dir: &Path, files: &[PathBuf]) -> Result<Vec<PathBuf>, UpdateError> {
    let mut swaps: Vec<Swap> = Vec::new();
    let result = (|| {
        for relative in files {
            let target = dir.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|_| UpdateError::Install)?;
            }
            let aside = if target.exists() {
                let aside = aside_name(&target).ok_or(UpdateError::Install)?;
                fs::rename(&target, &aside).map_err(|_| UpdateError::Install)?;
                Some(aside)
            } else {
                None
            };
            swaps.push(Swap {
                target: target.clone(),
                aside,
                placed: false,
            });
            fs::rename(staged.join(relative), &target).map_err(|_| UpdateError::Install)?;
            if let Some(last) = swaps.last_mut() {
                last.placed = true;
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        roll_back(&swaps);
        return Err(error);
    }

    let mut leftovers = read_leftovers(dir);
    for swap in &swaps {
        if let Some(relative) = swap
            .aside
            .as_ref()
            .and_then(|aside| aside.strip_prefix(dir).ok())
        {
            if !leftovers.iter().any(|known| known == relative) {
                leftovers.push(relative.to_path_buf());
            }
        }
    }
    if write_leftovers(dir, &leftovers).is_err() {
        roll_back(&swaps);
        return Err(UpdateError::Install);
    }
    Ok(leftovers)
}

// ---------------------------------------------------------------------------------
// The install lock

/// Marks `work` as belonging to a running install for as long as it is held.
struct InstallLock {
    _file: fs::File,
}

fn lock_path(work: &Path) -> PathBuf {
    work.join(LOCK_FILE)
}

/// Takes the lock for `work`, which must exist.
fn lock_install(work: &Path) -> Result<InstallLock, UpdateError> {
    let path = lock_path(work);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    // Sharing nothing is the lock itself: no other process can open or delete the
    // file while this handle is alive, and `remove_dir_all` fails on it.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let mut file = options.open(&path).map_err(|_| UpdateError::Install)?;
    // Elsewhere the owner's pid is the lock: another instance checks it is alive.
    file.write_all(std::process::id().to_string().as_bytes())
        .map_err(|_| UpdateError::Install)?;
    file.flush().map_err(|_| UpdateError::Install)?;
    Ok(InstallLock { _file: file })
}

/// Whether another cutix is installing into `work` right now.
fn install_locked(work: &Path) -> bool {
    let path = lock_path(work);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        match fs::OpenOptions::new().read(true).share_mode(0).open(&path) {
            Ok(_) => false,
            Err(error) => error.kind() != io::ErrorKind::NotFound,
        }
    }
    #[cfg(not(windows))]
    {
        let Ok(text) = fs::read_to_string(&path) else {
            return false;
        };
        let Ok(pid) = text.trim().parse::<u32>() else {
            return false;
        };
        pid == std::process::id() || Path::new("/proc").join(pid.to_string()).is_dir()
    }
}

// ---------------------------------------------------------------------------------
// Installing

/// Downloads, verifies and installs `release` into `dir`. Returns the executable to
/// start; the caller restarts into it once nothing else is running.
pub fn install(
    dir: &Path,
    release: &Release,
    mut progress: impl FnMut(f32),
) -> Result<PathBuf, UpdateError> {
    let work = dir.join(WORK_DIR);
    if install_locked(&work) {
        return Err(UpdateError::Install);
    }
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|_| UpdateError::Install)?;
    let lock = lock_install(&work)?;

    let result = (|| {
        let agent = agent();
        let listing = fetch_text(&agent, &release.checksums_url)?;
        let expected = checksum_for(&listing, &release.asset_name).ok_or(UpdateError::Checksum)?;

        let archive = work.join(&release.asset_name);
        let actual = download(&agent, &release.asset_url, &archive, &mut progress)?;
        if actual != expected {
            return Err(UpdateError::Checksum);
        }

        let staged = work.join(STAGED_DIR);
        fs::create_dir_all(&staged).map_err(|_| UpdateError::Install)?;
        let written = extract(&archive, &staged)?;
        let files = verify_staged(&staged, &written)?;
        swap_in(&staged, dir, &files)?;
        Ok(dir.join(EXECUTABLE))
    })();

    // The open lock file would keep the directory alive on Windows.
    drop(lock);
    let _ = fs::remove_dir_all(&work);
    result
}

/// Starts the freshly installed executable.
pub fn relaunch(exe: &Path) -> Result<(), UpdateError> {
    let mut command = std::process::Command::new(exe);
    command.arg(UPDATED_FLAG);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| UpdateError::Install)
}

/// Drops what a previous update left behind in `dir` — unless another instance is
/// installing there right now, whose work directory stays. Returns how many leftovers
/// are still locked.
fn clean_up_in(dir: &Path) -> usize {
    let work = dir.join(WORK_DIR);
    if !install_locked(&work) {
        let _ = fs::remove_dir_all(&work);
    }
    remove_leftovers(dir)
}

/// Run once at startup: drops what a previous update left behind. After a restart the
/// old process may still hold its executable for a moment, so the deletion is retried
/// in the background for a while.
pub fn clean_up() {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    if clean_up_in(&dir) == 0 {
        return;
    }
    let restarted = std::env::args_os().any(|argument| argument == UPDATED_FLAG);
    if restarted {
        let _ = std::thread::Builder::new()
            .name("cutix-update-cleanup".into())
            .spawn(move || {
                for _ in 0..60 {
                    std::thread::sleep(Duration::from_millis(500));
                    if remove_leftovers(&dir) == 0 {
                        return;
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.0.3", "0.0.2"));
        assert!(is_newer("0.1.0", "0.0.9"));
        assert!(is_newer("v0.0.10", "0.0.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.0.2", "0.0.2"));
        assert!(!is_newer("0.0.2.0", "0.0.2"));
        assert!(!is_newer("v0.0.1", "0.0.2"));
        assert!(is_newer("v0.0.3-beta", "0.0.2"));
        assert!(!is_newer("nightly", "0.0.2"));
        assert!(!is_newer("", "0.0.2"));
        assert_eq!(parse_version("v1.2.3+build"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("1..2"), None);
    }

    #[test]
    fn checksums_are_read_per_file() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let listing = format!(
            "{a}  cutix-windows-x64.zip\n{b} *cutix-linux-x64.tar.gz\nnot a line\nabc  short.zip\n"
        );
        assert_eq!(
            checksum_for(&listing, "cutix-windows-x64.zip").as_deref(),
            Some(a.as_str())
        );
        assert_eq!(
            checksum_for(&listing, "cutix-linux-x64.tar.gz"),
            Some("b".repeat(64))
        );
        assert_eq!(checksum_for(&listing, "short.zip"), None);
        assert_eq!(checksum_for(&listing, "cutix-linux-arm64.tar.gz"), None);
    }

    #[test]
    fn assets_follow_the_platform_and_never_name_the_nixos_wrapper() {
        assert_eq!(
            asset_for("windows", "x86_64"),
            Some("cutix-windows-x64.zip")
        );
        assert_eq!(
            asset_for("windows", "aarch64"),
            Some("cutix-windows-arm64.zip")
        );
        assert_eq!(asset_for("linux", "x86_64"), Some("cutix-linux-x64.tar.gz"));
        assert_eq!(
            asset_for("linux", "aarch64"),
            Some("cutix-linux-arm64.tar.gz")
        );
        assert_eq!(asset_for("macos", "aarch64"), None);
        assert_eq!(asset_for("windows", "x86"), None);
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64")] {
            assert!(
                !asset_for(os, arch).unwrap_or_default().contains("nixos"),
                "the nixos tarball only runs from the CI's store"
            );
        }
    }

    #[test]
    fn a_directory_files_can_be_created_in_is_writable_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(writable(dir.path()));
        assert!(
            !dir.path().join(PROBE_FILE).exists(),
            "the probe must not be left behind"
        );
        assert!(!writable(&dir.path().join("does-not-exist")));
    }

    #[test]
    fn a_running_installs_work_dir_survives_another_instances_clean_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        let work = dir.path().join(WORK_DIR);
        fs::create_dir_all(&work).expect("work");
        fs::write(work.join("partial.zip"), b"...").expect("download");
        let lock = lock_install(&work).expect("lock");
        assert!(install_locked(&work));

        assert_eq!(clean_up_in(dir.path()), 0);
        assert!(
            work.join("partial.zip").exists(),
            "a second instance must not delete the running download"
        );

        drop(lock);
        assert!(!install_locked(&work));
        assert_eq!(clean_up_in(dir.path()), 0);
        assert!(!work.exists());
    }

    #[test]
    fn a_staged_set_missing_a_file_is_not_swapped_in() {
        let dir = tempfile::tempdir().expect("tempdir");
        let staged = dir.path().join(STAGED_DIR);
        fs::create_dir_all(staged.join("lang")).expect("staged");
        fs::write(staged.join(EXECUTABLE), b"new").expect("exe");
        fs::write(staged.join("lang").join("en.json"), b"{}").expect("lang");
        let written = vec![
            PathBuf::from("lang").join("en.json"),
            PathBuf::from(EXECUTABLE),
            // The same entry twice in an archive is still one staged file.
            PathBuf::from(EXECUTABLE),
        ];
        assert_eq!(
            verify_staged(&staged, &written).expect("complete"),
            vec![
                PathBuf::from(EXECUTABLE),
                PathBuf::from("lang").join("en.json")
            ]
        );

        fs::remove_file(staged.join("lang").join("en.json")).expect("vanish");
        assert_eq!(
            verify_staged(&staged, &written),
            Err(UpdateError::Install),
            "a set that lost a file would install the new exe with the old lang/"
        );

        let no_exe = vec![PathBuf::from("lang").join("en.json")];
        fs::write(staged.join("lang").join("en.json"), b"{}").expect("lang");
        fs::remove_file(staged.join(EXECUTABLE)).expect("drop exe");
        assert_eq!(verify_staged(&staged, &no_exe), Err(UpdateError::Archive));
    }

    #[test]
    fn a_release_needs_a_newer_tag_the_archive_and_checksums() {
        let payload = |tag: &str, assets: &[&str]| {
            let assets: Vec<String> = assets
                .iter()
                .map(|name| {
                    format!(
                        r#"{{"name":"{name}","browser_download_url":"https://example.com/{name}"}}"#
                    )
                })
                .collect();
            format!(
                r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{}]}}"#,
                assets.join(",")
            )
        };
        let full = payload("v0.0.3", &["checksums.txt", "cutix-windows-x64.zip"]);
        let picked = pick_release(&full, "0.0.2", "cutix-windows-x64.zip").expect("newer");
        assert_eq!(picked.label(), "v0.0.3");
        assert_eq!(
            picked.asset_url,
            "https://example.com/cutix-windows-x64.zip"
        );
        assert_eq!(picked.checksums_url, "https://example.com/checksums.txt");

        assert!(pick_release(&full, "0.0.3", "cutix-windows-x64.zip").is_none());
        assert!(pick_release(&full, "0.0.2", "cutix-linux-x64.tar.gz").is_none());
        let unsigned = payload("v0.0.3", &["cutix-windows-x64.zip"]);
        assert!(pick_release(&unsigned, "0.0.2", "cutix-windows-x64.zip").is_none());
        let draft = full.replace(r#""draft":false"#, r#""draft":true"#);
        assert!(pick_release(&draft, "0.0.2", "cutix-windows-x64.zip").is_none());
        assert!(pick_release("not json", "0.0.2", "cutix-windows-x64.zip").is_none());
    }

    #[test]
    fn archive_entries_cannot_escape() {
        assert_eq!(
            entry_path("lang/en.json").unwrap(),
            Some(PathBuf::from("lang").join("en.json"))
        );
        assert_eq!(entry_path("./cutix").unwrap(), Some(PathBuf::from("cutix")));
        assert_eq!(
            entry_path("lang\\de.json").unwrap(),
            Some(PathBuf::from("lang").join("de.json"))
        );
        assert_eq!(entry_path("./").unwrap(), None);
        assert_eq!(entry_path("lang/").unwrap(), Some(PathBuf::from("lang")));
        for bad in [
            "../cutix",
            "lang/../../x",
            "/etc/passwd",
            "\\Windows\\x.dll",
            "C:/Windows/x.dll",
            "C:x.dll",
            "a/..",
        ] {
            assert!(entry_path(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn leftovers_round_trip_and_only_old_files_are_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let listed = vec![
            PathBuf::from("cutix.exe.old"),
            PathBuf::from("lang").join("en.json.old2"),
        ];
        write_leftovers(dir.path(), &listed).expect("write");
        assert_eq!(read_leftovers(dir.path()), listed);

        fs::write(
            dir.path().join(LEFTOVERS_FILE),
            "cutix.exe.old\n../outside.old\nLICENSE\n/abs.old\nlang/x.json.old\n",
        )
        .expect("write raw");
        assert_eq!(
            read_leftovers(dir.path()),
            vec![
                PathBuf::from("cutix.exe.old"),
                PathBuf::from("lang").join("x.json.old")
            ]
        );

        fs::write(dir.path().join("cutix.exe.old"), b"old").expect("old file");
        fs::write(dir.path().join("LICENSE"), b"keep").expect("license");
        assert_eq!(remove_leftovers(dir.path()), 0);
        assert!(!dir.path().join("cutix.exe.old").exists());
        assert!(dir.path().join("LICENSE").exists());
        assert!(!dir.path().join(LEFTOVERS_FILE).exists());
    }

    #[test]
    fn swapping_moves_old_files_aside_and_records_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let staged = dir.path().join(WORK_DIR).join(STAGED_DIR);
        fs::create_dir_all(staged.join("lang")).expect("staged");
        fs::write(staged.join(EXECUTABLE), b"new").expect("exe");
        fs::write(staged.join("lang").join("en.json"), b"{}").expect("lang");
        fs::write(dir.path().join(EXECUTABLE), b"old").expect("old exe");

        let files = staged_files(&staged).expect("files");
        let leftovers = swap_in(&staged, dir.path(), &files).expect("swap");
        assert_eq!(leftovers, vec![PathBuf::from(format!("{EXECUTABLE}.old"))]);
        assert_eq!(fs::read(dir.path().join(EXECUTABLE)).unwrap(), b"new");
        assert_eq!(
            fs::read(dir.path().join(format!("{EXECUTABLE}.old"))).unwrap(),
            b"old"
        );
        assert_eq!(read_leftovers(dir.path()), leftovers);
    }

    #[test]
    fn a_failed_swap_puts_everything_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let staged = dir.path().join("staged");
        fs::create_dir_all(&staged).expect("staged");
        fs::write(staged.join("a"), b"new a").expect("a");
        fs::write(dir.path().join("a"), b"old a").expect("old a");

        // `b` is listed but was never staged, so moving it in fails after `a` moved.
        let files = vec![PathBuf::from("a"), PathBuf::from("b")];
        assert_eq!(
            swap_in(&staged, dir.path(), &files),
            Err(UpdateError::Install)
        );
        assert_eq!(fs::read(dir.path().join("a")).unwrap(), b"old a");
        assert!(!dir.path().join("a.old").exists());
        assert!(read_leftovers(dir.path()).is_empty());
    }

    fn checksummed(mut header: [u8; 512]) -> [u8; 512] {
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        header[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        header
    }

    fn tar_header(name: &str, kind: u8, size: usize) -> [u8; 512] {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..107].copy_from_slice(b"0000644");
        header[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        header[156] = kind;
        header[257..263].copy_from_slice(b"ustar\0");
        checksummed(header)
    }

    fn tar_entry_with(out: &mut Vec<u8>, header: [u8; 512], body: &[u8]) {
        out.extend_from_slice(&header);
        out.extend_from_slice(body);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }

    fn tar_entry(out: &mut Vec<u8>, name: &str, kind: u8, body: &[u8]) {
        tar_entry_with(out, tar_header(name, kind, body.len()), body);
    }

    #[test]
    fn only_a_posix_ustar_prefix_joins_the_entry_name() {
        let mut archive = Vec::new();
        let mut posix = tar_header("en.json", b'0', 2);
        posix[345..349].copy_from_slice(b"lang");
        tar_entry_with(&mut archive, checksummed(posix), b"{}");
        // A GNU header keeps its atime where ustar keeps the prefix.
        let mut gnu = tar_header("cutix", b'0', 3);
        gnu[257..265].copy_from_slice(b"ustar  \0");
        gnu[345..357].copy_from_slice(b"14721234567\0");
        tar_entry_with(&mut archive, checksummed(gnu), b"bin");
        archive.extend_from_slice(&[0u8; 1024]);

        let dir = tempfile::tempdir().expect("tempdir");
        let written = untar(archive.as_slice(), dir.path()).expect("untar");
        assert_eq!(
            written,
            vec![
                PathBuf::from("lang").join("en.json"),
                PathBuf::from("cutix")
            ]
        );
        assert_eq!(fs::read(dir.path().join("cutix")).unwrap(), b"bin");
        assert!(!dir.path().join("14721234567").exists());
    }

    #[test]
    fn tarballs_unpack_and_refuse_traversal() {
        let mut archive = Vec::new();
        tar_entry(&mut archive, "./", b'5', b"");
        tar_entry(&mut archive, "./lang/", b'5', b"");
        tar_entry(&mut archive, "./lang/en.json", b'0', b"{\"a\":1}");
        tar_entry(&mut archive, "././@LongLink", b'L', b"./LICENSE\0");
        tar_entry(&mut archive, "ignored", b'0', b"MIT");
        archive.extend_from_slice(&[0u8; 1024]);

        let dir = tempfile::tempdir().expect("tempdir");
        untar(archive.as_slice(), dir.path()).expect("untar");
        assert_eq!(
            fs::read(dir.path().join("lang").join("en.json")).unwrap(),
            b"{\"a\":1}"
        );
        assert_eq!(fs::read(dir.path().join("LICENSE")).unwrap(), b"MIT");
        assert!(!dir.path().join("ignored").exists());

        let mut evil = Vec::new();
        tar_entry(&mut evil, "../escape", b'0', b"x");
        let target = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            untar(evil.as_slice(), &target.path().join("inner")),
            Err(UpdateError::Archive)
        );
        assert!(!target.path().join("escape").exists());
    }

    #[test]
    fn zips_unpack_and_refuse_traversal() {
        use zip::write::SimpleFileOptions;

        let dir = tempfile::tempdir().expect("tempdir");
        let good = dir.path().join("good.zip");
        {
            let mut writer = zip::ZipWriter::new(fs::File::create(&good).unwrap());
            writer
                .add_directory("lang/", SimpleFileOptions::default())
                .unwrap();
            writer
                .start_file("lang/en.json", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"{}").unwrap();
            writer
                .start_file(EXECUTABLE, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"binary").unwrap();
            writer.finish().unwrap();
        }
        let out = dir.path().join("out");
        extract(&good, &out).expect("unzip");
        assert_eq!(fs::read(out.join(EXECUTABLE)).unwrap(), b"binary");
        assert_eq!(fs::read(out.join("lang").join("en.json")).unwrap(), b"{}");

        let evil = dir.path().join("evil.zip");
        {
            let mut writer = zip::ZipWriter::new(fs::File::create(&evil).unwrap());
            writer
                .start_file("../escape.txt", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(
            extract(&evil, &dir.path().join("evil")),
            Err(UpdateError::Archive)
        );
        assert!(!dir.path().join("escape.txt").exists());
    }
}

//! SealSuite's own updates: GitHub Releases for Stable and Beta, Build Test artifacts for Alpha.
//! The managed apps' updates are a separate module; nothing here touches them.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

const REPOSITORY: &str = "mkiera/SealSuite";
const RELEASES_URL: &str = "https://api.github.com/repos/mkiera/SealSuite/releases?per_page=100";
const ALPHA_RUNS_URL: &str = "https://api.github.com/repos/mkiera/SealSuite/actions/workflows/build-test.yml/runs?status=success&per_page=50";
const BRANCHES_URL: &str = "https://api.github.com/repos/mkiera/SealSuite/branches?per_page=100";
/// GitHub's own artifact download needs a token even on a public repository; nightly.link
/// serves the same zip anonymously.
const NIGHTLY_LINK_URL: &str = "https://nightly.link/mkiera/SealSuite/actions/runs";

/// build-release.yml uploads exactly this name, and build-test.yml uploads it inside this artifact.
pub const INSTALLER_NAME: &str = "SealSuite-Setup.exe";
pub const ALPHA_ARTIFACT: &str = "SealSuite-Setup";
const NOTES_END: &str = "<!-- app-notes-end -->";
pub const PROGRESS_EVENT: &str = "self-update-progress";

/// Each alpha row costs nothing extra (the artifact name is constant), but the list stays short.
const ALPHA_MAX_BRANCHES: usize = 8;
/// The installer is a few MB; this ceiling only stops a hostile or corrupt zip from filling the disk.
const ALPHA_INSTALLER_LIMIT: u64 = 128 * 1024 * 1024;
const LIST_CACHE_TTL: Duration = Duration::from_secs(300);
const ALLOWED_HOSTS: [&str; 5] = [
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "nightly.link",
];

const ARTIFACT_GONE: &str = "That build's installer is no longer downloadable. CI keeps artifacts for 30 days.";

/// Stored as strings so adding a channel never reinterprets old settings. Shown as Stable, Beta, Alpha.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Prerelease,
    Alpha,
}

impl Channel {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "stable" => Some(Self::Stable),
            "prerelease" | "beta" => Some(Self::Prerelease),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Update,
    Downgrade,
    Reinstall,
    Install,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub version: String,
    pub sha: Option<String>,
    pub branch: Option<String>,
    pub run_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub release_url: String,
    pub download_url: String,
    pub asset_name: String,
    pub size_bytes: u64,
    pub notes: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlphaBuild {
    pub run_id: u64,
    pub run_number: u64,
    pub branch: String,
    pub sha: String,
    pub created_at: Option<String>,
    pub run_url: String,
    pub download_url: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdateSettings {
    pub channel: Channel,
    pub auto_check: bool,
}

impl Default for SelfUpdateSettings {
    fn default() -> Self {
        Self { channel: Channel::Stable, auto_check: true }
    }
}

#[derive(Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize, Clone)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize, Clone)]
struct GhRunList {
    #[serde(default)]
    workflow_runs: Vec<GhRun>,
}

#[derive(Deserialize, Clone)]
struct GhRun {
    id: u64,
    #[serde(default)]
    head_branch: Option<String>,
    #[serde(default)]
    head_sha: String,
    #[serde(default)]
    run_number: u64,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
}

#[derive(Deserialize, Clone)]
struct GhBranch {
    name: String,
}

// ---------------------------------------------------------------- build identity

/// CI sets these while cargo compiles; a local build has none, so it never matches an alpha row.
pub fn build_info(app: &tauri::AppHandle) -> BuildInfo {
    let branch = (option_env!("GITHUB_REF_TYPE") == Some("branch"))
        .then(|| option_env!("GITHUB_REF_NAME").map(str::to_string))
        .flatten();
    BuildInfo {
        version: app.package_info().version.to_string(),
        sha: option_env!("GITHUB_SHA").map(str::to_string),
        branch,
        run_id: option_env!("GITHUB_RUN_ID").and_then(|id| id.parse().ok()),
    }
}

// ---------------------------------------------------------------- version selection

pub fn action_for(candidate: &semver::Version, current: Option<&semver::Version>) -> Action {
    match current.map(|current| candidate.cmp_precedence(current)) {
        None => Action::Install,
        Some(std::cmp::Ordering::Greater) => Action::Update,
        Some(std::cmp::Ordering::Less) => Action::Downgrade,
        Some(std::cmp::Ordering::Equal) => Action::Reinstall,
    }
}

fn safe_file_name(name: &str) -> bool {
    name.len() <= 128
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Exact host match: a suffix comparison would let `evilgithub.com` through.
fn allowed_url(raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https" && url.host_str().is_some_and(|host| ALLOWED_HOSTS.contains(&host))
}

fn release_notes(body: Option<&str>) -> String {
    let body = body.unwrap_or_default().replace("\r\n", "\n");
    body.split(NOTES_END).next().unwrap_or_default().trim().to_string()
}

/// Every installable release on the channel, newest first, older ones included.
fn releases_for_channel(
    releases: Vec<GhRelease>,
    channel: Channel,
    current: Option<&semver::Version>,
) -> Vec<ReleaseInfo> {
    let mut listed: Vec<(semver::Version, ReleaseInfo)> = Vec::new();
    for release in releases {
        if release.draft {
            continue;
        }
        let Ok(version) = semver::Version::parse(release.tag_name.trim_start_matches('v')) else {
            continue;
        };
        let prerelease = release.prerelease || !version.pre.is_empty();
        if prerelease && channel != Channel::Prerelease {
            continue;
        }
        // A release whose upload has not finished has a tag and no asset, so it is skipped.
        let Some(asset) = release.assets.iter().find(|asset| {
            asset.name.eq_ignore_ascii_case(INSTALLER_NAME)
                && safe_file_name(&asset.name)
                && allowed_url(&asset.browser_download_url)
                && asset.size > 0
        }) else {
            continue;
        };
        listed.push((
            version.clone(),
            ReleaseInfo {
                version: version.to_string(),
                tag_name: release.tag_name.clone(),
                published_at: release.published_at.clone(),
                prerelease,
                release_url: release.html_url.clone(),
                download_url: asset.browser_download_url.clone(),
                asset_name: asset.name.clone(),
                size_bytes: asset.size,
                notes: release_notes(release.body.as_deref()),
                action: action_for(&version, current),
            },
        ));
    }
    // GitHub's order is by creation date, not by version.
    listed.sort_by(|(a, _), (b, _)| b.cmp_precedence(a));
    listed.into_iter().map(|(_, info)| info).collect()
}

/// The automatic offer: the highest release strictly newer than the running build. Alpha builds
/// are installed by hand only, and a running version that does not parse gets no offer.
fn pick_update(
    releases: Vec<GhRelease>,
    current: Option<&semver::Version>,
    channel: Channel,
) -> Option<ReleaseInfo> {
    if channel == Channel::Alpha {
        return None;
    }
    current?;
    releases_for_channel(releases, channel, current)
        .into_iter()
        .find(|release| release.action == Action::Update)
}

fn alpha_builds(runs: Vec<GhRun>, live_branches: Option<&[String]>, running_sha: Option<&str>) -> Vec<AlphaBuild> {
    let live: Option<Vec<String>> = live_branches
        .map(|names| names.iter().map(|name| name.to_lowercase()).collect::<Vec<_>>())
        .filter(|names| !names.is_empty());

    let mut newest: Vec<GhRun> = Vec::new();
    for run in runs {
        if run.id == 0 || run.conclusion.as_deref().is_some_and(|c| c != "success") {
            continue;
        }
        let Some(branch) = run.head_branch.clone().filter(|name| !name.trim().is_empty()) else {
            continue;
        };
        let key = branch.to_lowercase();
        if live.as_ref().is_some_and(|live| !live.contains(&key)) {
            continue;
        }
        match newest.iter().position(|kept| kept.head_branch.as_deref().map(str::to_lowercase) == Some(key.clone())) {
            Some(index) => {
                let kept = &newest[index];
                if (run.run_number, run.id) > (kept.run_number, kept.id) {
                    newest[index] = run;
                }
            }
            None => newest.push(run),
        }
    }
    newest.sort_by_key(|run| std::cmp::Reverse((run.run_number, run.id)));

    let running = running_sha.map(short_sha).filter(|sha| sha.len() == 7);
    newest
        .into_iter()
        .take(ALPHA_MAX_BRANCHES)
        .map(|run| {
            let sha = short_sha(&run.head_sha);
            AlphaBuild {
                run_id: run.id,
                run_number: run.run_number,
                branch: run.head_branch.unwrap_or_default(),
                is_current: running.as_deref() == Some(sha.as_str()),
                sha,
                created_at: run.created_at,
                run_url: format!("https://github.com/{REPOSITORY}/actions/runs/{}", run.id),
                // Only the numeric run ID and the constant artifact name: never the branch name.
                download_url: format!("{NIGHTLY_LINK_URL}/{}/{ALPHA_ARTIFACT}.zip", run.id),
            }
        })
        .collect()
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect::<String>().to_ascii_lowercase()
}

// ---------------------------------------------------------------- settings

/// Unknown keys survive a save, so a downgraded build keeps settings a newer build wrote.
fn load_settings_from(path: &Path) -> SelfUpdateSettings {
    let mut settings = SelfUpdateSettings::default();
    let Some(fields) = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
    else {
        return settings;
    };
    if let Some(channel) = fields.get("channel").and_then(|v| v.as_str()).and_then(Channel::parse) {
        settings.channel = channel;
    }
    if let Some(auto_check) = fields.get("autoCheck").and_then(|v| v.as_bool()) {
        settings.auto_check = auto_check;
    }
    settings
}

fn save_settings_to(path: &Path, settings: &SelfUpdateSettings) -> Result<(), String> {
    let mut fields = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let known = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    if let Some(known) = known.as_object() {
        for (key, value) in known {
            fields.insert(key.clone(), value.clone());
        }
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create the settings folder: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(fields)).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| format!("Could not save the update settings: {e}"))?;
    fs::rename(&temp, path).map_err(|e| format!("Could not save the update settings: {e}"))
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("self-update.json"))
        .map_err(|e| format!("Could not find the settings folder: {e}"))
}

fn load_settings(app: &tauri::AppHandle) -> SelfUpdateSettings {
    settings_path(app).map(|path| load_settings_from(&path)).unwrap_or_default()
}

fn save_settings(app: &tauri::AppHandle, settings: &SelfUpdateSettings) -> Result<(), String> {
    save_settings_to(&settings_path(app)?, settings)
}

// ---------------------------------------------------------------- busy guard

/// Set while SealSuite does something an exit would break, such as installing one of the apps.
static BUSY: Mutex<Option<String>> = Mutex::new(None);

/// Not called yet: the app-install module sets it around each install.
#[allow(dead_code)]
/// `Some(reason)` blocks self-updates and names the reason to the user; `None` clears it.
pub fn set_busy(reason: Option<String>) {
    *BUSY.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = reason;
}

fn refuse_while_busy() -> Result<(), String> {
    match BUSY.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref() {
        Some(reason) => Err(format!("{reason} Let it finish, then update SealSuite.")),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------- network

fn user_agent(app: &tauri::AppHandle) -> String {
    format!("SealSuite/{} (+https://github.com/{REPOSITORY})", app.package_info().version)
}

fn github_error(status: reqwest::StatusCode) -> String {
    if status.as_u16() == 403 || status.as_u16() == 429 {
        "GitHub is limiting update checks right now. Try again in an hour.".to_string()
    } else {
        format!("GitHub answered the update check with HTTP {status}.")
    }
}

async fn github_json<T: serde::de::DeserializeOwned>(app: &tauri::AppHandle, url: &str) -> Result<T, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Could not start the update check: {e}"))?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent(app))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Could not reach GitHub: {e}"))?;
    if !response.status().is_success() {
        return Err(github_error(response.status()));
    }
    response.json().await.map_err(|e| format!("GitHub's answer could not be read: {e}"))
}

type Cached<T> = Mutex<Option<(T, Instant)>>;
static RELEASES_CACHE: Cached<Vec<GhRelease>> = Mutex::new(None);
static ALPHA_CACHE: Cached<Vec<AlphaBuild>> = Mutex::new(None);
static OFFER: Mutex<Option<(Channel, ReleaseInfo)>> = Mutex::new(None);

fn cached<T: Clone>(cache: &Cached<T>) -> Option<T> {
    let cache = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (value, at) = cache.as_ref()?;
    (at.elapsed() < LIST_CACHE_TTL).then(|| value.clone())
}

fn store<T>(cache: &Cached<T>, value: T) {
    *cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((value, Instant::now()));
}

async fn releases(app: &tauri::AppHandle, refresh: bool) -> Result<Vec<GhRelease>, String> {
    if !refresh {
        if let Some(releases) = cached(&RELEASES_CACHE) {
            return Ok(releases);
        }
    }
    let releases: Vec<GhRelease> = github_json(app, RELEASES_URL).await?;
    store(&RELEASES_CACHE, releases.clone());
    Ok(releases)
}

fn current_version(app: &tauri::AppHandle) -> Option<semver::Version> {
    semver::Version::parse(&app.package_info().version.to_string()).ok()
}

fn channel_or_saved(app: &tauri::AppHandle, channel: Option<String>) -> Result<Channel, String> {
    match channel {
        Some(raw) => Channel::parse(&raw).ok_or_else(|| format!("{raw} is not an update channel.")),
        None => Ok(load_settings(app).channel),
    }
}

// ---------------------------------------------------------------- download and install

fn updates_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|dir| dir.join("updates"))
        .map_err(|e| format!("Could not find the updates folder: {e}"))
}

/// The process that started an installer exits first, so startup clears what it left behind.
pub fn clear_updates_dir(app: &tauri::AppHandle) {
    let Ok(entries) = updates_dir(app).and_then(|dir| fs::read_dir(dir).map_err(|e| e.to_string())) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
    }
}

async fn download(
    app: &tauri::AppHandle,
    url: &str,
    dest: &Path,
    expected: Option<u64>,
    gone: Option<&str>,
) -> Result<(), String> {
    if !allowed_url(url) {
        return Err("That download address is not on GitHub, so SealSuite will not run it.".to_string());
    }
    let client = reqwest::Client::builder()
        // No overall timeout: a slow connection is not an error.
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Could not start the download: {e}"))?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent(app))
        .send()
        .await
        .map_err(|e| format!("Could not reach the download: {e}"))?;
    if !response.status().is_success() {
        if let (404, Some(gone)) = (response.status().as_u16(), gone) {
            return Err(gone.to_string());
        }
        return Err(format!("The download answered with HTTP {}.", response.status()));
    }
    let total = expected.or_else(|| response.content_length()).filter(|total| *total > 0);

    let result = async {
        let mut file = fs::File::create(dest).map_err(|e| format!("Could not write the download: {e}"))?;
        let mut stream = response.bytes_stream();
        let mut written: u64 = 0;
        let mut last = (0.0_f64, Instant::now());
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("The download stopped early: {e}"))?;
            file.write_all(&chunk).map_err(|e| format!("Could not write the download: {e}"))?;
            written += chunk.len() as u64;
            if let Some(total) = total {
                let fraction = (written as f64 / total as f64).min(1.0);
                if fraction - last.0 >= 0.01 || last.1.elapsed() >= Duration::from_millis(100) {
                    last = (fraction, Instant::now());
                    let _ = app.emit(PROGRESS_EVENT, fraction);
                }
            }
        }
        file.flush().map_err(|e| format!("Could not finish writing the download: {e}"))?;
        if written == 0 || total.is_some_and(|total| written != total) {
            return Err(format!(
                "The download stopped early: got {written} of {} bytes. Please try again.",
                total.unwrap_or_default()
            ));
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    let _ = app.emit(PROGRESS_EVENT, 1.0_f64);
    result
}

/// Takes only a root-level entry named like the installer, and writes it to `dest`, never to a
/// path from the zip.
fn extract_installer(zip_path: &Path, dest: &Path, limit: u64) -> Result<(), String> {
    let unreadable = || "That build's download could not be unpacked.".to_string();
    let file = fs::File::open(zip_path).map_err(|_| unreadable())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| unreadable())?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index_raw(i)
                .is_ok_and(|entry| entry.is_file() && entry.name().eq_ignore_ascii_case(INSTALLER_NAME))
        })
        .ok_or_else(|| "That build does not contain SealSuite-Setup.exe.".to_string())?;
    let entry = archive.by_index(index).map_err(|_| unreadable())?;
    let mut out = fs::File::create(dest).map_err(|e| format!("Could not write the installer: {e}"))?;
    let copied = std::io::copy(&mut entry.take(limit + 1), &mut out);
    drop(out);
    let failure = match copied {
        Ok(0) => Some("That build's installer is empty.".to_string()),
        Ok(n) if n > limit => Some("That build's installer is larger than any SealSuite installer should be.".to_string()),
        Ok(_) => None,
        Err(_) => Some(unreadable()),
    };
    match failure {
        Some(message) => {
            let _ = fs::remove_file(dest);
            Err(message)
        }
        None => Ok(()),
    }
}

/// Hands the installer to Windows, watches it start, then exits so it can replace the files.
fn launch_installer(app: &tauri::AppHandle, installer: &Path, dir: &Path) -> Result<(), String> {
    let inside = installer
        .canonicalize()
        .ok()
        .zip(dir.canonicalize().ok())
        .is_some_and(|(file, dir)| file.starts_with(dir));
    let named = installer
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| safe_file_name(name) && name.to_ascii_lowercase().ends_with(&INSTALLER_NAME.to_ascii_lowercase()));
    if !inside || !named {
        return Err("The downloaded installer is not where SealSuite left it, so it was not run.".to_string());
    }
    // Asked again: the download is long enough for an app install to have started since the click.
    refuse_while_busy()?;

    // /SILENT keeps the progress window. Never /DIR or /TASKS, which would reset UsePreviousAppDir
    // and UsePreviousTasks, and never /SUPPRESSMSGBOXES, which hides why an install stopped.
    let mut log = std::ffi::OsString::from("/LOG=");
    log.push(dir.join("update.log"));
    let mut command = std::process::Command::new(installer);
    command.args(["/SILENT", "/CLOSEAPPLICATIONS", "/NORESTARTAPPLICATIONS"]).arg(log).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let mut child = command.spawn().map_err(|e| format!("Could not start the installer: {e}"))?;

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                return Err(format!("The installer stopped with {status}. Its log is in {}.", dir.display()));
            }
            // Exit code 0 this early means its [Run] entry already started the new SealSuite.
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("Could not watch the installer: {e}")),
        }
    }
    app.exit(0);
    Ok(())
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn self_update_build_info(app: tauri::AppHandle) -> BuildInfo {
    build_info(&app)
}

#[tauri::command]
pub fn self_update_get_settings(app: tauri::AppHandle) -> SelfUpdateSettings {
    load_settings(&app)
}

#[tauri::command]
pub fn self_update_set_channel(app: tauri::AppHandle, channel: String) -> Result<(), String> {
    let channel = Channel::parse(&channel).ok_or_else(|| format!("{channel} is not an update channel."))?;
    let mut settings = load_settings(&app);
    settings.channel = channel;
    save_settings(&app, &settings)?;
    // An offer from the old channel no longer applies.
    *OFFER.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    Ok(())
}

#[tauri::command]
pub fn self_update_set_auto_check(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = load_settings(&app);
    settings.auto_check = enabled;
    save_settings(&app, &settings)
}

/// Asks GitHub, ignoring the automatic-check setting: the caller that checks unprompted reads it.
#[tauri::command]
pub async fn self_update_check(app: tauri::AppHandle, channel: Option<String>) -> Result<Option<ReleaseInfo>, String> {
    let channel = channel_or_saved(&app, channel)?;
    let releases = releases(&app, true).await?;
    let offer = pick_update(releases, current_version(&app).as_ref(), channel);
    *OFFER.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = offer.clone().map(|offer| (channel, offer));
    Ok(offer)
}

/// The last check's offer for the saved channel, without a network request.
#[tauri::command]
pub fn self_update_available(app: tauri::AppHandle) -> Option<ReleaseInfo> {
    let channel = load_settings(&app).channel;
    OFFER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .filter(|(offered_on, _)| *offered_on == channel)
        .map(|(_, offer)| offer.clone())
}

#[tauri::command]
pub async fn self_update_list_releases(
    app: tauri::AppHandle,
    channel: Option<String>,
    refresh: bool,
) -> Result<Vec<ReleaseInfo>, String> {
    let channel = channel_or_saved(&app, channel)?;
    if channel == Channel::Alpha {
        return Err("Alpha builds come from CI runs, so ask for the alpha build list instead.".to_string());
    }
    let releases = releases(&app, refresh).await?;
    Ok(releases_for_channel(releases, channel, current_version(&app).as_ref()))
}

#[tauri::command]
pub async fn self_update_list_alpha_builds(app: tauri::AppHandle, refresh: bool) -> Result<Vec<AlphaBuild>, String> {
    let sha = build_info(&app).sha;
    if !refresh {
        if let Some(builds) = cached(&ALPHA_CACHE) {
            return Ok(alpha_rows_marked(builds, sha.as_deref()));
        }
    }
    let runs: GhRunList = github_json(&app, ALPHA_RUNS_URL).await?;
    // A failed branch request keeps the list unfiltered rather than emptying it.
    let branches: Option<Vec<String>> = github_json::<Vec<GhBranch>>(&app, BRANCHES_URL)
        .await
        .ok()
        .map(|branches| branches.into_iter().map(|b| b.name).collect());
    let builds = alpha_builds(runs.workflow_runs, branches.as_deref(), sha.as_deref());
    store(&ALPHA_CACHE, builds.clone());
    Ok(builds)
}

fn alpha_rows_marked(builds: Vec<AlphaBuild>, running_sha: Option<&str>) -> Vec<AlphaBuild> {
    let running = running_sha.map(short_sha);
    builds
        .into_iter()
        .map(|mut build| {
            build.is_current = running.as_deref() == Some(build.sha.as_str());
            build
        })
        .collect()
}

#[tauri::command]
pub async fn self_update_install_release(app: tauri::AppHandle, release: ReleaseInfo) -> Result<(), String> {
    refuse_while_busy()?;
    if !safe_file_name(&release.asset_name) || !release.asset_name.eq_ignore_ascii_case(INSTALLER_NAME) {
        return Err(format!("{} is not SealSuite's installer.", release.asset_name));
    }
    // Without a length to check, a truncated installer cannot be told from a complete one.
    if release.size_bytes == 0 {
        return Err("The release does not list the installer's size, so the download cannot be checked.".to_string());
    }
    let dir = updates_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the updates folder: {e}"))?;
    let dest = dir.join(INSTALLER_NAME);
    download(&app, &release.download_url, &dest, Some(release.size_bytes), None).await?;
    launch_installer(&app, &dest, &dir)
}

#[tauri::command]
pub async fn self_update_install_alpha(app: tauri::AppHandle, build: AlphaBuild) -> Result<(), String> {
    refuse_while_busy()?;
    let url = format!("{NIGHTLY_LINK_URL}/{}/{ALPHA_ARTIFACT}.zip", build.run_id);
    let dir = updates_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the updates folder: {e}"))?;
    let zip = dir.join(format!("alpha-{}.zip", build.run_id));
    let installer = dir.join(format!("alpha-{}-{INSTALLER_NAME}", build.run_id));
    let result = async {
        download(&app, &url, &zip, None, Some(ARTIFACT_GONE)).await?;
        extract_installer(&zip, &installer, ALPHA_INSTALLER_LIMIT)
    }
    .await;
    let _ = fs::remove_file(&zip);
    if let Err(e) = result {
        let _ = fs::remove_file(&installer);
        return Err(e);
    }
    launch_installer(&app, &installer, &dir)
}

#[cfg(test)]
#[path = "selfupdate_tests.rs"]
mod tests;

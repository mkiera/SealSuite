//! Commands behind the main window: list, check, install or update, open, and the GitHub link.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::apps::{self, CatalogApp, Installed};
use crate::releases::{self, Offer};

pub const PROGRESS_EVENT: &str = "install-progress";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    id: &'static str,
    name: &'static str,
    role: &'static str,
    repo: &'static str,
    installed: Option<Installed>,
    /// For an installed app the update on its channel; for a missing app the latest stable release.
    offer: Option<Offer>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: &'static str,
    phase: &'static str,
    percent: u8,
}

fn view(app: &'static CatalogApp, offer: Option<Offer>, error: Option<String>) -> AppView {
    AppView { id: app.id, name: app.name, role: app.role, repo: app.repo, installed: apps::detect(app), offer, error }
}

fn lookup(id: &str) -> Result<&'static CatalogApp, String> {
    apps::find(id).ok_or_else(|| format!("{id} is not a SealSuite app."))
}

async fn offer_for(app: &'static CatalogApp, installed: Option<&Installed>) -> Result<Option<Offer>, String> {
    let releases = releases::fetch(app.repo).await?;
    let channel = installed.map_or(apps::Channel::Stable, |i| i.channel);
    Ok(releases::pick(&releases, installed.map(|i| i.version.as_str()), channel, app.asset))
}

#[tauri::command]
pub fn list_apps() -> Vec<AppView> {
    apps::CATALOG.iter().map(|app| view(app, None, None)).collect()
}

#[tauri::command]
pub async fn check_updates() -> Vec<AppView> {
    let checks = apps::CATALOG.iter().map(|app| async move {
        let installed = apps::detect(app);
        match offer_for(app, installed.as_ref()).await {
            Ok(offer) => view(app, offer, None),
            Err(error) => view(app, None, Some(error)),
        }
    });
    futures_util::future::join_all(checks).await
}

#[tauri::command]
pub async fn install_app(handle: AppHandle, id: String) -> Result<AppView, String> {
    let app = lookup(&id)?;
    let installed = apps::detect(app);
    let offer = offer_for(app, installed.as_ref())
        .await?
        .ok_or_else(|| format!("{} is already up to date.", app.name))?;

    let dir = std::env::temp_dir().join("SealSuite");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not prepare the download folder: {e}"))?;
    let installer = dir.join(app.asset);
    download(&handle, app.id, &offer, &installer).await?;

    let _ = handle.emit(PROGRESS_EVENT, Progress { id: app.id, phase: "install", percent: 100 });
    let result = run_installer(installer.clone(), dir).await;
    let _ = std::fs::remove_file(&installer);
    result?;
    Ok(view(app, None, None))
}

#[tauri::command]
pub fn open_app(id: String) -> Result<(), String> {
    let app = lookup(&id)?;
    let installed = apps::detect(app).ok_or_else(|| format!("{} is not installed.", app.name))?;
    std::process::Command::new(installed.location.join(app.exe))
        .current_dir(&installed.location)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {}: {e}", app.name))
}

#[tauri::command]
pub fn open_repo(id: String) -> Result<(), String> {
    let app = lookup(&id)?;
    tauri_plugin_opener::open_url(format!("https://github.com/{}", app.repo), None::<&str>)
        .map_err(|e| format!("Could not open the browser: {e}"))
}

async fn download(handle: &AppHandle, id: &'static str, offer: &Offer, dest: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        // No overall timeout: the body is a whole installer and a slow connection is not an error.
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Could not start the download: {e}"))?;
    let response = client
        .get(&offer.url)
        .header(reqwest::header::USER_AGENT, releases::user_agent())
        .send()
        .await
        .map_err(|_| "Could not reach the download.".to_string())?;
    if !response.status().is_success() {
        return Err(format!("The download answered with HTTP {}.", response.status().as_u16()));
    }
    let expected = Some(offer.size).filter(|s| *s > 0).or_else(|| response.content_length());
    let mut file = std::fs::File::create(dest).map_err(|e| format!("Could not write the download: {e}"))?;
    let mut stream = response.bytes_stream();
    let mut written: u64 = 0;
    let mut last = Instant::now();
    let fail = |message: String| {
        let _ = std::fs::remove_file(dest);
        message
    };
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| fail(format!("The download stopped early: {e}")))?;
        file.write_all(&chunk).map_err(|e| fail(format!("Could not write the download: {e}")))?;
        written += chunk.len() as u64;
        if let Some(total) = expected {
            if last.elapsed() > Duration::from_millis(150) {
                last = Instant::now();
                let percent = (written.saturating_mul(100) / total.max(1)).min(99) as u8;
                let _ = handle.emit(PROGRESS_EVENT, Progress { id, phase: "download", percent });
            }
        }
    }
    drop(file);
    if expected.is_some_and(|total| written != total) {
        return Err(fail("The download stopped early.".into()));
    }
    Ok(())
}

/// Runs an app's own Inno Setup installer and waits for it.
async fn run_installer(installer: PathBuf, cwd: PathBuf) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        // The same switches the apps' own updaters pass: /SILENT keeps the progress window,
        // /CLOSEAPPLICATIONS closes a running copy, and the installer's [Run] entry owns any relaunch.
        let mut command = std::process::Command::new(&installer);
        command.args(["/SILENT", "/CLOSEAPPLICATIONS", "/NORESTARTAPPLICATIONS"]).current_dir(cwd);
        let status = command.status().map_err(|e| format!("Could not start the installer: {e}"))?;
        match status.code() {
            Some(0) => Ok(()),
            Some(2) | Some(5) => Err("The installer was cancelled.".into()),
            Some(code) => Err(format!("The installer stopped with code {code}.")),
            None => Err("The installer stopped unexpectedly.".into()),
        }
    })
    .await
    .map_err(|e| format!("The installer could not be watched: {e}"))?
}

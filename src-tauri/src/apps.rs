//! The apps SealSuite manages, and what is installed on this PC.
//!
//! SealSuite only reads what each app's own installer and settings left behind (Design rules in the
//! README): the Inno Setup uninstall entry under HKCU, and the app's update-channel setting.

use std::path::PathBuf;

use serde::Serialize;

pub struct CatalogApp {
    pub id: &'static str,
    pub name: &'static str,
    pub role: &'static str,
    pub repo: &'static str,
    /// Inno Setup AppId from the app's installer.iss; the uninstall key is `<AppId>_is1`.
    pub app_id: &'static str,
    pub exe: &'static str,
    pub asset: &'static str,
    channel_file: ChannelFile,
}

/// Where an app keeps its update channel, relative to a Windows known folder.
struct ChannelFile {
    base: &'static str,
    path: &'static str,
    key: &'static str,
}

pub const CATALOG: [CatalogApp; 3] = [
    CatalogApp {
        id: "finfetcher",
        name: "FinFetcher",
        role: "Downloading",
        repo: "mkiera/FinFetcher",
        app_id: "{6A29BB89-8456-4003-9223-9B44A0F66834}",
        exe: "FinFetcher.exe",
        asset: "FinFetcher-Setup.exe",
        channel_file: ChannelFile { base: "APPDATA", path: r"FinFetcher\config.json", key: "update_channel" },
    },
    CatalogApp {
        id: "flipperclipper",
        name: "FlipperClipper",
        role: "Clipping",
        repo: "mkiera/FlipperClipper",
        app_id: "{A67FDBB5-CF45-489D-8A41-0E7576A446F1}",
        exe: "FlipperClipper.exe",
        asset: "FlipperClipper-Setup.exe",
        channel_file: ChannelFile { base: "APPDATA", path: r"com.mkiera.flipperclipper\settings.json", key: "updateChannel" },
    },
    CatalogApp {
        id: "blubberbound",
        name: "BlubberBound",
        role: "Compressing",
        repo: "mkiera/BlubberBound",
        app_id: "{596690BB-D4AC-4C45-A35A-5C39B158156C}",
        exe: "BlubberBound.exe",
        asset: "BlubberBound-Setup.exe",
        channel_file: ChannelFile { base: "LOCALAPPDATA", path: r"BlubberBound\updates.json", key: "channel" },
    },
];

pub fn find(id: &str) -> Option<&'static CatalogApp> {
    CATALOG.iter().find(|app| app.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Prerelease,
    Alpha,
}

#[derive(Clone, Debug, Serialize)]
pub struct Installed {
    pub version: String,
    pub channel: Channel,
    #[serde(skip)]
    pub location: PathBuf,
}

/// The channel an app's settings name. Anything missing or unknown is Stable, as in each app.
pub fn channel_from_settings(settings: &str, key: &str) -> Channel {
    let value: Option<serde_json::Value> = serde_json::from_str(settings).ok();
    match value.as_ref().and_then(|v| v.get(key)).and_then(|v| v.as_str()) {
        Some("prerelease") => Channel::Prerelease,
        Some("alpha") => Channel::Alpha,
        _ => Channel::Stable,
    }
}

fn read_channel(app: &CatalogApp) -> Channel {
    let file = &app.channel_file;
    std::env::var_os(file.base)
        .map(|base| PathBuf::from(base).join(file.path))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| channel_from_settings(&text, file.key))
        .unwrap_or(Channel::Stable)
}

pub fn detect(app: &CatalogApp) -> Option<Installed> {
    let (version, location) = registry_entry(app.app_id)?;
    if !location.join(app.exe).is_file() {
        return None;
    }
    Some(Installed { version, channel: read_channel(app), location })
}

#[cfg(windows)]
fn registry_entry(app_id: &str) -> Option<(String, PathBuf)> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_id}_is1"))
        .ok()?;
    let version: String = key.get_value("DisplayVersion").ok()?;
    let location: String = key.get_value("InstallLocation").ok()?;
    Some((version, PathBuf::from(location)))
}

#[cfg(not(windows))]
fn registry_entry(_app_id: &str) -> Option<(String, PathBuf)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_app_reads_its_own_channel_key() {
        assert_eq!(channel_from_settings(r#"{"update_channel":"prerelease"}"#, "update_channel"), Channel::Prerelease);
        assert_eq!(channel_from_settings(r#"{"updateChannel":"prerelease"}"#, "updateChannel"), Channel::Prerelease);
        assert_eq!(channel_from_settings(r#"{"channel":"alpha","automatic":true}"#, "channel"), Channel::Alpha);
    }

    #[test]
    fn missing_or_unknown_channels_are_stable() {
        assert_eq!(channel_from_settings("{}", "channel"), Channel::Stable);
        assert_eq!(channel_from_settings(r#"{"channel":"nightly"}"#, "channel"), Channel::Stable);
        assert_eq!(channel_from_settings("not json", "channel"), Channel::Stable);
    }

    #[test]
    fn catalog_ids_and_assets_are_consistent() {
        for app in &CATALOG {
            assert_eq!(app.repo, format!("mkiera/{}", app.name));
            assert_eq!(app.exe, format!("{}.exe", app.name));
            assert_eq!(app.asset, format!("{}-Setup.exe", app.name));
            assert!(app.app_id.starts_with('{') && app.app_id.ends_with('}'));
        }
    }
}

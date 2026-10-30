//! Which GitHub release to offer for an app.

use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::apps::Channel;

#[derive(Debug, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub version: String,
    #[serde(skip)]
    pub url: String,
    #[serde(skip)]
    pub size: u64,
}

pub fn parse_version(raw: &str) -> Option<Version> {
    Version::parse(raw.trim().trim_start_matches('v')).ok()
}

/// The newest installable release for `channel`, or `None` when `installed` is already as new.
/// Alpha builds are Build Test artifacts, not releases, so an Alpha install is never offered one.
/// Tags that are not semver (such as FinFetcher's `v1.2.9f-flipperclipper`) are skipped.
pub fn pick(releases: &[GhRelease], installed: Option<&str>, channel: Channel, asset: &str) -> Option<Offer> {
    if installed.is_some() && channel == Channel::Alpha {
        return None;
    }
    let (version, release) = releases
        .iter()
        .filter(|r| !r.draft && (channel == Channel::Prerelease || !r.prerelease))
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(_, r)| r.assets.iter().any(|a| a.name == asset))
        .max_by(|(a, _), (b, _)| a.cmp(b))?;
    if let Some(raw) = installed {
        // An installed version that does not parse cannot be compared, so nothing is offered.
        let Some(current) = parse_version(raw) else { return None };
        if version <= current {
            return None;
        }
        // An alpha is a branch build, usually ahead of the release with the same number
        // (1.2.0-alpha.32 can hold commits made after v1.2.0). Only a higher release number is newer.
        if is_alpha_build(&current) && core(&version) <= core(&current) {
            return None;
        }
    }
    let file = release.assets.iter().find(|a| a.name == asset)?;
    Some(Offer { version: version.to_string(), url: file.browser_download_url.clone(), size: file.size })
}

fn is_alpha_build(version: &Version) -> bool {
    version.pre.as_str().starts_with("alpha")
}

fn core(version: &Version) -> (u64, u64, u64) {
    (version.major, version.minor, version.patch)
}

pub fn user_agent() -> String {
    format!("SealSuite/{} (+https://github.com/mkiera/SealSuite)", env!("CARGO_PKG_VERSION"))
}

pub async fn fetch(repo: &str) -> Result<Vec<GhRelease>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Could not start the update check: {e}"))?;
    let response = client
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=30"))
        .header(reqwest::header::USER_AGENT, user_agent())
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Could not reach GitHub.".to_string())?;
    if response.status() == reqwest::StatusCode::FORBIDDEN || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("GitHub is limiting update checks. Try again in an hour.".into());
    }
    if !response.status().is_success() {
        return Err(format!("GitHub answered with HTTP {}.", response.status().as_u16()));
    }
    response.json().await.map_err(|_| "GitHub sent a release list SealSuite could not read.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSET: &str = "App-Setup.exe";

    fn release(tag: &str, prerelease: bool) -> GhRelease {
        GhRelease {
            tag_name: tag.into(),
            prerelease,
            draft: false,
            assets: vec![GhAsset { name: ASSET.into(), browser_download_url: format!("https://example.test/{tag}/{ASSET}"), size: 10 }],
        }
    }

    fn feed() -> Vec<GhRelease> {
        vec![
            release("v1.3.0-beta.2", true),
            release("v1.3.0-beta.1", true),
            release("v1.2.9", false),
            release("v1.2.9f-flipperclipper", true),
            release("v1.2.8", false),
        ]
    }

    fn offered(installed: Option<&str>, channel: Channel) -> Option<String> {
        pick(&feed(), installed, channel, ASSET).map(|o| o.version)
    }

    #[test]
    fn stable_skips_prereleases() {
        assert_eq!(offered(Some("1.2.8"), Channel::Stable).as_deref(), Some("1.2.9"));
        assert_eq!(offered(Some("1.2.9"), Channel::Stable), None);
    }

    #[test]
    fn beta_offers_the_newest_prerelease() {
        assert_eq!(offered(Some("1.2.9"), Channel::Prerelease).as_deref(), Some("1.3.0-beta.2"));
        assert_eq!(offered(Some("1.3.0-beta.1"), Channel::Prerelease).as_deref(), Some("1.3.0-beta.2"));
        assert_eq!(offered(Some("1.3.0-beta.2"), Channel::Prerelease), None);
    }

    #[test]
    fn a_beta_user_on_stable_is_not_downgraded() {
        assert_eq!(offered(Some("1.3.0-beta.2"), Channel::Stable), None);
    }

    #[test]
    fn alpha_installs_and_unreadable_versions_get_no_offer() {
        assert_eq!(offered(Some("1.2.8"), Channel::Alpha), None);
        assert_eq!(offered(Some("1.2.8-custom build"), Channel::Stable), None);
    }

    #[test]
    fn an_alpha_build_is_offered_only_a_higher_release_number() {
        // 1.2.9-alpha.32 may hold commits made after v1.2.9, so v1.2.9 is not an update.
        assert_eq!(offered(Some("1.2.9-alpha.32"), Channel::Stable), None);
        assert_eq!(offered(Some("1.2.8-alpha.4"), Channel::Stable).as_deref(), Some("1.2.9"));
        assert_eq!(offered(Some("1.3.0-alpha.7"), Channel::Prerelease), None);
        assert_eq!(offered(Some("1.2.9-alpha.7"), Channel::Prerelease).as_deref(), Some("1.3.0-beta.2"));
    }

    #[test]
    fn a_missing_app_is_offered_the_latest_stable() {
        assert_eq!(offered(None, Channel::Stable).as_deref(), Some("1.2.9"));
    }

    #[test]
    fn releases_without_the_installer_and_drafts_are_skipped() {
        let mut releases = feed();
        releases.insert(0, GhRelease { tag_name: "v1.4.0".into(), prerelease: false, draft: false, assets: vec![] });
        releases.insert(0, GhRelease { draft: true, ..release("v1.5.0", false) });
        assert_eq!(pick(&releases, Some("1.2.8"), Channel::Stable, ASSET).map(|o| o.version).as_deref(), Some("1.2.9"));
    }

    #[test]
    fn the_github_feed_parses() {
        let json = r#"[{"tag_name":"v1.1.1","prerelease":false,"draft":false,"assets":[{"name":"BlubberBound-Setup.exe","browser_download_url":"https://github.com/x","size":5,"extra":1}],"body":"notes"}]"#;
        let releases: Vec<GhRelease> = serde_json::from_str(json).unwrap();
        assert_eq!(pick(&releases, None, Channel::Stable, "BlubberBound-Setup.exe").unwrap().version, "1.1.1");
    }
}

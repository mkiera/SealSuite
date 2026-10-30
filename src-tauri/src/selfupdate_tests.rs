use super::*;

fn version(raw: &str) -> semver::Version {
    semver::Version::parse(raw).expect("test version should parse")
}

fn releases(json: &str) -> Vec<GhRelease> {
    serde_json::from_str(json).expect("test payload should deserialise")
}

fn release(tag: &str, prerelease: bool) -> String {
    format!(
        r#"{{"html_url":"https://github.com/mkiera/SealSuite/releases/tag/{tag}","tag_name":"{tag}","draft":false,"prerelease":{prerelease},
           "body":"- Change in {tag}.\r\n\r\n<!-- app-notes-end -->\r\n\r\n### Installation",
           "assets":[{{"name":"SealSuite-Setup.exe","size":10,"browser_download_url":"https://github.com/mkiera/SealSuite/releases/download/{tag}/SealSuite-Setup.exe"}}]}}"#
    )
}

fn feed(items: &[(&str, bool)]) -> Vec<GhRelease> {
    let items: Vec<String> = items.iter().map(|(tag, pre)| release(tag, *pre)).collect();
    releases(&format!("[{}]", items.join(",")))
}

fn tags(listed: &[ReleaseInfo]) -> Vec<&str> {
    listed.iter().map(|r| r.tag_name.as_str()).collect()
}

/// Field names and types as api.github.com returns them, so a payload serde cannot read fails here.
const REAL_PAYLOAD: &str = r#"[
  {
    "html_url": "https://github.com/mkiera/FlipperClipper/releases/tag/v1.2.0",
    "tag_name": "v1.2.0",
    "draft": false,
    "prerelease": false,
    "published_at": "2026-10-20T19:02:11Z",
    "body": "- Notes.",
    "assets": [
      {
        "name": "SealSuite-Setup.exe",
        "size": 3309048,
        "browser_download_url": "https://github.com/mkiera/SealSuite/releases/download/v1.2.0/SealSuite-Setup.exe"
      }
    ]
  }
]"#;

#[test]
fn channel_strings_parse_and_unknown_ones_are_refused() {
    assert_eq!(Channel::parse("stable"), Some(Channel::Stable));
    assert_eq!(Channel::parse("prerelease"), Some(Channel::Prerelease));
    assert_eq!(Channel::parse("beta"), Some(Channel::Prerelease));
    assert_eq!(Channel::parse("alpha"), Some(Channel::Alpha));
    assert_eq!(Channel::parse("nightly"), None);
    assert_eq!(serde_json::to_string(&Channel::Prerelease).unwrap(), "\"prerelease\"");
}

#[test]
fn the_real_payload_shape_is_offered() {
    let offer = pick_update(releases(REAL_PAYLOAD), Some(&version("1.0.0")), Channel::Stable).expect("1.2.0 is newer");
    assert_eq!(offer.version, "1.2.0");
    assert_eq!(offer.size_bytes, 3309048);
    assert_eq!(offer.action, Action::Update);
}

#[test]
fn stable_skips_prereleases_and_beta_keeps_both() {
    let releases = || feed(&[("v1.1.0-beta.2", true), ("v1.0.0", false), ("v1.1.0-beta.1", true), ("v0.9.0", false)]);
    assert_eq!(tags(&releases_for_channel(releases(), Channel::Stable, None)), ["v1.0.0", "v0.9.0"]);
    assert_eq!(
        tags(&releases_for_channel(releases(), Channel::Prerelease, None)),
        ["v1.1.0-beta.2", "v1.1.0-beta.1", "v1.0.0", "v0.9.0"]
    );
}

#[test]
fn a_prerelease_tag_is_kept_off_stable_even_without_the_github_flag() {
    let listed = releases_for_channel(feed(&[("v1.1.0-beta.1", false)]), Channel::Stable, None);
    assert!(listed.is_empty());
}

#[test]
fn the_alpha_channel_never_gets_an_automatic_offer() {
    assert!(pick_update(feed(&[("v9.0.0", false)]), Some(&version("1.0.0")), Channel::Alpha).is_none());
}

#[test]
fn the_newest_release_wins_whatever_order_github_lists_them_in() {
    let offer = pick_update(feed(&[("v1.3.0", false), ("v1.10.0", false), ("v1.9.0", false)]), Some(&version("1.0.0")), Channel::Stable);
    assert_eq!(offer.unwrap().version, "1.10.0");
}

#[test]
fn the_stable_release_replaces_its_own_beta_on_both_channels() {
    for channel in [Channel::Stable, Channel::Prerelease] {
        let offer = pick_update(feed(&[("v1.1.0", false), ("v1.1.0-beta.2", true)]), Some(&version("1.1.0-beta.2")), channel);
        assert_eq!(offer.expect("1.1.0 outranks 1.1.0-beta.2").version, "1.1.0");
    }
}

#[test]
fn beta_numbers_compare_numerically() {
    let offer = pick_update(feed(&[("v1.1.0-beta.2", true), ("v1.1.0-beta.10", true)]), Some(&version("1.1.0-beta.9")), Channel::Prerelease);
    assert_eq!(offer.unwrap().version, "1.1.0-beta.10");
}

#[test]
fn the_running_version_and_older_ones_are_not_offered() {
    assert!(pick_update(feed(&[("v1.0.0", false)]), Some(&version("1.0.0")), Channel::Stable).is_none());
    assert!(pick_update(feed(&[("v1.0.0", false)]), Some(&version("1.0.1-alpha.3")), Channel::Stable).is_none());
}

#[test]
fn an_alpha_build_is_offered_the_release_of_its_core() {
    let offer = pick_update(feed(&[("v1.0.1", false)]), Some(&version("1.0.1-alpha.3")), Channel::Stable);
    assert_eq!(offer.unwrap().version, "1.0.1");
}

#[test]
fn an_unparseable_running_version_gets_no_offer_and_install_labels() {
    assert!(pick_update(feed(&[("v1.0.0", false)]), None, Channel::Stable).is_none());
    assert_eq!(releases_for_channel(feed(&[("v1.0.0", false)]), Channel::Stable, None)[0].action, Action::Install);
}

#[test]
fn action_labels_compare_by_semver_precedence() {
    let current = version("1.2.0");
    assert_eq!(action_for(&version("1.3.0"), Some(&current)), Action::Update);
    assert_eq!(action_for(&version("1.2.0-beta.3"), Some(&current)), Action::Downgrade);
    assert_eq!(action_for(&version("1.2.0"), Some(&current)), Action::Reinstall);
    assert_eq!(action_for(&version("1.2.0+build.7"), Some(&current)), Action::Reinstall);
    assert_eq!(action_for(&version("1.2.0"), None), Action::Install);
}

#[test]
fn drafts_bad_tags_and_unsafe_assets_are_skipped() {
    let payload = r#"[
      {"html_url":"h","tag_name":"v9.0.0","draft":true,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":1,"browser_download_url":"https://github.com/a"}]},
      {"html_url":"h","tag_name":"nightly","draft":false,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":1,"browser_download_url":"https://github.com/a"}]},
      {"html_url":"h","tag_name":"v8.0.0","draft":false,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":0,"browser_download_url":"https://github.com/a"}]},
      {"html_url":"h","tag_name":"v7.0.0","draft":false,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":1,"browser_download_url":"http://github.com/a"}]},
      {"html_url":"h","tag_name":"v6.0.0","draft":false,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":1,"browser_download_url":"https://evilgithub.com/a"}]},
      {"html_url":"h","tag_name":"v5.0.0","draft":false,"prerelease":false,
       "assets":[{"name":"FlipperClipper-Setup.exe","size":1,"browser_download_url":"https://github.com/a"}]},
      {"html_url":"h","tag_name":"v4.0.0","draft":false,"prerelease":false,"assets":[]},
      {"html_url":"h","tag_name":"v2.0.0","draft":false,"prerelease":false,
       "assets":[{"name":"SealSuite-Setup.exe","size":2,"browser_download_url":"https://github.com/ok"}]}
    ]"#;
    let listed = releases_for_channel(releases(payload), Channel::Stable, None);
    assert_eq!(tags(&listed), ["v2.0.0"]);
}

#[test]
fn notes_stop_at_the_app_notes_marker() {
    let listed = releases_for_channel(feed(&[("v1.0.0", false)]), Channel::Stable, None);
    assert_eq!(listed[0].notes, "- Change in v1.0.0.");
}

fn run(id: u64, number: u64, branch: &str, sha: &str) -> GhRun {
    GhRun {
        id,
        head_branch: Some(branch.to_string()),
        head_sha: sha.to_string(),
        run_number: number,
        created_at: None,
        conclusion: Some("success".to_string()),
    }
}

#[test]
fn alpha_rows_keep_the_newest_run_of_each_live_branch() {
    let runs = vec![
        run(10, 5, "main", "aaaaaaa1"),
        run(11, 7, "Main", "bbbbbbb2"),
        run(12, 6, "release-pipeline", "ccccccc3"),
        run(13, 9, "deleted", "ddddddd4"),
        GhRun { conclusion: Some("failure".to_string()), ..run(14, 10, "main", "eeeeeee5") },
        run(15, 11, " ", "fffffff6"),
    ];
    let live = vec!["main".to_string(), "release-pipeline".to_string()];
    let rows = alpha_builds(runs, Some(&live), Some("CCCCCCC3ffff"));
    let shape: Vec<(u64, &str, bool)> = rows.iter().map(|r| (r.run_number, r.sha.as_str(), r.is_current)).collect();
    assert_eq!(shape, [(7, "bbbbbbb", false), (6, "ccccccc", true)]);
    assert_eq!(rows[0].download_url, "https://nightly.link/mkiera/SealSuite/actions/runs/11/SealSuite-Setup.zip");
    assert_eq!(rows[0].run_url, "https://github.com/mkiera/SealSuite/actions/runs/11");
}

#[test]
fn a_failed_branch_request_keeps_the_alpha_list_unfiltered_and_short() {
    let runs: Vec<GhRun> = (1..=12).map(|n| run(n, n, &format!("b{n}"), "1234567")).collect();
    let rows = alpha_builds(runs, None, None);
    assert_eq!(rows.len(), ALPHA_MAX_BRANCHES);
    assert_eq!(rows[0].run_number, 12);
    assert!(rows.iter().all(|r| !r.is_current));
}

#[test]
fn settings_default_to_stable_and_keep_keys_a_newer_build_wrote() {
    let dir = tempdir();
    let path = dir.join("self-update.json");
    assert_eq!(load_settings_from(&path).channel, Channel::Stable);
    assert!(load_settings_from(&path).auto_check);

    fs::write(&path, r#"{"channel":"weekly","autoCheck":"yes","futureKey":42}"#).unwrap();
    let loaded = load_settings_from(&path);
    assert_eq!(loaded.channel, Channel::Stable);
    assert!(loaded.auto_check);

    save_settings_to(&path, &SelfUpdateSettings { channel: Channel::Prerelease, auto_check: false }).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["channel"], "prerelease");
    assert_eq!(saved["autoCheck"], false);
    assert_eq!(saved["futureKey"], 42);
    assert_eq!(load_settings_from(&path).channel, Channel::Prerelease);
    fs::remove_dir_all(dir).unwrap();
}

fn tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("sealsuite-selfupdate-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn alpha_zips_yield_only_a_root_level_installer_within_the_limit() {
    let dir = tempdir();
    let zip = dir.join("build.zip");
    let dest = dir.join("alpha-1-SealSuite-Setup.exe");

    write_zip(&zip, &[("nested/SealSuite-Setup.exe", b"nested"), ("SealSuite-Setup.exe", b"installer")]);
    extract_installer(&zip, &dest, 64).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"installer");

    write_zip(&zip, &[("nested/SealSuite-Setup.exe", b"nested"), ("../SealSuite-Setup.exe", b"escape")]);
    assert!(extract_installer(&zip, &dest, 64).is_err());

    write_zip(&zip, &[("SealSuite-Setup.exe", b"")]);
    assert!(extract_installer(&zip, &dest, 64).is_err());
    assert!(!dest.exists());

    write_zip(&zip, &[("SealSuite-Setup.exe", &[0u8; 65])]);
    assert!(extract_installer(&zip, &dest, 64).is_err());
    assert!(!dest.exists());

    fs::write(&zip, b"not a zip").unwrap();
    assert!(extract_installer(&zip, &dest, 64).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn download_urls_must_be_https_on_an_allowed_host() {
    assert!(allowed_url("https://github.com/mkiera/SealSuite/releases/download/v1.0.0/SealSuite-Setup.exe"));
    assert!(allowed_url("https://nightly.link/mkiera/SealSuite/actions/runs/1/SealSuite-Setup.zip"));
    assert!(!allowed_url("http://github.com/x"));
    assert!(!allowed_url("https://github.com.evil.example/x"));
    assert!(!allowed_url("not a url"));
}

#[test]
fn the_busy_guard_blocks_and_clears() {
    set_busy(Some("SealSuite is installing FlipperClipper.".to_string()));
    assert!(refuse_while_busy().unwrap_err().starts_with("SealSuite is installing FlipperClipper."));
    set_busy(None);
    assert!(refuse_while_busy().is_ok());
}

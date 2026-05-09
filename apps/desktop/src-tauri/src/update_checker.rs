use std::time::Duration;

use semver::Version;
use serde::Deserialize;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/MengBad/lan-audio/releases/latest";
const RELEASE_PAGE_FALLBACK: &str = "https://github.com/MengBad/lan-audio/releases";
const REQUEST_TIMEOUT_SECONDS: u64 = 6;

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}

pub fn check_update(current_version: &str) -> Option<UpdateInfo> {
    let current = parse_version(current_version)?;
    let release = fetch_latest_release()?;
    if release.draft || release.prerelease {
        return None;
    }

    let latest = parse_version(&release.tag_name)?;
    if latest <= current {
        return None;
    }

    Some(UpdateInfo {
        latest_version: latest.to_string(),
        release_url: if release.html_url.trim().is_empty() {
            RELEASE_PAGE_FALLBACK.to_string()
        } else {
            release.html_url
        },
    })
}

fn fetch_latest_release() -> Option<GithubLatestRelease> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .build()
        .ok()?;
    client
        .get(LATEST_RELEASE_API)
        .header("User-Agent", "lan-audio-desktop-update-checker")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<GithubLatestRelease>()
        .ok()
}

fn parse_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim().trim_start_matches('v');
    Version::parse(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_accepts_v_prefix() {
        let parsed = parse_version("v1.4.2").expect("version");
        assert_eq!(parsed, Version::new(1, 4, 2));
    }

    #[test]
    fn parse_version_rejects_invalid_text() {
        assert!(parse_version("release-next").is_none());
    }
}

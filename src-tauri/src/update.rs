//! Checking GitHub for a newer release.
//!
//! Notify-and-link, not download-and-install: the app is not notarized, so the OS
//! prompts on install anyway, and installing in place would replace the app bundle —
//! which is what macOS ties Microphone and Accessibility grants to.

use serde::{Deserialize, Serialize};

const RELEASES_API: &str = "https://api.github.com/repos/kamicrafted/voxable/releases/latest";
/// GitHub rejects requests without one.
const USER_AGENT: &str = "voxable-update-check";

/// What the UI needs to describe an available update.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// The new version, without the tag's leading `v`.
    pub version: String,
    /// The release page, for the download button.
    pub url: String,
    /// Release notes, as markdown.
    pub notes: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

/// Does this release carry something the current platform can actually install?
///
/// Releases here are sometimes one-platform-only, and pointing someone at a release
/// page with no download for their machine is worse than staying quiet.
fn has_asset_for_this_platform(assets: &[Asset]) -> bool {
    assets.iter().any(|a| {
        let name = a.name.to_ascii_lowercase();
        if cfg!(target_os = "macos") {
            name.ends_with(".dmg") || name.ends_with(".app.tar.gz")
        } else if cfg!(windows) {
            name.ends_with(".exe") || name.ends_with(".msi")
        } else {
            name.ends_with(".appimage") || name.ends_with(".deb")
        }
    })
}

/// Ask GitHub for the latest release; `None` when there is nothing newer to offer.
///
/// Every failure is an error rather than a silent `None`, so "could not check" and
/// "already current" stay distinguishable in the UI.
pub async fn check(current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("could not build the http client: {e}"))?;

    let resp = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub returned {}", resp.status()));
    }

    let release: Release = resp
        .json()
        .await
        .map_err(|e| format!("could not read the release: {e}"))?;

    if release.draft || release.prerelease {
        return Ok(None);
    }
    if !voxable_core::version::is_newer(current_version, &release.tag_name) {
        return Ok(None);
    }
    if !has_asset_for_this_platform(&release.assets) {
        log::info!(
            "release {} is newer but has no download for this platform",
            release.tag_name
        );
        return Ok(None);
    }

    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: release.html_url,
        notes: release.body,
    }))
}

/// Open a URL in the user's browser.
pub fn open_in_browser(url: &str) -> Result<(), String> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open {url}: {e}"))
}

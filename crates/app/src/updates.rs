//! Looks for a newer release on GitHub in the background. A found update is
//! shown in the status bar with a link to the release page; nothing is
//! downloaded or installed automatically.
//!
//! This edition (Traditional Chinese, more AI providers) looks for releases
//! of its own repository, not for those of the original project.

use std::sync::mpsc::{Receiver, channel};

const LATEST: &str = "https://api.github.com/repos/Smallston12345/lvglab/releases/latest";
const RELEASES: &str = "https://github.com/Smallston12345/lvglab/releases";

/// Version of this build. The release workflow builds from a tag such as
/// `v0.15.0.1`; GitHub Actions sets `GITHUB_REF_NAME` to it, so a release
/// knows its own (possibly four-part) version. Other builds use the crate
/// version.
pub fn version() -> &'static str {
    release_version(option_env!("GITHUB_REF_NAME")).unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn release_version(tag: Option<&'static str>) -> Option<&'static str> {
    let version = tag?.strip_prefix('v')?;
    parse(version).is_some_and(|parts| parts.len() >= 3).then_some(version)
}

fn parse(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|p| p.parse().ok()).collect()
}

/// A newer release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// What the last check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Newer(Update),
    UpToDate,
    Failed(String),
}

#[derive(Default)]
pub struct UpdateCheck {
    running: Option<Receiver<Outcome>>,
    pub outcome: Option<Outcome>,
    /// The check was started from the menu, so "up to date" is reported as well.
    pub manual: bool,
}

impl UpdateCheck {
    /// Starts a check in a background thread (not in tests).
    pub fn start(&mut self, manual: bool) {
        if self.running.is_some() || crate::sandbox::active() {
            return;
        }
        self.manual = manual;
        let (send, receive) = channel();
        self.running = Some(receive);
        std::thread::spawn(move || {
            let _ = send.send(check());
        });
    }

    /// The result, once the background thread is done.
    pub fn poll(&mut self) -> Option<Outcome> {
        let outcome = self.running.as_ref()?.try_recv().ok()?;
        self.running = None;
        self.outcome = Some(outcome.clone());
        Some(outcome)
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

fn check() -> Outcome {
    let response = ureq::get(LATEST)
        .set("User-Agent", &format!("LVGLab/{}", version()))
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .call();
    let text = match response.map(|r| r.into_string()) {
        Ok(Ok(text)) => text,
        Ok(Err(err)) => return Outcome::Failed(err.to_string()),
        Err(err) => return Outcome::Failed(err.to_string()),
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Outcome::Failed("unexpected answer".to_owned());
    };
    let tag = json["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_owned();
    let url = json["html_url"].as_str().unwrap_or(RELEASES).to_owned();
    if is_newer(&tag, version()) { Outcome::Newer(Update { version: tag, url }) } else { Outcome::UpToDate }
}

/// `1.2.10` is newer than `1.2.9`; anything that is not a version is not newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.13.0", "0.12.0"));
        assert!(is_newer("0.12.10", "0.12.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.12.0", "0.12.0"));
        assert!(!is_newer("0.11.5", "0.12.0"));
        assert!(!is_newer("nightly", "0.12.0"));
        assert!(is_newer("0.15.0.1", "0.15.0"));
        assert!(is_newer("0.15.0.2", "0.15.0.1"));
        assert!(!is_newer("0.15.0.1", "0.15.0.1"));
    }

    #[test]
    fn release_builds_take_the_version_from_the_tag() {
        assert_eq!(release_version(Some("v0.15.0.1")), Some("0.15.0.1"));
        assert_eq!(release_version(Some("v1.2.3")), Some("1.2.3"));
        assert_eq!(release_version(Some("ai-providers")), None, "branch builds");
        assert_eq!(release_version(Some("v1")), None);
        assert_eq!(release_version(None), None);
        assert!(parse(version()).is_some());
    }

    #[test]
    fn looks_for_releases_of_this_edition() {
        let repository = "Smallston12345/lvglab";
        assert!(LATEST.contains(repository) && RELEASES.contains(repository));
        assert!(!LATEST.contains("bluecraank"));
    }

    /// Asks GitHub: `cargo test -p lvglab github_answers -- --ignored`
    #[test]
    #[ignore]
    fn github_answers() {
        let outcome = check();
        assert!(!matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    }
}

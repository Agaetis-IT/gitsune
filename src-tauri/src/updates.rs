//! Looks for a newer version of the application: the latest published release of the project on
//! GitHub is read (the public API, no token, nothing about the user or their repositories is
//! sent) and its version compared with the running one. Nothing is downloaded or installed: the
//! page of the release is opened in the browser, where the packages are.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

const API_URL: &str = "https://api.github.com/repos/Agaetis-IT/gitsune/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/Agaetis-IT/gitsune/releases/tag/";
/// More than this of the release notes is left out of the answer.
const MAX_NOTES: usize = 4000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    /// The running version, `0.1.0`.
    pub current: String,
    /// The version of the latest release, without the leading `v`.
    pub latest: String,
    /// The latest release is newer than the running version.
    pub available: bool,
    /// The page of the latest release.
    pub url: String,
    /// What the release says is new (plain text, possibly shortened).
    pub notes: String,
}

/// `v1.2.3`, `1.2.3` or `1.2.3-beta.1` as numbers; a suffix after `-` or `+` is ignored.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// Whether `latest` is a newer version than `current`. A version that cannot be read is never
/// newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// A tag is used to build an address, so only the characters of a version are accepted.
fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 40
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// The release the API described, compared with `current`. The address of the page is built
/// from the tag, never taken from the answer.
pub fn read_release(answer: &Value, current: &str) -> Result<UpdateCheck, String> {
    let tag = answer
        .get("tag_name")
        .and_then(Value::as_str)
        .filter(|tag| valid_tag(tag))
        .ok_or("The latest release has no readable version.")?;
    if parse_version(tag).is_none() {
        return Err(format!(
            "The version {tag} of the latest release is not readable."
        ));
    }
    let latest = tag.trim_start_matches('v').to_string();
    let notes: String = answer
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(MAX_NOTES)
        .collect();
    Ok(UpdateCheck {
        current: current.to_string(),
        available: is_newer(&latest, current),
        latest,
        url: format!("{RELEASE_PAGE}{tag}"),
        notes: notes.trim().to_string(),
    })
}

/// Asks GitHub for the latest release and compares it with the running version.
pub fn check(current: &str) -> Result<UpdateCheck, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let answer: Value = agent
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "gitsune")
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(404) => "No release is published yet.".to_string(),
            ureq::Error::StatusCode(403 | 429) => {
                "GitHub is limiting the requests from this network: try again later.".to_string()
            }
            ureq::Error::StatusCode(code) => format!("GitHub answered with the error {code}."),
            other => format!("Could not reach GitHub: {other}"),
        })?
        .body_mut()
        .read_json()
        .map_err(|error| format!("GitHub sent an answer that is not readable: {error}"))?;
    read_release(&answer, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_are_read_with_or_without_the_v_and_a_suffix() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10.0"), Some((0, 10, 0)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn versions_are_compared_as_numbers_not_as_text() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("0.2.0", "dev"));
    }

    #[test]
    fn a_newer_release_is_announced_with_its_page_and_notes() {
        let answer = json!({
            "tag_name": "v0.2.0",
            "body": "  # Gitsune 0.2.0\n\nStashes in the graph.  ",
            "html_url": "https://evil.example/phish",
        });
        let check = read_release(&answer, "0.1.0").unwrap();
        assert!(check.available);
        assert_eq!(
            (check.current.as_str(), check.latest.as_str()),
            ("0.1.0", "0.2.0")
        );
        assert_eq!(
            check.url,
            "https://github.com/Agaetis-IT/gitsune/releases/tag/v0.2.0"
        );
        assert_eq!(check.notes, "# Gitsune 0.2.0\n\nStashes in the graph.");
    }

    #[test]
    fn the_same_or_an_older_release_is_not_an_update() {
        let answer = json!({ "tag_name": "v0.1.0", "body": null });
        let check = read_release(&answer, "0.1.0").unwrap();
        assert!(!check.available);
        assert_eq!(check.notes, "");
        assert!(!read_release(&answer, "0.3.0").unwrap().available);
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_refused() {
        for tag in ["", "nightly", "v1.2.3/../x", "v1.2.3 ", "v1.2"] {
            let answer = json!({ "tag_name": tag });
            assert!(read_release(&answer, "0.1.0").is_err(), "{tag:?}");
        }
        assert!(read_release(&json!({}), "0.1.0").is_err());
    }

    #[test]
    fn long_notes_are_cut() {
        let body = "x".repeat(MAX_NOTES + 500);
        let check = read_release(&json!({ "tag_name": "v0.2.0", "body": body }), "0.1.0").unwrap();
        assert_eq!(check.notes.chars().count(), MAX_NOTES);
    }
}

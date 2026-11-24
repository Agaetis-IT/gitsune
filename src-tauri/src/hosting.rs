//! GitHub and Azure DevOps: the tokens (kept in the macOS keychain, never sent back to the
//! frontend) and the lookup of the open pull request of a branch.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SERVICE: &str = "git-hydra";

/// The hosts a token can be kept for.
pub const HOSTS: [&str; 2] = ["github.com", "dev.azure.com"];

fn entry(host: &str) -> Result<keyring::Entry, String> {
    if !HOSTS.contains(&host) {
        return Err(format!("unknown host: {host}"));
    }
    keyring::Entry::new(SERVICE, host).map_err(|error| error.to_string())
}

pub fn token_set(host: &str, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("the token is empty or has spaces in it".to_string());
    }
    entry(host)?
        .set_password(token)
        .map_err(|error| error.to_string())
}

pub fn token_clear(host: &str) -> Result<(), String> {
    match entry(host)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn read_token(host: &str) -> Result<Option<String>, String> {
    match entry(host)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// A repository on one of the hosts (same shape as the frontend's `HostedRepo`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HostedRepo {
    Github {
        owner: String,
        repo: String,
    },
    Azure {
        org: String,
        project: String,
        repo: String,
    },
}

impl HostedRepo {
    pub fn host(&self) -> &'static str {
        match self {
            HostedRepo::Github { .. } => "github.com",
            HostedRepo::Azure { .. } => "dev.azure.com",
        }
    }
}

/// What the lookup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PullRequestLookup {
    /// No token is kept for the host: nothing was asked.
    NoToken,
    /// The branch has no open pull request.
    None,
    Found {
        id: u64,
        title: String,
    },
}

/// Percent-encodes everything but the unreserved characters (and `/` when `keep_slash`).
pub fn encode(text: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        let plain = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (keep_slash && byte == b'/');
        if plain {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The API address that lists the open pull requests of `branch`.
pub fn lookup_url(repo: &HostedRepo, branch: &str) -> String {
    match repo {
        HostedRepo::Github { owner, repo } => format!(
            "https://api.github.com/repos/{}/{}/pulls?state=open&head={}",
            encode(owner, false),
            encode(repo, false),
            encode(&format!("{owner}:{branch}"), false),
        ),
        HostedRepo::Azure { org, project, repo } => format!(
            "https://dev.azure.com/{}/{}/_apis/git/repositories/{}/pullrequests?searchCriteria.sourceRefName={}&searchCriteria.status=active&api-version=7.1",
            encode(org, false),
            encode(project, false),
            encode(repo, false),
            encode(&format!("refs/heads/{branch}"), false),
        ),
    }
}

/// Standard base64 (for the Basic authorization of Azure DevOps).
pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The first pull request in the answer of the API.
pub fn first_pull_request(repo: &HostedRepo, answer: &Value) -> PullRequestLookup {
    let (list, id_key) = match repo {
        HostedRepo::Github { .. } => (answer.as_array(), "number"),
        HostedRepo::Azure { .. } => (
            answer.get("value").and_then(Value::as_array),
            "pullRequestId",
        ),
    };
    let found = list.and_then(|items| items.first()).and_then(|pr| {
        let id = pr.get(id_key)?.as_u64()?;
        let title = pr.get("title").and_then(Value::as_str).unwrap_or("");
        Some(PullRequestLookup::Found {
            id,
            title: title.to_string(),
        })
    });
    found.unwrap_or(PullRequestLookup::None)
}

/// A readable message for a failed request.
fn describe(host: &str, error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
            format!(
                "{host} refused the token: it may have expired or lack the right to read pull requests."
            )
        }
        ureq::Error::StatusCode(404) => {
            format!("{host} does not know this repository, or the token cannot see it.")
        }
        ureq::Error::StatusCode(code) => format!("{host} answered with the error {code}."),
        other => format!("Could not reach {host}: {other}"),
    }
}

/// Looks for the open pull request of `branch`. Without a token for the host nothing is asked.
pub fn find_pull_request(repo: &HostedRepo, branch: &str) -> Result<PullRequestLookup, String> {
    if branch.is_empty() || branch.chars().any(char::is_control) {
        return Err("the branch name is not valid".to_string());
    }
    let host = repo.host();
    let Some(token) = read_token(host)? else {
        return Ok(PullRequestLookup::NoToken);
    };
    let authorization = match repo {
        HostedRepo::Github { .. } => format!("Bearer {token}"),
        HostedRepo::Azure { .. } => format!("Basic {}", base64(format!(":{token}").as_bytes())),
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(&lookup_url(repo, branch))
        .header("Authorization", &authorization)
        .header("Accept", "application/json")
        .header("User-Agent", "git-hydra")
        .call()
        .map_err(|error| describe(host, &error))?;
    let answer: Value = response
        .body_mut()
        .read_json()
        .map_err(|error| format!("{host} sent an answer that could not be read: {error}"))?;
    Ok(first_pull_request(repo, &answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn github() -> HostedRepo {
        HostedRepo::Github {
            owner: "acme".into(),
            repo: "tool".into(),
        }
    }
    fn azure() -> HostedRepo {
        HostedRepo::Azure {
            org: "org".into(),
            project: "My Project".into(),
            repo: "repo".into(),
        }
    }

    #[test]
    fn the_lookup_addresses_encode_the_branch() {
        assert_eq!(
            lookup_url(&github(), "feature/x y"),
            "https://api.github.com/repos/acme/tool/pulls?state=open&head=acme%3Afeature%2Fx%20y"
        );
        let url = lookup_url(&azure(), "feature/x");
        assert!(
            url.starts_with("https://dev.azure.com/org/My%20Project/_apis/git/repositories/repo/")
        );
        assert!(url.contains("sourceRefName=refs%2Fheads%2Ffeature%2Fx"));
        assert!(url.ends_with("api-version=7.1"));
    }

    #[test]
    fn a_branch_cannot_add_parameters_to_the_address() {
        let url = lookup_url(&github(), "x&state=closed#frag");
        assert!(!url.contains("&state=closed"));
        assert!(!url.contains('#'));
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b":secret"), "OnNlY3JldA==");
    }

    #[test]
    fn the_first_pull_request_is_read_from_each_service() {
        let gh = json!([{ "number": 7, "title": "Add x" }, { "number": 9, "title": "other" }]);
        assert_eq!(
            first_pull_request(&github(), &gh),
            PullRequestLookup::Found {
                id: 7,
                title: "Add x".into()
            }
        );
        let az = json!({ "value": [{ "pullRequestId": 12, "title": "Fix y" }], "count": 1 });
        assert_eq!(
            first_pull_request(&azure(), &az),
            PullRequestLookup::Found {
                id: 12,
                title: "Fix y".into()
            }
        );
    }

    #[test]
    fn no_pull_request_or_an_unexpected_answer_is_none() {
        assert_eq!(
            first_pull_request(&github(), &json!([])),
            PullRequestLookup::None
        );
        assert_eq!(
            first_pull_request(&azure(), &json!({ "value": [] })),
            PullRequestLookup::None
        );
        assert_eq!(
            first_pull_request(&azure(), &json!("nope")),
            PullRequestLookup::None
        );
        assert_eq!(
            first_pull_request(&github(), &json!([{ "title": "no id" }])),
            PullRequestLookup::None
        );
    }

    #[test]
    fn only_known_hosts_have_a_token_and_blank_tokens_are_refused() {
        assert!(token_set("evil.example.com", "abc").is_err());
        assert!(token_set("github.com", "  ").is_err());
        assert!(token_set("github.com", "a b").is_err());
        assert!(read_token("evil.example.com").is_err());
    }

    #[test]
    fn an_invalid_branch_is_refused_before_anything_is_asked() {
        assert!(find_pull_request(&github(), "").is_err());
        assert!(find_pull_request(&github(), "a\nb").is_err());
    }

    #[test]
    fn the_lookup_result_has_the_shape_the_frontend_reads() {
        assert_eq!(
            serde_json::to_value(PullRequestLookup::NoToken).unwrap(),
            json!({ "state": "noToken" })
        );
        assert_eq!(
            serde_json::to_value(PullRequestLookup::Found {
                id: 3,
                title: "t".into()
            })
            .unwrap(),
            json!({ "state": "found", "id": 3, "title": "t" })
        );
    }
}

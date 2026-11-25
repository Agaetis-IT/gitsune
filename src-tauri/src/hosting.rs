//! GitHub and Azure DevOps: the lookup of the open pull request of a branch. The application
//! has no credentials of its own: it asks the command line tools the user signed in with, `az`
//! (Azure CLI, `az login` opens a login page in the browser), `gh` (GitHub CLI) and `glab` (GitLab
//! CLI), for a token when it needs one. The token is kept in memory until it expires, never written anywhere.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The hosts that can be linked to.
pub const HOSTS: [&str; 3] = ["github.com", "dev.azure.com", "gitlab.com"];

/// The Azure DevOps application, the resource a token is asked for.
const AZURE_DEVOPS_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";

/// Why a token could not be had from a command line tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The tool is not installed (or not where the application looks).
    Missing,
    /// The tool is installed but the user is not signed in with it.
    SignedOut,
    /// Anything else, with what the tool said.
    Failed(String),
}

/// A token and the moment it stops being valid (`None`: not known, ask again next time).
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub expires_at: Option<Instant>,
}

struct Output {
    status: i32,
    stdout: String,
    stderr: String,
}

/// Runs `program` with `PATH` set to `path`, waiting at most `limit`.
fn run_cli(path: &str, program: &str, args: &[&str], limit: Duration) -> Result<Output, CliError> {
    let mut child = Command::new(program)
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CliError::Missing,
            _ => CliError::Failed(error.to_string()),
        })?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    });
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = stderr.as_mut() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CliError::Failed(format!(
                    "{program} did not answer in time"
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(CliError::Failed(error.to_string())),
        }
    };
    Ok(Output {
        status: status.code().unwrap_or(-1),
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

/// The first line a tool prints (stdout, else stderr: `ssh -V` writes there), `None` when it is
/// missing, fails to start or does not answer in time. Used to read versions.
pub(crate) fn tool_line(program: &str, args: &[&str], limit: Duration) -> Option<String> {
    let output = run_cli(crate::git::tool_path(), program, args, limit).ok()?;
    [output.stdout.as_str(), output.stderr.as_str()]
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// The first line of what a tool said, for a message (never the token: only stderr is quoted).
fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("it failed without saying why")
        .chars()
        .take(200)
        .collect()
}

fn signed_out(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    [
        "az login",
        "not logged in",
        "no oauth token",
        "gh auth login",
        "please run",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
}

/// Reads the answer of `az account get-access-token --output json`.
pub fn parse_az_token(json: &str, now: Instant) -> Result<AccessToken, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|_| "az answered with something unreadable".to_string())?;
    let token = value
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "az did not give a token".to_string())?;
    // `expires_on` is a Unix time; older versions only have a local date text.
    let expires_at = value
        .get("expires_on")
        .and_then(Value::as_u64)
        .and_then(|at| {
            let current = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some(now + Duration::from_secs(at.saturating_sub(current)))
        });
    Ok(AccessToken {
        token: token.to_string(),
        expires_at,
    })
}

fn azure_token(path: &str) -> Result<AccessToken, CliError> {
    let output = run_cli(
        path,
        "az",
        &[
            "account",
            "get-access-token",
            "--resource",
            AZURE_DEVOPS_RESOURCE,
            "--output",
            "json",
        ],
        Duration::from_secs(30),
    )?;
    if output.status != 0 {
        return Err(if signed_out(&output.stderr) {
            CliError::SignedOut
        } else {
            CliError::Failed(format!("az said: {}", first_line(&output.stderr)))
        });
    }
    parse_az_token(&output.stdout, Instant::now()).map_err(CliError::Failed)
}

fn github_token(path: &str) -> Result<AccessToken, CliError> {
    let output = run_cli(
        path,
        "gh",
        &["auth", "token", "--hostname", "github.com"],
        Duration::from_secs(15),
    )?;
    let token = output.stdout.trim();
    if output.status != 0 || token.is_empty() {
        return Err(if output.status == 0 || signed_out(&output.stderr) {
            CliError::SignedOut
        } else {
            CliError::Failed(format!("gh said: {}", first_line(&output.stderr)))
        });
    }
    Ok(AccessToken {
        token: token.to_string(),
        expires_at: None,
    })
}

/// Removes the colors a terminal-oriented tool puts in its output.
fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The token in the output of `glab auth status --show-token` (a text for people: a line like
/// `Token: glpat-xxxx`, written to either stream, with other lines about the scopes or the
/// account that must not be taken for it).
pub fn parse_glab_token(output: &str) -> Option<String> {
    strip_ansi(output).lines().find_map(|line| {
        let lower = line.to_lowercase();
        if !lower.contains("token") {
            return None;
        }
        let candidate = line.rsplit_once(':')?.1.trim();
        let plain = candidate.len() >= 8
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        (plain && !candidate.chars().all(|c| c == '*')).then(|| candidate.to_string())
    })
}

/// A host name that is safe to put in an address and in a command line.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        && !host.starts_with(['.', '-'])
}

fn gitlab_token(path: &str, host: &str) -> Result<AccessToken, CliError> {
    if !valid_host(host) {
        return Err(CliError::Failed(format!("not a valid host name: {host}")));
    }
    let output = run_cli(
        path,
        "glab",
        &["auth", "status", "--hostname", host, "--show-token"],
        Duration::from_secs(20),
    )?;
    // `glab` writes its report to the error stream.
    let all = format!("{}\n{}", output.stdout, output.stderr);
    if output.status != 0 {
        return Err(CliError::SignedOut);
    }
    match parse_glab_token(&all) {
        Some(token) => Ok(AccessToken {
            token,
            expires_at: None,
        }),
        None => Err(CliError::Failed(
            "glab did not show a token (its output has changed?)".to_string(),
        )),
    }
}

/// The Azure token in use, so that `az` is not run for every lookup.
static AZURE_CACHE: Mutex<Option<AccessToken>> = Mutex::new(None);

/// A token for `host`: the cached one while it has more than five minutes left, or a new one.
pub(crate) fn token_for(host: &str, path: &str) -> Result<AccessToken, CliError> {
    match host {
        "github.com" => github_token(path),
        "dev.azure.com" => {
            let mut cache = AZURE_CACHE
                .lock()
                .map_err(|_| CliError::Failed("lock".into()))?;
            if let Some(known) = cache.as_ref()
                && known
                    .expires_at
                    .is_some_and(|at| at > Instant::now() + Duration::from_secs(300))
            {
                return Ok(known.clone());
            }
            let fresh = azure_token(path)?;
            *cache = Some(fresh.clone());
            Ok(fresh)
        }
        other => gitlab_token(path, other),
    }
}

/// Whether the user is signed in with the tool of a host, and what to do when not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub host: String,
    /// The tool that holds the sign-in ("Azure CLI", "GitHub CLI").
    pub tool: String,
    /// `signedIn`, `signedOut` or `missing`.
    pub state: String,
    /// The account when it is known.
    pub account: Option<String>,
}

fn host_status_with(host: &str, path: &str) -> HostStatus {
    let (tool, program, args): (&str, &str, Vec<&str>) = match host {
        "github.com" => (
            "GitHub CLI",
            "gh",
            vec!["auth", "status", "--hostname", "github.com"],
        ),
        "dev.azure.com" => (
            "Azure CLI",
            "az",
            vec!["account", "show", "--query", "user.name", "--output", "tsv"],
        ),
        other => (
            "GitLab CLI",
            "glab",
            vec!["auth", "status", "--hostname", other],
        ),
    };
    let (state, account) = match run_cli(path, program, &args, Duration::from_secs(20)) {
        Err(CliError::Missing) => ("missing", None),
        Err(_) => ("signedOut", None),
        Ok(output) if output.status == 0 => (
            "signedIn",
            (program == "az")
                .then(|| output.stdout.trim().to_string())
                .filter(|name| !name.is_empty()),
        ),
        Ok(_) => ("signedOut", None),
    };
    HostStatus {
        host: host.to_string(),
        tool: tool.to_string(),
        state: state.to_string(),
        account,
    }
}

pub fn host_statuses() -> Vec<HostStatus> {
    let path = crate::git::tool_path();
    HOSTS
        .iter()
        .map(|host| host_status_with(host, path))
        .collect()
}

/// Opens the sign-in page of the Azure CLI (`az login`) and waits for the user to finish.
pub fn azure_sign_in() -> Result<(), String> {
    let output = run_cli(
        crate::git::tool_path(),
        "az",
        &["login", "--output", "none"],
        Duration::from_secs(300),
    )
    .map_err(|error| match error {
        CliError::Missing => "The Azure CLI (az) is not installed.".to_string(),
        CliError::SignedOut => "The sign-in was not completed.".to_string(),
        CliError::Failed(message) => message,
    })?;
    if output.status == 0 {
        if let Ok(mut cache) = AZURE_CACHE.lock() {
            *cache = None;
        }
        Ok(())
    } else {
        Err(format!("az login failed: {}", first_line(&output.stderr)))
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
    /// `path` is the project with its groups: `group/subgroup/project`.
    Gitlab {
        host: String,
        path: String,
    },
}

impl HostedRepo {
    pub fn host(&self) -> String {
        match self {
            HostedRepo::Github { .. } => "github.com".to_string(),
            HostedRepo::Azure { .. } => "dev.azure.com".to_string(),
            HostedRepo::Gitlab { host, .. } => host.clone(),
        }
    }
}

/// What the lookup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PullRequestLookup {
    /// The user is not signed in with the tool of the host (or it is not installed): nothing
    /// was asked.
    SignedOut,
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
        HostedRepo::Gitlab { host, path } => format!(
            "https://{host}/api/v4/projects/{}/merge_requests?state=opened&source_branch={}",
            encode(path, false),
            encode(branch, false),
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

/// The first pull request in the answer of the API.
pub fn first_pull_request(repo: &HostedRepo, answer: &Value) -> PullRequestLookup {
    let (list, id_key) = match repo {
        HostedRepo::Github { .. } => (answer.as_array(), "number"),
        HostedRepo::Gitlab { .. } => (answer.as_array(), "iid"),
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
pub(crate) fn describe(host: &str, error: &ureq::Error) -> String {
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
    if let HostedRepo::Gitlab { host, path } = repo
        && (!valid_host(host) || path.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return Err("the GitLab address is not valid".to_string());
    }
    let host = repo.host();
    let access = match token_for(&host, crate::git::tool_path()) {
        Ok(access) => access,
        Err(CliError::Missing | CliError::SignedOut) => return Ok(PullRequestLookup::SignedOut),
        Err(CliError::Failed(message)) => return Err(message),
    };
    let token = access.token;
    let authorization = format!("Bearer {token}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(&lookup_url(repo, branch))
        .header("Authorization", &authorization)
        .header("Accept", "application/json")
        .header("User-Agent", "gitsune")
        .call()
        .map_err(|error| describe(&host, &error))?;
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
    fn an_invalid_branch_is_refused_before_anything_is_asked() {
        assert!(find_pull_request(&github(), "").is_err());
        assert!(find_pull_request(&github(), "a\nb").is_err());
    }

    #[test]
    fn the_lookup_result_has_the_shape_the_frontend_reads() {
        assert_eq!(
            serde_json::to_value(PullRequestLookup::SignedOut).unwrap(),
            json!({ "state": "signedOut" })
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

    /// A directory with fake `az` and `gh` scripts, used as the whole `PATH`.
    #[cfg(unix)]
    fn fake_tools(az: &str, gh: &str) -> tempfile::TempDir {
        fake_tools_with_glab(az, gh, "exit 1")
    }

    #[cfg(unix)]
    fn fake_tools_with_glab(az: &str, gh: &str, glab: &str) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        for (name, body) in [("az", az), ("gh", gh), ("glab", glab)] {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    #[cfg(unix)]
    fn path_of(dir: &tempfile::TempDir) -> String {
        // `sh` and `cat` etc. are still needed by the scripts.
        format!("{}:/bin:/usr/bin", dir.path().display())
    }

    #[test]
    fn the_azure_token_is_read_from_the_answer_of_az() {
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;
        let json = format!(r#"{{"accessToken":"tok-1","expires_on":{future},"tenant":"t"}}"#);
        let access = parse_az_token(&json, Instant::now()).unwrap();
        assert_eq!(access.token, "tok-1");
        let left = access.expires_at.unwrap() - Instant::now();
        assert!(left > Duration::from_secs(3500) && left <= Duration::from_secs(3600));
        // Without an expiry the token is used once.
        assert!(
            parse_az_token(r#"{"accessToken":"x"}"#, Instant::now())
                .unwrap()
                .expires_at
                .is_none()
        );
        assert!(parse_az_token(r#"{"accessToken":""}"#, Instant::now()).is_err());
        assert!(parse_az_token("not json", Instant::now()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_token_is_asked_of_the_tools_the_user_signed_in_with() {
        let dir = fake_tools(r#"echo '{"accessToken":"azure-token"}'"#, "echo gh-token");
        let path = path_of(&dir);
        assert_eq!(azure_token(&path).unwrap().token, "azure-token");
        assert_eq!(github_token(&path).unwrap().token, "gh-token");
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_tool_and_a_signed_out_user_are_told_apart() {
        let empty = tempfile::TempDir::new().unwrap();
        let path = path_of(&empty);
        assert_eq!(azure_token(&path).unwrap_err(), CliError::Missing);
        assert_eq!(github_token(&path).unwrap_err(), CliError::Missing);

        let out = fake_tools(
            "echo \"ERROR: Please run 'az login' to setup account.\" >&2; exit 1",
            "echo 'no oauth token found for github.com' >&2; exit 1",
        );
        let path = path_of(&out);
        assert_eq!(azure_token(&path).unwrap_err(), CliError::SignedOut);
        assert_eq!(github_token(&path).unwrap_err(), CliError::SignedOut);
    }

    #[cfg(unix)]
    #[test]
    fn another_failure_says_what_the_tool_said_but_never_a_token() {
        let dir = fake_tools(
            "echo 'secret-on-stdout'; echo 'ERROR: network is down' >&2; exit 3",
            "exit 4",
        );
        let path = path_of(&dir);
        let CliError::Failed(message) = azure_token(&path).unwrap_err() else {
            panic!("expected a failure");
        };
        assert!(message.contains("network is down"));
        assert!(!message.contains("secret-on-stdout"));
    }

    #[cfg(unix)]
    #[test]
    fn a_tool_that_hangs_is_stopped() {
        let dir = fake_tools("sleep 5", "sleep 5");
        let started = Instant::now();
        let result = run_cli(&path_of(&dir), "az", &[], Duration::from_millis(200));
        assert!(matches!(result, Err(CliError::Failed(_))));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn the_state_of_each_host_is_told() {
        let dir = fake_tools(r#"echo "user@example.com""#, "exit 0");
        let status = host_status_with("dev.azure.com", &path_of(&dir));
        assert_eq!(status.state, "signedIn");
        assert_eq!(status.account.as_deref(), Some("user@example.com"));
        assert_eq!(
            host_status_with("github.com", &path_of(&dir)).state,
            "signedIn"
        );

        let out = fake_tools("exit 1", "exit 1");
        assert_eq!(
            host_status_with("github.com", &path_of(&out)).state,
            "signedOut"
        );
        let empty = tempfile::TempDir::new().unwrap();
        assert_eq!(
            host_status_with("dev.azure.com", &path_of(&empty)).state,
            "missing"
        );
    }

    fn gitlab() -> HostedRepo {
        HostedRepo::Gitlab {
            host: "gitlab.example.fr".into(),
            path: "group/sub/project".into(),
        }
    }

    #[test]
    fn the_gitlab_lookup_encodes_the_project_path_and_the_branch() {
        assert_eq!(
            lookup_url(&gitlab(), "feature/x y"),
            "https://gitlab.example.fr/api/v4/projects/group%2Fsub%2Fproject/merge_requests?state=opened&source_branch=feature%2Fx%20y"
        );
        let mr = json!([{ "iid": 4, "title": "Add x", "id": 99 }]);
        assert_eq!(
            first_pull_request(&gitlab(), &mr),
            PullRequestLookup::Found {
                id: 4,
                title: "Add x".into()
            }
        );
        assert_eq!(
            first_pull_request(&gitlab(), &json!([])),
            PullRequestLookup::None
        );
    }

    #[test]
    fn the_glab_token_is_found_in_its_report() {
        let report = "gitlab.com\n  \u{1b}[32m✓\u{1b}[0m Logged in to gitlab.com as me\n  ✓ Git operations for gitlab.com configured to use ssh protocol.\n  ✓ Token found: glpat-AbC123_xyz-9\n  ✓ Token scopes: api, read_user\n";
        assert_eq!(
            parse_glab_token(report).as_deref(),
            Some("glpat-AbC123_xyz-9")
        );
        assert_eq!(
            parse_glab_token("Token: glpat-abcdefgh1234").as_deref(),
            Some("glpat-abcdefgh1234")
        );
        // A masked token or no token is not one.
        assert_eq!(
            parse_glab_token("✓ Token found: **************************"),
            None
        );
        assert_eq!(parse_glab_token("✓ Token scopes: api, read_user"), None);
        assert_eq!(parse_glab_token("nothing here"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_gitlab_token_is_asked_of_glab_for_the_host() {
        let dir = fake_tools_with_glab(
            "exit 1",
            "exit 1",
            r#"echo "  ✓ Token found: glpat-fake1234567" >&2; test "$4" = gitlab.example.fr"#,
        );
        let path = path_of(&dir);
        assert_eq!(
            gitlab_token(&path, "gitlab.example.fr").unwrap().token,
            "glpat-fake1234567"
        );
        // The host is passed through, and a bad name is refused before any command runs.
        assert_eq!(
            gitlab_token(&path, "other.example.fr").unwrap_err(),
            CliError::SignedOut
        );
        assert!(matches!(
            gitlab_token(&path, "x; rm -rf /"),
            Err(CliError::Failed(_))
        ));
        let signed_out =
            fake_tools_with_glab("exit 1", "exit 1", "echo 'not logged in' >&2; exit 1");
        assert_eq!(
            gitlab_token(&path_of(&signed_out), "gitlab.com").unwrap_err(),
            CliError::SignedOut
        );
        let empty = tempfile::TempDir::new().unwrap();
        assert_eq!(
            gitlab_token(&path_of(&empty), "gitlab.com").unwrap_err(),
            CliError::Missing
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_glab_that_shows_no_token_is_reported_as_changed() {
        let dir = fake_tools_with_glab("exit 1", "exit 1", "echo 'Logged in' >&2");
        let CliError::Failed(message) = gitlab_token(&path_of(&dir), "gitlab.com").unwrap_err()
        else {
            panic!("expected a failure");
        };
        assert!(message.contains("glab did not show a token"));
    }

    #[cfg(unix)]
    #[test]
    fn host_names_are_checked_and_the_gitlab_status_is_told() {
        assert!(valid_host("gitlab.example.fr"));
        for bad in ["", "a b", "a/b", "-x.com", "a;b", "a:80"] {
            assert!(!valid_host(bad), "{bad:?}");
        }
        let dir = fake_tools_with_glab("exit 1", "exit 1", "exit 0");
        assert_eq!(
            host_status_with("gitlab.com", &path_of(&dir)).state,
            "signedIn"
        );
        assert_eq!(
            host_status_with("gitlab.com", &path_of(&dir)).tool,
            "GitLab CLI"
        );
        let empty = tempfile::TempDir::new().unwrap();
        assert_eq!(
            host_status_with("gitlab.com", &path_of(&empty)).state,
            "missing"
        );
    }

    #[test]
    fn a_gitlab_repository_with_a_bad_address_is_refused() {
        let bad = HostedRepo::Gitlab {
            host: "a b".into(),
            path: "x/y".into(),
        };
        assert!(find_pull_request(&bad, "main").is_err());
        let spaced = HostedRepo::Gitlab {
            host: "gitlab.com".into(),
            path: "x/y z".into(),
        };
        assert!(find_pull_request(&spaced, "main").is_err());
    }
}

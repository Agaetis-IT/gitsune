//! Creating a pull request from the application, and finding the default branch of the
//! repository (the target proposed). Tokens come from the command line tools, as for the rest
//! of [`crate::hosting`].

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::hosting::{CliError, HostedRepo, describe, encode, token_for, valid_host};

/// What the user filled in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPullRequest {
    pub source: String,
    pub target: String,
    pub title: String,
    pub description: String,
    pub draft: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The address and the body of the request that creates the pull request.
pub fn create_request(repo: &HostedRepo, new: &NewPullRequest) -> (String, Value) {
    match repo {
        HostedRepo::Github { owner, repo } => (
            format!(
                "https://api.github.com/repos/{}/{}/pulls",
                encode(owner, false),
                encode(repo, false)
            ),
            json!({
                "title": new.title,
                "head": new.source,
                "base": new.target,
                "body": new.description,
                "draft": new.draft,
            }),
        ),
        HostedRepo::Gitlab { host, path } => (
            format!(
                "https://{host}/api/v4/projects/{}/merge_requests",
                encode(path, false)
            ),
            json!({
                "source_branch": new.source,
                "target_branch": new.target,
                // GitLab marks a draft by the start of the title.
                "title": if new.draft { format!("Draft: {}", new.title) } else { new.title.clone() },
                "description": new.description,
            }),
        ),
        HostedRepo::Azure { org, project, repo } => (
            format!(
                "https://dev.azure.com/{}/{}/_apis/git/repositories/{}/pullrequests?api-version=7.1",
                encode(org, false),
                encode(project, false),
                encode(repo, false)
            ),
            json!({
                "sourceRefName": format!("refs/heads/{}", new.source),
                "targetRefName": format!("refs/heads/{}", new.target),
                "title": new.title,
                "description": new.description,
                "isDraft": new.draft,
            }),
        ),
    }
}

/// The number of the pull request in the answer that created it.
pub fn created_id(repo: &HostedRepo, answer: &Value) -> Option<u64> {
    let key = match repo {
        HostedRepo::Github { .. } => "number",
        HostedRepo::Gitlab { .. } => "iid",
        HostedRepo::Azure { .. } => "pullRequestId",
    };
    answer.get(key)?.as_u64()
}

/// The address that tells the default branch of the repository.
pub fn default_branch_url(repo: &HostedRepo) -> String {
    match repo {
        HostedRepo::Github { owner, repo } => format!(
            "https://api.github.com/repos/{}/{}",
            encode(owner, false),
            encode(repo, false)
        ),
        HostedRepo::Gitlab { host, path } => {
            format!("https://{host}/api/v4/projects/{}", encode(path, false))
        }
        HostedRepo::Azure { org, project, repo } => format!(
            "https://dev.azure.com/{}/{}/_apis/git/repositories/{}?api-version=7.1",
            encode(org, false),
            encode(project, false),
            encode(repo, false)
        ),
    }
}

pub fn parse_default_branch(repo: &HostedRepo, answer: &Value) -> Option<String> {
    let name = match repo {
        HostedRepo::Azure { .. } => text(answer, "defaultBranch")
            .strip_prefix("refs/heads/")
            .unwrap_or(""),
        _ => text(answer, "default_branch"),
    };
    (!name.is_empty()).then(|| name.to_string())
}

/// What the host said was wrong, from the body of an error answer.
pub fn error_text(answer: &Value) -> Option<String> {
    let mut parts = Vec::new();
    match answer.get("message") {
        Some(Value::String(message)) => parts.push(message.clone()),
        Some(Value::Null) | None => {}
        Some(other) => parts.push(other.to_string()),
    }
    for detail in answer
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match detail {
            Value::String(message) => parts.push(message.clone()),
            other => {
                if let Some(message) = other.get("message").and_then(Value::as_str) {
                    parts.push(message.to_string());
                }
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn check_repo(repo: &HostedRepo) -> Result<(), String> {
    if let HostedRepo::Gitlab { host, path } = repo
        && (!valid_host(host) || path.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return Err("the GitLab address is not valid".to_string());
    }
    Ok(())
}

fn valid_branch(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-') && !name.chars().any(char::is_control)
}

fn authorized_agent(repo: &HostedRepo) -> Result<Option<(ureq::Agent, String)>, String> {
    let access = match token_for(&repo.host(), crate::git::tool_path()) {
        Ok(access) => access,
        Err(CliError::Missing | CliError::SignedOut) => return Ok(None),
        Err(CliError::Failed(message)) => return Err(message),
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into();
    Ok(Some((agent, format!("Bearer {}", access.token))))
}

/// The default branch of the repository; `None` when the user is not signed in or the host
/// does not say.
pub fn default_branch(repo: &HostedRepo) -> Result<Option<String>, String> {
    check_repo(repo)?;
    let Some((agent, authorization)) = authorized_agent(repo)? else {
        return Ok(None);
    };
    let mut response = agent
        .get(&default_branch_url(repo))
        .header("Authorization", &authorization)
        .header("Accept", "application/json")
        .header("User-Agent", "gitsune")
        .call()
        .map_err(|error| describe(&repo.host(), &error))?;
    if response.status().as_u16() >= 400 {
        return Ok(None);
    }
    let answer: Value = response.body_mut().read_json().unwrap_or(Value::Null);
    Ok(parse_default_branch(repo, &answer))
}

/// Creates the pull request and returns its number.
pub fn create_pull_request(repo: &HostedRepo, new: &NewPullRequest) -> Result<u64, String> {
    check_repo(repo)?;
    if !valid_branch(&new.source) || !valid_branch(&new.target) {
        return Err("the branch name is not valid".to_string());
    }
    if new.title.trim().is_empty() {
        return Err("the pull request needs a title".to_string());
    }
    if new.source == new.target {
        return Err("the source and the target are the same branch".to_string());
    }
    let host = repo.host();
    let Some((agent, authorization)) = authorized_agent(repo)? else {
        return Err(format!(
            "You are not signed in to {host} with its command line tool, so the pull request cannot be created here."
        ));
    };
    let (url, body) = create_request(repo, new);
    let mut response = agent
        .post(&url)
        .header("Authorization", &authorization)
        .header("Accept", "application/json")
        .header("User-Agent", "gitsune")
        .send_json(body)
        .map_err(|error| describe(&host, &error))?;
    let status = response.status().as_u16();
    let answer: Value = response.body_mut().read_json().unwrap_or(Value::Null);
    if status >= 400 {
        let said = error_text(&answer);
        return Err(match (status, said) {
            (401 | 403, _) => format!(
                "{host} refused the token: it may have expired or lack the right to create pull requests."
            ),
            (_, Some(said)) => format!("{host} refused it: {said}"),
            (code, None) => format!("{host} answered with the error {code}."),
        });
    }
    created_id(repo, &answer)
        .ok_or_else(|| format!("{host} created the pull request but did not say its number."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_pr(draft: bool) -> NewPullRequest {
        NewPullRequest {
            source: "feat/x y".into(),
            target: "main".into(),
            title: "Add x".into(),
            description: "Body".into(),
            draft,
        }
    }

    #[test]
    fn github_request_names_head_base_and_draft() {
        let repo = HostedRepo::Github {
            owner: "acme".into(),
            repo: "tool".into(),
        };
        let (url, body) = create_request(&repo, &new_pr(true));
        assert_eq!(url, "https://api.github.com/repos/acme/tool/pulls");
        assert_eq!(body["head"], "feat/x y");
        assert_eq!(body["base"], "main");
        assert_eq!(body["draft"], true);
    }

    #[test]
    fn gitlab_marks_a_draft_in_the_title() {
        let repo = HostedRepo::Gitlab {
            host: "gitlab.example.fr".into(),
            path: "group/sub/project".into(),
        };
        let (url, body) = create_request(&repo, &new_pr(true));
        assert_eq!(
            url,
            "https://gitlab.example.fr/api/v4/projects/group%2Fsub%2Fproject/merge_requests"
        );
        assert_eq!(body["title"], "Draft: Add x");
        assert_eq!(create_request(&repo, &new_pr(false)).1["title"], "Add x");
        assert_eq!(body["source_branch"], "feat/x y");
    }

    #[test]
    fn azure_uses_full_ref_names() {
        let repo = HostedRepo::Azure {
            org: "acme".into(),
            project: "My Project".into(),
            repo: "demo".into(),
        };
        let (url, body) = create_request(&repo, &new_pr(false));
        assert_eq!(
            url,
            "https://dev.azure.com/acme/My%20Project/_apis/git/repositories/demo/pullrequests?api-version=7.1"
        );
        assert_eq!(body["sourceRefName"], "refs/heads/feat/x y");
        assert_eq!(body["targetRefName"], "refs/heads/main");
        assert_eq!(body["isDraft"], false);
    }

    #[test]
    fn the_number_is_read_from_each_service() {
        let github = HostedRepo::Github {
            owner: "a".into(),
            repo: "b".into(),
        };
        let gitlab = HostedRepo::Gitlab {
            host: "gitlab.com".into(),
            path: "a/b".into(),
        };
        let azure = HostedRepo::Azure {
            org: "o".into(),
            project: "p".into(),
            repo: "r".into(),
        };
        assert_eq!(created_id(&github, &json!({"number": 7})), Some(7));
        assert_eq!(created_id(&gitlab, &json!({"iid": 8})), Some(8));
        assert_eq!(created_id(&azure, &json!({"pullRequestId": 9})), Some(9));
        assert_eq!(created_id(&azure, &json!({})), None);
    }

    #[test]
    fn the_default_branch_is_read_from_each_service() {
        let github = HostedRepo::Github {
            owner: "a".into(),
            repo: "b".into(),
        };
        let azure = HostedRepo::Azure {
            org: "o".into(),
            project: "p".into(),
            repo: "r".into(),
        };
        assert_eq!(
            parse_default_branch(&github, &json!({"default_branch": "develop"})),
            Some("develop".into())
        );
        assert_eq!(
            parse_default_branch(&azure, &json!({"defaultBranch": "refs/heads/main"})),
            Some("main".into())
        );
        assert_eq!(parse_default_branch(&azure, &json!({})), None);
    }

    #[test]
    fn an_error_answer_is_put_in_words() {
        assert_eq!(
            error_text(&json!({"message": "Validation Failed",
                "errors": [{"message": "A pull request already exists for acme:feat."}]})),
            Some("Validation Failed A pull request already exists for acme:feat.".into())
        );
        assert_eq!(
            error_text(&json!({"message": ["Another open merge request already exists"]})),
            Some("[\"Another open merge request already exists\"]".into())
        );
        assert_eq!(error_text(&json!({})), None);
    }

    #[test]
    fn a_bad_request_is_refused_before_anything_is_asked() {
        let repo = HostedRepo::Github {
            owner: "a".into(),
            repo: "b".into(),
        };
        let mut bad = new_pr(false);
        bad.title = "  ".into();
        assert!(create_pull_request(&repo, &bad).is_err());
        let mut same = new_pr(false);
        same.target = same.source.clone();
        assert!(create_pull_request(&repo, &same).is_err());
        let mut dash = new_pr(false);
        dash.source = "-x".into();
        assert!(create_pull_request(&repo, &dash).is_err());
    }
}

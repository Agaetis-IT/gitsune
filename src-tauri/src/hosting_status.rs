//! The open pull requests of a repository with their review and build state, for the badges on
//! the branches. One request reads the list (GitHub: one GraphQL query; GitLab and Azure DevOps:
//! the list of the REST API); Azure DevOps needs one more request per pull request for builds.
//! The tokens come from the command line tools, as for [`crate::hosting::find_pull_request`].

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::hosting::{CliError, HostedRepo, describe, encode, token_for, valid_host};

/// Pull requests read per repository; the most recent ones come first on every host.
const MAX_PULL_REQUESTS: usize = 50;
/// Azure DevOps pull requests whose builds are asked for.
const MAX_BUILD_LOOKUPS: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Review {
    /// Nobody reviewed it, or the host does not say.
    None,
    Approved,
    ChangesRequested,
    /// A review is required and missing.
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Checks {
    /// No build or check, or the host does not say.
    None,
    Pending,
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPr {
    /// The source branch, without `refs/heads/`.
    pub branch: String,
    pub id: u64,
    pub title: String,
    pub draft: bool,
    pub review: Review,
    pub checks: Checks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PullRequestList {
    /// Not signed in with the tool of the host (or it is not installed): nothing was asked.
    SignedOut,
    Listed {
        items: Vec<BranchPr>,
    },
}

const GITHUB_QUERY: &str = "query($owner: String!, $name: String!, $count: Int!) { \
repository(owner: $owner, name: $name) { \
pullRequests(states: OPEN, first: $count, orderBy: {field: UPDATED_AT, direction: DESC}) { \
nodes { number title isDraft headRefName reviewDecision \
headRepository { nameWithOwner } \
commits(last: 1) { nodes { commit { statusCheckRollup { state } } } } } } } }";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The pull requests of a GitHub GraphQL answer. Those from a fork are left out: their branch
/// is not one of this repository.
pub fn parse_github(owner: &str, repo: &str, answer: &Value) -> Vec<BranchPr> {
    let nodes = answer
        .pointer("/data/repository/pullRequests/nodes")
        .and_then(Value::as_array);
    let own = format!("{owner}/{repo}").to_lowercase();
    nodes
        .into_iter()
        .flatten()
        .filter_map(|pr| {
            let from = pr.pointer("/headRepository/nameWithOwner")?.as_str()?;
            if from.to_lowercase() != own {
                return None;
            }
            let review = match text(pr, "reviewDecision") {
                "APPROVED" => Review::Approved,
                "CHANGES_REQUESTED" => Review::ChangesRequested,
                "REVIEW_REQUIRED" => Review::Pending,
                _ => Review::None,
            };
            let checks = match pr
                .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
                .and_then(Value::as_str)
            {
                Some("SUCCESS") => Checks::Success,
                Some("FAILURE" | "ERROR") => Checks::Failure,
                Some("PENDING" | "EXPECTED") => Checks::Pending,
                _ => Checks::None,
            };
            Some(BranchPr {
                branch: text(pr, "headRefName").to_string(),
                id: pr.get("number")?.as_u64()?,
                title: text(pr, "title").to_string(),
                draft: pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                review,
                checks,
            })
        })
        .collect()
}

/// The merge requests of a GitLab list. Approvals are not part of the list, so the review is
/// left unknown.
pub fn parse_gitlab(answer: &Value) -> Vec<BranchPr> {
    answer
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mr| {
            let status = mr
                .pointer("/head_pipeline/status")
                .or_else(|| mr.pointer("/pipeline/status"))
                .and_then(Value::as_str);
            let checks = match status {
                Some("success") => Checks::Success,
                Some("failed") => Checks::Failure,
                Some(
                    "created"
                    | "pending"
                    | "running"
                    | "preparing"
                    | "waiting_for_resource"
                    | "scheduled",
                ) => Checks::Pending,
                _ => Checks::None,
            };
            let draft = mr.get("draft").and_then(Value::as_bool).unwrap_or(false)
                || mr
                    .get("work_in_progress")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            Some(BranchPr {
                branch: text(mr, "source_branch").to_string(),
                id: mr.get("iid")?.as_u64()?,
                title: text(mr, "title").to_string(),
                draft,
                review: Review::None,
                checks,
            })
        })
        .collect()
}

/// The pull requests of an Azure DevOps list (builds are filled in afterwards), with the
/// project id each needs to ask for its builds.
pub fn parse_azure(answer: &Value) -> Vec<(BranchPr, String)> {
    answer
        .get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|pr| {
            let branch = text(pr, "sourceRefName").strip_prefix("refs/heads/")?;
            let votes: Vec<i64> = pr
                .get("reviewers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|reviewer| reviewer.get("vote")?.as_i64())
                .collect();
            let review = if votes.iter().any(|vote| *vote < 0) {
                Review::ChangesRequested
            } else if votes.iter().any(|vote| *vote >= 5) {
                Review::Approved
            } else {
                Review::None
            };
            let project = pr
                .pointer("/repository/project/id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some((
                BranchPr {
                    branch: branch.to_string(),
                    id: pr.get("pullRequestId")?.as_u64()?,
                    title: text(pr, "title").to_string(),
                    draft: pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                    review,
                    checks: Checks::None,
                },
                project,
            ))
        })
        .collect()
}

/// The build state from the policy evaluations of an Azure DevOps pull request: only build
/// policies count.
pub fn azure_checks(answer: &Value) -> Checks {
    let statuses: Vec<&str> = answer
        .get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|evaluation| {
            evaluation
                .pointer("/configuration/type/displayName")
                .and_then(Value::as_str)
                == Some("Build")
        })
        .map(|evaluation| text(evaluation, "status"))
        .collect();
    if statuses.is_empty() {
        Checks::None
    } else if statuses.iter().any(|s| matches!(*s, "rejected" | "broken")) {
        Checks::Failure
    } else if statuses.iter().any(|s| matches!(*s, "running" | "queued")) {
        Checks::Pending
    } else if statuses
        .iter()
        .all(|s| matches!(*s, "approved" | "notApplicable"))
    {
        Checks::Success
    } else {
        Checks::None
    }
}

fn get_json(agent: &ureq::Agent, url: &str, authorization: &str) -> Result<Value, ureq::Error> {
    agent
        .get(url)
        .header("Authorization", authorization)
        .header("Accept", "application/json")
        .header("User-Agent", "git-hydra")
        .call()?
        .body_mut()
        .read_json()
}

fn azure_checks_of(
    agent: &ureq::Agent,
    org: &str,
    project: &str,
    project_id: &str,
    id: u64,
    authorization: &str,
) -> Checks {
    if project_id.is_empty() {
        return Checks::None;
    }
    let artifact = format!("vstfs:///CodeReview/CodeReviewId/{project_id}/{id}");
    let url = format!(
        "https://dev.azure.com/{}/{}/_apis/policy/evaluations?artifactId={}&api-version=7.1-preview.1",
        encode(org, false),
        encode(project, false),
        encode(&artifact, false),
    );
    get_json(agent, &url, authorization)
        .map(|answer| azure_checks(&answer))
        .unwrap_or(Checks::None)
}

/// Lists the open pull requests of the repository. Without a token for the host nothing is
/// asked.
pub fn list_pull_requests(repo: &HostedRepo) -> Result<PullRequestList, String> {
    if let HostedRepo::Gitlab { host, path } = repo
        && (!valid_host(host) || path.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return Err("the GitLab address is not valid".to_string());
    }
    let host = repo.host();
    let access = match token_for(&host, crate::git::tool_path()) {
        Ok(access) => access,
        Err(CliError::Missing | CliError::SignedOut) => return Ok(PullRequestList::SignedOut),
        Err(CliError::Failed(message)) => return Err(message),
    };
    let authorization = format!("Bearer {}", access.token);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let read_error = |error: ureq::Error| describe(&host, &error);

    let items = match repo {
        HostedRepo::Github { owner, repo: name } => {
            let body = json!({
                "query": GITHUB_QUERY,
                "variables": { "owner": owner, "name": name, "count": MAX_PULL_REQUESTS },
            });
            let answer: Value = agent
                .post("https://api.github.com/graphql")
                .header("Authorization", &authorization)
                .header("User-Agent", "git-hydra")
                .send_json(body)
                .map_err(read_error)?
                .body_mut()
                .read_json()
                .map_err(read_error)?;
            parse_github(owner, name, &answer)
        }
        HostedRepo::Gitlab { path, .. } => {
            let url = format!(
                "https://{host}/api/v4/projects/{}/merge_requests?state=opened&per_page={MAX_PULL_REQUESTS}",
                encode(path, false),
            );
            parse_gitlab(&get_json(&agent, &url, &authorization).map_err(read_error)?)
        }
        HostedRepo::Azure {
            org,
            project,
            repo: name,
        } => {
            let url = format!(
                "https://dev.azure.com/{}/{}/_apis/git/repositories/{}/pullrequests?searchCriteria.status=active&$top={MAX_PULL_REQUESTS}&api-version=7.1",
                encode(org, false),
                encode(project, false),
                encode(name, false),
            );
            let mut listed =
                parse_azure(&get_json(&agent, &url, &authorization).map_err(read_error)?);
            let jobs: Vec<(u64, String)> = listed
                .iter()
                .take(MAX_BUILD_LOOKUPS)
                .map(|(pr, project_id)| (pr.id, project_id.clone()))
                .collect();
            let checks: Vec<Checks> = std::thread::scope(|scope| {
                let handles: Vec<_> = jobs
                    .iter()
                    .map(|(id, project_id)| {
                        let (agent, authorization) = (&agent, &authorization);
                        scope.spawn(move || {
                            azure_checks_of(agent, org, project, project_id, *id, authorization)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or(Checks::None))
                    .collect()
            });
            for ((pr, _), checks) in listed.iter_mut().zip(checks) {
                pr.checks = checks;
            }
            listed.into_iter().map(|(pr, _)| pr).collect()
        }
    };
    Ok(PullRequestList::Listed { items })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_reads_review_checks_and_skips_forks() {
        let answer = json!({"data": {"repository": {"pullRequests": {"nodes": [
            {"number": 7, "title": "Add x", "isDraft": false, "headRefName": "feat/x",
             "reviewDecision": "APPROVED", "headRepository": {"nameWithOwner": "Acme/Tool"},
             "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "FAILURE"}}}]}},
            {"number": 8, "title": "Draft", "isDraft": true, "headRefName": "wip",
             "reviewDecision": null, "headRepository": {"nameWithOwner": "acme/tool"},
             "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]}},
            {"number": 9, "title": "From a fork", "isDraft": false, "headRefName": "main",
             "reviewDecision": "REVIEW_REQUIRED", "headRepository": {"nameWithOwner": "bob/tool"},
             "commits": {"nodes": []}},
        ]}}}});
        let items = parse_github("acme", "tool", &answer);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].branch, "feat/x");
        assert_eq!(items[0].review, Review::Approved);
        assert_eq!(items[0].checks, Checks::Failure);
        assert!(items[1].draft);
        assert_eq!(items[1].checks, Checks::None);
    }

    #[test]
    fn github_tolerates_an_unexpected_answer() {
        assert!(parse_github("a", "b", &json!({"errors": []})).is_empty());
    }

    #[test]
    fn gitlab_reads_the_pipeline_and_the_draft_flag() {
        let answer = json!([
            {"iid": 3, "title": "A", "source_branch": "a", "draft": true,
             "head_pipeline": {"status": "running"}},
            {"iid": 4, "title": "B", "source_branch": "b", "work_in_progress": true,
             "pipeline": {"status": "success"}},
            {"iid": 5, "title": "C", "source_branch": "c"},
        ]);
        let items = parse_gitlab(&answer);
        assert_eq!(items.len(), 3);
        assert_eq!((items[0].draft, items[0].checks), (true, Checks::Pending));
        assert_eq!((items[1].draft, items[1].checks), (true, Checks::Success));
        assert_eq!((items[2].draft, items[2].checks), (false, Checks::None));
        assert!(items.iter().all(|pr| pr.review == Review::None));
    }

    #[test]
    fn azure_reads_votes_and_the_source_branch() {
        let answer = json!({"value": [
            {"pullRequestId": 12, "title": "Fix", "isDraft": false,
             "sourceRefName": "refs/heads/fix/login/5310",
             "reviewers": [{"vote": 10}, {"vote": 0}],
             "repository": {"project": {"id": "proj-1"}}},
            {"pullRequestId": 13, "title": "Rework", "isDraft": true,
             "sourceRefName": "refs/heads/rework",
             "reviewers": [{"vote": 10}, {"vote": -5}], "repository": {"project": {}}},
            {"pullRequestId": 14, "title": "Tag", "sourceRefName": "refs/tags/v1"},
        ]});
        let items = parse_azure(&answer);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0.branch, "fix/login/5310");
        assert_eq!(items[0].0.review, Review::Approved);
        assert_eq!(items[0].1, "proj-1");
        assert_eq!(items[1].0.review, Review::ChangesRequested);
        assert!(items[1].0.draft);
    }

    #[test]
    fn azure_builds_come_from_build_policies_only() {
        let build = |status: &str| json!({"status": status, "configuration": {"type": {"displayName": "Build"}}});
        let other = json!({"status": "rejected",
            "configuration": {"type": {"displayName": "Minimum number of reviewers"}}});
        assert_eq!(
            azure_checks(&json!({"value": [other.clone()]})),
            Checks::None
        );
        assert_eq!(
            azure_checks(&json!({"value": [build("approved"), other.clone()]})),
            Checks::Success
        );
        assert_eq!(
            azure_checks(&json!({"value": [build("approved"), build("running")]})),
            Checks::Pending
        );
        assert_eq!(
            azure_checks(&json!({"value": [build("running"), build("rejected")]})),
            Checks::Failure
        );
        assert_eq!(azure_checks(&json!({})), Checks::None);
    }

    #[test]
    fn the_list_has_the_shape_the_frontend_reads() {
        let item = BranchPr {
            branch: "a".into(),
            id: 1,
            title: "t".into(),
            draft: false,
            review: Review::ChangesRequested,
            checks: Checks::Pending,
        };
        let value = serde_json::to_value(PullRequestList::Listed { items: vec![item] }).unwrap();
        assert_eq!(value["state"], "listed");
        assert_eq!(value["items"][0]["review"], "changesRequested");
        assert_eq!(value["items"][0]["checks"], "pending");
        assert_eq!(
            serde_json::to_value(PullRequestList::SignedOut).unwrap()["state"],
            "signedOut"
        );
    }
}

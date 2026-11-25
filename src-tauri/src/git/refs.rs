use gix::ThreadSafeRepository;
use gix::refs::Category;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefInfo {
    /// Short name, e.g. `main`, `origin/main`, `v1.0.0`.
    pub name: String,
    pub full_name: String,
    pub kind: RefKind,
    /// Commit id the ref points to (annotated tags are peeled).
    pub target: String,
    pub is_head: bool,
}

/// Lists local branches, remote branches and tags, sorted by kind then name.
pub fn list_refs(repo: &ThreadSafeRepository) -> Result<Vec<RefInfo>, String> {
    let repo = repo.to_thread_local();
    let head_full = repo
        .head_name()
        .map_err(|e| format!("cannot read HEAD: {e}"))?
        .map(|n| n.to_string());

    let platform = repo.references().map_err(|e| e.to_string())?;
    let all = platform.all().map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for reference in all {
        let mut reference = reference.map_err(|e| e.to_string())?;
        let full_name = reference.name().as_bstr().to_string();
        let Some((category, short)) = reference.name().category_and_short_name() else {
            continue;
        };
        let short = short.to_string();
        let kind = match category {
            Category::LocalBranch => RefKind::Local,
            Category::RemoteBranch => RefKind::Remote,
            Category::Tag => RefKind::Tag,
            _ => continue,
        };
        // Skip symbolic refs such as `refs/remotes/origin/HEAD` that cannot be peeled.
        let Ok(target) = reference.peel_to_id() else {
            continue;
        };
        out.push(RefInfo {
            name: short,
            is_head: head_full.as_deref() == Some(full_name.as_str()),
            full_name,
            kind,
            target: target.to_string(),
        });
    }
    out.sort_by(|a, b| (kind_order(a.kind), &a.name).cmp(&(kind_order(b.kind), &b.name)));
    Ok(out)
}

fn kind_order(kind: RefKind) -> u8 {
    match kind {
        RefKind::Local => 0,
        RefKind::Remote => 1,
        RefKind::Tag => 2,
    }
}

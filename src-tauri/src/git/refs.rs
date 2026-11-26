use gix::ThreadSafeRepository;
use gix::refs::Category;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
    /// A stash, shown in the graph only (never in the lists of branches and tags).
    Stash,
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
        RefKind::Stash => 3,
    }
}

/// The name that hides every stash from the graph (with the other full ref names of `hidden`).
pub const STASH_REF: &str = "refs/stash";

/// The stashes of the repository, most recent first (`stash@{0}`), as refs for the graph.
///
/// They are read from the reflog of `refs/stash`, the only place git keeps the older ones.
/// An unreadable reflog gives no stash.
pub fn list_stash_refs(repo: &ThreadSafeRepository) -> Vec<RefInfo> {
    let repo = repo.to_thread_local();
    let Ok(log) = std::fs::read_to_string(repo.common_dir().join("logs").join(STASH_REF)) else {
        return Vec::new();
    };
    stash_refs_from_log(&log)
}

/// One line of the reflog is `<old id> <new id> <who and when>\t<message>`: the new id is the
/// stash, and the last line is the most recent one.
fn stash_refs_from_log(log: &str) -> Vec<RefInfo> {
    let ids: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|id| id.len() >= 40 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .collect();
    ids.iter()
        .rev()
        .enumerate()
        .map(|(index, id)| RefInfo {
            name: format!("stash@{{{index}}}"),
            full_name: format!("{STASH_REF}@{{{index}}}"),
            kind: RefKind::Stash,
            target: (*id).to_string(),
            is_head: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const ZERO: &str = "0000000000000000000000000000000000000000";

    #[test]
    fn the_last_line_of_the_reflog_is_stash_zero() {
        let log = format!(
            "{ZERO} {A} Ada <ada@example.invalid> 1700000000 +0000\tWIP on main: first\n\
             {A} {B} Ada <ada@example.invalid> 1700000100 +0000\tOn main: second\n"
        );
        let refs = stash_refs_from_log(&log);
        assert_eq!(refs.len(), 2);
        assert_eq!(
            (refs[0].name.as_str(), refs[0].target.as_str()),
            ("stash@{0}", B)
        );
        assert_eq!(
            (refs[1].name.as_str(), refs[1].target.as_str()),
            ("stash@{1}", A)
        );
        assert_eq!(refs[0].full_name, "refs/stash@{0}");
        assert!(refs.iter().all(|r| r.kind == RefKind::Stash && !r.is_head));
    }

    #[test]
    fn lines_without_a_valid_id_are_left_out() {
        assert!(stash_refs_from_log("").is_empty());
        assert!(stash_refs_from_log("not a reflog line\nx y z").is_empty());
    }
}

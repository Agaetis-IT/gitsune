//! Lane naming helpers: merge-subject parsing and stable color assignment.

/// Number of colors in the frontend palette; lane colors are indices below this.
pub const PALETTE_SIZE: u32 = 12;

/// Stable color index for a lane name (FNV-1a hash, so it never changes between runs).
pub fn color_for(key: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in key.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash % PALETTE_SIZE
}

/// Strips the remote name from a remote-tracking branch (`origin/feature/x` -> `feature/x`).
pub fn strip_remote(name: &str) -> &str {
    name.split_once('/').map_or(name, |(_, rest)| rest)
}

/// Extracts the merged branch name from a merge commit subject, when recognizable.
///
/// Handles `Merge branch 'x'`, `Merge branches 'x' and 'y'` (first one),
/// `Merge remote-tracking branch 'origin/x'` and `Merge pull request #1 from owner/x`.
pub fn merge_branch_name(subject: &str) -> Option<String> {
    let subject = subject.trim();
    let name = if let Some(rest) = subject.strip_prefix("Merge pull request ") {
        let (_, source) = rest.split_once(" from ")?;
        strip_remote(source.trim())
    } else if let Some(rest) = subject.strip_prefix("Merge remote-tracking branch ") {
        strip_remote(first_quoted(rest)?)
    } else if let Some(rest) = subject
        .strip_prefix("Merge branch ")
        .or_else(|| subject.strip_prefix("Merge branches "))
    {
        first_quoted(rest)?
    } else {
        return None;
    };
    (!name.is_empty()).then(|| name.to_string())
}

fn first_quoted(text: &str) -> Option<&str> {
    let start = text.find('\'')? + 1;
    let len = text[start..].find('\'')?;
    Some(&text[start..start + len])
}

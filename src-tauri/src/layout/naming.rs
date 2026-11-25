//! Lane naming helpers: merge-subject parsing and stable color assignment.

use std::collections::HashMap;

/// Number of colors in the frontend palette; lane colors are indices below this.
pub const PALETTE_SIZE: u32 = 12;

/// FNV-1a hash: the same on every run and platform.
fn hash(key: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in key.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// The main branches have a color of their own, so they are recognized at a glance and never
/// change with the other branches that are shown or with the branch that is checked out.
const FIXED_COLORS: [(&str, u32); 3] = [("main", 6), ("master", 7), ("develop", 3)];

/// The color a main branch always has (`main`, `master`, `develop`).
pub fn fixed_color(key: &str) -> Option<u32> {
    FIXED_COLORS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|&(_, color)| color)
}

/// The color a name asks for: stable, but two names can ask for the same one.
pub fn color_for(key: &str) -> u32 {
    fixed_color(key).unwrap_or(hash(key) % PALETTE_SIZE)
}

/// Picks a palette color for every name so that names shown at the same time (their rows
/// overlap) get different colors whenever the palette allows it.
///
/// The main branches come first and always keep their own color. Every other name asks for its
/// hash color and keeps it unless an overlapping name already took it;
/// then it gets the next free one (or, when more names overlap than the palette has colors,
/// the least used one). Names are handled in hash order, so the result only depends on the
/// names and their extents, not on the order of the lanes.
///
/// `names` are (name, first row, last row); names given twice are merged into one extent.
pub fn assign_colors(names: &[(&str, usize, usize)]) -> HashMap<String, u32> {
    let mut extents: HashMap<&str, (usize, usize)> = HashMap::new();
    for &(name, first, last) in names {
        let extent = extents.entry(name).or_insert((first, last));
        extent.0 = extent.0.min(first);
        extent.1 = extent.1.max(last);
    }
    let mut ordered: Vec<(&str, (usize, usize))> = extents.into_iter().collect();
    ordered.sort_by_key(|(name, _)| (fixed_color(name).is_none(), hash(name), *name));

    let mut assigned: Vec<(usize, usize, u32)> = Vec::with_capacity(ordered.len());
    let mut colors = HashMap::with_capacity(ordered.len());
    for (name, (first, last)) in ordered {
        let mut uses = [0u32; PALETTE_SIZE as usize];
        for &(other_first, other_last, color) in &assigned {
            if other_first <= last && first <= other_last {
                uses[color as usize] += 1;
            }
        }
        let preferred = color_for(name);
        if fixed_color(name).is_some() {
            assigned.push((first, last, preferred));
            colors.insert(name.to_string(), preferred);
            continue;
        }
        let color = (0..PALETTE_SIZE)
            .map(|step| (preferred + step) % PALETTE_SIZE)
            .min_by_key(|&color| uses[color as usize])
            .expect("the palette is not empty");
        assigned.push((first, last, color));
        colors.insert(name.to_string(), color);
    }
    colors
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
    } else {
        // Anything else is not a merge subject the application knows.
        let rest = subject
            .strip_prefix("Merge branch ")
            .or_else(|| subject.strip_prefix("Merge branches "))?;
        first_quoted(rest)?
    };
    (!name.is_empty()).then(|| name.to_string())
}

fn first_quoted(text: &str) -> Option<&str> {
    let start = text.find('\'')? + 1;
    let len = text[start..].find('\'')?;
    Some(&text[start..start + len])
}

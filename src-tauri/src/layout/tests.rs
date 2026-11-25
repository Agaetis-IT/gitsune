//! Layout tests on invented graphs. Commits are listed newest first, parents by name.

use super::naming::{PALETTE_SIZE, color_for, merge_branch_name, strip_remote};
use super::*;

fn graph(spec: &[(&str, &[&str])]) -> Vec<Commit> {
    graph_with_subjects(&spec.iter().map(|(n, p)| (*n, *p, "")).collect::<Vec<_>>())
}

fn graph_with_subjects(spec: &[(&str, &[&str], &str)]) -> Vec<Commit> {
    let row_of = |name: &str| spec.iter().position(|(n, _, _)| *n == name).unwrap();
    spec.iter()
        .map(|(_, parents, subject)| Commit {
            parents: parents.iter().map(|p| row_of(p)).collect(),
            subject: subject.to_string(),
        })
        .collect()
}

fn label(row: usize, name: &str, kind: RefKind, is_head: bool) -> RefLabel {
    RefLabel {
        row,
        name: name.to_string(),
        kind,
        is_head,
    }
}

fn cols(layout: &Layout) -> Vec<u32> {
    layout.rows.iter().map(|r| r.col).collect()
}

// ---------- structure ----------

#[test]
fn empty_graph_has_no_rows() {
    let layout = layout(&[], &[]);
    assert!(layout.rows.is_empty() && layout.lanes.is_empty());
    assert_eq!(layout.max_cols, 0);
}

#[test]
fn linear_history_is_a_single_straight_lane() {
    let commits = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 0, 0]);
    assert_eq!(layout.lanes.len(), 1);
    assert!(layout.rows.iter().all(|r| r.edges.is_empty()));
    assert_eq!(
        (layout.lanes[0].first_row, layout.lanes[0].last_row),
        (0, 2)
    );
    assert_eq!(layout.max_cols, 1);
}

#[test]
fn branch_and_merge_forks_to_the_right_and_converges_back() {
    //  M  merge of B and F2
    //  F2 (feature)   B (main)
    //  F1 (feature)   |
    //  A  fork point
    let commits = graph(&[
        ("M", &["B", "F2"]),
        ("F2", &["F1"]),
        ("B", &["A"]),
        ("F1", &["A"]),
        ("A", &[]),
    ]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 1, 0, 1, 0]);
    assert_eq!(
        layout.rows[0].edges,
        vec![Edge {
            lane: 1,
            from_col: 0,
            to_col: 1,
            bend: Bend::Start
        }]
    );
    assert_eq!(
        layout.rows[3].edges,
        vec![Edge {
            lane: 1,
            from_col: 1,
            to_col: 0,
            bend: Bend::End
        }]
    );
    assert_eq!(
        (layout.lanes[1].first_row, layout.lanes[1].last_row),
        (0, 4)
    );
    assert_eq!(layout.max_cols, 2);
}

#[test]
fn octopus_merge_spreads_parents_over_adjacent_columns() {
    let commits = graph(&[
        ("O", &["P1", "P2", "P3"]),
        ("P1", &["R"]),
        ("P2", &["R"]),
        ("P3", &["R"]),
        ("R", &[]),
    ]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 0, 1, 2, 0]);
    assert_eq!(layout.rows[0].edges.len(), 2);
    assert!(layout.lanes[1..].iter().all(|l| l.forked && l.joins));
    // Both side lanes run down to R and bend into its column in the last span.
    let ends: Vec<_> = layout.rows[3]
        .edges
        .iter()
        .map(|e| (e.from_col, e.to_col))
        .collect();
    assert_eq!(ends, vec![(1, 0), (2, 0)]);
}

#[test]
fn separate_histories_get_their_own_lanes_and_reuse_freed_columns() {
    // Two unrelated chains: the second tip takes column 1 while the first is alive,
    // and a third chain started after the first ended reuses column 0.
    let commits = graph(&[
        ("x2", &["x1"]),
        ("y2", &["y1"]),
        ("x1", &[]),
        ("z1", &[]),
        ("y1", &[]),
    ]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 1, 0, 0, 1]);
    assert_eq!(layout.lanes.len(), 3);
}

#[test]
fn several_children_of_one_commit_converge_on_the_leftmost_lane() {
    // Two branch tips on top of the same base.
    let commits = graph(&[("t1", &["base"]), ("t2", &["base"]), ("base", &[])]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 1, 0]);
    assert_eq!(
        layout.rows[1].edges,
        vec![Edge {
            lane: 1,
            from_col: 1,
            to_col: 0,
            bend: Bend::End
        }]
    );
}

#[test]
fn merging_the_very_next_commit_bends_once_from_the_merge_commit() {
    // The second parent is the next row: fork and converge happen in the same span.
    let commits = graph(&[("M", &["A", "B"]), ("B", &["A"]), ("A", &[])]);
    let layout = layout(&commits, &[]);
    assert_eq!(cols(&layout), vec![0, 1, 0]);
    assert_eq!(
        layout.rows[1].edges,
        vec![Edge {
            lane: 1,
            from_col: 1,
            to_col: 0,
            bend: Bend::End
        }]
    );
}

#[test]
fn layout_is_deterministic() {
    let commits = graph(&[
        ("M", &["B", "F2"]),
        ("F2", &["F1"]),
        ("B", &["A"]),
        ("F1", &["A"]),
        ("A", &[]),
    ]);
    let refs = [label(0, "main", RefKind::Local, true)];
    assert_eq!(layout(&commits, &refs), layout(&commits, &refs));
}

// ---------- naming and colors ----------

#[test]
fn lane_takes_the_best_ref_head_then_local_then_remote_then_tag() {
    let commits = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    let refs = [
        label(0, "v9", RefKind::Tag, false),
        label(0, "origin/main", RefKind::Remote, false),
        label(1, "old", RefKind::Local, false),
        label(2, "main", RefKind::Local, true),
    ];
    let layout = layout(&commits, &refs);
    assert_eq!(layout.lanes[0].name.as_deref(), Some("main"));
    assert_eq!(layout.lanes[0].kind, LaneKind::Branch);

    // Without the head ref the earliest local branch wins over the remote and the tag.
    let layout = super::layout(&commits, &refs[..3]);
    assert_eq!(layout.lanes[0].name.as_deref(), Some("old"));
    let layout = super::layout(&commits, &refs[..2]);
    assert_eq!(layout.lanes[0].name.as_deref(), Some("origin/main"));
    let layout = super::layout(&commits, &refs[..1]);
    assert_eq!(layout.lanes[0].name.as_deref(), Some("v9"));
}

#[test]
fn a_local_branch_and_its_remote_share_the_same_color() {
    let commits = graph(&[("a", &[]), ("b", &[])]);
    let local = layout(&commits, &[label(0, "feature/x", RefKind::Local, false)]);
    let remote = layout(
        &commits,
        &[label(0, "origin/feature/x", RefKind::Remote, false)],
    );
    assert_eq!(local.lanes[0].color, remote.lanes[0].color);
    assert_eq!(local.lanes[0].color, Some(color_for("feature/x")));
}

#[test]
fn merged_lane_without_ref_is_named_after_the_merge_subject() {
    let commits = graph_with_subjects(&[
        ("M", &["B", "F"], "Merge branch 'feature/login'"),
        ("F", &["A"], ""),
        ("B", &["A"], ""),
        ("A", &[], ""),
    ]);
    let layout = layout(&commits, &[label(0, "main", RefKind::Local, true)]);
    let merged = &layout.lanes[1];
    assert_eq!(merged.name.as_deref(), Some("feature/login"));
    assert_eq!(merged.kind, LaneKind::Merged);
    assert_eq!(merged.color, Some(color_for("feature/login")));
}

#[test]
fn a_surviving_ref_wins_over_the_merge_subject() {
    let commits = graph_with_subjects(&[
        ("M", &["B", "F"], "Merge branch 'login'"),
        ("F", &["A"], ""),
        ("B", &["A"], ""),
        ("A", &[], ""),
    ]);
    let refs = [label(1, "feature/login", RefKind::Local, false)];
    let layout = layout(&commits, &refs);
    assert_eq!(layout.lanes[1].name.as_deref(), Some("feature/login"));
    assert_eq!(layout.lanes[1].kind, LaneKind::Branch);
}

#[test]
fn unnamed_lane_is_anonymous_without_color() {
    let commits = graph_with_subjects(&[
        ("M", &["B", "F"], "Integrate things"),
        ("F", &["A"], ""),
        ("B", &["A"], ""),
        ("A", &[], ""),
    ]);
    let layout = layout(&commits, &[]);
    assert_eq!(layout.lanes[1].kind, LaneKind::Anonymous);
    assert_eq!(
        (layout.lanes[1].name.clone(), layout.lanes[1].color),
        (None, None)
    );
}

#[test]
fn merge_subjects_are_parsed() {
    let cases = [
        ("Merge branch 'feature/x'", Some("feature/x")),
        ("Merge branch 'x' into 'main'", Some("x")),
        ("Merge branches 'a' and 'b'", Some("a")),
        (
            "Merge remote-tracking branch 'origin/feature/y'",
            Some("feature/y"),
        ),
        (
            "Merge pull request #42 from fakeuser/fix-bug",
            Some("fix-bug"),
        ),
        (
            "Merge pull request #7 from fakeuser/feature/z",
            Some("feature/z"),
        ),
        ("Merge tag 'v1.0'", None),
        ("Merge branch ''", None),
        ("Update docs", None),
    ];
    for (subject, expected) in cases {
        assert_eq!(merge_branch_name(subject).as_deref(), expected, "{subject}");
    }
}

#[test]
fn colors_are_stable_and_within_the_palette() {
    assert_eq!(color_for("main"), color_for("main"));
    for name in ["main", "develop", "feature/a", "feature/b", "release/1.0"] {
        assert!(color_for(name) < PALETTE_SIZE);
    }
    assert_eq!(strip_remote("origin/main"), "main");
    assert_eq!(strip_remote("main"), "main");
}

// ---------- topological order ----------

fn raw(spec: &[(i64, &[usize])]) -> Vec<RawCommit> {
    spec.iter()
        .map(|(time, parents)| RawCommit {
            parents: parents.to_vec(),
            time: *time,
        })
        .collect()
}

#[test]
fn topological_order_puts_children_first_and_newest_first_among_ready() {
    // 0 <- 1 <- 3 and 0 <- 2 <- 3 style diamond, plus an independent tip.
    // index: 0=root(t=1), 1=left(t=5), 2=right(t=3), 3=merge(t=9), 4=other tip(t=4)
    let commits = raw(&[(1, &[]), (5, &[0]), (3, &[0]), (9, &[1, 2]), (4, &[0])]);
    assert_eq!(topological_order(&commits), vec![3, 1, 4, 2, 0]);
}

#[test]
fn topological_order_survives_clock_skew() {
    // The parent claims to be newer than its child.
    let commits = raw(&[(100, &[]), (1, &[0])]);
    assert_eq!(topological_order(&commits), vec![1, 0]);
}

#[test]
fn topological_order_breaks_ties_by_index() {
    let commits = raw(&[(7, &[]), (7, &[]), (7, &[])]);
    assert_eq!(topological_order(&commits), vec![0, 1, 2]);
}

// ---------- invariants on random graphs ----------

/// Checks that the layout really draws what the graph says.
pub(crate) fn validate(commits: &[Commit], layout: &Layout) -> Result<(), String> {
    let n = commits.len();
    if layout.rows.len() != n {
        return Err(format!("{} rows for {n} commits", layout.rows.len()));
    }
    let lane_ok = |id: u32| (id as usize) < layout.lanes.len();

    let mut occupied: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
    for (i, row) in layout.rows.iter().enumerate() {
        if !lane_ok(row.lane) {
            return Err(format!("row {i}: unknown lane"));
        }
        let lane = &layout.lanes[row.lane as usize];
        if row.col != lane.col || row.col >= layout.max_cols {
            return Err(format!(
                "row {i}: column {} not lane column {}",
                row.col, lane.col
            ));
        }
        if !(lane.first_row <= i && i <= lane.last_row) {
            return Err(format!("row {i}: outside its lane extent"));
        }
        for e in &row.edges {
            if !lane_ok(e.lane) || e.from_col >= layout.max_cols || e.to_col >= layout.max_cols {
                return Err(format!("row {i}: bad edge {e:?}"));
            }
        }
        occupied[i].push((row.col, row.lane));
    }
    for lane in &layout.lanes {
        // Vertical presence strictly between first and last row.
        for slot in occupied
            .iter_mut()
            .take(lane.last_row)
            .skip(lane.first_row + 1)
        {
            slot.push((lane.col, lane.id));
        }
    }
    for (i, slot) in occupied.iter_mut().enumerate() {
        slot.sort_unstable();
        slot.dedup();
        if slot.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(format!("row {i}: two lanes share a column: {slot:?}"));
        }
    }

    for lane in &layout.lanes {
        let starts_with_edge = layout.rows[lane.first_row]
            .edges
            .iter()
            .any(|e| e.lane == lane.id && e.bend == Bend::Start);
        if lane.forked != starts_with_edge {
            return Err(format!(
                "lane {}: forked={} but start edge={starts_with_edge}",
                lane.id, lane.forked
            ));
        }
        let ends_at_node = layout.rows[lane.last_row].lane == lane.id;
        if lane.joins == ends_at_node {
            return Err(format!(
                "lane {}: joins={} but ends_at_node={ends_at_node}",
                lane.id, lane.joins
            ));
        }
        let has_join_edge = lane.last_row > 0
            && layout.rows[lane.last_row - 1]
                .edges
                .iter()
                .any(|e| e.lane == lane.id);
        if lane.joins && !has_join_edge {
            return Err(format!(
                "lane {}: joins without an edge into row {}",
                lane.id, lane.last_row
            ));
        }
    }

    for (i, commit) in commits.iter().enumerate() {
        let row = &layout.rows[i];
        let forks: Vec<u32> = row
            .edges
            .iter()
            .filter(|e| e.lane != row.lane && layout.lanes[e.lane as usize].first_row == i)
            .map(|e| e.lane)
            .collect();
        if forks.len() != commit.parents.len().saturating_sub(1) {
            return Err(format!(
                "row {i}: {} forks for {} parents",
                forks.len(),
                commit.parents.len()
            ));
        }
        for (k, &parent) in commit.parents.iter().enumerate() {
            let lane_id = if k == 0 { row.lane } else { forks[k - 1] };
            // The lane must not be used by an unrelated commit before reaching the parent.
            if (i + 1..parent).any(|r| layout.rows[r].lane == lane_id) {
                return Err(format!(
                    "row {i}: lane {lane_id} crosses a foreign commit before {parent}"
                ));
            }
            if layout.rows[parent].lane != lane_id {
                let before = &layout.rows[parent - 1];
                let arrives = before
                    .edges
                    .iter()
                    .any(|e| e.lane == lane_id && e.to_col == layout.rows[parent].col);
                if !arrives || layout.lanes[lane_id as usize].last_row != parent {
                    return Err(format!(
                        "row {i}: lane {lane_id} does not arrive at parent {parent}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Small deterministic generator so failures are reproducible.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }
}

fn random_graph(seed: u64, n: usize) -> Vec<Commit> {
    let mut rng = Lcg(seed);
    (0..n)
        .map(|i| {
            let mut parents = Vec::new();
            let available = n - 1 - i;
            if available > 0 && rng.next(50) != 0 {
                let wanted = match rng.next(20) {
                    0 => 3,
                    1..=3 => 2,
                    _ => 1,
                };
                while parents.len() < wanted.min(available) {
                    let parent = i + 1 + rng.next(available.min(25));
                    if !parents.contains(&parent) {
                        parents.push(parent);
                    }
                }
            }
            let subject = if parents.len() > 1 {
                format!("Merge branch 'topic-{i}'")
            } else {
                String::new()
            };
            Commit { parents, subject }
        })
        .collect()
}

#[test]
fn random_graphs_keep_every_invariant() {
    for seed in 0..40 {
        for n in [1, 2, 5, 40, 400] {
            let commits = random_graph(seed, n);
            let layout = layout(&commits, &[]);
            if let Err(message) = validate(&commits, &layout) {
                panic!("seed {seed}, n {n}: {message}");
            }
        }
    }
}

// ---------- color assignment ----------

use super::naming::assign_colors;

/// Names whose hash colors collide, found by brute force so the test does not depend on the hash.
fn colliding_names(count: usize) -> Vec<String> {
    let mut by_color: std::collections::HashMap<u32, Vec<String>> =
        std::collections::HashMap::new();
    for n in 0.. {
        let name = format!("feature/topic-{n}");
        let group = by_color.entry(color_for(&name)).or_default();
        group.push(name);
        if group.len() == count {
            return group.clone();
        }
    }
    unreachable!()
}

#[test]
fn a_name_keeps_the_color_it_asks_for_when_nothing_competes() {
    let colors = assign_colors(&[("main", 0, 100), ("feature/x", 10, 20)]);
    // Two names can still ask for the same color: avoiding that is the point of the next tests.
    if color_for("main") != color_for("feature/x") {
        assert_eq!(colors["main"], color_for("main"));
        assert_eq!(colors["feature/x"], color_for("feature/x"));
    }
    let alone = assign_colors(&[("only", 0, 5)]);
    assert_eq!(alone["only"], color_for("only"));
}

#[test]
fn names_shown_at_the_same_time_get_different_colors() {
    let names = colliding_names(4);
    let entries: Vec<(&str, usize, usize)> = names.iter().map(|n| (n.as_str(), 0, 50)).collect();
    let colors = assign_colors(&entries);
    let distinct: std::collections::HashSet<u32> = colors.values().copied().collect();
    assert_eq!(distinct.len(), 4, "{colors:?}");
}

#[test]
fn names_that_never_meet_may_share_a_color() {
    let names = colliding_names(2);
    let colors = assign_colors(&[(names[0].as_str(), 0, 10), (names[1].as_str(), 11, 20)]);
    assert_eq!(
        colors[&names[0]], colors[&names[1]],
        "no need to move a name that has no neighbour"
    );
    // Touching extents do overlap.
    let touching = assign_colors(&[(names[0].as_str(), 0, 10), (names[1].as_str(), 10, 20)]);
    assert_ne!(touching[&names[0]], touching[&names[1]]);
}

#[test]
fn more_overlapping_names_than_colors_spread_evenly() {
    let entries: Vec<(String, usize, usize)> =
        (0..30).map(|n| (format!("branch-{n}"), 0, 100)).collect();
    let borrowed: Vec<(&str, usize, usize)> = entries
        .iter()
        .map(|(n, a, b)| (n.as_str(), *a, *b))
        .collect();
    let colors = assign_colors(&borrowed);
    let mut counts = [0u32; PALETTE_SIZE as usize];
    for color in colors.values() {
        assert!(*color < PALETTE_SIZE);
        counts[*color as usize] += 1;
    }
    // 30 names over 12 colors: two or three per color, never a pile-up.
    assert!(counts.iter().all(|&c| (2..=3).contains(&c)), "{counts:?}");
}

#[test]
fn colors_do_not_depend_on_the_order_the_lanes_are_listed() {
    let names = colliding_names(3);
    let forward: Vec<(&str, usize, usize)> = names.iter().map(|n| (n.as_str(), 0, 9)).collect();
    let mut backward = forward.clone();
    backward.reverse();
    assert_eq!(assign_colors(&forward), assign_colors(&backward));
}

#[test]
fn a_name_on_several_lanes_has_one_color_and_one_merged_extent() {
    // `main` and `origin/main` are two lanes with the same color name.
    let colors = assign_colors(&[("main", 0, 10), ("main", 50, 60), ("other", 20, 30)]);
    assert_eq!(colors.len(), 2);
    // `other` sits inside the merged extent of `main`, so they must differ.
    assert_ne!(colors["main"], colors["other"]);
}

#[test]
fn the_layout_gives_overlapping_branches_different_colors() {
    // Many parallel branches with colliding names, drawn on top of each other.
    let names = colliding_names(5);
    let mut spec: Vec<(String, Vec<String>)> = Vec::new();
    for (i, _) in names.iter().enumerate() {
        spec.push((format!("t{i}"), vec![format!("b{i}")]));
    }
    for i in 0..names.len() {
        spec.push((format!("b{i}"), vec!["root".to_string()]));
    }
    spec.push(("root".to_string(), vec![]));
    let borrowed: Vec<(&str, Vec<&str>)> = spec
        .iter()
        .map(|(n, p)| (n.as_str(), p.iter().map(String::as_str).collect()))
        .collect();
    let commits = graph(
        &borrowed
            .iter()
            .map(|(n, p)| (*n, p.as_slice()))
            .collect::<Vec<_>>(),
    );
    let refs: Vec<RefLabel> = names
        .iter()
        .enumerate()
        .map(|(i, n)| label(i, n, RefKind::Local, false))
        .collect();

    let layout = layout(&commits, &refs);
    let colors: std::collections::HashSet<_> = layout
        .lanes
        .iter()
        .filter(|l| l.name.is_some())
        .map(|l| l.color)
        .collect();
    assert_eq!(
        colors.len(),
        5,
        "{:?}",
        layout
            .lanes
            .iter()
            .map(|l| (&l.name, l.color))
            .collect::<Vec<_>>()
    );
}

// ---------- colors do not depend on the checked-out branch ----------

#[test]
fn switching_branches_does_not_repaint_a_lane() {
    // One lane carries `feature/x` and `develop`: the checked-out one names the lane, but
    // `develop` colors it, whichever branch is checked out.
    let commits = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    let refs = |head: &str| {
        vec![
            label(0, "feature/x", RefKind::Local, head == "feature/x"),
            label(1, "develop", RefKind::Local, head == "develop"),
        ]
    };
    let on_feature = layout(&commits, &refs("feature/x"));
    let on_develop = layout(&commits, &refs("develop"));
    assert_eq!(on_feature.lanes[0].name.as_deref(), Some("feature/x"));
    assert_eq!(on_develop.lanes[0].name.as_deref(), Some("develop"));
    assert_eq!(on_feature.lanes[0].color, Some(color_for("develop")));
    assert_eq!(on_develop.lanes[0].color, on_feature.lanes[0].color);
}

#[test]
fn two_ordinary_branches_on_a_lane_keep_their_color_when_the_head_moves() {
    let commits = graph(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
    let refs = |head: &str| {
        vec![
            label(0, "feature/x", RefKind::Local, head == "feature/x"),
            label(1, "feature/y", RefKind::Local, head == "feature/y"),
        ]
    };
    let on_x = layout(&commits, &refs("feature/x"));
    let on_y = layout(&commits, &refs("feature/y"));
    assert_eq!(on_x.lanes[0].color, on_y.lanes[0].color);
}

#[test]
fn the_main_branches_keep_their_own_color_among_any_crowd() {
    use super::naming::fixed_color;
    let crowd = colliding_names(12);
    let mut entries: Vec<(&str, usize, usize)> =
        crowd.iter().map(|n| (n.as_str(), 0, 50)).collect();
    entries.extend([("main", 0, 50), ("master", 0, 50), ("develop", 0, 50)]);
    let colors = assign_colors(&entries);
    for name in ["main", "master", "develop"] {
        assert_eq!(colors[name], fixed_color(name).unwrap(), "{name}");
        assert_eq!(colors[name], color_for(name));
    }
    let own: std::collections::HashSet<u32> = ["main", "master", "develop"]
        .iter()
        .map(|n| colors[*n])
        .collect();
    assert_eq!(own.len(), 3, "the main branches are told apart");
}

//! Read-only git access (backed by `gix`).

mod graph;
mod refs;
mod repo;

pub use graph::load_graph;
pub use refs::{RefInfo, list_refs};
pub use repo::{RepoInfo, open_repository};

#[cfg(test)]
mod tests;

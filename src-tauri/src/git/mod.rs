//! Read-only git access (backed by `gix`).

mod graph;
mod graph_page;
mod refs;
mod repo;

pub use graph_page::{GraphCache, GraphMeta, GraphPage, build_graph_cache, graph_meta, graph_page};
pub use refs::{RefInfo, list_refs};
pub use repo::{RepoInfo, open_repository};

#[cfg(test)]
mod tests;

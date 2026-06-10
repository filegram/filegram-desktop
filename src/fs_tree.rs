//! Filesystem tree arena (§6.1 ANALYSIS.md): instead of parent references —
//! a flat `Vec<FsNode>` with indices; folder sizes are aggregated in a single
//! post-order pass at build time, and children are sorted by size descending.

use std::path::PathBuf;

/// A directory's own entry size is a fixed 4096 bytes, as in the original.
pub const DIR_ENTRY_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// A raw node built by the scanner. For files `size` is the file length,
/// for directories the field is ignored (the aggregate is computed by [`FsTree::from_temp`]).
#[derive(Debug)]
pub struct TempNode {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<TempNode>,
}

#[derive(Debug)]
pub struct FsNode {
    pub name: Box<str>,
    pub path: PathBuf,
    /// For folders — the subtree aggregate (including their own 4096).
    pub size: u64,
    pub is_dir: bool,
    /// Sorted by `size` in descending order.
    pub children: Vec<NodeId>,
}

#[derive(Debug)]
pub struct FsTree {
    pub nodes: Vec<FsNode>,
    pub root: NodeId,
}

impl FsTree {
    pub fn from_temp(root: TempNode) -> FsTree {
        let mut nodes = Vec::new();
        let (root_id, _) = push_subtree(&mut nodes, root);
        FsTree {
            nodes,
            root: NodeId(root_id),
        }
    }

    pub fn node(&self, id: NodeId) -> &FsNode {
        &self.nodes[id.0]
    }
}

/// Packs a subtree into the arena, returns (index, aggregated size).
fn push_subtree(nodes: &mut Vec<FsNode>, temp: TempNode) -> (usize, u64) {
    let TempNode {
        name,
        path,
        size,
        is_dir,
        children,
    } = temp;

    let id = nodes.len();
    nodes.push(FsNode {
        name: name.into_boxed_str(),
        path,
        size: 0,
        is_dir,
        children: Vec::new(),
    });

    let mut aggregate = if is_dir { DIR_ENTRY_SIZE } else { size };
    let mut kids: Vec<(NodeId, u64)> = children
        .into_iter()
        .map(|child| {
            let (child_id, child_size) = push_subtree(nodes, child);
            aggregate += child_size;
            (NodeId(child_id), child_size)
        })
        .collect();
    kids.sort_by(|a, b| b.1.cmp(&a.1));

    nodes[id].size = aggregate;
    nodes[id].children = kids.into_iter().map(|(child_id, _)| child_id).collect();
    (id, aggregate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> TempNode {
        TempNode {
            name: name.to_string(),
            path: PathBuf::from(format!("/test/{name}")),
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }

    fn dir(name: &str, children: Vec<TempNode>) -> TempNode {
        TempNode {
            name: name.to_string(),
            path: PathBuf::from(format!("/test/{name}")),
            size: 0,
            is_dir: true,
            children,
        }
    }

    #[test]
    fn directory_size_aggregates_children() {
        let tree = FsTree::from_temp(dir(
            "root",
            vec![file("a", 100), file("b", 200), file("c", 300)],
        ));
        assert_eq!(tree.node(tree.root).size, DIR_ENTRY_SIZE + 600);
    }

    #[test]
    fn children_sorted_descending_by_size() {
        let tree = FsTree::from_temp(dir(
            "root",
            vec![file("small", 100), file("big", 300), file("mid", 200)],
        ));
        let sizes: Vec<u64> = tree
            .node(tree.root)
            .children
            .iter()
            .map(|&id| tree.node(id).size)
            .collect();
        assert_eq!(sizes, vec![300, 200, 100]);
    }

    #[test]
    fn nested_directories_aggregate_to_root() {
        let tree = FsTree::from_temp(dir(
            "root",
            vec![dir("sub", vec![file("a", 500)]), file("b", 50)],
        ));
        let root = tree.node(tree.root);
        assert_eq!(root.size, DIR_ENTRY_SIZE + (DIR_ENTRY_SIZE + 500) + 50);
        // The subfolder (4596) is larger than the file (50) — it comes first.
        let first = tree.node(root.children[0]);
        assert!(first.is_dir);
        assert_eq!(first.size, DIR_ENTRY_SIZE + 500);
    }

    #[test]
    fn empty_directory_has_entry_size() {
        let tree = FsTree::from_temp(dir("root", Vec::new()));
        assert_eq!(tree.node(tree.root).size, DIR_ENTRY_SIZE);
        assert!(tree.node(tree.root).children.is_empty());
    }
}

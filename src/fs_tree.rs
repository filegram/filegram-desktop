//! Filesystem tree arena: instead of parent references —
//! a flat `Vec<FsNode>` with indices. The scanner fills a shared append-only arena of
//! [`ScanNode`]s; [`FsTree::from_arena`] can at any moment (including mid-scan)
//! build a tree snapshot out of it: it aggregates folder sizes and sorts children
//! by size in descending order. Arena indices are stable, so `NodeId`s of one
//! snapshot remain valid in the next ones.

use std::path::Path;
use std::sync::Arc;

/// A directory's own entry size is a fixed 4096 bytes, as in the original.
pub const DIR_ENTRY_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// A node of the shared scan arena. Arena invariants: the root has index 0
/// and is added first; a parent is added before any of its children
/// (`parent` is always less than the node's own index; 0 for the root).
/// `Arc` fields make cloning the arena tail for a snapshot cheap.
#[derive(Debug, Clone)]
pub struct ScanNode {
    pub name: Arc<str>,
    pub path: Arc<Path>,
    /// For files — the file length; for directories it is ignored
    /// (the aggregate is computed by [`FsTree::from_arena`]).
    pub size: u64,
    pub is_dir: bool,
    pub parent: usize,
}

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: Arc<str>,
    pub path: Arc<Path>,
    /// For folders — the subtree aggregate (including their own 4096).
    pub size: u64,
    pub is_dir: bool,
    /// Sorted by `size` in descending order.
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct FsTree {
    pub nodes: Vec<FsNode>,
    pub root: NodeId,
}

impl FsTree {
    /// Builds a tree snapshot from a (possibly partially filled) arena.
    /// The arena must not be empty: the root is placed there before the scan starts.
    pub fn from_arena(arena: &[ScanNode]) -> FsTree {
        assert!(!arena.is_empty(), "the arena always contains the root");

        // Sizes are aggregated in a single reverse pass: a child is always to the right
        // of its parent, so by the time sizes[parent] is read all contributions are in.
        let mut sizes: Vec<u64> = arena
            .iter()
            .map(|n| if n.is_dir { DIR_ENTRY_SIZE } else { n.size })
            .collect();
        for i in (1..arena.len()).rev() {
            sizes[arena[i].parent] += sizes[i];
        }

        let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); arena.len()];
        for (i, node) in arena.iter().enumerate().skip(1) {
            children[node.parent].push(NodeId(i));
        }
        for kids in &mut children {
            kids.sort_by(|a, b| sizes[b.0].cmp(&sizes[a.0]));
        }

        let nodes = arena
            .iter()
            .zip(sizes)
            .zip(children)
            .map(|((scan, size), kids)| FsNode {
                name: scan.name.clone(),
                path: scan.path.clone(),
                size,
                is_dir: scan.is_dir,
                children: kids,
            })
            .collect();
        FsTree {
            nodes,
            root: NodeId(0),
        }
    }

    pub fn node(&self, id: NodeId) -> &FsNode {
        &self.nodes[id.0]
    }

    /// Removes a direct child of `parent` (after a successful filesystem delete)
    /// and subtracts its size from `parent` and every node in `ancestors` —
    /// the navigation chain from the root down to `parent`, excluding `parent`
    /// itself (its size is adjusted here already).
    /// Returns `false` — and changes nothing — if `child` is not a direct child.
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId, ancestors: &[NodeId]) -> bool {
        debug_assert!(
            !ancestors.contains(&parent),
            "ancestors must not include parent: its size would be subtracted twice"
        );
        let Some(position) = self.nodes[parent.0]
            .children
            .iter()
            .position(|&id| id == child)
        else {
            return false;
        };
        let removed = self.nodes[child.0].size;
        self.nodes[parent.0].children.remove(position);
        // Saturating: a broken ancestor list must clamp at zero, not wrap
        // around in release builds and corrupt every size above it.
        self.nodes[parent.0].size = self.nodes[parent.0].size.saturating_sub(removed);
        for &ancestor in ancestors {
            self.nodes[ancestor.0].size = self.nodes[ancestor.0].size.saturating_sub(removed);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(parent: usize, name: &str, size: u64) -> ScanNode {
        ScanNode {
            name: name.into(),
            path: Path::new(&format!("/test/{name}")).into(),
            size,
            is_dir: false,
            parent,
        }
    }

    fn dir(parent: usize, name: &str) -> ScanNode {
        ScanNode {
            name: name.into(),
            path: Path::new(&format!("/test/{name}")).into(),
            size: 0,
            is_dir: true,
            parent,
        }
    }

    #[test]
    #[should_panic(expected = "the arena always contains the root")]
    fn empty_arena_panics() {
        FsTree::from_arena(&[]);
    }

    #[test]
    fn directory_size_aggregates_children() {
        let tree = FsTree::from_arena(&[
            dir(0, "root"),
            file(0, "a", 100),
            file(0, "b", 200),
            file(0, "c", 300),
        ]);
        assert_eq!(tree.node(tree.root).size, DIR_ENTRY_SIZE + 600);
    }

    #[test]
    fn children_sorted_descending_by_size() {
        let tree = FsTree::from_arena(&[
            dir(0, "root"),
            file(0, "small", 100),
            file(0, "big", 300),
            file(0, "mid", 200),
        ]);
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
        let tree = FsTree::from_arena(&[
            dir(0, "root"),
            dir(0, "sub"),
            file(1, "a", 500),
            file(0, "b", 50),
        ]);
        let root = tree.node(tree.root);
        assert_eq!(root.size, DIR_ENTRY_SIZE + (DIR_ENTRY_SIZE + 500) + 50);
        // The subfolder (4596) is larger than the file (50) — it comes first.
        let first = tree.node(root.children[0]);
        assert!(first.is_dir);
        assert_eq!(first.size, DIR_ENTRY_SIZE + 500);
    }

    #[test]
    fn empty_directory_has_entry_size() {
        let tree = FsTree::from_arena(&[dir(0, "root")]);
        assert_eq!(tree.node(tree.root).size, DIR_ENTRY_SIZE);
        assert!(tree.node(tree.root).children.is_empty());
    }

    /// A parallel scan interleaves entries of different directories; all that
    /// matters is the "parent before child" invariant.
    #[test]
    fn interleaved_appends_resolve_to_same_tree() {
        let tree = FsTree::from_arena(&[
            dir(0, "root"),
            dir(0, "sub1"),
            dir(0, "sub2"),
            file(2, "in2", 700),
            file(1, "in1", 40),
            file(0, "top", 5),
        ]);
        let root = tree.node(tree.root);
        assert_eq!(
            root.size,
            DIR_ENTRY_SIZE + (DIR_ENTRY_SIZE + 40) + (DIR_ENTRY_SIZE + 700) + 5
        );
        let names: Vec<&str> = root
            .children
            .iter()
            .map(|&id| tree.node(id).name.as_ref())
            .collect();
        assert_eq!(names, vec!["sub2", "sub1", "top"]);
    }

    #[test]
    fn remove_child_updates_parent_and_ancestors() {
        // root → sub → file(500); root also holds top(50).
        let mut tree = FsTree::from_arena(&[
            dir(0, "root"),
            dir(0, "sub"),
            file(1, "a", 500),
            file(0, "top", 50),
        ]);
        let sub = NodeId(1);
        assert!(tree.remove_child(sub, NodeId(2), &[tree.root]));
        assert!(tree.node(sub).children.is_empty());
        assert_eq!(tree.node(sub).size, DIR_ENTRY_SIZE);
        assert_eq!(tree.node(tree.root).size, DIR_ENTRY_SIZE * 2 + 50);
        // The root still has both children: sub and top.
        assert_eq!(tree.node(tree.root).children.len(), 2);
    }

    #[test]
    fn remove_child_saturates_on_broken_ancestors() {
        // A bogus ancestor smaller than the removed child must clamp at
        // zero instead of wrapping around and corrupting the tree.
        let mut tree = FsTree::from_arena(&[
            dir(0, "root"),
            dir(0, "sub"),
            file(1, "big", 500),
            file(0, "small", 10),
        ]);
        let small = NodeId(3);
        assert!(tree.remove_child(NodeId(1), NodeId(2), &[small]));
        assert_eq!(tree.node(small).size, 0);
    }

    #[test]
    fn remove_child_rejects_non_direct_child() {
        let mut tree = FsTree::from_arena(&[dir(0, "root"), dir(0, "sub"), file(1, "a", 500)]);
        let before = tree.node(tree.root).size;
        // The file is a child of sub, not of root — nothing changes.
        assert!(!tree.remove_child(tree.root, NodeId(2), &[]));
        assert_eq!(tree.node(tree.root).size, before);
        assert_eq!(tree.node(NodeId(1)).children.len(), 1);
    }

    /// A mid-scan snapshot: the folder node is already in the arena, its children are not yet appended.
    /// `NodeId`s of a partial snapshot are valid in the full one (the arena is append-only).
    #[test]
    fn partial_arena_is_a_valid_snapshot() {
        let mut arena = vec![dir(0, "root"), dir(0, "sub"), file(0, "top", 10)];
        let partial = FsTree::from_arena(&arena);
        assert_eq!(partial.node(partial.root).size, DIR_ENTRY_SIZE * 2 + 10);
        let sub_id = partial.node(partial.root).children[0];
        assert_eq!(partial.node(sub_id).name.as_ref(), "sub");

        arena.push(file(1, "late", 999));
        let full = FsTree::from_arena(&arena);
        // The same NodeId points to the same folder, now with a child.
        assert_eq!(full.node(sub_id).name.as_ref(), "sub");
        assert_eq!(full.node(sub_id).size, DIR_ENTRY_SIZE + 999);
        assert_eq!(full.node(sub_id).children.len(), 1);
    }
}

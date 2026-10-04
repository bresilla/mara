//! Addressing a level inside a nested document — PLAN_NODE.md P7.
//!
//! This file is checked by `make check` to contain no backend types.

use smallvec::SmallVec;

use crate::vendored::NodeUid;

/// The chain of subgraph instances from the document root down to some
/// level. Empty means the root itself.
///
/// Built from [`NodeUid`] rather than [`crate::vendored::NodeId`]
/// because a path outlives the frame that made it, and slab keys are
/// recycled — a path of `NodeId`s would silently re-address itself the
/// first time anything upstream was deleted.
///
/// # Why an app needs this
///
/// Subgraph definitions are shared: placing the same full-adder eight
/// times gives eight instances of *one* definition, so interior nodes
/// have one identity across all eight. Any per-instance runtime state —
/// a carry bit, a run result, a preview texture — must therefore be
/// keyed by `NodePath` extended with the interior node's uid, not by
/// the interior node alone. Keying by the node alone makes all eight
/// adders share one carry bit, and the symptom is a simulation that
/// looks subtly wrong rather than one that fails.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodePath(pub SmallVec<[NodeUid; 4]>);

impl NodePath {
    /// The document root.
    #[must_use]
    pub fn root() -> Self {
        Self(SmallVec::new())
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// How many levels down this path is.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.len()
    }

    /// This path with `instance` appended — one level deeper.
    #[must_use]
    pub fn child(&self, instance: NodeUid) -> Self {
        let mut next = self.clone();
        next.0.push(instance);
        next
    }

    /// This path with its last element dropped. Returns `None` at the
    /// root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let mut up = self.clone();
        up.0.pop();
        Some(up)
    }

    /// This path truncated to `depth` levels — what a breadcrumb click
    /// on segment `depth` jumps to.
    #[must_use]
    pub fn truncated(&self, depth: usize) -> Self {
        let mut out = self.clone();
        out.0.truncate(depth.min(out.0.len()));
        out
    }

    /// The instance this level sits inside, if any.
    #[must_use]
    pub fn last(&self) -> Option<NodeUid> {
        self.0.last().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = NodeUid> + '_ {
        self.0.iter().copied()
    }
}

/// One segment of a breadcrumb trail.
#[derive(Clone, Debug, PartialEq)]
pub struct Crumb {
    /// Name shown on the chip. The root's name is the document's.
    pub name: String,
    pub color: Option<mara_core::vocab::Color32>,
    /// The definition this level is inside; `None` for the root.
    pub def: Option<crate::vendored::subgraph::DefId>,
    /// Path to jump to when this segment is clicked.
    pub path: NodePath,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(n: u64) -> NodeUid {
        NodeUid(n)
    }

    #[test]
    fn root_is_empty_and_has_no_parent() {
        let r = NodePath::root();
        assert!(r.is_root());
        assert_eq!(r.depth(), 0);
        assert_eq!(r.parent(), None);
        assert_eq!(r.last(), None);
    }

    #[test]
    fn child_and_parent_round_trip() {
        let p = NodePath::root().child(uid(7)).child(uid(9));
        assert_eq!(p.depth(), 2);
        assert_eq!(p.last(), Some(uid(9)));
        let up = p.parent().unwrap();
        assert_eq!(up.depth(), 1);
        assert_eq!(up.last(), Some(uid(7)));
        assert_eq!(up.parent(), Some(NodePath::root()));
    }

    #[test]
    fn truncating_addresses_a_breadcrumb_segment() {
        let p = NodePath::root().child(uid(1)).child(uid(2)).child(uid(3));
        assert_eq!(p.truncated(0), NodePath::root());
        assert_eq!(p.truncated(2), NodePath::root().child(uid(1)).child(uid(2)));
        assert_eq!(p.truncated(99), p, "truncating past the end is a no-op");
    }

    /// Two instances of the same definition must produce different
    /// paths — that is the whole reason this type exists.
    #[test]
    fn sibling_instances_have_distinct_paths() {
        let a = NodePath::root().child(uid(11));
        let b = NodePath::root().child(uid(12));
        assert_ne!(a, b);
    }
}

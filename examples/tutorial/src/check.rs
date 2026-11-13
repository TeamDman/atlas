//! Reject programs that break the rules.
use crate::parse::Tree;

/// A tree that passed every check.
pub struct Checked {
    pub tree: Tree,
}

/// Check a parsed tree.
pub fn check(tree: Tree) -> Result<Checked, String> {
    if tree.words.is_empty() {
        return Err("empty program".into());
    }
    Ok(Checked { tree })
}

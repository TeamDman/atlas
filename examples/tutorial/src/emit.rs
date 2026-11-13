//! Write checked programs as text.
use crate::check::Checked;

/// Render a checked program.
pub fn emit(program: &Checked) -> String {
    program.tree.words.join(" ")
}

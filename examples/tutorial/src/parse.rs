//! Turn source text into a syntax tree.

/// The parsed program.
pub struct Tree {
    pub words: Vec<String>,
}

/// Split text into words.
pub fn parse(text: &str) -> Tree {
    Tree {
        words: text.split_whitespace().map(str::to_owned).collect(),
    }
}

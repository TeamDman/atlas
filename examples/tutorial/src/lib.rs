//! A tiny compiler: parse text, check it, then emit output.
pub mod check;
pub mod emit;
pub mod parse;

/// What a row of the pipeline table describes.
pub enum Role {
    /// A step that transforms the program.
    Pass,
}

/// The program's stages, in order: module, entry function, role, label.
/// `atlas overview` reads this table through a declaration adapter.
pub const PIPELINE: &[(&str, &str, Role, &str)] = &[
    ("parse", "parse", Role::Pass, "Parse text"),
    ("check", "check", Role::Pass, "Check rules"),
    ("emit", "emit", Role::Pass, "Emit text"),
];

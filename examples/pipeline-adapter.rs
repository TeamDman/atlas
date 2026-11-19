//! An example declaration adapter, built only on the public adapter contract.
//!
//! It reads a literal `PIPELINE` table from the declaration file:
//!
//! ```text
//! enum Role { Pass, Resource }
//! const PIPELINE: &[(&str, &str, Role, &str)] = &[
//!     ("parse", "parse", Role::Pass, "Parser"),
//!     ("ast", "", Role::Resource, "Syntax tree"),
//! ];
//! ```
//!
//! Each row names a module, its entry function, its role and a display label.
//! Copy this file to start an adapter for your own project's convention.
#![expect(
    clippy::print_stderr,
    reason = "the adapter executable reports failures on stderr, reserving stdout for its protocol"
)]
use cargo_atlas::{
    Result,
    adapter::{self, ProjectAdapter, Request},
    architecture::{Architecture, Kind, Module},
};
struct Pipeline;
impl ProjectAdapter for Pipeline {
    fn name(&self) -> &'static str {
        "example-pipeline"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn declarations(&self, input: &Request) -> Result<Architecture> {
        let pipeline = input.snapshot.constant(&input.source, "PIPELINE")?;
        let rows = pipeline
            .value
            .array()
            .ok_or("PIPELINE must be a literal array")?;
        let mut modules = Vec::new();
        for row in rows {
            let values = row.tuple().ok_or("pipeline row must be a tuple")?;
            let [name, entry, role, label] = values else {
                return Err("pipeline row expects name, entry, role, label".into());
            };
            let name = name.string().ok_or("module name must be a string")?;
            let kind = match role.path().and_then(|p| p.last()).map(String::as_str) {
                Some("Pass") => Kind::Stage,
                Some("Resource") => Kind::Support,
                _ => return Err("expected Role::Pass or Role::Resource".into()),
            };
            modules.push(Module {
                name: name.into(),
                kind,
                label: Some(label.string().ok_or("label must be a string")?.into()),
                path: input.snapshot.module_root(name)?,
                entry: entry.string().ok_or("entry must be a string")?.into(),
                types: vec![],
                dependencies: vec![],
                directory: false,
            });
        }
        Ok(Architecture {
            schema: 1,
            provenance: vec![pipeline.id()],
            modules,
            workflows: vec![],
        })
    }
}
fn main() {
    if let Err(e) = adapter::serve(&Pipeline) {
        eprintln!("pipeline adapter: {e}");
        std::process::exit(1);
    }
}

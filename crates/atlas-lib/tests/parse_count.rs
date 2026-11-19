//! How many times each file is parsed. Counts are per thread, and each test
//! runs on its own, so parallel tests do not disturb them.
mod support;
use atlas_lib::{analysis::Analysis, discovery, source::parse_count};
use support::Fixture;

const FILES: usize = 3;

fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"parse-count\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod b;\npub struct Root;\n"),
        (
            "src/a.rs",
            "pub struct A;\npub fn make() -> A { crate::b::helper(); A }\n",
        ),
        ("src/b.rs", "pub fn helper() {}\n"),
    ])
}

fn metadata(f: &Fixture) -> cargo_metadata::Metadata {
    cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("fixture metadata")
}

#[test]
fn capture_parses_each_file_once() {
    let f = fixture();
    let metadata = metadata(&f);
    let before = parse_count();
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    assert_eq!(snapshot.graph().files.len(), FILES);
    assert_eq!(parse_count() - before, FILES);
}

#[test]
fn a_lazy_analysis_parses_each_file_again() {
    let f = fixture();
    let metadata = metadata(&f);
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    let before = parse_count();
    let analysis = Analysis::new(&snapshot).expect("analysis");
    analysis.representations().expect("representations");
    // Today: capture's parse is not reused, so analysis parses every file
    // once more. atlas-6ln lowers this to zero for prepared analyses.
    assert_eq!(parse_count() - before, FILES);
    let architecture = atlas_lib::architecture::Architecture {
        schema: 1,
        provenance: vec![],
        modules: vec![],
        workflows: vec![],
    };
    analysis.index(&architecture).expect("source index");
    assert_eq!(
        parse_count() - before,
        FILES,
        "receiver and default-body facts reuse the syntax observation"
    );
}

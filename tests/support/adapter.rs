//! Build the example declaration adapter as an independent public-API consumer.
//!
//! The crate is compiled once per test process into its own target directory, so
//! building it never rebuilds the stock CLI under test.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

/// The consumer crate's location, shared by every test binary that builds it.
pub fn consumer_target() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/adapter-consumer")
}

/// The compiled `examples/pipeline-adapter.rs` executable.
pub fn example() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(build)
}

fn build() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = consumer_target().join("crate");
    fs::create_dir_all(crate_dir.join("src")).expect("consumer crate directory");
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            "[package]\nname='external-atlas-consumer'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\natlas={{path={:?}}}\n[[bin]]\nname='pipeline-adapter'\npath='src/pipeline.rs'\n",
            root.to_str().expect("UTF-8 manifest directory")
        ),
    )
    .expect("consumer manifest");
    fs::write(
        crate_dir.join("src/pipeline.rs"),
        include_str!("../../examples/pipeline-adapter.rs"),
    )
    .expect("consumer source");
    let build = Command::new("cargo")
        .args(["build", "--offline", "--quiet"])
        .current_dir(&crate_dir)
        .env("CARGO_TARGET_DIR", consumer_target())
        .output()
        .expect("cargo");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    consumer_target()
        .join("debug/pipeline-adapter")
        .with_extension(std::env::consts::EXE_EXTENSION)
}

//! Read exact Git objects into an owned temporary tree; never check out the user's tree.
use crate::Result;
mod store;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub use store::GitStore;
#[non_exhaustive]
pub struct Revision {
    pub root: PathBuf,
    pub commit: String,
    // Cleanup authority follows the allocation, never the public compatibility path.
    directory: tempfile::TempDir,
    object_id: String,
}
fn output(command: &mut Command) -> Result<String> {
    let o = command.output()?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string().into());
    }
    Ok(String::from_utf8(o.stdout)?.trim().into())
}
impl Revision {
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
    pub fn commit_id(&self) -> &str {
        &self.object_id
    }
    /// Read an ordinary file within this extraction, refusing symlink traversal.
    pub fn file(&self, path: &str) -> Result<Vec<u8>> {
        let root = self.path().canonicalize()?;
        Ok(std::fs::read(crate::architecture::bind_path(&root, path)?)?)
    }
    /// Requires Git/tar. jj workspaces resolve their actual Git store through `jj git root`.
    pub fn read(root: &Path, revision: &str) -> Result<Self> {
        GitStore::discover(root)?.revision(revision)
    }
}
impl GitStore {
    /// Extract an exact resolved object into a private owned directory.
    pub fn revision(&self, revision: &str) -> Result<Revision> {
        let commit = self.resolve(revision)?;
        let directory = tempfile::Builder::new()
            .prefix("atlas-revision-")
            .tempdir()?;
        let result = Revision {
            root: directory.path().to_owned(),
            object_id: commit.clone(),
            commit,
            directory,
        };
        let mut archive = self
            .command()
            .args(["archive", "--format=tar", &result.object_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let archive_stdout = archive
            .stdout
            .take()
            .ok_or("git archive pipe unavailable")?;
        let extraction = Command::new("tar")
            .arg("-xf")
            .arg("-")
            .arg("-C")
            .arg(&result.root)
            .stdin(archive_stdout)
            .output();
        let status = archive.wait_with_output()?;
        let extracted = extraction?;
        if !status.status.success() || !extracted.status.success() {
            return Err(format!(
                "revision extraction failed: {} {}",
                String::from_utf8_lossy(&status.stderr),
                String::from_utf8_lossy(&extracted.stderr)
            )
            .into());
        }
        if !result.root.join("Cargo.toml").is_file() {
            return Err("revision lacks a root Cargo.toml; subdirectory workspaces are not supported by revision comparison".into());
        }
        // A standalone package walks up for a workspace; one around the
        // extraction directory would claim it and Cargo would refuse.
        if !declares_workspace(&result.root.join("Cargo.toml"))?
            && let Some(enclosing) = enclosing_workspace(&result.root)
        {
            return Err(format!(
                "the revision was extracted to {}, inside the Cargo workspace {}; Cargo would read the revision's standalone package as part of it. Point TMPDIR at a directory outside any Cargo workspace",
                result.root.display(),
                enclosing.display()
            )
            .into());
        }
        Ok(result)
    }
}

/// Whether a manifest has a `[workspace]` table. An unreadable manifest
/// counts as one, as Cargo would also stop there.
fn declares_workspace(manifest: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(manifest)?;
    Ok(text
        .parse::<toml::Table>()
        .map_or(true, |table| table.contains_key("workspace")))
}

/// The nearest manifest above `directory` that declares a workspace.
fn enclosing_workspace(directory: &Path) -> Option<PathBuf> {
    directory.ancestors().skip(1).find_map(|ancestor| {
        let manifest = ancestor.join("Cargo.toml");
        (manifest.is_file() && declares_workspace(&manifest).unwrap_or(true)).then_some(manifest)
    })
}

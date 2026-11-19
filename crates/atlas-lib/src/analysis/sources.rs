//! Invocation-local syntax and workflow caches over retained physical source.
//! Analysis resets selected workflow facts; the shared portable Source is unchanged.
use super::workflow;
use crate::{Result, source::model::SourceGraph};
use std::{cell::OnceCell, collections::BTreeMap, path::Path, sync::Arc};

/// One retained physical source; syntax is lazy and workflow facts are shared across indexes.
pub(super) struct CachedSource {
    source: Arc<crate::source::Source>,
    ast: OnceCell<std::result::Result<syn::File, String>>,
    workflow: OnceCell<workflow::ParsedFile>,
    defaults: OnceCell<Vec<workflow::Workflow>>,
    receivers: OnceCell<super::index::receivers::ReceiverFacts>,
}
impl CachedSource {
    pub(super) fn text(&self) -> &str {
        self.source.text()
    }
    pub(super) fn source(&self) -> &crate::source::Source {
        &self.source
    }
    pub(super) fn ast(&self, path: &str) -> Result<&syn::File> {
        self.ast
            .get_or_init(|| {
                crate::source::parse_file(self.source.text())
                    .map_err(|error| format!("parse source {path}: {error}"))
            })
            .as_ref()
            .map_err(|error| error.clone().into())
    }
    pub(super) fn clear_workflow(&mut self) {
        self.workflow.take();
        self.defaults.take();
        self.receivers.take();
    }
    fn prepare_facts(&self, path: &str, tests: crate::population::TestSelection, ast: &syn::File) {
        self.workflow
            .get_or_init(|| workflow::read_ast(&self.source, path, ast, tests));
        self.defaults
            .get_or_init(|| workflow::read_defaults(&self.source, path, ast, tests));
        self.receivers
            .get_or_init(|| super::index::receivers::read(&self.source, path, ast, tests));
    }
    pub(super) fn defaults(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
    ) -> Result<&[workflow::Workflow]> {
        if self.defaults.get().is_none() {
            self.visit_once(path, tests, |_| ())?;
        }
        self.defaults
            .get()
            .map(Vec::as_slice)
            .ok_or_else(|| "default body cache missing".into())
    }
    pub(super) fn receivers(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
    ) -> Result<&super::index::receivers::ReceiverFacts> {
        if self.receivers.get().is_none() {
            self.visit_once(path, tests, |_| ())?;
        }
        self.receivers
            .get()
            .ok_or_else(|| "receiver fact cache missing".into())
    }
    pub(super) fn workflow(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
    ) -> Result<&workflow::ParsedFile> {
        if let Some(parsed) = self.workflow.get() {
            return Ok(parsed);
        }
        let ast = self.ast(path)?;
        Ok(self
            .workflow
            .get_or_init(|| workflow::read_ast(&self.source, path, ast, tests)))
    }
}
impl CachedSource {
    /// Run `visit` over this file's syntax and cache its workflow facts. A file
    /// not parsed before is parsed for this call only, so a pass over every
    /// file holds one syntax tree at a time instead of all of them.
    pub(super) fn visit_once<T>(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
        visit: impl FnOnce(&syn::File) -> T,
    ) -> Result<T> {
        if self.ast.get().is_some() {
            let ast = self.ast(path)?;
            self.prepare_facts(path, tests, ast);
            return Ok(visit(ast));
        }
        let ast = crate::source::parse_file(self.source.text())
            .map_err(|error| format!("parse source {path}: {error}"))?;
        self.prepare_facts(path, tests, &ast);
        Ok(visit(&ast))
    }
}
pub(super) type Sources = BTreeMap<String, CachedSource>;
/// Retain each physical source once; selected ASTs are cached on first use.
pub(super) fn retain_sources(
    sources: impl IntoIterator<Item = Result<(String, Arc<crate::source::Source>)>>,
) -> Result<Sources> {
    sources
        .into_iter()
        .map(|input| {
            let (path, source) = input?;
            Ok((
                path,
                CachedSource {
                    source,
                    ast: OnceCell::new(),
                    workflow: OnceCell::new(),
                    defaults: OnceCell::new(),
                    receivers: OnceCell::new(),
                },
            ))
        })
        .collect()
}
pub(super) fn read_sources(root: &Path, graph: &SourceGraph) -> Result<Sources> {
    retain_sources(graph.physical_files().filter(|f| !f.test_only).map(|f| {
        let source = std::fs::read_to_string(root.join(&f.path))?;
        if crate::discovery::fingerprint(source.as_bytes()) != f.content_fingerprint {
            return Err(format!(
                "source changed after discovery: {}; rescan before extracting",
                f.path
            )
            .into());
        }
        Ok((
            f.path.clone(),
            Arc::new(crate::source::Source::of(Path::new(&f.path), source)?),
        ))
    }))
}

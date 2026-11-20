//! Source-body control and bounded expansion; raw workflow evidence stays distinct from presentation.
use super::{
    json,
    output::{self, Backend, RenderOptions, protected_paths, render},
    profile::architecture,
};
use cargo_atlas::{
    Result,
    architecture::WorkflowAnchor,
    diagram::{Diagram, Level, Query, View},
    discovery,
    render::{Artifact, Ascii, Document, Renderer},
    workflow,
};
use clap::Args;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};
#[derive(Args)]
pub(super) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    /// Expand matched source calls; recursion/unknown bodies remain explicit (max 24).
    #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u8).range(0..=i64::from(workflow::MAX_DEPTH)),requires="path")]
    depth: u8,
    /// Fold borrow operands and generated ? bridges; raw steps remain in evidence.
    #[arg(long, value_enum, default_value = "expressions")]
    detail: workflow::Detail,
    /// Profile workflow name. Omit to show independent source bodies together.
    anchor: Option<String>,
    /// Select a declared workflow without a positional ROOT.
    #[arg(long = "anchor", conflicts_with = "anchor")]
    named_anchor: Option<String>,
    #[arg(long)]
    architecture: Option<PathBuf>,
    /// Opt-in declared future contracts and source-bound resolution evidence.
    #[arg(long, requires = "path", requires = "function")]
    future_profile: Option<PathBuf>,
    #[arg(long, requires = "function")]
    path: Option<String>,
    /// The function in --path: its name, a qualified name (module::name,
    /// Type::method, Trait::method for a default body, <Type as Trait>::method),
    /// or the LINE (or LINE:COLUMN) where its name is written.
    #[arg(long, requires = "path")]
    function: Option<String>,
    #[command(flatten)]
    render: RenderOptions,
}

pub(super) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let (profile_text, profile) = architecture(root, options.architecture.as_deref())?;
    let future_profile = options
        .future_profile
        .as_ref()
        .map(|path| load_future_profile(root, path))
        .transpose()?;
    if future_profile.is_some() && options.depth == 0 {
        return Err("--future-profile requires --depth greater than zero".into());
    }
    let selected_anchor = options.named_anchor.as_ref().or(options.anchor.as_ref());
    let anchors = if let (Some(path), Some(function)) = (&options.path, &options.function) {
        if selected_anchor.is_some() {
            return Err("choose a named workflow or --path/--function, not both".into());
        }
        vec![WorkflowAnchor {
            name: function.clone(),
            path: path.clone(),
            function: function.clone(),
        }]
    } else {
        let profile = profile
            .as_ref()
            .ok_or("flow needs --architecture or both --path and --function")?;
        let anchors: Vec<_> = profile
            .workflows
            .iter()
            .filter(|a| selected_anchor.is_none_or(|name| name == &a.name))
            .cloned()
            .collect();
        if anchors.is_empty() {
            return Err("no matching workflow anchors; inspect the architecture profile".into());
        }
        anchors
    };
    // Depth zero reads only the selected bodies; its inputs are protected
    // from a path walk rather than a capture of the whole workspace.
    let mut sources = Vec::new();
    let (flows, mut diagram) = if options.depth > 0 {
        let path = options
            .path
            .as_deref()
            .ok_or("--depth needs --path and --function")?;
        let function = options
            .function
            .as_deref()
            .ok_or("--depth needs --function")?;
        let snapshot = discovery::snapshot_metadata(metadata)?;
        if let Some(profile) = &future_profile {
            check_future_snapshot(root, &snapshot, profile)?;
        }
        sources.extend(
            snapshot
                .graph()
                .files
                .iter()
                .map(|file| metadata.workspace_root.join(&file.path).into_std_path_buf()),
        );
        let analysis = atlas_lib::analysis::Analysis::new(&snapshot)?;
        let (flow, diagram) = if let Some(profile) = &future_profile {
            analysis.expanded_with_future_profile(
                path,
                function,
                options.depth,
                options.detail,
                profile,
            )?
        } else {
            analysis.expanded_with_detail(path, function, options.depth, options.detail)?
        };
        (vec![(function.to_owned(), flow)], diagram)
    } else {
        sources = discovery::source_paths(metadata)?;
        let selections: Vec<_> = anchors
            .iter()
            .map(|a| (a.path.as_str(), a.function.as_str()))
            .collect();
        let flows: Vec<_> = anchors
            .iter()
            .zip(workflow::read_many(root, &selections)?)
            .map(|(a, f)| (a.name.clone(), f))
            .collect();
        let diagram = workflow_diagram(&flows, options.detail)?;
        (flows, diagram)
    };
    let query = Query {
        focus: selected_anchor
            .cloned()
            .or(options.function.clone())
            .or(Some("workflows".into())),
        level: Level::Declarations,
        view: View::Workflow,
    };
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_paths(
            metadata,
            sources,
            options
                .architecture
                .iter()
                .map(|path| root.join(path))
                .chain(anchors.iter().map(|anchor| root.join(&anchor.path)))
                .chain(options.future_profile.iter().map(|path| root.join(path)))
                .chain(future_profile.iter().flat_map(|profile| {
                    profile.sources.iter().map(|source| root.join(&source.path))
                })),
        ),
    );
    let pending = destination.prepare(
        None,
        &mut diagram,
        profile_text.as_deref(),
        Some(("workflow", &json(&flows)?)),
    )?;
    let artifact = if options.render.backend == Backend::Ascii
        && (options.depth > 0 || options.detail == workflow::Detail::Control)
    {
        Ascii.render(&diagram)
    } else if options.render.backend == Backend::Ascii {
        let mut text = flows
            .iter()
            .map(|(name, flow)| format!("{name}\n{}", flow.ascii()))
            .collect::<Vec<_>>()
            .join("\n");
        for (label, url) in &diagram.evidence_links {
            write!(text, "\n{label}: {url}").expect("formatting into a String cannot fail");
        }
        text.push('\n');
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: Vec::new(),
        })
    } else {
        render(&options.render, &diagram)
    };
    destination.write(artifact, pending)
}

fn load_future_profile(
    root: &Path,
    path: &Path,
) -> Result<workflow::future_profile::FutureProfile> {
    use std::collections::BTreeSet;
    let profile: workflow::future_profile::FutureProfile =
        serde_json::from_slice(&std::fs::read(root.join(path))?)?;
    if profile.schema != 1 {
        return Err("unsupported future profile schema".into());
    }
    let mut sources = BTreeSet::new();
    for source in &profile.sources {
        let full = root.join(&source.path).canonicalize()?;
        let bytes = std::fs::read(&full)?;
        let digest = sha256_hex(&bytes);
        if digest != source.sha256 {
            return Err(format!("future profile source digest mismatch: {}", source.path).into());
        }
        if !sources.insert(full) {
            return Err("duplicate future profile source".into());
        }
    }
    let mut tokens = BTreeSet::new();
    for resolution in &profile.resolutions {
        let source = root.join(&resolution.path).canonicalize()?;
        if !sources.contains(&source) {
            return Err("future profile resolution source not pinned".into());
        }
        let text = std::fs::read_to_string(source)?;
        let line = text
            .lines()
            .nth(
                resolution
                    .line
                    .checked_sub(1)
                    .ok_or("invalid profile line")?,
            )
            .ok_or("invalid profile line")?;
        if !token_matches(line, resolution.column, &resolution.spelling) {
            return Err("future profile resolution token mismatch".into());
        }
        if !tokens.insert((resolution.path.clone(), resolution.line, resolution.column)) {
            return Err("duplicate future profile resolution token".into());
        }
        let (definition_prefix, definition_column) = resolution
            .definition
            .rsplit_once(':')
            .ok_or("invalid profile definition")?;
        let (definition_path, definition_line) = definition_prefix
            .rsplit_once(':')
            .ok_or("invalid profile definition")?;
        let definition_path = root.join(definition_path).canonicalize()?;
        let definition_line: usize = definition_line.parse()?;
        let definition_column: usize = definition_column.parse()?;
        if !sources.contains(&definition_path) {
            return Err("future profile definition source not pinned".into());
        }
        let definition_source = std::fs::read_to_string(definition_path)?;
        let line = definition_source
            .lines()
            .nth(
                definition_line
                    .checked_sub(1)
                    .ok_or("invalid definition line")?,
            )
            .ok_or("invalid definition line")?;
        if !token_matches(line, definition_column, &resolution.spelling) {
            return Err("future profile definition token mismatch".into());
        }
    }
    let mut definitions = BTreeSet::new();
    for contract in &profile.contracts {
        if contract.evidence.trim().is_empty() {
            return Err("future profile contract missing evidence".into());
        }
        if !definitions.insert(&contract.definition) {
            return Err("duplicate future profile contract".into());
        }
        if !profile
            .resolutions
            .iter()
            .any(|resolution| resolution.definition == contract.definition)
        {
            return Err("future profile contract has no resolution".into());
        }
    }
    Ok(profile)
}

fn token_matches(line: &str, column: usize, token: &str) -> bool {
    line.get(..column).is_some_and(|prefix| {
        prefix.is_ascii()
            && prefix
                .chars()
                .next_back()
                .is_none_or(|ch| ch != '_' && !ch.is_alphanumeric())
    }) && line
        .get(column..)
        .and_then(|tail| tail.strip_prefix(token))
        .is_some_and(|tail| {
            tail.chars()
                .next()
                .is_none_or(|ch| ch != '_' && !ch.is_alphanumeric())
        })
}

fn check_future_snapshot(
    root: &Path,
    snapshot: &discovery::Snapshot,
    profile: &workflow::future_profile::FutureProfile,
) -> Result<()> {
    let root = root.canonicalize()?;
    for source in &profile.sources {
        let path = root.join(&source.path).canonicalize()?;
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        if relative
            .extension()
            .is_none_or(|extension| extension != "rs")
        {
            continue;
        }
        let path = relative.to_str().ok_or("profile path is not UTF-8")?;
        let captured = snapshot
            .source_facts(path)
            .ok_or("profile source missing from captured workspace")?;
        let digest = sha256_hex(captured.text().as_bytes());
        if digest != source.sha256 {
            return Err(format!("future profile differs from captured source: {path}").into());
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(out, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    out
}
fn workflow_diagram(
    flows: &[(String, workflow::Workflow)],
    detail: workflow::Detail,
) -> Result<Diagram> {
    let mut combined = Diagram {
        title: "Source workflows (independent bodies)".into(),
        level: Level::Declarations,
        view: View::Workflow,
        nodes: vec![],
        edges: vec![],
        inventory: vec![],
        coverage: cargo_atlas::diagram::Coverage::default(),
        notes: vec![],
        evidence_links: vec![],
    };
    for (index, (name, flow)) in flows.iter().enumerate() {
        let mut diagram = flow.diagram_with_detail(detail)?;
        for node in &mut diagram.nodes {
            node.id = format!("w{index}_{}", node.id);
            node.group = format!("{} · {}", node.group, name);
        }
        for edge in &mut diagram.edges {
            edge.from = format!("w{index}_{}", edge.from);
            edge.to = format!("w{index}_{}", edge.to);
        }
        for node in &mut diagram.coverage.summarized_nodes {
            node.id = format!("w{index}_{}", node.id);
            node.group = format!("{} · {}", node.group, name);
        }
        combined
            .coverage
            .summarized_nodes
            .extend(diagram.coverage.summarized_nodes);
        combined
            .coverage
            .represented_files
            .extend(diagram.coverage.represented_files);
        combined.nodes.extend(diagram.nodes);
        combined.edges.extend(diagram.edges);
        combined.inventory.extend(diagram.inventory);
        combined.notes.extend(diagram.notes);
    }
    combined.notes.sort();
    combined.notes.dedup();
    combined.inventory.sort();
    combined.inventory.dedup();
    combined.validate()?;
    Ok(combined)
}

#[cfg(test)]
mod future_profile_tests {
    use super::*;
    use workflow::future_profile::{Contract, FutureProfile, Operation, ProfileSource, Resolution};

    #[test]
    fn profile_refuses_stale_sources_and_misidentified_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let source = "fn forward() {}\nfn run(){forward();}\n";
        std::fs::write(root.join("lib.rs"), source).unwrap();
        let hash = sha256_hex(source.as_bytes());
        let profile_path = root.join("profile.json");
        let mut profile = FutureProfile {
            schema: 1,
            routes: vec![],
            scope: vec![],
            sources: vec![ProfileSource {
                path: "lib.rs".into(),
                sha256: hash,
            }],
            resolutions: vec![Resolution {
                path: "lib.rs".into(),
                line: 2,
                column: 9,
                spelling: "forward".into(),
                definition: "lib.rs:1:3".into(),
            }],
            contracts: vec![Contract {
                definition: "lib.rs:1:3".into(),
                operation: Operation::ArgumentForward { argument: 0 },
                evidence: "reviewed source".into(),
            }],
        };
        let save = |profile: &FutureProfile| {
            std::fs::write(&profile_path, serde_json::to_vec(profile).unwrap()).unwrap();
        };
        save(&profile);
        assert!(load_future_profile(root, &profile_path).is_ok());
        profile.resolutions[0].column = 8;
        save(&profile);
        assert!(load_future_profile(root, &profile_path).is_err());
        profile.resolutions[0].column = 9;
        profile.resolutions.push(profile.resolutions[0].clone());
        save(&profile);
        assert!(load_future_profile(root, &profile_path).is_err());
        profile.resolutions.pop();
        save(&profile);
        std::fs::write(root.join("lib.rs"), format!("{source}// changed\n")).unwrap();
        assert!(load_future_profile(root, &profile_path).is_err());
        assert!(!token_matches("éforward", 1, "forward"));
        assert!(!token_matches("forward_more", 0, "forward"));
        assert!(!token_matches("otherforward", 5, "forward"));
    }
}

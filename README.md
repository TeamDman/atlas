# Atlas

Atlas reads a Rust workspace and answers structural questions about it: where a
name is declared, what changed between two commits, which files call each other,
how a function's control flow branches, and whether the code still follows the
architecture you wrote down.

It is a command-line tool and a Rust library. It never edits the code it reads,
it never compiles or runs it, and every answer carries the source locations it
came from.

```console
$ atlas name SourceGraph
SourceGraph: 2 declaration sites (ambiguous)

crates/atlas-core/src/source/model.rs:107:SourceGraph [Pub Struct]
  All source observations for one scan, serialized with a schema version.
  3 outgoing, 36 incoming candidate-unique, 0 incoming ambiguous
  pub struct SourceGraph {

crates/atlas-core/src/source/model.rs:116:SourceGraph [Private Impl]
  0 outgoing, 0 incoming candidate-unique, 0 incoming ambiguous
  impl SourceGraph {
```

## Why Atlas

Tools such as rust-analyzer resolve Rust precisely inside an editor. Atlas is for
the questions you ask about a whole codebase at once, from a terminal, a script,
a review or a CI job:

- **Orientation.** Count and locate declarations, read a module's surface, and
  follow a type through the functions that produce and consume it.
- **Review.** Compare a change with an earlier revision: changed declarations,
  moved bodies, visibility changes, module size and nesting.
- **Diagrams.** Draw file dependencies, representation contracts, control flow
  and before/after comparisons as text or SVG.
- **Architecture checks.** Test stage ordering, module seams, size budgets and
  ratchets against a policy file you write.

Atlas does not guess when the source leaves a question open. When a name could
refer to two declarations, Atlas reports both. When it cannot resolve a call,
the call is marked unresolved and is never counted as absent. A lexical mention,
a resolved call, a signature type and a rule you declared are separate kinds of
evidence, and Atlas never presents one as another.

## Install

```sh
cargo install --locked --git https://github.com/flowerornament/atlas.git atlas
```

Requirements:

- Rust 1.99.1 or newer and Cargo. Atlas uses `cargo metadata` to find source.
- Node.js 20 or newer for SVG output only. The renderer ships inside the binary,
  so no npm install or network access is needed.
- Git (and `tar`) to read earlier revisions. In a [jj](https://github.com/jj-vcs/jj)
  workspace, Atlas uses `jj` and never snapshots the working copy.

For reproducible tooling, pin an exact commit with `--rev <commit>`.

## Quick tour

Run these commands inside any Cargo project, or pass `--root path/to/project`.

```sh
# Where am I?
atlas census                          # files and declarations per kind
atlas module src/lib.rs               # one module's header, items and imports
atlas name Config --population all    # every declaration named Config

# What changed? (in jj, use a revset such as @- instead of HEAD~1)
atlas diff HEAD~1 --items --graph     # changed declarations and their relationships
atlas moved HEAD~1 --scope src        # bodies that moved, exactly or with new paths
atlas visibility HEAD~1               # written visibility changes

# How is it shaped?
atlas spans --scope src               # items and their source spans
atlas concentration --scope src       # block depth and item breadth per file
```

Add `--json` to any command for structured output. Run `atlas <command> --help`
to see what that command counts and what its answer does and does not mean.

## Diagrams

```sh
atlas dependencies --scope src --relation calls --backend pele
atlas flow --path src/main.rs --function main --detail control --backend pele
atlas representations --type Config --panel producers
atlas compare --before HEAD~1 --backend pele
```

Text is the default output. With `--backend pele`, Atlas writes a standalone SVG
under `target/atlas/`, with a JSON evidence file and a Mermaid source beside it.
Use `--out path.svg` to choose the location.

`flow` on this repository's `main` gives:

```text
+-- src/main.rs · main
|  [n1] fn main
|  [n2] return from main → ()
|  [n3] cli::run()
|  [n4] let Err(error) = cli::run()
|  [n5] if let Err(error) = cli::run()
|  [n6] then (true / pattern matches)
|  [n7] [opaque] eprintln!("atlas: {error}");
|  [n8] std::process::exit(1)
|  [n9] else (false / pattern does not match)
```

The macro call is shown as opaque because Atlas does not expand macros. Saved
questions in JSON make a diagram repeatable; see
[`examples/queries/atlas.json`](examples/queries/atlas.json) and the
[diagram guide](docs/diagrams.md).

## Architecture checks

`stages`, `seams`, `policy` and `delta` compare captured source with an
architecture policy you write in TOML: which stages may read which, how large
modules may grow, which debts are allowed to shrink but not grow. `comments`
audits documentation coverage. Atlas reports three different outcomes:

- **violations**, where the source breaks a rule,
- **unavailable**, where evidence a rule needs is missing,
- **observations**, which are reported without any verdict.

Missing evidence is never reported as a pass. See [architecture checks](docs/checks.md).

## Use the library

The `atlas-core` crate provides the source model, navigation, review reports,
qualified analysis and checks. It has no CLI, renderer or Node dependency.

```toml
[dependencies]
atlas-core = { git = "https://github.com/flowerornament/atlas.git", rev = "<commit>" }
```

```rust,no_run
use atlas_core::{analysis::Analysis, discovery};

fn inspect(root: &std::path::Path) -> atlas_core::Result<()> {
    let snapshot = discovery::snapshot_workspace(root)?;   // read the source once
    let analysis = Analysis::new(&snapshot)?;              // qualified index over it
    let facts = analysis.representations()?;               // signatures, fields, calls
    let (_workflow, _diagram) = analysis.expanded("src/main.rs", "main", 1)?;
    println!("{} functions", facts.functions.len());
    Ok(())
}
```

The `atlas` package adds the renderers and a JSON protocol for project-specific
declaration adapters. See the [library guide](docs/library.md) and
[extending Atlas](docs/extensions.md).

## What Atlas reads, and what it does not

Atlas parses each file with [syn](https://docs.rs/syn) and follows Cargo targets
and module declarations. It does not expand macros, evaluate `cfg` feature
combinations, infer types or resolve trait dispatch. It does not prove that a
path can execute. These limits are stated in each report rather than hidden, and
a file that fails to parse is reported as a syntax error instead of being skipped.

## Documentation

- [Guide](docs/guide.md): concepts, populations, scopes, revisions and output
- [Diagrams](docs/diagrams.md): every view and how to read it
- [Architecture checks](docs/checks.md): policies, stages, seams and deltas
- [Library](docs/library.md): using `atlas-core` from Rust
- [Extending Atlas](docs/extensions.md): declaration adapters and renderers
- [Architecture](ARCHITECTURE.md): how the code is organized
- [Contributing](CONTRIBUTING.md) and the [changelog](CHANGELOG.md)

## License

Atlas is available under either the [MIT](LICENSE-MIT) or
[Apache 2.0](LICENSE-APACHE) license, at your option. The bundled
[Pele](vendor/pele/README.md) renderer is MIT licensed.

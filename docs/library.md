# Using atlas-core

`atlas-core` is the library behind the Atlas CLI. It captures a Cargo
workspace's source once and answers navigation, review, qualified-analysis and
architecture-check questions over that capture. It has no CLI, renderer or Node
dependency, so you can call it from your own tools, tests and build scripts.

```toml
[dependencies]
atlas-core = { git = "https://github.com/flowerornament/atlas.git", rev = "<commit>", default-features = false }
```

The optional `clap` feature adds `ValueEnum` implementations for diagram
selections. The `atlas` package (the CLI) re-exports these modules under the same
paths and adds renderers and declaration adapters.

## Orientation

| You want to | Start with |
| --- | --- |
| Read a workspace once | `discovery::snapshot_workspace(root)` or `snapshot_metadata(&metadata)` |
| Ask several qualified questions | `analysis::Analysis::new(&snapshot)` |
| Navigate declarations | `queries::navigation::Navigation` |
| Measure files or compare revisions | `review::outline`, `review::diff`, `review::moves` |
| Read an earlier commit | `revision::Revision::read` |
| Check an architecture policy | `checks` (see [architecture checks](checks.md)) |

Reports are typed values first. Formatting and process exit codes belong to the
caller. Several reports keep a fixed line-based counting unit, which messages
and docs call *native*. It differs from the byte-exact syntax sites used by
qualified views, and Atlas never substitutes one for the other.

```rust,no_run
fn inspect(root: &std::path::Path) -> atlas_core::Result<()> {
    let snapshot = atlas_core::discovery::snapshot_workspace(root)?;
    let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
    let facts = analysis.representations()?;
    let (workflow, diagram) = analysis.expanded("src/main.rs", "main", 1)?;
    Ok(())
}
```

## Capture

`discovery::snapshot_metadata` accepts Cargo metadata you have already loaded,
and `snapshot_workspace` loads it for you. The snapshot keeps normalized physical
paths, original bytes, lexical outlines and test-boundary evidence, together with
the relationship graph from the same pass. `source_facts(path)` borrows that
evidence. Later filesystem edits do not change a snapshot.
`discovery::scan_metadata` returns only the graph.

`source::Source::of` captures structural facts with syn; item spans include
attached comments and attributes. `source::RustSourceProjection` retains exact
text and a comment/string mask for line-based observations. Its structural
compatibility methods (outlines, declaration lines and test regions) delegate to
the same syn grammar and return `SyntaxError` for invalid or unsupported syntax.
Macro payloads stay opaque, Rust types are not resolved.

The native LOC boundary excludes cfg-test modules, including their attached
attributes. Individually cfg-test-gated helpers outside such modules remain in
this counting population. Other views can select a different population; do not
substitute this counter for the production-symbol selection of a qualified view.

`Snapshot::source` retains failed bytes; `source_facts` is absent for invalid
Rust, and `syntax_error` explains why. The graph retains that file's fingerprint
and diagnostic. Qualified requests refuse selected invalid source; native checks
retain required-source unavailability. Opaque macro bodies and unsupported
Verbatim/trait-alias items are never reinterpreted by a lexical fallback.

Items expose exact syntax and attached byte ranges alongside their native line
witnesses. File top-level is structural nesting, not indentation. Same-line
imports remain distinct and stage paths retain the code around them. Source IDs
keep their existing shape when unique; colliding physical sites append a byte
offset before the package-context suffix. Native presentation sites retain their exact byte positions while serialized
line witnesses and report shapes stay unchanged.

## Qualified analysis

For several qualified questions, capture the workspace once and reuse an
`analysis::Analysis`. It shares the captured source and lazily parses each selected
file once for that invocation. Workflow facts are also shared across its indexes,
rather than traversing every body again for another question. The snapshot owns its Cargo context, so a
later change to the caller's metadata or disk files cannot change its namespace
or observations.

Physical paths and Cargo contexts are distinct. `SourceGraph.files` contains
package-file observations; a path shared by two packages has two observations.
`physical_files()` counts each path once, and `Snapshot::physical_file(path)`
provides a direct lookup. Physical reports prefer a production observation when
a file also has a test-only context. Package navigation and move matching retain
the package context. Package-specific diagnostics carry their package, so one
package's incomplete mount cannot become another package's finding.

Mounts retain test ancestry, and distinguish targets that share a root path.
Qualified function identities include the target and module path. Representation
facts describe physical syntax sites: when contexts resolve a signature or call
differently, its target stays unknown. Declaration diagrams label shared sites
with their package; file diagrams still contain one box per physical path.
Declaration adapters refuse multi-package file authority because their current
protocol cannot express that distinction.

`Snapshot::files(FileSelection::SourceTree)` selects each package's `src`
inventory; `Discovered` also includes the discovered Cargo roots outside it.
`Analysis::with_tests` selects `Production`, `OutsideTestModules` or `All` from
the same retained test evidence. Changing this selection clears workflow facts,
retaining the AST cache. Excluded files need not parse; selecting them makes a
syntax error an explicit failure. Imports use the same population and use-tree
grammar as declarations. Other cfg alternatives are retained without evaluating
feature combinations.

Explicit file workflows (`workflow::read`, `read_many`, `read_file`) remain syntax
queries over the requested bodies, including test bodies. They do not acquire a
production-population filter merely because qualified workspace views use one.

File-module declarations come from syn, independent of layout. Native file children inside inline modules,
external siblings outside the discovered inventory, symlink mounts and missing
or escaping paths are diagnosed rather than assigned invented namespaces. This
is source evidence, not the compiler's expanded module tree.

```rust,no_run
fn inspect(root: &std::path::Path) -> atlas_core::Result<()> {
    let snapshot = atlas_core::discovery::snapshot_workspace(root)?;
    let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
    let facts = analysis.representations()?;
    let (workflow, diagram) = analysis.expanded("src/main.rs", "main", 1)?;
    Ok(())
}
```

Application qualified views, program queries and expanded workflows use this
capture path. Unexpanded file-only workflows still read just their selected
files once per batch. Older public graph-plus-root conveniences remain explicit
fingerprint-checked reread adapters for callers that did not retain source; they
are not a fallback selected during analysis. A parse failure names its file and
refuses the qualified analysis rather than producing an empty report.

## Review reports

`review::outline` consumes the retained snapshot and a physical path scope. It
returns item spans, source fingerprints, non-test lines, top-level item breadth,
largest top-level block and code outside recognized spans. No matching source is
an error. Block depth follows the native review unit: from the declaration line
to the next top-level declaration or test-region boundary. It differs from the
attached span ending at an item's closing brace.

```sh
atlas spans --root . --scope crates/atlas-core/src --json
atlas nontest-loc --root . --scope crates/atlas-core/src
atlas concentration --root . --scope crates/atlas-core/src
```

Scope defaults to the whole workspace; positional PATH remains an alias for
`--scope`. A supplied scope with no matches refuses.

Scope defaults to the whole workspace. A scope with no matches is an error.
These reports are evidence for reading and review; they assign no verdict.

`review::diff` compares the production line streams and shape reports of two
captured snapshots. Test-module-only edits are excluded; individually gated
helpers retain the native counting boundary. A scope present only in the before
snapshot remains selectable, so deletions remain visible. Changed files carry
before/after fingerprints and optional shape measures; the report does not infer
renames, equivalent behavior or an architectural verdict.

```sh
atlas diff HEAD --root . --scope crates/atlas-core --json
```

The revision is resolved through the shared immutable revision reader. The JSON
basis identifies its exact commit; `after: null` denotes current working source,
whose changed-file fingerprints are in the report. `compare` remains the separate
question about representation contracts and constructors.

`review::moves::{moved, visibility}` match declarations by the native lexical
crate/class/name key, including impl owners and inline modules. A physical path
scope narrows reporting only: matching still spans each selected crate. Duplicate
keys pair approximately and are counted explicitly. Reports retain file
fingerprints, discovery diagnostics and their matching limits.
Move and visibility keying reuse each captured import's parse result. An unreadable
required import fails the query with its source location; it cannot disappear as
an empty binding list or imply that no binding changed.

```sh
atlas moved HEAD --root . --scope src/backend --json
atlas visibility HEAD --root . --scope src/backend
```

`moved` keeps exact normalized tokens, equal-after-rerooting, changed, one-sided
and unreadable spans separate. Exact ignores docs, written visibility and selected layout; macro input tokens
and parenthesis commas are preserved conservatively;
rerooting also removes selected crate-relative module qualifiers, so different
paths can compare equal in that class. Neither proves behavior equivalent.
`visibility` reports the native written-visibility ladder, including restricted
paths; it does not calculate Rust's effective visibility or resolve relative
scopes after a move. Native union-field visibility and glob-import retargeting
remain unmeasured and are named in the report limits. These are report-only
tools, never architecture verdicts.

### Documentation audit

Documentation audits use a caller-supplied `documentation::Policy`. It selects
item kinds, written visibility, test population, header population and optional
citation markers. `OutsideTestModules` preserves native counting of individually
gated helpers; `Production` additionally excludes those helpers. Neither changes
the captured test evidence. The source API also exposes projected lines, trait
impl heads and struct-field visibility for review tools.

```sh
atlas comments --audit --root . --population pub --json
```

Omitting `--audit` refuses. Citation checking is explicitly unrequested unless
`--citation-markers` supplies the project's own strings. Header selection states whether only the file or any module may supply `//!`.
Missing headers, uncited headers and missing outer `///` comments are separate
observations with source witnesses. This native line-comment metric does not
count doc attributes or block documentation; its limits travel in the report;
this command does not emit an architecture PASS verdict. The shared library
contains no project-specific citation-marker defaults.

## Navigation

`queries::navigation::Navigation` reads the same snapshot with an explicit
`VisibilitySelection`. Its structured `census`, `name` and `module` results
provide native tally units, the workspace/package/directory/file ladder,
definitions and hidden matches, reexports, impl sites, approximate textual
references and lexical relationship evidence. Reports include captured source
fingerprints and diagnostics. A shared file retains every package context;
module edges are attached to their own package.

```rust,ignore
use atlas_core::{discovery, population::VisibilitySelection, queries::navigation::Navigation};
let snapshot = discovery::snapshot_workspace(root)?;
let navigation = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)?;
let ladder = navigation.census(None)?;
let definition = navigation.name("grade::Cost")?;
let module = navigation.module("crates/example/src/grade.rs")?;
```

The CLI selects these reports explicitly:

```sh
atlas census --root . --population pub-ish
atlas census --root . --scope atlas-core --pub-only --json
atlas name grade::Cost --root . --population all --json
atlas module src/lib.rs --root . --population pub
```

`--pub-only` aliases `--population pub`; conflicting selectors refuse. Unflagged invocations and graph-only Rust query functions keep their
earlier report contract. The native-style query's `all` includes private subjects; test modules
remain outside its population. Directly test-gated helpers retain the native
outside-test-modules boundary. Macros and line accounting do not narrow with
visibility. Project wrappers select `pub-ish` by default and format these facts
into their own established output; project-specific meanings remain
the wrapper's concern.

Name fragments filter both definition and reexport sites, as in the native tool.
Relationships use the existing approximate lexical derivation, with target
visibility independent of source-body selection. Externally shadowed spellings
remain explicitly unresolved, a conservative difference from the old native
tool's omitted edge. They are never promoted to a workspace definition.
`unmentioned_names` retains the native workspace-wide spelling measure: a mention
in one package rescues that spelling everywhere. It does not identify unused
resolved definitions. Fan-in/out are written-use heuristics over retained import
sites. Malformed required imports make name/module reading unavailable.

Text bounds matches, edges and diagnostics and names omitted counts; JSON retains
all observations and ambiguity. Header text is normalized by the shared lexical
projection; the CLI supplies comment decoration. The query context shares one
lazy relationship/site cache and releases its temporary selected graph after
those observations are derived. No source files are reopened.

Capture diagnostics are available separately as `Snapshot::source_diagnostics`;
`Snapshot::graph().diagnostics` keeps the compatibility combined graph view.
Native census reports capture/membership limitations. Name/module reports add
relationship limitations at displayed sites and state the fixed lexical limits,
rather than repeating every unrelated graph warning. Repeated readings preserve
one witness per package/file/line/message. Excluded source remains visible in
all these reports. This selection does not turn unknown source into a zero.

`Navigation::dump()` supplies selected declarations, their file-derived native
homes, normalized export occurrences, and selected ordinary relationship edges.
`ExportKind` distinguishes public conventional-root facades from other reexports.
`BindingEvidence` states whether candidates matched a written qualifier suffix,
a package-local name, or a workspace name. `Resolution` retains every ambiguous
candidate and an explicit reason for a missing target. A unique lexical candidate
is not Rust name resolution; a fallback can be inspected directly in this evidence.
The source model's contextual declaration IDs are retained. Binding IDs append
an occurrence ordinal to the source declaration ID, so grouped aliases and shared
physical files remain distinct.

```sh
atlas dump --root . --population all --json
atlas dump --root . --pub-only --json
```

Population-selected CLI JSON includes `schema`, `question`, selection and source basis.
The unflagged graph dump retains its existing schema. A native project wrapper
maps the typed declaration/export facts into its flat `items`/`edges` format,
legacy IDs and visibility labels; these generic facts are not recalculated there.

### Declaration review

`current.declaration_diff(&prior, scope, include_relationships)` compares captured
native declaration blocks, complete attached docs/attributes and their observed
owners. Both navigators must select the same visibility population; a mismatch
refuses instead of inventing additions/removals. The report records that
population. Added, modified and removed are enum variants with exactly the
required source versions. Pure line relocation does not change content, and
same-named items in different packages or observed owners cannot cancel.

```sh
atlas diff HEAD --root . --items --graph --json
atlas diff HEAD --root . --items --pub-only
atlas module src/lib.rs --root . --items --graph
```

Unflagged diff retains its module-shape contract. `--items` defaults to all native
subjects; population controls narrow those subjects, not the line totals.
`--graph` requires `--items` and reports known incident lexical relationships
from prior/current source. An ambiguous target-only incidence is outside that
native rule. `relationships: null` means the relationship query was not requested;
an empty array means it was requested and found no known incidents. Module
`--items` and `--graph` control presentation of already-typed module facts.

Function declaration blocks end at the AST body opener, in both inline and
multiline forms. An implementation-body edit is therefore outside the signature
comparison in either layout. Full attached doc and attribute payloads are compared;
internal blank lines in a raw attribute string are preserved. Body moves and
qualified contract comparison remain their own questions.

## Revisions

`revision::Revision::read` extracts a resolved Git commit into an owned temporary
directory. The private directory lease owns cleanup independently of the legacy
public `root` path; changing that path cannot redirect deletion. `root`/`commit`
readers remain available, but callers construct revisions through `read`, not
struct literals. Dropping one revision does not delete another extraction or
modify the inspected working tree.

Revision subprocesses isolate repository selection from inherited `GIT_*`
environment variables without changing the caller's process environment. Ordinary
Git reads verify that its reported toplevel is the requested workspace root;
jj workspaces resolve only their own explicit backing. An ancestor repository
cannot supply a revision merely because the selected source sits beneath it.

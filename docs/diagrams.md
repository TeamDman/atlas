# Diagrams

Each Atlas view answers one question about written Rust source and keeps the
source sites behind every box and edge. Text output is the default; add
`--backend pele` for SVG. Every view also writes a JSON evidence file so you can
check exactly what was observed and what was left unknown.

| Question | Command |
| --- | --- |
| Which files call, mention or store each other's types? | `dependencies` |
| Who produces, accepts and stores a type? | `representations` |
| How does one function's control flow branch and return? | `flow` |
| How did those contracts change between two revisions? | `compare` |
| How do the stages you declared hand types to each other? | `overview` |
| How do declared groups or lexical mentions connect? | `map`, `program` |

Atlas never invents an architecture for a project. A diagram's selection comes
from a saved question, a command-line selector or declarations you wrote. To
explore Atlas itself:

```sh
atlas representations --query examples/queries/atlas.json --panel all --backend pele
```

## Qualified file relationships

```sh
atlas dependencies --scope src/backend --relation calls
atlas dependencies --scope src/backend --relation signatures --backend pele
atlas dependencies --scope src/backend --relation fields --backend pele
atlas dependencies --scope src/backend --relation all --json
```

Every displayed edge has positive source witnesses, with file, line, source item,
target item and evidence kind. Calls use conservative path/receiver resolution;
signature types and direct field types use AST type positions, not a lexical word
match. Enum variants and generic parameters do not become same-spelled type edges.
The physical scope selects both endpoints. Observations outside it and unresolved
calls remain in JSON; they are not claims of absence. Disconnected files remain
in the view.

## Representations and producers

```sh
atlas representations --type Graph --panel producers
atlas representations --type Graph,Checked,Executable --panel contracts --backend pele
atlas representations --query question.json --panel all --backend pele
```

A saved question selects source symbols, never arrows:

```json
{
  "title": "From source model to rendered artifact",
  "types": [
    { "name": "SourceGraph", "file": "crates/atlas-core/src/source/model.rs" },
    { "name": "Diagram", "file": "crates/atlas-core/src/diagram/mod.rs" },
    { "name": "Artifact", "file": "src/render/mod.rs" }
  ],
  "preparation_files": ["crates/atlas-core/src/diagram/projection.rs", "src/cli/map.rs"]
}
```

Optional `apis` selects exact function names and files. Otherwise written-public
functions and trait implementation methods returning selected forms are used;
this syntactic selection does not prove public reachability through reexports.
Ambiguous names refuse. Methods use their written impl spelling, such as
`Generator<'g>::candidates`. Query types and API roots select the figure; extra
literal initializers outside their resolved reach are not erased.

Panels have different visual grammars:

- `contracts`: input/return type relationships, grouped by physical source file.
  Resolved workspace error forms appear as `Err` branches even when not selected
  as main forms; external/primitive errors may remain unresolved. Source titles
  preserve signatures and observed literal sites. Same type does
  not mean same runtime value.
- `producers`: every observed returner and immediate literal constructor of each
  selected type, in separate columns. A forwarding returner is not called a literal
  constructor. A repeated route prompts reading; it is not a duplication verdict.
- `preparation`: returners in selected files beside their source-ordered direct
  producer calls. Repeated calls remain repeated observations.
- `retention`: an optional sorted direct-field matrix. `&` marks references; `*`
  marks nested type syntax. Complete expressions remain in titles/JSON:
  `Arc<Graph>` does not establish ownership of Graph.
- `all`: contracts, producers and preparation in one composite SVG.

Select 1–12 types and at most 40 contracts/preparation rows per question. These
projection limits apply to every backend, including JSON/ASCII; narrow the question
when it exceeds them. Figures refuse without truncating source facts. Retention reserves two lines per cell; longer cell labels
require a narrower question or ASCII.

## Compare a change

```sh
atlas compare --before HEAD
atlas compare --before OLD_COMMIT --after NEW_COMMIT --query question.json --backend pele
```

The same selection is applied to both sources. Defaults compare the exact `before`
commit with current working source; `--after` reads another exact revision. Git
objects are read into an owned temporary tree and removed afterward. Atlas neither
checks out nor snapshots the user's working copy. No fetch happens.

Keys use physical file and qualified source symbol, excluding line-number churn.
The Pele comparison aligns before/after values, names both revisions, and labels
added/removed/changed observations. Several rows can describe the same source edit;
unchanged observations are omitted. It shows types, contract positions, field syntax,
literal-site counts and direct producer-call sequences. Removed types remain visible.
Moves/renames are reported as removal/addition; ambiguous identities refuse. Pele
refuses more than 40 changes; narrow the query or use ASCII/JSON. The current reader
requires a repository-root Cargo workspace; subdirectory workspaces and external
path dependencies in a historical snapshot may refuse. Git submodule contents are
not materialized. No semantic equivalence or rename inference is claimed.

## Overview from declared stages

```sh
atlas overview --adapter path/to/adapter --backend pele
atlas overview --adapter path/to/adapter --declarations src/stages.rs --json
```

`overview` draws the representation contracts between the stages a project
declares in its own source, for example a literal `const PIPELINE` table. A
**declaration adapter**, a small executable you build against the `atlas`
library, interprets that table. Atlas gives the adapter the constant's syntax and
the module mounts from its own parse; the adapter does not reparse or compile the
project. Atlas then checks the reply against the source snapshot before drawing
anything. Computed declarations are refused. See
[extending Atlas](extensions.md) for the protocol and
[`examples/pipeline-adapter.rs`](../examples/pipeline-adapter.rs) for a complete
adapter.

The main forms are the successful output types of each stage's entry function.
Public methods join the picture when they produce a form that another stage
accepts, or construct an entry type. Unexpected literal constructors outside the
entry functions' resolved reach stay visible. Instrument modules get a separate
panel of their observed direct calls. Declared dependencies never become observed
edges, and they never hide unexpected relationships. This is a contract view, not
an execution order.

## Navigation and source-body views

```sh
atlas census --json
atlas name Plan
atlas module src/backend
atlas name Plan --relations --json
atlas flow --path src/lib.rs --function compile_with --depth 1 --backend pele --format svg
```

`flow` reads a selected function's expression control: operand evaluation, branches,
match patterns and guards, loops, explicit returns and each residual `?`. With
`--depth 1` (up to 6), it expands source-resolved callees and pairs each call
occurrence with its own return continuation. Physical source files enclose nodes;
repeated calls never share a return vertex. Recursion, depth limits and unresolved
calls are labelled boundaries that continue only if they return. Expansion refuses
above 600 nodes without emitting a partial picture; select a smaller entry or depth.
Without `--depth`, it shows one source body without resolving callees.

This remains a source-control picture, not feasibility analysis or a proof of
termination. Panics/unwinding, destructors, implicit operator dispatch and macro
execution are not inferred. Closures, async/try/unsafe blocks, labelled blocks and
unsupported syntax stay opaque; they are not flattened into ordinary work.
Compound assignment order depends on types, so it also stays opaque. Call/return
edges describe the supported ordinary-return path. The executable cases in
`tests/source_control.rs` show each construct.

`program --architecture PROFILE` and
`map --architecture PROFILE` remain available for existing callers. Their profiles
are declared boundaries/permissions, not observed program behavior.

`name --relations` adds approximate incoming/outgoing lexical evidence to a name
card. The old `related` spelling is a hidden compatibility alias and warns on stderr;
its JSON contract is unchanged. `dump` retains the complete lexical source model for
library and machine consumers. `map` retains declared-boundary/quotient views, which
are different questions from qualified direct file relationships.

The older `map --view types`, `name --relations` and `dump` relationships are explicitly
approximate lexical co-mentions. Use `dependencies` for qualified calls/signature/
field relationships. This distinction is deliberate; legacy evidence is not silently
upgraded into a semantic claim. Saved profiles are optional legacy inputs, never
required for the source-declaration overview.

## What qualified facts cover

The shared source facts cover written production structs/enums, signatures, direct
fields, immediate struct literals and conservative resolved calls. Exact source
expressions preserve containers, references and tuple positions. Macro expansion,
trait/alias normalization, Default/Clone construction, indirect state and dynamic
dispatch are not complete. Deferred literal construction is excluded. Conditional
alternatives remain alternatives. Unknown type spellings include primitives and
external types as well as unavailable resolution. Name resolution needs unambiguous
Cargo mounts; unusual module-declaration formatting may exceed the lexical discovery
scanner. None of these unknowns is a proved missing edge.

The `atlas` library exposes the same models: representation facts and
projections, qualified dependencies, declaration adapters, comparisons, diagram
models and the ASCII and Pele renderers. Renderers never reopen source. See the
[library guide](library.md).

## Named aliases and control detail

Representation queries include named Rust aliases such as `type Forms = Vec<Form>`.
Their written RHS is shown explicitly; the alias stays the signature's handoff.
RHS references are `alias-target` observations, never fields or ownership claims.
They are not recursively expanded. External/ambiguous RHS names remain unknown;
cycles remain finite written links. Comparison reports RHS changes.

For an entry sequence without separate borrow and synthetic `?` bridge boxes:

```sh
atlas flow --root . --path src/main.rs --function main --detail control --backend pele --format svg --out target/atlas/main.svg
```

`--detail expressions` is the default. Control detail contracts only designated
borrow/bridge nodes with one incoming and one outgoing edge. A residual decision
can join an unexpanded call with one continuation in the same file. Branches,
loops, guards, opaque operations, exits and occurrence-specific call returns remain.
Control detail refuses over 600 raw source-control nodes before folding, also at
depth zero. The original unexpanded expression detail keeps its existing limit.
Folded node labels and witnesses are retained in `coverage.summarized_nodes` in
`diagram.json`; `workflow.json` retains the original selected workflow steps.
Source-control possibilities are not path feasibility or a termination proof.

The Pele preparation panel groups functions by file and connects actual resolved
producer calls to one box per function identity. Two callers sharing a callee are
visible as two routes to that implementation, not a claim of one runtime value.
Locations witness call sites without asserting an evaluation order.
ASCII retains the corresponding comparable preparation rows.

Use `preparation_functions` in a saved query for an exact overview, for example
`[{"name":"lower","file":"src/frontend/lower.rs"},
{"name":"build","file":"src/frontend/program.rs"}]`.
Each selector must resolve uniquely and return a selected representation.
`preparation_files`, when also present, limits those selections to those files.
Omit function selection to retain all producer-returning rows in the chosen files.
All source facts remain in JSON; the query selects a view, not an asserted architecture.

## Renderer resource boundaries

Each embedded Pele invocation accepts at most 16 MiB of serialized JSON request
(including the bundled renderer), 32 MiB of SVG stdout and 64 KiB of diagnostic
stderr. Escaped JSON bytes count toward the request limit; serialization refuses
before starting Node when it exceeds the cap. A composed figure can contain more
than one independently bounded panel. The pinned renderer also retains its own
50,000-character diagram-source limit; the host I/O caps do not raise it.

The host concurrently writes the request and reads both output streams. Overflow
refuses instead of truncating success, and an I/O failure stops and reaps the
owned child. Startup diagnostics and the observed exit status survive a failed
request write. An otherwise successful child cannot supply an artifact after
incomplete input delivery. Non-UTF-8 output or output without an SVG root/closing
tag refuses; this protocol check is not a complete XML validator.

These are byte/resource boundaries, not a total execution-time or CPU budget.
Invoked processes must close their pipes and must not leave descendants retaining
them. Atlas owns the direct child; it does not manage a process tree. The embedded
bundle creates no child processes. Existing destination protection still applies
before publication. Native view failures publish no completed artifact; the map/
flow route retains captured source/diagram evidence and Mermaid after a failed
render, with an unsuccessful exit and no finished SVG/HTML.

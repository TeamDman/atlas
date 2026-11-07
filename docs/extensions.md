# Extending Atlas

Atlas separates source observations, project declarations, view models and output.
Its Rust library supports embedding. Independently installed declaration adapters
use a small JSON protocol; they do not link into the running CLI.

## Declaration adapters

Many projects already describe their own structure in source: a table of
pipeline stages, a list of layers, a registry of entry points. A declaration
adapter teaches Atlas to read that convention, so `atlas overview` can draw the
project the way its authors describe it.

```sh
atlas overview --adapter path/to/my-adapter --backend pele
atlas overview --adapter path/to/my-adapter --declarations src/stages.rs --json
```

The adapter is a separate executable that you pass explicitly. Atlas never runs
commands it finds in project configuration, and it never downloads plugins.
`--declarations` names the source file to read (default `src/lib.rs`). The
adapter receives the literal constant syntax that Atlas has already parsed, so it
does not compile the project or parse its source again.

Declarations decide which modules count as stages, what their entry functions
are and which types they expose. They never supply observed call edges. The JSON
evidence records the adapter's reply, its name and version, the source snapshot
and any diagnostics, so you can always see which parts came from declarations.

## Write an adapter

Implement `adapter::ProjectAdapter` and call `adapter::serve` from `main`.
[`examples/pipeline-adapter.rs`](../examples/pipeline-adapter.rs) is a complete
adapter for a `PIPELINE` table. Copy it as a starting point:

```sh
cargo build --example pipeline-adapter
atlas overview --adapter target/debug/examples/pipeline-adapter
```

A project usually keeps its adapter in its own repository, as a small binary
that depends on `atlas` at a pinned revision.

`Request` provides the selected declaration file, normalized constant syntax and
production-file fingerprints/module mounts. `Snapshot::constant` refuses absent
or ambiguous declarations. `Snapshot::module_root` accepts a qualified module path
or a globally unique leaf; leaves do not imply top-level Rust scope. Computed
expressions remain opaque. There is no Rust syntax evaluator in the adapter.

Return `Architecture` with source constant IDs in `provenance`. The host validates
those references and binds the declarations to the project. It checks consistency
and source custody, not the truth of your interpretation. Preserve all ambiguity
and scope limits rather than guessing.

The protocol uses one request/reply on stdin/stdout, version `1`, interface
`project-declarations/v1`, with an echoed input snapshot ID and producer identity.
The reply is declarations or an explicit refusal. stdout must contain only JSON;
stderr is for diagnostics. Request/reply/diagnostic limits are 16 MiB / 2 MiB /
64 KiB. Incompatible, malformed, oversized or stale results refuse before any
diagram is written. Successful diagnostics remain available in the invocation
result and generated evidence. An explicit adapter is trusted executable code,
not a sandboxed process.

For embedding, `adapter::adapt` applies the same interface directly, and
`validate_reply` applies the same reference checks. The process invocation returns
`Invocation { reply, diagnostics }`. These APIs do not install anything or decide
a project's landing policy.

## Implement a rendering backend

`render::View` carries one native model: graph, representations (including producer
tables and retention matrices), comparison rows, or overview. Implement
`Renderer::render_view` for the families you support. Return `Artifact` documents;
do not inspect source files to invent a second analysis.

ASCII and Pele implement these families through the same entry. View-specific
meaning, ordering and source evidence belong to the projection. A renderer must
not convert a table to an edgeless graph to claim support.

Existing graph-only `Renderer::render(&Diagram)` implementations remain valid.
Their default `render_view` accepts graphs and returns typed `UnsupportedView`
errors for other families. Pele honors HTML wrapping while preserving the SVG and
Mermaid companions; ASCII returns text. External renderer processes and arbitrary
new view schemas are outside this first interface.

## Limits

Source snapshots cover the supplied file manifest and bytes, not an atomic
filesystem transaction. Fingerprints detect change; they are not cryptographic.
Producer names are self-reported. The scanner limits in the [guide](guide.md)
still apply. Independent-consumer and refusal cases live in `tests/extensions.rs`.

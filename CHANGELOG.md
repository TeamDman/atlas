# Changelog

Atlas has two packages: `cargo-atlas` (the `cargo atlas` command and its
library) and `atlas-lib`. Before 0.11.0 they were named `atlas` and
`atlas-core`. Versions below name the package they apply to.

## cargo-atlas 0.12.0 / atlas-lib 0.5.0

Declaration adapters must be rebuilt against `cargo-atlas` 0.12.0: the adapter
protocol is now version 2, and an adapter built for version 1 is refused with
"unsupported adapter protocol".

- Adapters can resolve a module in the package that declares it:
 `snapshot.module_root_from(&input.source, "name")`. A module with the same
 name in another workspace package is no longer a competing candidate.
 `module_root` keeps its meaning (every package). Each snapshot file lists its
 `packages`.
- **Breaking (atlas-lib):** constants with call syntax whose callee is a plain
 path, such as `Some(Part::Host)` or `Wrapper(1, X)`, arrive as
 `Literal::Call { path, args }` (JSON kind `call`) instead of `Opaque`. This
 is syntax only; nothing is evaluated. Method calls, macros and callees with
 generic arguments stay `Opaque`.
- **Breaking (atlas-lib):** `Literal` is `#[non_exhaustive]`. Add a wildcard arm
 to matches on it.

## cargo-atlas 0.11.0 / atlas-lib 0.4.0

The packages are renamed for crates.io, where `atlas` and `atlas-core` were
already taken, and the command becomes a Cargo subcommand. To migrate:

| Before | After |
| --- | --- |
| `cargo install --git … atlas` | `cargo install --locked cargo-atlas` |
| binary `atlas`, run as `atlas <command>` | binary `cargo-atlas`, run as `cargo atlas <command>` (there is no `atlas` binary) |
| package `atlas`, `use atlas::…` | package `cargo-atlas`, `use cargo_atlas::…` |
| package `atlas-core`, `use atlas_core::…` | package `atlas-lib`, `use atlas_lib::…` |
| delta JSON `"command": "atlas delta …"` | `"command": "cargo atlas delta …"` |

- **Breaking:** help, examples, report headings and suggested commands say
 `cargo atlas`. The delta report's `command` field names the command to re-run,
 so it uses the binary that now exists.
- **Breaking:** declaration adapters depend on `cargo-atlas` instead of `atlas`.
- Both packages can be published to crates.io. Their `include` lists are
 anchored to the package root, so untracked or linked directories in a working
 copy are never packaged.

## atlas 0.10.0

- Diagram images are dark by default. `--theme light` restores dark text on a
 white background; every command with `--backend pele` accepts `--theme`.
- SVG images carry concrete colours and an opaque background instead of CSS
 variables, so viewers and converters that ignore CSS variables (such as
 `rsvg-convert`) no longer draw solid black boxes.
- **Breaking (library):** `render::Pele` has a `theme` field, and
 `representations::pele`, `overview::pele` and `Comparison::pele` take a
 `render::Theme`. `Pele::default()` is dark.

## atlas 0.9.0

- **Breaking:** the project-specific overview adapter that earlier versions
 installed beside `atlas` is no longer part of this package, and
 `atlas overview` now requires `--adapter PATH`. Projects keep their
 declaration adapter in their own repository; see
 [`examples/pipeline-adapter.rs`](examples/pipeline-adapter.rs).
- The example adapter is renamed from `foreign-adapter` to `pipeline-adapter`
 and reports the producer name `example-pipeline`.
- Documentation is rewritten for new users: a getting-started tour built on a
 small example project (`examples/tutorial`), a concepts guide with a glossary,
 and guides to diagrams, architecture checks (including a policy file
 reference), the library and extensions.
- `atlas compare` indents multi-line values, such as a struct's fields, under
 their label.

## atlas 0.8.0 / atlas-core 0.3.0

- Structural source facts come from syn. Invalid files stay in the inventory as
 syntax errors instead of disappearing.
- `Source::of`, projection `outline`/`declaration_lines`/`test_regions`, and the standalone `non_test_loc`, `TestRegions::of`, `struct_fields`, `impl_trait`, `restricted_path` and `import_bindings` readers return Result. A parse failure cannot masquerade as an empty inventory. `struct_fields` returns named `Field` values; `declaration_lines` returns owned `(line, Item)` facts. `LineItem` is retired. Obtain Item and navigation Site values from source/navigation rather than constructing partial struct literals. Public compatibility module paths still name the same implementations.

## atlas 0.7.0 / atlas-core 0.2.0

- The index owns correlated name/function/boundary tables. Its `functions()`, `boundaries()`, `roots()` and `inventory()` return borrowing slices; public mutable vectors are no longer exposed. Replace old field reads with these methods and use `to_vec()` when a client needs its own copy. Numeric positions belong to the index that produced them. `Target::InvalidFunction` explicitly reports an out-of-range caller; views must refuse it rather than classify it as an unresolved source call. Root public module paths and legacy JSON schemas remain stable.
- Embedded rendering bounds its child process I/O and keeps failure diagnostics.

## atlas 0.5.0

- The library's project-specific `architecture::source::read` is removed in
 favor of the declaration adapter protocol.

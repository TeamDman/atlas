# Changelog

Atlas has two packages: the `atlas` CLI and library, and `atlas-core`. Versions
below name the package they apply to. Neither is published to crates.io yet;
depend on an exact Git revision.

## atlas 0.9.0

- **Breaking:** the project-specific overview adapter that earlier versions installed
 beside `atlas` is no longer part of this package, and `atlas overview` now requires `--adapter PATH`. Projects
 keep their declaration adapter in their own repository; see
 [`examples/pipeline-adapter.rs`](examples/pipeline-adapter.rs).
- The example adapter is renamed from `foreign-adapter` to `pipeline-adapter`
 and reports the producer name `example-pipeline`.
- Documentation is reorganized into a guide, diagram, checks, library and
 extension pages.

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

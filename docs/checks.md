# Architecture checks

Atlas can check captured source against an architecture you write down. The
mechanisms are fixed Rust code in `atlas-core`: stage ordering, item seams,
size and surface budgets, typed-error spelling, documentation coverage and
ratchets between a baseline and a candidate. You supply the rules in a TOML
policy. Atlas has no plugin or scripting layer.

| Command | Question |
| --- | --- |
| `comments --audit` | Which selected items or files lack line documentation? |
| `policy` | Is the policy itself consistent with the source it describes? |
| `stages` | Do production paths respect the declared stage order and interfaces? |
| `seams` | Which items cross stage boundaries, who writes shared types, which tests couple to which stages? |
| `delta` | Did a candidate revision raise any budget or debt relative to its baseline? |

## Three outcomes

Every check separates:

- **violations**: the source breaks a rule,
- **unavailable**: evidence a rule needs is missing, for example a file failed
  to parse, an import could not be read or an external authority was not supplied,
- **observations**: facts reported without a verdict.

A PASS requires no violations *and* nothing unavailable. Missing evidence takes
precedence in the status, and known violations stay visible beside it. A check
that reads only part of a project never claims the project's complete PASS.

## Project wrappers

Some rules depend on meanings only the project knows, such as how a stage's
boundary is written or which issue identifiers a commit message may cite.
`atlas-core` accepts those as typed inputs (`policy::Inputs`, `delta::Inputs`,
`stages::seams::BoundaryDeclaration`) from a small project-owned wrapper. Run on its own,
the CLI leaves them unavailable, so a policy that needs them cannot pass through
the CLI alone. This keeps project-specific rules out of the shared parser.

## Documentation coverage

```sh
atlas comments --audit --scope src --population pub-ish
atlas comments --audit --citation-markers docs/,RFC- --json
```

Missing file headers (`//!`), headers without a citation and items without an
outer `///` comment are reported separately, each with its source site.
`--citation-markers` supplies the strings that count as a citation; without it,
citation checking is not requested. The audit counts line comments, not
`#[doc]` attributes or block comments. It reports coverage only and assigns no
architecture verdict.

## Stage paths and policy data

`checks::policy::Policy::parse` reads native schema-1 authored policy data,
rejecting unknown fields. It preserves the native field families and opaque
external references; it does not establish that a written authority is valid.
`stage_references(scope)` returns only the references required by that stage
selection. Local authority resolution belongs to the project's wrapper.

`checks::stages(snapshot, policy, scope)` ranks production textual paths from
the retained source and import facts. Root aliases, reexports, unplaced modules,
forward reads and unknown paths remain distinct. Competing conditional root
imports stay unjudged. Malformed required imports produce unavailable evidence,
never an empty successful reading. The report includes policy findings for the
same selected package; it does not claim complete policy PASS. Captured
file-module locations and directly declared functions/constants/statics can
prove a module/value import pair. Imports bring both namespaces; terminal inline
paths read the value and continued module paths read the module. External or
chained aliases, competing same-namespace bindings and targets already known in
both namespaces remain unjudged. This is bounded source evidence, not complete
Rust namespace resolution.

```sh
atlas stages --root . --policy .config/architecture.toml --scope my-package --json
```

`Snapshot::mounted_sources(package)` retains the native outside-src policy
population separately from Cargo module contexts. Each location and any missing
mount's attempted paths remain available to test-coupling consumers. This does
not expand source-tree budgets or establish compiler namespace resolution.
Unrelated test mounts do not make a production stage reading unavailable.

`checks::seams` derives item debt, unlisted exports, writer calls, test coupling
and monotone baseline ratchets from the same snapshot. Its additional input is
each project's interpreted `BoundaryDeclaration`, keyed by package and stage,
including the root file and captured fingerprint. Missing or stale declarations
cannot construct a seam reading. The project wrapper owns the meaning of its
boundary declarations and the resolution of any citations; core carries
the resulting citation findings into the ratchets without reinterpreting them.

`SeamReading` exposes native structured observations (`json`, `totals`), listing
changes and `refusals` against a baseline reading and `PublishedStages`. These
are source/path/count rules, not a complete gate verdict. Test-root resolution
uses the same resolver over all-test facts; required unjudged test paths make
the dependent census unavailable. Mounted support and every required source
input retain their fingerprints and diagnostics.

The standalone `atlas seams --policy FILE --json` reports unavailable when a
required project boundary interpreter is absent and directs callers to the
project wrapper. A policy with no selected seam rule reports `not-applicable`.
It never supplies an empty boundary or invents a project authority.

`seams --items` includes the complete item-observation packet in text output.
JSON always retains that packet. This presentation control does not supply
missing boundary/authority evidence or promote a report to a project verdict.
The stages command does not accept this seam-only control.

## Typed-error spelling

`checks::typed_errors::read(&analysis, packages)` reads the native fixed spelling
rule from the existing AST cache. It visits written type positions in the native
source-tree production population, including callbacks, fields and aliases.
The accepted constructor spellings are bare `Result` or `std::result::Result`
and `core::result::Result`; errors are bare `String` or the `std`/`alloc` string
paths. Qualified forms may start with `::`. Trivia does not hide a match. This
preserves the native written-spelling scope; raw identifier spellings, aliases
and macro expansion are not normalized into these names.

The unit is a distinct source line, even when several matching types appear on
it. Each package retains source fingerprints, matched lines and typed unavailable
inputs. Invalid required syntax, excluded source paths and empty populations
cannot become an empty successful reading. The rule uses no source I/O or
separate parser; an analysis remains valid after its captured source is removed.
It does not establish the project's complete policy verdict.

## Policy inputs

`checks::policy` validates the native schema-1 **policy inputs**. Configuration
membership and arithmetic, retained production authority homes and forbidden
items, fixed typed-error syntax, and required host/project observations share
one implementation. The command is not a measured landing verdict: size,
surface and concentration caps, dependency judgments and baseline/candidate
ratchets belong to `delta`.

```sh
atlas policy --root . --policy .config/architecture.toml --json
```

The library workflow is `Policy::parse` → capture `Snapshot` → build `Analysis`
→ `policy::observe` → `policy::validate`. Reports keep violations and unavailable
inputs separately. PASS requires both lists to be empty. Required unavailable
evidence takes status precedence while known violations remain visible; both
statuses exit unsuccessfully. The CLI labels the judgment `policy-inputs` in
JSON and text.

The host observer probes normalized workspace-relative forbidden paths and
runs authored patterns with `ast-grep` against a private temporary replica of
captured source bytes. It does not reread original Rust files. Paths crossing
intermediate symlinks are unavailable; a final symlink is an existing forbidden
path. Syntax helpers must exit 0 or 1, emit valid JSON and no stderr, and return
source lines inside the captured query population. Missing, malformed or stale
observations never mean zero matches. File/directory selectors use path-component
boundaries. Pattern populations retain test files as in the native helper;
authority declarations use production test evidence. The helper process has no
Atlas timeout or output-size cap; project wrappers retain control of execution
budgets. Atlas writes no inspected project files.

`Inputs::new(&snapshot)` lets a project wrapper supply explicit filesystem,
pattern and local-authority observations. Pattern readings retain an exact
file/fingerprint manifest, checked against the snapshot. Local references bind
the field path and original written value to `Resolution`; parsing an issue-like
string is not authority. The standalone CLI leaves those project resolutions
unavailable. Use the owning wrapper for the project's complete judgment. This
is a fixed Rust interface, not a dynamic policy-plugin system.

## Baseline and candidate delta

`checks::delta::check` composes policy-input validation, native size/public-surface
and normal-dependency measurements, ceiling changes, stage/seam judgments and
concentration debt ratchets over baseline/candidate captures. The native
`concentration` command remains the separate max-block/breadth shape report.
Body-line debt excludes navigation; size token-lines retain it. Native accounting
keeps individually gated helpers outside test modules. Those units do not change
the shared production population used by stage and typed-error checks.

```sh
atlas delta main --policy .config/architecture.toml --citation-markers docs/ --json
```

The host resolves the baseline and candidate (the current Git/JJ commit by
default), extracts the baseline, captures current source, and verifies the
materialized tree and capture against the owned candidate extraction. It then
loads the tracked policies, observes inputs, judges, and presents the result.
JJ name resolution always uses `--ignore-working-copy`: save working edits
outside Atlas before asking for a pinned delta. A different candidate, dirty
tracked content or extra untracked source refuses. Revision lookup requires
`.git` or `.jj` at the workspace root; ancestor/subdirectory workspaces are not
silently associated with a repository. Extraction omissions and source limits
remain errors/unavailable evidence. Executable-mode custody requires Unix.

JSON retains `verb`, exact `basis.baseline`/`basis.candidate`, `policy.raises`,
`violations`, `verdict` and a human `display`. As in the native gate interface,
JSON REFUSE is data and exits successfully: its consumer must admit the verdict
and any required purchase. UNAVAILABLE exits unsuccessfully; missing measurements
are null. Text refuses unsuccessfully. These are architecture observations and
judgments, not compilation or runtime-equivalence proofs.

A project wrapper supplies `delta::Inputs`: policy authority resolutions,
source-bound before/after stage Boundary declarations, header citations and
project-parsed commit citations. Header facts carry captured file fingerprints;
commit facts carry the exact description fingerprint. Project test-support
features are explicit input data, never inferred from a feature containing
`test`; the shared cfg implication reader handles their permitted remaps.
`--test-support-features` supplies those names for generic CLI use.

The standalone command does **not** interpret project boundary declarations or commit
citations. A policy requiring `stage_seams` or those authorities therefore stays
UNAVAILABLE through this command; use its owning project wrapper for a decisive
project verdict. This keeps project legislation out of the shared parser and
does not introduce a policy-plugin framework or another authored policy file.

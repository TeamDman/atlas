# Atlas guide

This guide explains the ideas that every Atlas command shares. Read it once, and
`atlas <command> --help` will cover the rest.

## How Atlas reads a project

Atlas asks Cargo for the workspace layout (`cargo metadata`), reads each package's
Rust files once into an immutable **snapshot**, and parses them with syn. Every
command answers from that snapshot. Editing files while a command runs cannot
change its answer, and Atlas never writes to the project it reads.

A physical file can belong to several Cargo targets, for example a file shared by
a library and a binary. Atlas keeps one copy of the file and one observation per
package context. It never picks a single owner for a shared file.

A file that fails to parse stays in the inventory as a syntax error. It is never
silently dropped, so it cannot turn into a zero count or a clean result.

## Three kinds of evidence

Commands that look alike can answer different questions. Atlas labels each
answer with the evidence it rests on.

| Evidence | What it means | Used by |
| --- | --- | --- |
| **Lexical** | A name is spelled somewhere. Candidates are approximate, and ambiguity is kept. | `census`, `name`, `module`, `dump`, `map --view types` |
| **Qualified** | A call, signature type or field type was resolved through Cargo targets, modules and imports. Unresolved targets stay unresolved. | `dependencies`, `representations`, `compare`, `overview`, `flow --depth` |
| **Declared** | Something you wrote down: an architecture policy, a profile, or declarations read by an adapter. | `stages`, `seams`, `policy`, `delta`, `program`, `map --architecture` |

A declared permission is never shown as an observed edge, and an observed edge is
never removed because a declaration does not allow it.

## Project root and scope

Commands run against the current directory. To read another project, pass
`--root path/to/project`. Older invocations that pass the root as the first
positional argument still work.

`--scope` narrows what a command reports. Review and diagram commands take a
workspace-relative path (`--scope crates/core/src`). Navigation commands also
accept a package or directory name. Stage and seam checks take a Cargo package
name. A scope that matches nothing is an error, not an empty report.

## Populations

Every count depends on what is included, so Atlas makes the selection explicit.

- `--population all` includes private items.
- `--population pub` includes only items written `pub`.
- `--population pub-ish` (the usual default) includes `pub` and restricted
  visibility such as `pub(crate)`.

Test modules (`#[cfg(test)] mod tests`) are outside the default population.
Helpers gated individually with `#[cfg(test)]` outside such modules are still
counted by line-oriented reports such as `nontest-loc`. Qualified views exclude
them. Each report states which rule it used.

## Revisions

Review commands compare the current source with an earlier revision:

```sh
atlas diff HEAD~1                 # Git
atlas diff @-                     # jj
atlas compare --before v1.2.0 --after v1.3.0
```

Atlas resolves the revision to an exact commit and extracts that commit into a
private temporary directory. It never checks out, stashes, fetches or snapshots
your working copy. The repository must be at the workspace root: Atlas does not
borrow a repository from a parent directory.

## Output

Text is the default. Long lists are trimmed, and each trim says how much it
left out.

- `--json` prints every observation, including ambiguous and unresolved ones.
  JSON reports carry a `schema` number and describe their selection and source
  basis.
- `--backend pele` renders an SVG through the embedded Pele renderer, which runs
  on Node.js. Commands that offer `--format` can also wrap the SVG in an HTML page.
- `--out path` chooses the file. Relative paths resolve against the workspace
  root. Without it, text goes to stdout and SVG goes to `target/atlas/<name>.svg`.

Each diagram is written with its JSON evidence and a Mermaid companion beside it.
Atlas refuses to write output over any file it read, including source, saved
queries and adapters. If rendering fails, it does not publish a partial file.

## Commands

| Task | Commands |
| --- | --- |
| Navigate | `census`, `name` (`--relations` for lexical neighbors), `module`, `dump` |
| Review a change | `diff` (`--items`, `--graph`), `moved`, `visibility`, `compare` |
| Measure shape | `spans`, `nontest-loc`, `concentration` |
| Audit documentation | `comments --audit` |
| Draw | `dependencies`, `representations`, `flow`, `compare`, `overview`, `program`, `map` |
| Check architecture | `policy`, `stages`, `seams`, `delta` |

`atlas --help` lists them all. Each command's `--help` ends with a short
statement of the question it answers, its population, its evidence and an example.

## Limits that apply everywhere

- Macros are not expanded. Their bodies appear as opaque steps.
- `cfg` alternatives are kept side by side. Feature combinations are not evaluated.
- Types are not inferred, and trait or dynamic dispatch is not resolved.
- Control-flow views show what the syntax allows. They do not prove that a path
  is feasible or that a loop terminates.
- Module discovery follows written `mod` declarations and Cargo targets. It is
  not the compiler's expanded module tree.

When one of these limits affects an answer, the report says so.

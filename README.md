# Atlas

Atlas helps you understand a Rust codebase. It reads the source code of a Cargo
project and answers questions about how the code is put together:

- Where is this type or function defined, and what uses it?
- What changed between two commits?
- Which files call into which other files?
- What paths can this function take?
- Does the code still follow the structure we agreed on?

Atlas is a command-line tool and a Rust library. It only reads your code: it
never edits it, compiles it or runs it. Every answer points back to the file
and line it came from, so you can check it yourself.

```console
$ atlas name check
check: 2 declaration sites (ambiguous)

src/check.rs:10:check [Pub Fn]
  Check a parsed tree.
  2 outgoing, 0 incoming candidate-unique, 0 incoming ambiguous
  pub fn check(tree: Tree) -> Result<Checked, String> {

src/lib.rs:2:check [Pub Mod]
  0 outgoing, 0 incoming candidate-unique, 0 incoming ambiguous
  pub mod check;
```

Here the name `check` belongs to both a function and a module. Atlas shows
both and says the name is ambiguous, instead of guessing which one you meant.

## Why use Atlas?

Your editor (with rust-analyzer) is the best tool for working on one piece of
code at a time. Atlas is for questions about the whole project at once, asked
from a terminal, a script, a code review or a CI job.

- **Find your way around.** Count what a project contains, look up any name,
  and list what a module holds.
- **Review changes.** See which files grew, which functions changed or moved,
  and which items became more or less public.
- **Draw diagrams.** Show how files depend on each other, which functions
  produce and consume a type, and how a function branches. The output is text,
  or an SVG image.
- **Check structure.** Write down rules, such as "the parser must not use the
  code generator" or "this crate stays under 2,000 lines", and have Atlas
  check every change against them.

Atlas is careful about what it knows. When a name could mean two things, it
shows both. When it cannot work out what a call refers to, it says so, rather
than leaving the call out. A missing answer is reported as missing, never as
"nothing found".

## Install

You need Rust 1.99.1 or newer.

```sh
cargo install --locked --git https://github.com/flowerornament/atlas.git atlas
```

Some features need more tools:

- **SVG diagrams** need Node.js 20 or newer. The drawing code is built into
  Atlas, so there is nothing else to install.
- **Comparing commits** needs Git and `tar`. In a
  [jj](https://github.com/jj-vcs/jj) repository, Atlas also needs `jj`, and
  reads the Git store behind it.

To get the same Atlas every time, for example in CI, add `--rev <commit>` to
the install command.

## Try it

This repository includes a tiny example project in
[`examples/tutorial`](examples/tutorial). [Getting started](docs/getting-started.md)
walks through it step by step. In short:

```sh
git clone https://github.com/flowerornament/atlas.git
cp -r atlas/examples/tutorial /tmp/tutorial
cd /tmp/tutorial

atlas census                 # what does this project contain?
atlas name Tree              # where is Tree defined, and what uses it?
atlas module src/check.rs    # what is in this file?
atlas flow --path src/main.rs --function main     # what can main do?
atlas dependencies --relation calls               # which files call which?
```

Inside your own project, run the same commands from the project's root
directory, or point Atlas at it with `--root path/to/project`. Add `--json` to
any command to get machine-readable output, and run `atlas <command> --help` to
learn exactly what a command counts.

## Documentation

- [Getting started](docs/getting-started.md): a guided tour using the example project
- [How Atlas works](docs/concepts.md): the ideas behind every command, and a glossary
- [Diagrams](docs/diagrams.md): each kind of diagram and how to read it
- [Architecture checks](docs/checks.md): writing rules for your project and checking them
- [Using the library](docs/library.md): calling Atlas from your own Rust code
- [Extending Atlas](docs/extensions.md): teaching Atlas your project's conventions
- [Contributing](CONTRIBUTING.md), [code architecture](ARCHITECTURE.md) and the
  [changelog](CHANGELOG.md)

## What Atlas does not do

Atlas reads source code the way a careful person would, without running the
compiler. That means it does not:

- expand macros (a macro call is shown as one opaque step),
- decide which `#[cfg(...)]` options are active,
- work out types that the code does not write down,
- decide which trait implementation a call will use at run time,
- prove that a path through a function can actually happen.

When one of these limits affects an answer, the output says so.

## License

Atlas is available under either the [MIT](LICENSE-MIT) or the
[Apache 2.0](LICENSE-APACHE) license, at your option. The bundled
[Pele](vendor/pele/README.md) diagram renderer is MIT licensed.

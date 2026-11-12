# One Program: Build the Tools into gdsuite

## Status

In progress. The tools have their library entry points (gdtest 0.2.7,
gdtidy 0.3.4, gdhealth 0.3.6, gdtypecheck 0.2.4), and gdsuite builds them
in. Changes part of [docs-and-editor-overhaul](docs-and-editor-overhaul.md), section 1.

## The problem

gdsuite installs four tools by downloading their release archives into each
project's `.gdsuite/`. Most of gdsuite's setup machinery exists only to make
that safe:

- `gdsuite init` and `gdsuite setup` download, unpack and check each archive;
- `.gdsuite-release.toml` lists the tested release of each tool and a SHA-256
  per platform, refreshed by a script on every tool release;
- `[tools]` pins, `gdsuite update`, drift warnings ("gdtest 0.2.2 is
  installed, but this project now uses 0.2.3") and the compatibility report
  keep the downloaded tools in step with gdsuite;
- the first `gdsuite` command in a fresh clone needs the network, and CI
  downloads the tools on every run.

For a user this reads as: install GDSuite, then install it again in every
project, then keep the two in step. `cargo install gdsuite` building only
gdsuite and then downloading prebuilt tools looks odd for the same reason.

## The decision

**gdsuite contains the four tools.** Each tool exposes its command line as a
library function, gdsuite depends on the four crates, and one `gdsuite`
program runs everything. Nothing is downloaded, and updating gdsuite updates
the tools.

The tools stay separate projects with their own repositories, releases, docs
and command lines; people who want only gdtidy still install gdtidy.

## How it works

### In each tool

The binary's `main` moves into the library as `run(args)`, and `main` calls
it with the process arguments:

```rust
// gdtidy/src/main.rs
fn main() -> std::process::ExitCode {
    gdtidy::run(std::env::args_os())
}
```

`args` includes the program name, as `std::env::args_os()` does, so clap
parses it unchanged. gdhealth already was a library and gets `run_from(args)`
next to `run()`. gdtest also exposes `addon_files()`, the addon files it
embeds for `gdtest init`, so gdsuite can install them without a download.

The change is mechanical: `main.rs` becomes `lib.rs`, plus a few lines. Each
tool's tests pass unchanged.

### In gdsuite

gdsuite still runs each tool **as its own process**, by running itself:

```text
gdsuite tool gdtidy fmt --check
```

`gdsuite tool <name> [ARGS]...` is a hidden command that calls the tool's
`run`. gdsuite also runs a tool when it is started under the tool's name
(`gdtest`, `gdtidy.exe`), like busybox, so a copy or link named `gdtest`
behaves as gdtest.

A separate process keeps everything that works today: each tool's output,
exit codes and `std::process::exit` calls stay its own, gdsuite still reads
JSON from a tool's standard output, and a tool crash doesn't take gdsuite
down. This keeps the rule in AGENTS.md: gdsuite runs the tools, it never
reimplements them.

In `gdsuite.toml`, the default source of each tool is `bundled`. The other
sources (`path:`, `git:`, `crate:`, `github:`) still work, for tool
developers. `gdsuite setup` with built-in tools writes only gdtest's addon
files into `.gdsuite/`, and clears what an earlier gdsuite downloaded.

## What the trial showed

On Windows, release builds, with the demo project from the docs:

| | Before | After |
|---|---|---|
| To install | gdsuite (7.5 MB), then 4 downloads per project (14.4 MB) | gdsuite (14.0 MB) |
| `.gdsuite/` in a project | 31 MB (archives, unpacked copies, binaries) | 235 KB (gdtest's addon files and state) |
| `gdsuite init` | needs the network | works offline |
| Older project | — | `gdsuite update` removes the `[tools]` pin, `setup` clears the downloads, `doctor` is ok |

`init`, `doctor`, `fmt`, `lint`, `typecheck`, `test`, `health`, `check` and
`report ci` all work, and `doctor` shows each tool's version as before
(`gdtidy 0.3.3`). gdsuite's 51 tests pass. Four tests that checked a mocked
gdtidy's or gdtest's output now check the real one, and the release-checksum
test goes with the downloads.

## What it changes for each channel

| Channel | Today | With the tools built in |
|---|---|---|
| **cargo** | builds gdsuite, which downloads the tools | `cargo install gdsuite` builds everything; nothing to download |
| **GitHub releases** | one gdsuite archive, then downloads | one archive per platform, complete |
| **Godot Asset Store** | the addon would have to download gdsuite (plan A) | see below |
| **CI** | downloads the tools on each run | one download, of gdsuite, which caches like any binary |

### The Asset Store

The store accepts GDExtension libraries but not executables. With the tools
built in, gdsuite is a library too, so the same code can be compiled as a
GDExtension (with godot-rust): the addon then carries the whole of GDSuite,
with no download, no terminal, and no separate install. This is plan B of the
overhaul RFC, and building the tools in is what makes it affordable:

- Formatting, linting and code health run inside the editor.
- Tests and type checking start Godot, as today; the GDExtension starts them
  as gdsuite does.
- The addon holds one library per platform, about 15 MB each (Windows,
  Linux, macOS universal). Check the store's size limits before relying on
  this.

Plan A (the addon downloads gdsuite on first use) stays the quickest way to
reach the store, and plan B replaces it once the GDExtension builds.

## What it changes for `gdsuite init`

`init` did four things. With the tools built in:

| `init` | Becomes |
|---|---|
| Write `gdsuite.toml` | Optional. Every setting has a default and there is no `[tools]` by default, so commands can run without the file |
| Download the tools | Gone |
| Find and remember Godot | Commands already find it; `--godot-exe` and `GODOT_EXE` cover the rest |
| Install the gdtest addons | Still needed: tests run inside Godot, so the harness must be in the project |

Proposal: **every command works in any Godot project, with no setup.**

- `gdsuite fmt`, `lint`, `typecheck` and `health` need nothing.
- `gdsuite test` installs the gdtest addons the first time, and says so.
- `gdsuite init` stays, optional: it writes a commented `gdsuite.toml` and
  installs the addons, for people who want them in place before the first
  run.
- In the Asset Store addon, enabling the plugin does what `init` does.

The docs' First steps then reads: install GDSuite, run `gdsuite check`.

## What goes away

Once the tools are built in by default:

- release downloads, archive unpacking, `.gdsuite-release.toml` checksums and
  `scripts/release_checksums.py`, and `reqwest`, `zip`, `tar`, `flate2` and
  `sha2` from gdsuite's dependencies (a smaller binary);
- the compatibility report between gdsuite and the installed tools, and the
  drift warnings: the tools are always the ones gdsuite was built with;
- `gdsuite update`, or it shrinks to removing old `[tools]` pins (a migration
  step that can move into `gdsuite migrate`);
- most of `gdsuite setup`: only the gdtest addons remain, which `test`
  installs itself.

Tool developers keep `path:` and `git:` sources to try a tool's changes in a
real project. Whether `crate:` and `github:` stay is an open question.

## Costs

- **A tool fix needs a gdsuite release.** In practice it already does: the
  bundled release manifest decides which tool version projects get. The
  release script releases the tool, then bumps gdsuite.
- **gdsuite builds the four tools.** A clean `cargo install gdsuite` takes
  longer, since it compiles the four tools as well.
- **The tools' library entry point becomes an API.** It is one function per
  tool whose input is the command line, which is already a public contract.
- **Version skew between tools is fixed at build time.** The four tools must
  agree on shared crates (gdsyntax, clap, serde); cargo picks one version of
  each. They already depend on the same gdsyntax.

## The editor panel

The gdtest panel runs a `gdtest` program. With the tools built in, there is
no `gdtest` file in `.gdsuite/`. Two options:

1. **The panel runs `gdsuite tool gdtest ...`** when it finds gdsuite.
   Simple; the future `addons/gdsuite/` panel works this way anyway.
2. **`setup` puts a link named `gdtest` in `.gdsuite/bin/`**, pointing at
   gdsuite, which runs as gdtest when started under that name (this already
   works). A hard link goes stale when gdsuite is replaced, so it must be
   refreshed on each run, or be a copy.

Option 2 is in place, so the panel keeps working without a gdtest
release: `setup` hard-links gdsuite to `.gdsuite/tools/gdtest/bin/gdtest`
(or copies it, 14 MB, where links don't work, such as across drives), and
every gdsuite command replaces the link when gdsuite changed since. Option 1
replaces it once the panel knows gdsuite.

## Order of work

1. **Tools** (done): each tool's library entry point, released.
2. **gdsuite** (done, 0.5.0): depend on the released crates, make `bundled`
   the default source.
3. **Docs** (done, 0.5.0): Install, Under the hood, Configure (`[tools]`), CI
   and Troubleshooting no longer talk about downloading tools.
4. **No setup needed** (0.6.0): commands work without `gdsuite.toml` or
   `setup`, the first `test` installs the addons, `init` is optional.
5. **Remove downloads** (0.6.0): release downloads, archives and checksums
   are gone, and with them `github:` and `crate:` sources (`update` removes
   them). `path:` and `git:` stay, for tool developers. The compatibility
   report stays: it still checks `path:` and `git:` builds.
6. **Editor**: the panel calls `gdsuite tool gdtest`; then the GDExtension
   (Asset Store plan B).

## Open questions

- `gdsuite tool <name>`: keep it hidden, or document it as a way to run a
  tool's commands that gdsuite has no shortcut for?
- What is the Asset Store's size limit for an addon?

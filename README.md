# gdsuite

GDSuite keeps a Godot project tidy, correct and easy to change: it formats and
lints GDScript, type checks the project with Godot, runs your tests, and
measures code health, with one command:

```bash
gdsuite check
```

**[Read the docs](https://q-rault.github.io/gdsuite/)**, starting with
[Install](https://q-rault.github.io/gdsuite/install): download `gdsuite` from
the [releases](https://github.com/q-rault/gdsuite/releases) (no Rust needed),
or `cargo install gdsuite`, then run `gdsuite init` in your project.

You only install gdsuite. It downloads and runs these tools for you, at
versions tested together; each also works on its own:

| Tool | Role | Repository | Docs |
| --- | --- | --- | --- |
| `gdtidy` | Format and lint | [github.com/q-rault/gdtidy](https://github.com/q-rault/gdtidy) | [docs](https://q-rault.github.io/gdtidy/) |
| `gdtypecheck` | GDScript and shader compile errors, broken references | [github.com/q-rault/gdtypecheck](https://github.com/q-rault/gdtypecheck) | [docs](https://q-rault.github.io/gdtypecheck/) |
| `gdtest` | Tests | [github.com/q-rault/gdtest](https://github.com/q-rault/gdtest) | [docs](https://q-rault.github.io/gdtest/) |
| `gdhealth` | Code health and architecture | [github.com/q-rault/gdhealth](https://github.com/q-rault/gdhealth) | [docs](https://q-rault.github.io/gdhealth/) |

gdtidy, gdtest and gdhealth parse GDScript with [gdsyntax](https://github.com/q-rault/gdsyntax). Every tool is a single binary, so setup needs no Python.

User documentation lives in the [docs portal](https://q-rault.github.io/gdsuite/) (source under [`docs/`](docs/)), which covers `gdsuite` and all four tools.

Common commands:

```bash
gdsuite init
gdsuite setup --output plain
gdsuite doctor --output json
gdsuite check
gdsuite fmt -- --check
gdsuite lint -- --fix
gdsuite typecheck
gdsuite test
gdsuite health
gdsuite report ci
```

`gdsuite.toml` is the one configuration file by default. Besides gdsuite's own settings, it can hold a section per tool, with the keys of that tool's own file: `[test]` for gdtest, `[health]` for gdhealth, `[lint]` and `[fmt]` for gdtidy. Each tool reads its section itself, so the settings also apply when the tool runs without gdsuite, such as gdtest from the Godot editor. `init` and `setup` add no tool settings: the tools' defaults apply until you add a section.

`gdtest.toml`, `gdhealth.toml` and `gdtidy.toml` still work, but we do not recommend mixing them with `gdsuite.toml`: when `gdsuite.toml` has a tool's section, that tool ignores its own file, with a warning, and nothing is merged. `gdsuite migrate` moves them into `gdsuite.toml`. See [the config docs](https://q-rault.github.io/gdsuite/configure).

`gdsuite fmt` runs `gdtidy fmt`, which follows the official style guide and never changes what the code does, and lint runs `gdtidy lint`, which has gdlint's rules and reads gdlint's silencing comments. gdtidy reads neither `gdlintrc` nor `gdformatrc`: `gdsuite setup` (or `gdsuite migrate`, once gdtidy is installed) converts them into the `[lint]` and `[fmt]` sections of `gdsuite.toml` with `gdtidy migrate`, and leaves them in place. A project formatted with gdformat keeps its `{"a": 1}` dictionaries; other changes, such as enums written one member per line, are listed in [gdtidy's docs](https://q-rault.github.io/gdtidy/docs/formatting). A file with a syntax error fails neither fmt nor lint: gdtidy lists it as skipped, and the typecheck step reports the error.

Projects set up by gdsuite 0.2 or earlier can delete the `.gdsuite/python` folder, which held gdtoolkit.

`init`, `setup`, and `doctor` default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

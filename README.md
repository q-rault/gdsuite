# gdsuite

Install with `cargo install gdsuite`.

`gdsuite` is the workflow entrypoint for Godot projects. It installs local tooling, owns orchestration settings in `gdsuite.toml`, and delegates execution to [`gdtidy`](https://github.com/q-rault/gdtidy) (format and lint), `gdtest`, `gdhealth`, and `gdtypecheck`. Every tool is a single binary, so setup needs no Python.

User documentation lives in the [docs portal](https://q-rault.github.io/gdsuite/) (source under [`docs/`](docs/)), which covers `gdsuite`, [`gdtest`](https://github.com/q-rault/gdtest) and [`gdhealth`](https://github.com/q-rault/gdhealth).

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

`gdtest.toml`, `gdhealth.toml` and `gdtidy.toml` still work, but we do not recommend mixing them with `gdsuite.toml`: when `gdsuite.toml` has a tool's section, that tool ignores its own file, with a warning, and nothing is merged. `gdsuite migrate` moves them into `gdsuite.toml`. See [the config docs](docs/content/docs/config/gdsuite-toml.mdx).

`gdsuite fmt` runs `gdtidy fmt`, which follows the official style guide and never changes what the code does, and lint runs `gdtidy lint`, which has gdlint's rules and reads gdlint's silencing comments. gdtidy reads neither `gdlintrc` nor `gdformatrc`: `gdsuite setup` (or `gdsuite migrate`, once gdtidy is installed) converts them into the `[lint]` and `[fmt]` sections of `gdsuite.toml` with `gdtidy migrate`, and leaves them in place. A project formatted with gdformat keeps its `{"a": 1}` dictionaries; other changes, such as enums written one member per line, are listed in [gdtidy's docs](https://github.com/q-rault/gdtidy/blob/main/docs/formatting.md). A file with a syntax error fails neither fmt nor lint: gdtidy lists it as skipped, and the typecheck step reports the error.

Projects set up by gdsuite 0.2 or earlier can delete the `.gdsuite/python` folder, which held gdtoolkit.

`init`, `setup`, and `doctor` default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

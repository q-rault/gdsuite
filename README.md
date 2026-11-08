# gdsuite

Install with `cargo install gdsuite`.

`gdsuite` is the workflow entrypoint for Godot projects. It installs local tooling, owns orchestration settings in `gdsuite.toml`, and delegates execution to [`gdtidy`](https://github.com/q-rault/gdtidy) (format and lint), `gdtest`, `gdhealth`, and `gdtypecheck`. Every tool is a single binary, so setup needs no Python.

User documentation lives in the docs portal under [`docs/`](docs/), which covers `gdsuite`, [`gdtest`](https://github.com/q-rault/gdtest) and [`gdhealth`](https://github.com/q-rault/gdhealth).

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

`gdsuite.toml` is orchestration-only. Native tool behavior stays in the native files for each delegated tool, such as `gdtidy.toml`, `gdtest.toml`, and `gdhealth.toml`.

`gdsuite fmt` runs `gdtidy fmt`, which follows the official style guide and never changes what the code does, and lint runs `gdtidy lint`, which has gdlint's rules and reads gdlint's silencing comments. gdtidy reads neither `gdlintrc` nor `gdformatrc`: `gdsuite setup` (or `gdsuite migrate`, once gdtidy is installed) converts them into `gdtidy.toml` with `gdtidy migrate`, and leaves them in place. A project formatted with gdformat keeps its `{"a": 1}` dictionaries; other changes, such as enums written one member per line, are listed in [gdtidy's docs](https://github.com/q-rault/gdtidy/blob/main/docs/formatting.md). A file with a syntax error fails neither fmt nor lint: gdtidy lists it as skipped, and the typecheck step reports the error.

Projects set up by gdsuite 0.2 or earlier can delete the `.gdsuite/python` folder, which held gdtoolkit.

`init`, `setup`, and `doctor` default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

# gdsuite

Install with `cargo install gdsuite`.

`gdsuite` is the workflow entrypoint for Godot projects. It installs local tooling, owns orchestration settings in `gdsuite.toml`, and delegates execution to [`gdtidy`](https://github.com/q-rault/gdtidy) (lint), `gdtoolkit` (`gdformat`, until gdtidy formats), `gdtest`, `gdhealth`, and `gdtypecheck`.

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

`gdsuite.toml` is orchestration-only. Native tool behavior stays in the native files for each delegated tool, such as `.gdformatrc`, `gdtidy.toml`, `gdtest.toml`, and `gdhealth.toml`.

Lint runs `gdtidy lint`, which has gdlint's rules and reads gdlint's silencing comments. gdtidy does not read `gdlintrc`: `gdsuite setup` (or `gdsuite migrate`, once gdtidy is installed) converts it into `gdtidy.toml` with `gdtidy migrate`, and leaves the gdlintrc in place. A file with a syntax error does not fail lint: gdtidy lists it as skipped, and the typecheck step reports the error.

`init`, `setup`, and `doctor` default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

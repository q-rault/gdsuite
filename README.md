# gdsuite

Install with `cargo install gdsuite`.

`gdsuite` is the workflow entrypoint for Godot projects. It installs local tooling, owns orchestration settings in `gdsuite.toml`, and delegates execution to `gdtoolkit`, `gdtest`, `gdhealth`, and `gdtypecheck`.

User documentation lives in the docs portal under [`docs/`](docs/), which covers `gdsuite`, [`gdtest`](https://github.com/q-rault/gdtest) and [`gdhealth`](https://github.com/q-rault/gdhealth).

Common commands:

```bash
gdsuite init
gdsuite setup --output plain
gdsuite doctor --output json
gdsuite check
gdsuite fmt -- --check
gdsuite lint -- --verbose
gdsuite typecheck
gdsuite test
gdsuite health
gdsuite report ci
```

`gdsuite.toml` is orchestration-only. Native tool behavior stays in the native files for each delegated tool, such as `.gdformatrc`, `.gdlintrc`, `gdtest.toml`, and `gdhealth.toml`.

`init`, `setup`, and `doctor` default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

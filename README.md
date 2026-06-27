# gdsuite

`gdsuite` is the suite-owned workflow entrypoint for Godot projects. It installs local tooling, owns the top-level config contract, and coordinates `gdtoolkit`, `gdtest`, and `gdhealth`.

The primary user documentation now lives in the local docs portal under [`docs/`](docs/). That portal is intended to be the main docs home for `gdsuite`, `gdtest`, and `gdhealth`.

Common commands:

```bash
gdsuite init
gdsuite setup --output plain
gdsuite doctor --output json
gdsuite check
gdsuite test
gdsuite health
gdsuite report ci
```

`init`, `setup`, and `doctor` now default to human-readable scoped logs like `[gdsuite:init] step ...`. Use `--output plain` for stable shell-friendly text and `--output json` for structured `doctor` integrations.

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

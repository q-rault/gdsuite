# gdsuite

`gdsuite` is the suite-owned workflow entrypoint for Godot projects. It installs local tooling, owns the top-level config contract, and coordinates `gdtoolkit`, `gdtest`, and `gdhealth`.

The primary user documentation now lives in the local docs portal under [`docs/`](docs/). That portal is intended to be the main docs home for `gdsuite`, `gdtest`, and `gdhealth`.

Common commands:

```bash
gdsuite init
gdsuite check
gdsuite test
gdsuite health
gdsuite report ci
```

Local validation for the Rust CLI remains:

```bash
cargo fmt
cargo test
```

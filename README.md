# gdsuite

`gdsuite` is a Rust CLI that wraps a project-local Godot tooling stack under `.gdsuite/`.
It pins and orchestrates:

- `gdtoolkit` for `gdformat` and `gdlint`
- `gdtest` for tests
- `gdhealth` for architectural health checks

## Commands

- `gdsuite setup` installs the local toolchain and writes missing config files.
- `gdsuite doctor` prints resolved tool paths, versions, and setup status.
- `gdsuite fmt [--check]` runs `gdformat` on discovered `.gd` files.
- `gdsuite lint [--fix] [--unsafe-fixes] [--report text|json]` runs lint and suite-owned fixers.
- `gdsuite typecheck` runs a Godot headless `load()` pass over discovered scripts.
- `gdsuite test -- ...` forwards arguments to project-local `gdtest`.
- `gdsuite health -- ...` forwards arguments to project-local `gdhealth`.
- `gdsuite check` runs the fast static gate: `fmt --check -> lint -> typecheck`.
- `gdsuite ci` runs the full sequential pipeline: `fmt --check -> lint -> typecheck -> test -> health`.

## Reporting

`gdlint` does not provide native JSON output. `gdsuite lint --report json` therefore emits a suite-owned JSON report derived from `gdlint` diagnostics, while `--report text` preserves the original text stream.

## Validation

Local verification:

```bash
cargo fmt
cargo test
```

Real-project validation against `../example-game`:

```bash
bash scripts/validate-example-game.sh
```

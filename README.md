# gdsuite

`gdsuite` is the platform entrypoint for a project-owned Godot developer stack.
It installs tooling under `.gdsuite/`, generates product configs, and gives a single workflow surface for:

- `gdtoolkit` for `gdformat` and `gdlint`
- `gdtest` for tests
- `gdhealth` for architectural health checks

The intended default flow is:

```bash
gdsuite init
gdsuite check
gdsuite ci
```

The suite-first workflow boundary is deliberate:

- `gdsuite check` is the default local validation command.
- `gdsuite ci` is the default CI validation command.
- `gdsuite test` and `gdsuite health` are delegated product entrypoints when you need direct subsystem control.

## Commands

- `gdsuite init` bootstraps the local platform state and is the recommended first command.
- `gdsuite setup` installs the local toolchain and writes missing config files.
- `gdsuite doctor` prints resolved tool paths, versions, and setup status.
- `gdsuite fmt [--check]` runs `gdformat` on discovered `.gd` files.
- `gdsuite lint [--fix] [--unsafe-fixes] [--report text|json]` runs lint and suite-owned fixers.
- `gdsuite typecheck` runs a Godot headless `load()` pass over discovered scripts.
- `gdsuite test -- ...` forwards arguments to project-local `gdtest`.
- `gdsuite health [--report text|json] -- ...` forwards arguments to project-local `gdhealth` and can emit a suite report.
- `gdsuite check [--report text|json]` runs the fast local gate: `fmt --check -> lint -> typecheck`.
- `gdsuite ci [--report text|json]` runs the CI-grade pipeline: `fmt --check -> lint -> typecheck -> test -> health`.
- `gdsuite report [latest|check|ci|health]` prints the last saved JSON suite report artifact.

## Workflow

Local development should usually use `gdsuite check` first and only add tests or health checks when the change justifies them:

```bash
gdsuite check
gdsuite test
gdsuite health
```

CI should use `gdsuite ci` as the canonical full validation contract:

```bash
gdsuite ci --report json
```

Delegated commands are intentionally narrow:

- `gdsuite test` defaults to the configured `gdtest` target and is the right entrypoint for behavior validation.
- `gdsuite test -- --filter combat` is for direct `gdtest` filtering or advanced flags.
- `gdsuite health` defaults to `gdhealth health` and preserves `gdhealth` artifact output.
- `gdsuite health -- --format json` or other forwarded arguments are for direct `gdhealth` usage beyond the suite default.

## Reporting

`gdlint` does not provide native JSON output. `gdsuite lint --report json` therefore emits a suite-owned JSON report derived from `gdlint` diagnostics, while `--report text` preserves the original text stream.

`gdsuite check --report json`, `gdsuite ci --report json`, and `gdsuite health --report json` emit a suite-level report envelope and save artifacts under `reports/gdsuite/`.

## CI Template

A starter GitHub Actions workflow is available at [`.github/workflows/gdsuite-ci.yml`](.github/workflows/gdsuite-ci.yml). It installs Rust and Python, bootstraps the local suite toolchain, and runs the canonical CI command:

```bash
gdsuite ci --report json
```

Additional workflow notes live in [`docs/ci-recipes.md`](docs/ci-recipes.md).

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

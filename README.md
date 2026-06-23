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
gdsuite test
gdsuite health
```

The suite-first workflow boundary is deliberate:

- `gdsuite check` is the default local validation command.
- CI should compose explicit suite steps instead of relying on a separate `ci` command.
- `gdsuite test` and `gdsuite health` are delegated product entrypoints when you need direct subsystem control.

## Commands

- `gdsuite init` bootstraps the local platform state and is the recommended first command.
- `gdsuite setup` installs the local toolchain and writes missing config files.
- `gdsuite doctor` prints resolved tool paths, versions, and setup status.
- `gdsuite fmt [--check]` runs `gdformat` on discovered `.gd` files.
- `gdsuite lint [--fix] [--unsafe-fixes] [--report text|json]` runs lint and suite-owned fixers.
- `gdsuite typecheck` runs a Godot headless `load()` pass over discovered scripts.
- `gdsuite test [--report text|json] -- ...` forwards arguments to project-local `gdtest` and can emit a suite report.
- `gdsuite health [--report text|json] -- ...` forwards arguments to project-local `gdhealth` and can emit a suite report.
- `gdsuite check [--profile <name>] [--report text|json]` runs the fast local gate by default and can use any configured suite profile, including the built-in `ci` profile.
- `gdsuite report [latest|check|test|health|ci]` prints a saved suite JSON report artifact, or aggregates the explicit CI artifacts into one final suite report.

## Workflow

Local development should usually use `gdsuite check` first and only add tests or health checks when the change justifies them:

```bash
gdsuite check
gdsuite test
gdsuite health
```

CI should use explicit suite steps. A typical flow is:

```bash
gdsuite check --report json
gdsuite test --report json
gdsuite health --report json
gdsuite report ci
```

Delegated commands are intentionally narrow:

- `gdsuite test` defaults to the configured `gdtest` target and is the right entrypoint for behavior validation.
- `gdsuite test -- --filter combat` is for direct `gdtest` filtering or advanced flags.
- `gdsuite health` defaults to `gdhealth health` and preserves `gdhealth` artifact output.
- `gdsuite health -- --format json` or other forwarded arguments are for direct `gdhealth` usage beyond the suite default.

## Reporting

`gdlint` does not provide native JSON output. `gdsuite lint --report json` therefore emits a suite-owned JSON report derived from `gdlint` diagnostics, while `--report text` preserves the original text stream.

`gdsuite check --report json`, `gdsuite test --report json`, and `gdsuite health --report json` emit a suite-level report envelope and save artifacts under `reports/gdsuite/`.

When CI runs those commands as separate steps, `gdsuite report ci` aggregates `check.json`, `test.json`, and `health.json` into a final `ci.json` suite report and refreshes `latest.json`.

`gdsuite.toml` can set the default suite report mode:

```toml
[reports]
default = "json"
artifacts_dir = "reports/gdsuite"
```

When this is set, `check`, `test`, `health`, and `lint` use the configured mode unless `--report` overrides it on the command line.

`gdsuite init` now writes these suite sections explicitly into `gdsuite.toml` by default instead of relying on hidden internal defaults.

## Profiles

`gdsuite.toml` can define named suite pipelines. Built-in defaults are available even when omitted from the file:

```toml
[profiles.local]
pipeline = ["fmt", "lint", "typecheck"]

[profiles.strict]
pipeline = ["fmt", "lint", "typecheck", "test", "health"]
health_fail_on = "warning"
```

The built-in `ci` profile also exists with `["fmt", "lint", "typecheck", "test", "health"]` and uses the default `[health].fail_on` threshold. `strict` overrides the health threshold to `warning`. `gdsuite check` defaults to the `local` profile. Use `--profile ci`, `--profile strict`, or another configured profile to override the pipeline for a single run.

## CI Template

A starter GitHub Actions workflow is available at [`.github/workflows/ci.yml`](.github/workflows/ci.yml). It installs Rust and Python, bootstraps the local suite toolchain, and runs explicit suite steps:

```bash
gdsuite check --report json
gdsuite test --report json
gdsuite health --report json
```

Additional workflow notes live in [`docs/ci-recipes.md`](docs/ci-recipes.md).

## Product Configs

`gdsuite init` generates plain product configs when they are missing:

- `gdtest.toml` gets suite-aligned test discovery patterns and a `profile.ci` reporter default.
- `gdhealth.toml` gets suite-aligned discovery include/exclude rules and logical file groups.

Those files remain ordinary `gdtest` and `gdhealth` configs. Users can run either product directly without `gdsuite`, and `gdsuite` will not overwrite an existing product config.

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

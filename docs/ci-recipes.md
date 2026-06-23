# CI Recipes

CI should compose explicit suite steps. The expected split is:

- local default: `gdsuite check`
- CI workflow: `gdsuite check`, `gdsuite test`, `gdsuite health`

## GitHub Actions

Use [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) as the baseline workflow.

The template assumes:

- Rust is available to build `gdsuite`
- Python is available for `gdtoolkit`
- `godot` is resolvable on `PATH`, or `GODOT_EXE` is set to the executable path

The baseline CI steps are:

```bash
./target/release/gdsuite check --report json
./target/release/gdsuite test --report json
./target/release/gdsuite health --report json
./target/release/gdsuite report ci
```

`gdsuite report ci` reads `reports/gdsuite/check.json`, `test.json`, and `health.json` and produces a final aggregated `reports/gdsuite/ci.json` artifact.

## Artifacts

The workflow uploads:

- `reports/gdsuite/` for suite JSON reports
- `reports/code-health/` for raw `gdhealth` bundle artifacts when present

## When To Use Delegated Commands

Keep CI centered on explicit suite steps. Use delegated commands only when a separate job has a clear reason to expose product-specific depth:

- `gdsuite test -- ...` for direct `gdtest` filtering, sharding, or product-specific flags
- `gdsuite health -- ...` for direct `gdhealth` subcommands or extra output formats

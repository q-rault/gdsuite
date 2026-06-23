# CI Recipes

`gdsuite ci` is the suite-owned CI contract. The expected Phase 2 split is:

- local default: `gdsuite check`
- CI default: `gdsuite ci`

## GitHub Actions

Use [`.github/workflows/gdsuite-ci.yml`](../.github/workflows/gdsuite-ci.yml) as the baseline workflow.

The template assumes:

- Rust is available to build `gdsuite`
- Python is available for `gdtoolkit`
- `godot` is resolvable on `PATH`, or `GODOT_EXE` is set to the executable path

The canonical CI command is:

```bash
./target/release/gdsuite ci --report json
```

## Artifacts

The workflow uploads:

- `reports/gdsuite/` for suite JSON reports
- `reports/code-health/` for raw `gdhealth` bundle artifacts when present

## When To Use Delegated Commands

Keep CI centered on `gdsuite ci`. Use delegated commands only when a separate job has a clear reason to expose product-specific depth:

- `gdsuite test -- ...` for direct `gdtest` filtering, sharding, or product-specific flags
- `gdsuite health -- ...` for direct `gdhealth` subcommands or extra output formats

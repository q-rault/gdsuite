# GDSuite PoC Design Note

## Command surface

`gdsuite` owns the user-facing workflow:

- `init`
- `migrate`
- `setup`
- `doctor`
- `fmt`
- `lint`
- `typecheck`
- `test`
- `health`
- `check`
- `report`

`test` and `health` are delegated commands with passthrough argument forwarding. `check` is the fast static gate and runs `fmt --check`, `lint`, and `typecheck`.

`check` resolves named suite profiles from `gdsuite.toml`. The built-in defaults are:

- `local` -> `fmt`, `lint`, `typecheck`
- `ci` -> `fmt`, `lint`, `typecheck`, `test`, `health`
- `strict` -> `fmt`, `lint`, `typecheck`, `test`, `health`

`strict` differs from `ci` through health enforcement: it overrides `gdhealth` to use `--fail-on warning`, while `ci` uses the configured default health threshold.

## Local install layout

The managed project layout is:

```text
.gdsuite/
  bin/
  python/
  tools/
  state/
    manifest.json
    versions.json
```

The venv supplies `gdformat` and `gdlint`. Rust tools are installed with `cargo install --root` into `.gdsuite/tools/*`.

## Config schema

`gdsuite.toml` is the suite-owned top-level config. `gdtest.toml` and `gdhealth.toml` remain delegated-tool configs, but migrate/sync operations now enforce explicit suite-owned invariants inside them.

Ownership split:

- suite-owned: the full `gdsuite.toml` contract, `.gdsuite/**` and report excludes, `gdtest` CI profile wiring, and other shared workflow defaults required for suite correctness
- product-owned: `gdhealth` groups and thresholds, `gdtest` advanced runtime knobs, and direct subsystem-specific behavior

`gdsuite migrate` is the canonical adoption path for an existing repo. It rewrites `gdsuite.toml` into the current suite contract, merges suite-owned invariants into product configs, and rewrites obvious `gdtest` / `gdhealth health` entrypoints in project docs and CI files. `setup --sync-product-configs` reuses the same config-sync layer but is framed as repair/bootstrap, not migration.

The PoC defaults assume local sibling source checkouts for `gdtest` and `gdhealth`, scripts under common Godot directories, and narrow safe/unsafe fixer rule sets.

## Fixer safety model

`lint --fix` is a suite-owned pipeline:

1. `gdformat`
2. native safe fixers
3. optional unsafe fixers
4. `gdlint` verification

Safe fixers are deterministic whitespace-level rewrites. The experimental unsafe fixer only handles declaration-only class files and skips ambiguous cases.

`lint --report json` is also suite-owned. `gdlint` only emits text, so `gdsuite` parses the text diagnostics into a stable JSON report rather than depending on upstream JSON support.

`[reports].default` in `gdsuite.toml` controls the default report mode for suite commands. `--report` still overrides config per invocation.

## Source resolution assumptions

Godot resolution order:

1. `--godot-exe`
2. `GODOT_EXE`
3. `gdsuite.toml`
4. PATH lookup
5. Windows probes

Tool source resolution order is explicit config first, then sibling checkout defaults embedded in generated config.

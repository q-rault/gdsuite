# GDSuite PoC Design Note

## Command surface

`gdsuite` owns the user-facing workflow:

- `init`
- `setup`
- `doctor`
- `fmt`
- `lint`
- `typecheck`
- `test`
- `health`
- `check`
- `ci`
- `report`

`test` and `health` are delegated commands with passthrough argument forwarding. `check` is the fast static gate and runs `fmt --check`, `lint`, and `typecheck`. `ci` is the full sequential gate and runs `fmt --check`, `lint`, `typecheck`, `test`, and `health`.

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

`gdsuite.toml` is the suite-owned top-level config. `gdtest.toml` and `gdhealth.toml` remain delegated-tool configs and are generated only when missing.

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

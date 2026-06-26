# Installation Contract And Source Model

## Status

Draft.

This RFC defines the intended installation contract for `gdsuite`.
It also records where the current prototype is narrower than the target model,
so follow-up implementation work can close the gap without changing the user
contract again.

## Why This Exists

`gdsuite` is the suite-owned entrypoint for project setup and workflow.
Teams need one stable contract for:

- how a fresh project is bootstrapped
- how tool versions are pinned and reproduced on another machine
- how `gdtest` and `gdhealth` sources are described
- how `gdsuite` itself is distributed

The suite remains a dispatcher. It installs, locates, and invokes delegated
tools, but it does not reimplement their domain logic.

## Goals

- Make `gdsuite init` and `gdsuite setup` predictable on a clean machine.
- Define one source model for delegated Rust tools, independent of where the
  bits come from.
- Support zero-config local onboarding for the common case.
- Support team-owned version pinning through committed `gdsuite.toml`.
- Keep direct `gdtest` and `gdhealth` usage viable after setup.

## Non-Goals

- Redefining `gdtest` or `gdhealth` CLI behavior.
- Inventing a package manager beyond what Cargo, GitHub releases, and local
  files already provide.
- Moving project-specific test or health logic into `gdsuite`.

## Terms

- `gdsuite`: the top-level CLI and workflow contract owner.
- delegated tools: `gdtest` and `gdhealth`.
- tool source: the install origin recorded in `gdsuite.toml`.
- project-local toolchain: the files installed under `.gdsuite/`.

## Installation Contract

### `gdsuite init`

`gdsuite init` is the first-run bootstrap command for a project.

On success it must:

1. Resolve the Godot executable.
2. Create `.gdsuite/` and its state/tool directories.
3. Write `gdsuite.toml` if it does not already exist.
4. Write missing delegated product configs (`gdtest.toml`,
   `gdhealth.toml`).
5. Install the project-local Python environment used for `gdtoolkit`.
6. Install `gdformat` and `gdlint` into the project-local toolchain.
7. Install `gdtest` and `gdhealth` from the sources declared in
   `gdsuite.toml`.
8. Persist the resolved manifest and version state under `.gdsuite/state/`.
9. Install the `gdtest` runtime/editor addon files into the project using the
   configured suite paths, without overwriting locally modified addon files.
10. Verify the resulting toolchain is executable.

`init` is allowed to create missing files. It is not the command for
reconciling an already customized project with new suite-owned invariants.

### `gdsuite setup`

`gdsuite setup` is the idempotent install-and-reconcile command for an existing
`gdsuite` project.

It must:

- reuse the committed `gdsuite.toml` contract when it exists
- install, restore, or refresh the project-local toolchain under `.gdsuite/`
- preserve existing project configs unless `--sync-product-configs` is used
- refresh manifests and version state
- remain safe to rerun on partial or drifted local state

`repair` is still useful language for one outcome of `setup`, but it is too
narrow for the command name. `setup` also covers first install after clone,
normal reruns, and deterministic reconciliation against the committed suite
contract.

With `--sync-product-configs`, `setup` also re-renders the suite-owned subset
of `gdtest.toml` and `gdhealth.toml` while preserving product-owned fields.

### `gdsuite doctor`

`doctor` is the read-only validation command.

It must report:

- config and manifest locations
- resolved executable paths
- actionable missing-tool guidance
- drift between delegated configs and suite-owned invariants

## Source Model

The suite should describe delegated tool origins with one logical key per tool,
regardless of transport:

```toml
[tools]
gdtest_source = "crate:gdtest@0.4.0"
gdhealth_source = "github:https://github.com/q-rault/gdhealth/releases/download/v0.7.1/gdhealth-x86_64-pc-windows-msvc.zip"
```

The important rule is the shape of the contract, not the exact field names in a
future parser. One tool has one source field. Different install origins are
expressed as different schemes inside that field.

### Supported source forms

- `crate:<name>@<version>`
  Uses Cargo/crates.io semantics for installable Rust binaries.
- `github:<release-url>`
  Uses a concrete GitHub release asset URL as the pinned binary/archive source.
- `path:<local-path>` or a plain local path
  Uses a checked-out local directory for developer workflows and sibling repos.

Examples:

```toml
[tools]
gdtest_source = "crate:gdtest@0.4.0"
gdhealth_source = "crate:gdhealth@0.7.1"
```

```toml
[tools]
gdtest_source = "github:https://github.com/q-rault/gdtest/releases/download/v0.4.0/gdtest-x86_64-unknown-linux-gnu.tar.gz"
gdhealth_source = "github:https://github.com/q-rault/gdhealth/releases/download/v0.7.1/gdhealth-x86_64-unknown-linux-gnu.tar.gz"
```

```toml
[tools]
gdtest_source = "path:../gdtest"
gdhealth_source = "../gdhealth/tools/gdhealth"
```

### Source semantics

- The source string is the pinned install contract.
- `setup` installs exactly what the source string describes.
- The manifest under `.gdsuite/state/` records resolved local paths, not the
  desired source of truth.
- `gdsuite.toml` remains the desired source of truth for shared workflows.

## Zero-Config Defaults

Zero-config means a new user can succeed with `gdsuite init` in the common
local-development layout without first hand-authoring config.

The default bootstrap contract is:

- detect Godot from CLI override, `GODOT_EXE`, `gdsuite.toml`, then common PATH
  names and common Windows install locations
- create `gdsuite.toml` with suite defaults if missing
- default `gdtoolkit` to a compatible floating major line
- default delegated tool sources to local sibling checkout paths during the
  prototype phase
- default report settings and delegated config invariants

Today the prototype writes local-path defaults such as:

- `gdtest_source = "../gdtest"`
- `gdhealth_source = "../gdhealth/tools/gdhealth"`

That is acceptable for the current sibling-repo workflow, but it is not the
final public distribution story. Public onboarding should eventually prefer
versioned `crate:` or `github:` defaults where possible.

## Team Workflow

The expected team flow is:

1. One developer bootstraps or updates `gdsuite.toml`.
2. The team commits `gdsuite.toml`.
3. Another developer clones the project and runs `gdsuite setup`.
4. `setup` installs the exact delegated tool versions/sources declared in
   `gdsuite.toml`.
5. The local `.gdsuite/` directory remains generated machine state and is not
   the shared contract.

This gives the repo one reviewed, diffable installation contract instead of
relying on each machine's ad hoc setup.

## Distribution Story For `gdsuite`

`gdsuite` itself should be installable independently of any single game repo.
The expected distribution paths are:

- `cargo install gdsuite`
- prebuilt release assets for users who do not want a Rust toolchain
- local `cargo install --path .` for development inside the repo

The distinction is important:

- `gdsuite` is the globally acquired entrypoint
- delegated tools are project-pinned dependencies installed into `.gdsuite/`

That split keeps user entry simple while preserving per-project reproducibility.

## Current Prototype Gaps

The current implementation is narrower than this RFC in a few ways:

- `gdtest_source` and `gdhealth_source` are currently typed as local paths.
- delegated tool installation currently uses `cargo install --path`.
- remote crate and GitHub-release installs are not implemented yet.

These are implementation gaps, not contract ambiguity. Future work should add
scheme-aware install resolution without changing the high-level workflow above.

## Consequences

- Docs should describe `gdsuite.toml` as the shared install contract.
- Future installer work should parse tool sources by scheme.
- Validation should continue to distinguish committed contract
  (`gdsuite.toml`) from generated machine state (`.gdsuite/state/`).

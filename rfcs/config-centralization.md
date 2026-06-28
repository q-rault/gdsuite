# Config Centralization

## Status

Draft.

This RFC defines the target configuration model for `gdsuite`.
It replaces the current "shared invariants plus synced delegated configs"
direction with a simpler rule:

- `gdsuite.toml` is the normal config surface for `gdsuite`
- `gdtest.toml` and `gdhealth.toml` belong to their own tools
- `gdsuite` only reads delegated product configs during adoption/import

## Why This Exists

The current prototype spreads configuration across three files:

- `gdsuite.toml`
- `gdtest.toml`
- `gdhealth.toml`

That shape creates ambiguous ownership:

- users cannot easily tell which file is authoritative
- overlapping settings are easy to desynchronize
- repair flows such as sync and drift detection become part of the product
  contract instead of remaining implementation details

The suite should have one primary configuration surface for normal operation.
Direct product CLIs may keep their own config files, but those files should not
shape `gdsuite`'s runtime contract.

## Goals

- Make `gdsuite.toml` the single user-facing config surface for normal
  `gdsuite` workflows.
- Make command responsibilities unambiguous.
- Keep direct `gdtest` and `gdhealth` CLI usage possible without forcing that
  compatibility path into normal `gdsuite` behavior.
- Make adoption of existing repositories predictable through one import step.
- Remove ongoing sync and drift concepts from the intended architecture.

## Non-Goals

- Redefining `gdtest` or `gdhealth` as products.
- Preventing users from keeping `gdtest.toml` or `gdhealth.toml` for direct
  product CLI usage.
- Reopening the `gdhealth` versus `gdlint` ownership boundary. That remains a
  separate concern from config centralization.
- Defining the exact internal handoff format used when `gdsuite` invokes
  delegated tools.

## Terms

- `gdsuite.toml`: the suite-owned project config.
- delegated product configs: `gdtest.toml` and `gdhealth.toml`.
- adoption/import: the one-time process of reading existing delegated configs
  and mapping their relevant settings into `gdsuite.toml`.
- direct product CLI usage: running `gdtest` or `gdhealth` outside of
  `gdsuite`.

## Decision

`gdsuite.toml` is the only configuration file that `gdsuite` should read during
normal operation.

`gdtest.toml` and `gdhealth.toml` are not part of the normal `gdsuite`
runtime contract. They may exist in a repository for users who also run the
delegated product CLIs directly, but `gdsuite` should treat them as external,
product-owned files.

`gdsuite init` is the only suite command that should read delegated product
configs, and only for adoption/import.

After import:

- `gdsuite` does not continue reading delegated product configs
- `gdsuite` does not synchronize delegated product configs
- `gdsuite` does not report drift between delegated product configs and
  `gdsuite.toml`
- users may keep or delete the old delegated product configs depending on
  whether they still use the product CLIs directly

## Ownership Model

### `gdsuite.toml`

`gdsuite.toml` owns the suite-facing workflow contract:

- tool source declarations
- command defaults
- report behavior
- suite-level health behavior
- delegated-tool settings that matter to `gdsuite` workflows

If a setting is part of the normal `gdsuite` user experience, it belongs in
`gdsuite.toml`.

### `gdtest.toml`

`gdtest.toml` is owned by `gdtest`.

It exists only for users who run `gdtest` directly and want `gdtest`'s native
config file. `gdsuite` should not depend on it for runtime behavior.

### `gdhealth.toml`

`gdhealth.toml` is owned by `gdhealth`.

It exists only for users who run `gdhealth` directly and want `gdhealth`'s
native config file. `gdsuite` should not depend on it for runtime behavior.

The architecture-and-policy boundary between `gdhealth` and `gdlint` remains
important, but it is orthogonal to whether `gdsuite` reads `gdhealth.toml`.

## Command Responsibilities

### `gdsuite init`

`init` is the adoption and bootstrap command.

In addition to toolchain setup, `init` is responsible for:

- creating `gdsuite.toml` when needed
- detecting pre-existing `gdtest.toml` and `gdhealth.toml`
- importing relevant settings from those files into `gdsuite.toml`
- leaving delegated product config files untouched after import

`init` may warn that delegated product config files remain present and are no
longer part of the `gdsuite` runtime contract.

`init` should not create `gdtest.toml` or `gdhealth.toml`.

### Runtime `gdsuite` commands

Runtime commands such as:

- `gdsuite check`
- `gdsuite test`
- `gdsuite health`
- `gdsuite doctor`
- other normal workflow commands

should read `gdsuite.toml` and suite-managed state only.

They should not read `gdtest.toml` or `gdhealth.toml` as runtime fallback
inputs.

### `gdsuite setup --sync-product-configs`

The sync-based model is not part of the target architecture.

This means the current `--sync-product-configs` story should be removed,
replaced, or explicitly downgraded as transitional behavior in follow-up work.
The central contract should not depend on ongoing delegated-config rendering or
reconciliation.

### `gdsuite doctor`

`doctor` should validate the suite contract, toolchain state, and actionable
setup problems.

It should not define or enforce drift between `gdsuite.toml` and delegated
product configs in the target model.

## Migration Model

Existing repositories may already have:

- `gdtest.toml`
- `gdhealth.toml`
- partial suite-owned concepts duplicated across multiple files

The intended migration path is:

1. Run `gdsuite init`.
2. Detect delegated product configs if they exist.
3. Import the relevant settings into `gdsuite.toml`.
4. Continue using `gdsuite.toml` as the sole suite runtime config.
5. Leave old delegated product configs alone for users who still need direct
   product CLI usage.

This keeps migration explicit without making delegated config synchronization a
permanent part of the architecture.

## Direct CLI Compatibility

Direct `gdtest` and `gdhealth` usage should remain possible.

That compatibility has a clear boundary:

- `gdsuite` owns `gdsuite.toml`
- `gdtest` owns `gdtest.toml`
- `gdhealth` owns `gdhealth.toml`

If a user wants to run `gdtest` directly, they may keep a `gdtest.toml` for
that purpose. The same applies to `gdhealth.toml` for direct `gdhealth` usage.

That does not require `gdsuite` to read or maintain those files.

## Consequences

### User-facing consequences

- there is one normal config surface to learn for `gdsuite`
- config precedence becomes simpler
- direct product CLI usage remains available, but outside the suite-owned
  runtime contract

### Product consequences

- drift detection between `gdsuite.toml` and delegated product configs should
  disappear from the intended architecture
- delegated config syncing should disappear from the intended architecture
- docs that describe delegated config generation or narrow invariant syncing
  need to be updated
- current implementation behavior may remain transitional for a time, but the
  target contract should be described using the single-config model

## Open Questions

The main architecture decision in this RFC is the ownership model above. The
following details can be handled in follow-up work:

- how `gdsuite` internally hands resolved settings to delegated tools
- which existing config fields move, merge, or disappear when the schema is
  cleaned up around the single-config model
- whether any current command names should change once sync is removed from the
  product story

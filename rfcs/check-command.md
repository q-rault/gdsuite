# Check Command

## Summary

Define `gdsuite check` as the single fast validation command.

`gdsuite check` should run:

- formatting check
- lint check
- type-aware validation

If the chosen linter already provides the required GDScript type-aware pass,
`check` should not run a second redundant typecheck step. If it does not,
`check` should run a dedicated GDScript typecheck step.

`gdsuite check` must not run tests.
`gdsuite check` must not run `gdhealth`.

## Motivation

The command should answer one narrow question:

> Is this change still clean enough to continue working?

That question is different from:

- "Do the tests pass?" (`gdsuite test`)
- "Is the architecture healthy?" (`gdhealth`, if it exists at all)

Mixing those concerns into one umbrella command makes the fast path slower,
harder to explain, and less predictable.

## Goals

- Make `gdsuite check` obvious and stable.
- Keep the default local gate fast.
- Avoid hidden pipeline shape behind configuration names such as `local`,
  `ci`, or `strict`.
- Keep type-aware validation in the fast path.
- Keep slow validation separate.

## Non-Goals

- Running the full test suite.
- Running architecture health analysis.
- Turning `check` into a configurable workflow DSL.
- Reimplementing delegated product logic inside `gdsuite`.

## Proposed Command

The command surface should be:

```bash
gdsuite check
gdsuite check --fix
```

No `--profile`.
No built-in `local`, `ci`, or `strict` variants.
No `--report json`.

If the product needs a full CI-oriented composition later, that should be a
separate command or an explicit CI script, not hidden behind `check`.

## Behavior

`gdsuite check` should run a fixed fast pipeline in order:

1. format verification
2. lint verification
3. type-aware verification

The type-aware stage has two valid implementations:

- use the linter's own type-aware pass, if that capability exists and is the
  intended source of truth
- run a dedicated GDScript typecheck step otherwise

The important contract is the user-facing outcome, not the internal split.
Users should get one fast "static correctness" command.

## Fix Mode

`gdsuite check --fix` should be part of the primary workflow.

In fix mode, `check` should:

- apply formatting fixes
- apply safe lint fixes where available
- still run the remaining static checks after fixes are applied
- fail if unfixable lint or type issues remain

This matches the intended everyday loop:

- `gdsuite check` to verify
- `gdsuite check --fix` to auto-correct what the suite can safely correct

If the project later gains staged-file support, `gdsuite check --fix` is the
right default hook command for staged paths, following the same role `vp check
--fix` plays in VitePlus workflows.

## Optional Step Flags

If `check` exposes step-selection flags, they should stay narrow and explicit:

- `--fix`
- `--no-fmt`
- `--no-lint`

These flags are useful only for temporarily narrowing the fast gate during
local iteration or enabling safe automatic cleanup.

`gdsuite check --no-fmt --no-lint` should be an error.
The command must not silently succeed when all meaningful work has been turned
off.

## Output Rules

`gdsuite check` should be human-first terminal output, not delegated command
echo.

It should:

- not print delegated commands
- not print internal subprocess invocations
- print one `pass:` line only after a phase completes successfully
- mention type checks in the lint success line only when type-check is enabled

Representative success shape:

```text
pass: Formatting check passed
pass: Lint and type checks passed
```

If lint does not own type checking, the success output may instead be:

```text
pass: Formatting check passed
pass: Lint check passed
pass: GDScript type check passed
```

On failure, `check` should print:

1. a human-readable `error:` line
2. the raw diagnostics
3. a blank line
4. a final summary sentence

Representative failure output:

```text
error: Formatting issues found
src/index.js
steps.json

Found formatting issues in 2 files. Run `gdsuite check --fix` to apply fixes.
```

```text
error: Lint or type issues found
...diagnostics...

Found 3 errors and 1 warning in 2 files.
```

The exact wording can be adapted for GDScript, but the shape should stay stable.
That structure is easier to scan than interleaved tool banners or echoed
subcommands.

## Why No Profiles

Profiles add indirection without enough benefit for this command.

With profiles, the user has to know hidden pipeline meaning:

- `check --profile ci`
- `check --profile strict`

That is weaker than a command whose meaning is fixed.

For `check`, the abstraction cost is not worth it:

- it makes the command less discoverable
- it makes docs harder to write clearly
- it blurs the line between fast checks and slow validation

The command should mean the same thing everywhere.

## Relationship To `test`

`gdsuite test` should remain separate because it is a slow runner.

Tests are important, but they answer a different question than `check`.
Making tests part of `check` by default will either slow the command down too
much or force users to learn conditional behavior. Both are worse than a clean
split.

The intended mental model is:

- `gdsuite check`: fast static gate
- `gdsuite test`: slower behavioral validation

## Relationship To `gdhealth`

`gdhealth` should not be part of `check`.

Architecture analysis, thresholds, ownership rules, and similar policy checks
are not lint. They are a different category of validation with different cost,
different review semantics, and different failure discussion.

If `gdhealth` continues to exist in the broader ecosystem, it should remain a
separate tool path rather than being mixed into the fast `check` contract.

The suite should avoid treating `gdhealth` as if it were just "lint with more
rules." That boundary is conceptually wrong and makes the CLI harder to reason
about.

## Errors And Exit Codes

`gdsuite check` should fail when:

- the project is not bootstrapped
- required setup state is missing
- `--no-fmt --no-lint` disables the whole command
- format verification fails
- lint verification fails
- the type-aware stage fails
- fix mode completes but unfixable issues remain

The command should provide direct remediation guidance, usually pointing to
`gdsuite init` or `gdsuite setup` when local setup state is missing.

## Drawbacks

- Teams lose one generic mechanism for naming arbitrary validation pipelines.
- Some future CI flows may need a separate top-level command instead of reusing
  `check`.

These are acceptable tradeoffs because they preserve the meaning of the most
important everyday command.

## Alternatives Considered

### Keep Profiles

Rejected.

This makes `check` more abstract, but not clearer. The user has to memorize
named pipeline variants instead of learning one command with one purpose.

### Keep `gdhealth` Inside `check`

Rejected.

That turns the fast gate into a mixed-purpose umbrella command and weakens the
boundary between lint/static validation and deeper architecture analysis.

### Put Tests Inside `check`

Rejected.

Tests are slower and belong to a separate command with separate expectations.

## Alignment With VitePlus

This RFC is now intentionally closer to the VitePlus `check-command` model:

- one command
- one stable meaning
- optimized for the fast path
- deeper validation remains separate

That structure fits `gdsuite` better than a profile-driven `check` model.

# Check Command

## Summary

Define `gdsuite check` as the single fast static gate for everyday GDScript
validation.

`gdsuite check` should answer one narrow question:

> Is this project still statically clean enough to keep working?

That fast gate should cover:

- formatting
- linting
- GDScript type checking

`gdsuite check` must stay separate from:

- `gdsuite test`, which answers behavioral correctness
- `gdsuite health`, which answers architecture and policy correctness

## Motivation

Today there is no single obvious command that means "is my GDScript correct
enough to continue?" without also pulling in slower or conceptually different
validation.

That gap causes confusion:

- `fmt` is too narrow
- `lint` is closer, but still does not clearly communicate the full fast gate
- `test` is slower and solves a different problem
- `health` is deeper policy analysis, not the normal inner loop

`gdsuite check` should be the stable answer to that everyday workflow question.
It should be fast, explicit, and easy to explain in docs and CI snippets.

## Goals

- Make `gdsuite check` the obvious default static validation command.
- Keep the command fast enough for routine local use.
- Give `check` one stable meaning everywhere.
- Keep type checking in the fast path.
- Keep slow or policy-oriented validation separate.

## Non-Goals

- Running the full test suite.
- Running `gdhealth`.
- Turning `check` into a profile selector or workflow DSL.
- Defining report artifact behavior as part of the `check` contract.
- Reimplementing delegated tool logic inside `gdsuite`.

## Command Syntax

The intended command surface is:

| Command / flag | Meaning |
| --- | --- |
| `gdsuite check` | Run the full fast static gate. |
| `gdsuite check --fix` | Apply safe automatic fixes, then run the remaining enabled checks. |
| `gdsuite check --no-fmt` | Skip the formatting phase. |
| `gdsuite check --no-lint` | Skip the lint phase. |
| `gdsuite check --no-typecheck` | Skip the typecheck phase. |

Rules:

- `--fix` only changes whether safe fixes are attempted before verification.
- `--no-fmt`, `--no-lint`, and `--no-typecheck` only disable named phases.
- The command must fail with a usage error if all meaningful phases are disabled.
- The command must not grow `--profile`, `--report`, `ci`, `strict`, or similar
  contract-shaping switches.

## Behavior

`gdsuite check` should run sequentially and fail fast.

The pipeline order is:

1. formatting
2. linting
3. type checking

Behavior rules:

- each enabled phase starts only after the previous enabled phase succeeds
- the command stops at the first failing enabled phase
- later phases do not run after a failure
- `test` and `health` are never folded into this pipeline

The typecheck phase may be implemented in one of two ways:

- use the linter's own type-aware mode if that is the intended source of truth
- run a dedicated GDScript typecheck step otherwise

That implementation detail does not change the user-facing contract. Users
should still experience one fast static gate.

## Output Rules

`gdsuite check` output should be human-first.

It must:

- print one `pass:` line after each successful enabled phase
- print one `error:` line before failure diagnostics
- print the raw diagnostics after the `error:` line
- avoid echoing delegated subcommands or internal subprocess invocations
- end failure output with a short remediation-oriented summary sentence

Representative success shape:

```text
pass: Formatting check passed
pass: Lint check passed
pass: GDScript type check passed
```

If lint owns the type-aware pass, the final two lines may collapse into:

```text
pass: Formatting check passed
pass: Lint and type checks passed
```

Representative failure shape:

```text
error: Formatting issues found
res://player/player.gd
res://ui/hud.gd

Found formatting issues in 2 files. Run `gdsuite check --fix` to apply fixes.
```

```text
error: GDScript type check failed
res://combat/hitbox.gd:41 Cannot infer the type of "target"

Found 1 type error. Fix the reported issue and rerun `gdsuite check`.
```

## Fix Mode

`gdsuite check --fix` should be part of the normal local workflow.

In fix mode, `check` should:

- apply formatting fixes when available
- apply safe lint fixes when available
- rerun the remaining enabled phases after applying fixes
- still fail if unfixable lint or type errors remain

This keeps the main loop simple:

- `gdsuite check` to verify
- `gdsuite check --fix` to auto-correct what the suite can safely correct

## Relationship To Existing Commands

| Command | Primary question | Speed / depth | Included in `check`? |
| --- | --- | --- | --- |
| `gdsuite check` | Is the code statically clean enough to continue? | Fast local gate | Yes |
| `gdsuite test` | Does behavior still pass the test suite? | Slower behavioral validation | No |
| `gdsuite health` | Does the project still satisfy architecture and policy rules? | Deeper structural validation | No |

This split is intentional. A command that tries to answer all three questions at
once becomes slower, harder to teach, and less predictable.

## Why No Profiles

`check` should have one stable meaning, not a hidden pipeline selected by a
profile name.

Reject:

- `gdsuite check --profile ci`
- `gdsuite check --profile strict`
- any other profile-driven command shape

Reasons:

- profiles make the command less discoverable
- profiles weaken the distinction between fast and slow validation
- profiles make the docs harder to keep precise

If the project needs explicit CI composition, it should use separate commands
and scripts instead of redefining `check`.

## Errors And Exit Codes

`gdsuite check` should exit `0` only when every enabled phase succeeds.

It should exit non-zero when:

- required local setup or tooling is missing
- flag combinations disable the entire command
- formatting fails
- linting fails
- type checking fails
- fix mode completes but unfixable issues remain

The command should provide direct remediation hints when setup state is missing,
usually pointing the user to `gdsuite init` or `gdsuite setup`.

## Drawbacks

- Teams lose one generic mechanism for naming arbitrary validation pipelines.
- Some future automation flows may need explicit scripts instead of reusing
  `check`.

These tradeoffs are acceptable because they preserve the meaning of the most
important everyday command.

## Alternatives Considered

### Keep Profiles

Rejected. The command becomes less obvious and starts to hide pipeline meaning
behind names instead of behavior.

### Put `gdhealth` Inside `check`

Rejected. Architecture and policy analysis are not part of the fast static gate.

### Put Tests Inside `check`

Rejected. Tests are slower and answer a different question.

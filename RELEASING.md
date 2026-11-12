# Releasing gdsuite

A release is one run of **Cut release**, from the Actions tab, as in the tools.
There is no local release command, and nothing releases on its own: merging to
`main`, the pin PR included, releases nothing.

Cut release commits the new version and pushes a `v<version>` tag, then starts
**Publish release** (`.github/workflows/release.yml`) on it. Publish release
checks that the tag matches the version in `Cargo.toml`, builds the four
archives, publishes gdsuite to crates.io (trusted publishing, no token), then
creates the GitHub release with the archives. Tags with a prerelease suffix
(`v0.6.0-rc.1`) become GitHub prereleases, never marked "Latest".

## Releasing

**Actions → Cut release → Run workflow**, on `main`:

| Input | Values |
| --- | --- |
| bump | `patch`, `minor` or `major` |
| pre | `none` for a stable release, or `alpha`, `beta`, `rc` |
| version | an exact version instead, such as `0.6.0-rc.1` (overrides the two above) |
| dry run | pick the version and run the tests, push nothing |

It updates the version in `Cargo.toml`, `.gdsuite-release.toml` (`[suite]`,
which `doctor` shows) and `Cargo.lock`, runs `cargo test --locked`, commits
`release: v<version>` to `main`, pushes the tag and starts Publish release and
the docs deploy (`docs.yml`). The run summary shows the version it picked.

Which bump: below 1.0, a change that breaks something (a removed option, a
setting read differently) is `minor`; anything else is `patch`. Going to 1.0
is `major`, on purpose.

How the version is picked, from the current one:

| Current | bump | pre | Next |
| --- | --- | --- | --- |
| 0.5.0 | patch | none | 0.5.1 |
| 0.5.0 | minor | none | 0.6.0 |
| 0.5.0 | minor | rc | 0.6.0-rc.1 |
| 0.6.0-rc.1 | minor | rc | 0.6.0-rc.2 |
| 0.6.0-rc.2 | minor | none | 0.6.0 |
| 0.6.0 | major | none | 1.0.0 |

From a prerelease, a bump that already leads to its version stays on it, so
keep the same `bump` through the betas, the release candidates and the
release. Going back (from `rc` to `beta`) is refused. To see a pick without
releasing, run Cut release with **dry run**, or
`python scripts/release.py next-version minor rc`.

## When a release fails

- **Publish release failed on a transient error** (runner or network): open
  the run and use **Re-run failed jobs**.
- **It needs a fix, and crates.io does not have the version yet**: delete the
  tag (Code → Tags, or `git push origin --delete v<version>`) and the GitHub
  release if one was created, merge the fix to `main`, then run Cut release
  with **version** set to the same version. It tags `main` as it is, since the
  version is already there.
- **crates.io already has the version**: it cannot be published again. Release
  the fix as the next patch.

## Pinning the tools

The tools are built into gdsuite from crates.io, at exact versions:
`gdtest = "=0.2.7"` in `Cargo.toml`, and the same version as `tested` in
`.gdsuite-release.toml`, which `doctor` and the CLI tests read.

`pin-tools.yml` runs when a tool is released: the last job of each tool's
`release.yml` starts it once the release is on crates.io and GitHub. It can
also be run by hand from the Actions tab. It runs `scripts/pin_tools.py`,
which:

moves each tool to its latest crates.io release, in `Cargo.toml`,
`Cargo.lock` (`cargo update --precise`, which fails when the new versions
can't build together) and `.gdsuite-release.toml`, with gdtest's harness
contract.

It opens a PR from the `auto/pin-tools` branch, or updates the one already
open, so tools released one after another end up in the same PR. Review it,
wait for CI, merge, then run Cut release: the PR says which bump it suggests
(minor when a tool's major or minor version changed, patch otherwise).

To pin by hand: `python scripts/pin_tools.py`, or `--dry-run` to see what it
would change.

## One-time setup: the `PIN_TOOLS_TOKEN` secret

A PR pushed with a workflow's default token gets no CI, so pin-tools pushes
the pin PR with a token of yours instead:

1. On GitHub, open your **account** settings (not the repository's):
   Settings → Developer settings → Personal access tokens →
   **Fine-grained tokens** → Generate new token.
2. Name it `gdsuite pin-tools`, owner `q-rault`, and an expiration (note the
   date: pin-tools fails once it expires).
3. Repository access: **Only select repositories** → `q-rault/gdsuite`.
   The tools' releases are public, so no access to their repositories is
   needed.
4. Repository permissions: **Contents: Read and write**, **Pull requests:
   Read and write** (Metadata: read-only is added automatically).
5. Generate it and copy it.
6. In the **gdsuite repository**: Settings → Secrets and variables → Actions
   → New repository secret, named `PIN_TOOLS_TOKEN`, with the token as value.
7. Check it: Actions → pin-tools → Run workflow. With every tool up to date,
   the run ends with "every tool is pinned to its latest release".

## One-time setup: the `GDSUITE_TRIGGER_TOKEN` secret

The tool repositories (gdtest, gdtidy, gdhealth, gdtypecheck) start
`pin-tools.yml` at the end of their release. That takes a second token, which
can only start workflows in gdsuite and cannot push code:

1. Account settings → Developer settings → Personal access tokens →
   Fine-grained tokens → Generate new token.
2. Name it `gdsuite trigger`, owner `q-rault`, with an expiration.
3. Repository access: **Only select repositories** → `q-rault/gdsuite`.
4. Repository permissions: **Actions: Read and write** (nothing else).
5. Store it as the `GDSUITE_TRIGGER_TOKEN` secret in each tool repository:

   ```sh
   for repo in gdtest gdtidy gdhealth gdtypecheck; do
     gh secret set GDSUITE_TRIGGER_TOKEN -R q-rault/$repo
   done
   ```

   `gh secret set` asks for the value each time; paste the token.

Without the secret, a tool release still succeeds and warns that gdsuite was
not told; run pin-tools by hand then.

The docs site is published from release and prerelease tags; see [docs/README.md](docs/README.md#published-versions).

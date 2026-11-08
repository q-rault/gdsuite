# Releasing gdsuite

A release is a version bump merged into `main`:

1. `tag-release.yml` sees that `Cargo.toml`'s version has no tag yet and
   pushes `v<version>`.
2. `release.yml` runs on that tag: it checks the tag matches `Cargo.toml`,
   builds the four archives, publishes the crate to crates.io (trusted
   publishing), then creates the GitHub release.

Do not push the tag yourself: `tag-release.yml` does it.

## Pinning the tools

`.gdsuite-release.toml` pins each tool's release and its archive checksums.
The CLI tests read the versions from it.

`pin-tools.yml` runs every 30 minutes (and on demand from the Actions tab).
When a tool has a newer release, it runs `scripts/pin_tools.py`, which:

- pins the latest release of each tool, with its checksums and gdtest's
  harness contract;
- bumps gdsuite's version: minor when a tool's major or minor version
  changed, patch otherwise.

It opens a PR from the `auto/pin-tools` branch, or updates the one already
open, so tools released one after another end up in the same PR. Review it,
wait for CI, merge: that is the release.

To pin by hand: `python scripts/pin_tools.py` (set `GITHUB_TOKEN` to avoid the
API rate limit), or `--dry-run` to see what it would change.

## One-time setup: the `PIN_TOOLS_TOKEN` secret

Pushes and tags made with a workflow's default token start no other
workflow, so the pin PR would get no CI and the tag would not start the
release. Both workflows use a token of yours instead:

1. On GitHub, open your **account** settings (not the repository's):
   Settings → Developer settings → Personal access tokens →
   **Fine-grained tokens** → Generate new token.
2. Name it `gdsuite pin-tools`, owner `q-rault`, and an expiration (note the
   date: the workflows fail once it expires).
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

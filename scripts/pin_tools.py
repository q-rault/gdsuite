"""Pin every tool in .gdsuite-release.toml to its latest GitHub release.

    python scripts/pin_tools.py                # pin, checksum and bump gdsuite
    python scripts/pin_tools.py --dry-run      # only say what would change

For each tool whose latest release differs from its pin, this updates the tag
and the tested version (and gdtest's harness contract), refreshes the
checksums with release_checksums.py, updates the example in the config docs,
and bumps gdsuite's version in the same change, so merging it is a release:
a minor bump when a tool's major or minor version changed, a patch bump
otherwise. The test fixtures read the versions from the manifest.

With --github-output FILE (GitHub Actions), it writes `changed`, `title` and
`body` for the pin-tools workflow. Set GITHUB_TOKEN to avoid the anonymous
API rate limit.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / ".gdsuite-release.toml"
CARGO_TOML = ROOT / "Cargo.toml"
CARGO_LOCK = ROOT / "Cargo.lock"
CONFIG_DOCS = ROOT / "docs" / "content" / "docs" / "configure.mdx"
ACQUIRE = re.compile(
    r'^\[tools\.(?P<tool>[a-z0-9_-]+)\]\s*\n'
    r'acquire = \{ kind = "github_release_repo", owner = "(?P<owner>[^"]+)", '
    r'repo = "(?P<repo>[^"]+)", tag = "(?P<tag>[^"]+)" \}\s*\n'
    r'tested = "==(?P<tested>[^"]+)"',
    re.MULTILINE,
)


def latest_tag(owner: str, repo: str) -> str:
    request = urllib.request.Request(
        f"https://api.github.com/repos/{owner}/{repo}/releases/latest",
        headers={"Accept": "application/vnd.github+json", "User-Agent": "gdsuite-pin-tools"},
    )
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request) as response:
        return json.load(response)["tag_name"]


def version_tuple(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"\d+", version)[:3])


def bump(version: str, minor: bool) -> str:
    major, minor_part, patch = version_tuple(version)
    if minor:
        return f"{major}.{minor_part + 1}.0"
    return f"{major}.{minor_part}.{patch + 1}"


def replace_once(text: str, old: str, new: str, where: Path) -> str:
    if old not in text:
        raise SystemExit(f"ERROR: `{old}` not found in {where.name}")
    return text.replace(old, new, 1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()

    manifest = MANIFEST.read_text(encoding="utf-8")
    changes = []
    for match in ACQUIRE.finditer(manifest):
        tool, owner, repo, tag = match.group("tool", "owner", "repo", "tag")
        latest = latest_tag(owner, repo)
        if latest != tag:
            changes.append((tool, owner, repo, tag, latest))

    if not changes:
        print("every tool is pinned to its latest release")
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as out:
                out.write("changed=false\n")
        return 0

    suite_version = re.search(r'^version = "([^"]+)"', CARGO_TOML.read_text(encoding="utf-8"), re.MULTILINE).group(1)
    minor = any(version_tuple(old)[:2] != version_tuple(new)[:2] for _, _, _, old, new in changes)
    new_version = bump(suite_version, minor)
    pins = ", ".join(f"{tool} {new.lstrip('v')}" for tool, _, _, _, new in changes)
    title = f"release: v{new_version} (pin {pins})"
    for tool, _, _, old, new in changes:
        print(f"{tool}: {old} -> {new}")
    print(f"gdsuite: {suite_version} -> {new_version}")
    if args.dry_run:
        return 0

    for tool, owner, repo, old, new in changes:
        old_version, new_version_tool = old.lstrip("v"), new.lstrip("v")
        manifest = replace_once(
            manifest,
            f'repo = "{repo}", tag = "{old}" }}\ntested = "=={old_version}"',
            f'repo = "{repo}", tag = "{new}" }}\ntested = "=={new_version_tool}"',
            MANIFEST,
        )
        if tool == "gdtest":
            manifest = re.sub(
                r'^required_harness = "==[^"]+"',
                f'required_harness = "=={new_version_tool}"',
                manifest,
                flags=re.MULTILINE,
            )
    manifest = re.sub(
        r'(\[suite\]\nversion = )"[^"]+"', rf'\g<1>"{new_version}"', manifest, count=1
    )
    MANIFEST.write_text(manifest, encoding="utf-8", newline="\n")
    subprocess.run([sys.executable, str(ROOT / "scripts" / "release_checksums.py")], check=True)

    docs = CONFIG_DOCS.read_text(encoding="utf-8")
    for tool, owner, repo, old, new in changes:
        docs = docs.replace(f"{owner}/{repo}@{old}", f"{owner}/{repo}@{new}")
    CONFIG_DOCS.write_text(docs, encoding="utf-8", newline="\n")

    cargo_toml = CARGO_TOML.read_text(encoding="utf-8")
    cargo_toml = replace_once(
        cargo_toml, f'version = "{suite_version}"', f'version = "{new_version}"', CARGO_TOML
    )
    CARGO_TOML.write_text(cargo_toml, encoding="utf-8", newline="\n")
    cargo_lock = CARGO_LOCK.read_text(encoding="utf-8")
    cargo_lock = replace_once(
        cargo_lock,
        f'name = "gdsuite"\nversion = "{suite_version}"',
        f'name = "gdsuite"\nversion = "{new_version}"',
        CARGO_LOCK,
    )
    CARGO_LOCK.write_text(cargo_lock, encoding="utf-8", newline="\n")

    if args.github_output:
        lines = [
            f"Pins the latest tool releases and bumps gdsuite to {new_version}. "
            "Merging this PR tags `v" + new_version + "`, which publishes the release.",
            "",
            "| Tool | Pinned | Latest |",
            "|---|---|---|",
        ]
        lines += [
            f"| {tool} | {old} | [{new}](https://github.com/{owner}/{repo}/releases/tag/{new}) |"
            for tool, owner, repo, old, new in changes
        ]
        lines += [
            "",
            "Checksums come from each release's assets. This PR is updated in place when another "
            "tool is released before it is merged.",
        ]
        body = "\n".join(lines)
        with args.github_output.open("a", encoding="utf-8") as out:
            out.write("changed=true\n")
            out.write(f"title={title}\n")
            out.write(f"version={new_version}\n")
            out.write(f"body<<PIN_TOOLS_EOF\n{body}\nPIN_TOOLS_EOF\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

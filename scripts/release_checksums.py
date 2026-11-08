"""Record the SHA-256 of every pinned GitHub release archive in .gdsuite-release.toml.

Run after changing a `github_release_repo` tag in the manifest:

    python scripts/release_checksums.py

The digests come from the GitHub release API (`assets[].digest`). Set GITHUB_TOKEN
to avoid the anonymous rate limit.
"""

import json
import os
import re
import sys
import urllib.request
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent.parent / ".gdsuite-release.toml"
TOOL_HEADER = re.compile(r"^\[tools\.([a-z0-9_-]+)\]\s*$")
REPO_ACQUIRE = re.compile(
    r'acquire\s*=\s*\{\s*kind\s*=\s*"github_release_repo"\s*,\s*owner\s*=\s*"([^"]+)"\s*,'
    r'\s*repo\s*=\s*"([^"]+)"\s*,\s*tag\s*=\s*"([^"]+)"\s*\}'
)


def release_digests(owner: str, repo: str, tag: str, tool: str) -> dict[str, str]:
    url = f"https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/vnd.github+json", "User-Agent": "gdsuite-release-checksums"},
    )
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request) as response:
        release = json.load(response)

    prefix = f"{tool}-{tag}-"
    digests = {}
    for asset in release["assets"]:
        name = asset["name"]
        digest = asset.get("digest") or ""
        if not name.startswith(prefix) or not digest.startswith("sha256:"):
            continue
        target = re.sub(r"\.(zip|tar\.gz)$", "", name[len(prefix):])
        digests[target] = digest.removeprefix("sha256:")
    if not digests:
        raise SystemExit(f"ERROR: {owner}/{repo}@{tag} has no {tool} archives with a sha256 digest")
    return digests


def main() -> None:
    lines = MANIFEST.read_text(encoding="utf-8").splitlines()
    output: list[str] = []
    current_tool = None
    skipping = False
    for line in lines:
        if re.match(r"^\[tools\.[a-z0-9_-]+\.sha256\]\s*$", line):
            skipping = True
            continue
        if line.startswith("["):
            skipping = False
        if skipping:
            continue
        header = TOOL_HEADER.match(line)
        if header:
            current_tool = header.group(1)
        output.append(line)
        acquire = REPO_ACQUIRE.search(line)
        if acquire and current_tool:
            owner, repo, tag = acquire.groups()
            digests = release_digests(owner, repo, tag, current_tool)
            pending = [f"[tools.{current_tool}.sha256]"]
            pending += [f'"{target}" = "{digests[target]}"' for target in sorted(digests)]
            # Emitted after the tool's own keys, at the next blank line.
            output.append(("__PENDING__", pending))  # type: ignore[arg-type]

    final: list[str] = []
    held = None
    for item in output:
        if isinstance(item, tuple):
            held = item[1]
            continue
        if held is not None and (item.strip() == "" or item.startswith("[")):
            final.extend(["", *held])
            held = None
        final.append(item)
    if held is not None:
        final.extend(["", *held])

    text = "\n".join(final)
    text = re.sub(r"\n{3,}", "\n\n", text).rstrip("\n") + "\n"
    MANIFEST.write_text(text, encoding="utf-8", newline="\n")
    print(f"updated {MANIFEST.name}")


if __name__ == "__main__":
    try:
        main()
    except urllib.error.HTTPError as error:
        sys.exit(f"ERROR: GitHub API request failed: {error}")

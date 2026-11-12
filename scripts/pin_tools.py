"""Build the latest release of every tool into gdsuite.

    python scripts/pin_tools.py                # pin
    python scripts/pin_tools.py --dry-run      # only say what would change

The tools are built into gdsuite from crates.io, at exact versions. For each
tool whose latest crates.io release differs from its pin, this updates the
dependency in Cargo.toml and Cargo.lock (`cargo update --precise`) and the
tested version in .gdsuite-release.toml (and gdtest's harness contract). The
test fixtures read the versions from the manifest. Releasing gdsuite is a
separate step, Cut release: the PR says which bump it suggests, minor when a
tool's major or minor version changed, patch otherwise.

With --github-output FILE (GitHub Actions), it writes `changed`, `title` and
`body` for the pin-tools workflow.
"""

import argparse
import json
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / ".gdsuite-release.toml"
CARGO_TOML = ROOT / "Cargo.toml"
TOOL = re.compile(
    r'^\[tools\.(?P<tool>[a-z0-9_-]+)\]\s*\n'
    r'acquire = \{ kind = "bundled" \}\s*\n'
    r'tested = "==(?P<tested>[^"]+)"',
    re.MULTILINE,
)


def latest_version(crate: str) -> str:
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{crate}",
        headers={"User-Agent": "gdsuite-pin-tools (https://github.com/q-rault/gdsuite)"},
    )
    with urllib.request.urlopen(request) as response:
        return json.load(response)["crate"]["max_stable_version"]


def version_tuple(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"\d+", version)[:3])


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
    for match in TOOL.finditer(manifest):
        tool, tested = match.group("tool", "tested")
        latest = latest_version(tool)
        if latest != tested:
            changes.append((tool, tested, latest))

    if not changes:
        print("every tool is pinned to its latest release")
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as out:
                out.write("changed=false\n")
        return 0

    cargo_toml = CARGO_TOML.read_text(encoding="utf-8")
    minor = any(version_tuple(old)[:2] != version_tuple(new)[:2] for _, old, new in changes)
    suggested = "minor" if minor else "patch"
    pins = ", ".join(f"{tool} {new}" for tool, _, new in changes)
    title = f"Build in {pins}"
    for tool, old, new in changes:
        print(f"{tool}: {old} -> {new}")
    print(f"then Cut release with bump: {suggested}")
    if args.dry_run:
        return 0

    for tool, old, new in changes:
        manifest = replace_once(
            manifest,
            f'[tools.{tool}]\nacquire = {{ kind = "bundled" }}\ntested = "=={old}"',
            f'[tools.{tool}]\nacquire = {{ kind = "bundled" }}\ntested = "=={new}"',
            MANIFEST,
        )
        if tool == "gdtest":
            manifest = re.sub(
                r'^required_harness = "==[^"]+"',
                f'required_harness = "=={new}"',
                manifest,
                flags=re.MULTILINE,
            )
        cargo_toml = replace_once(cargo_toml, f'{tool} = "={old}"', f'{tool} = "={new}"', CARGO_TOML)
    MANIFEST.write_text(manifest, encoding="utf-8", newline="\n")
    CARGO_TOML.write_text(cargo_toml, encoding="utf-8", newline="\n")
    # Moves the tools, and what they need, in Cargo.lock; fails when the new
    # versions can't build together.
    for tool, _, new in changes:
        subprocess.run(["cargo", "update", "-p", tool, "--precise", new], cwd=ROOT, check=True)

    if args.github_output:
        lines = [
            "Builds the latest tool releases into gdsuite. Merging it releases nothing: "
            f"then run **Cut release** with bump **{suggested}** "
            "(minor when a tool's major or minor version changed, patch otherwise).",
            "",
            "| Tool | Built in | Latest |",
            "|---|---|---|",
        ]
        lines += [
            f"| {tool} | {old} | [{new}](https://crates.io/crates/{tool}/{new}) |"
            for tool, old, new in changes
        ]
        lines += [
            "",
            "This PR is updated in place when another tool is released before it is merged.",
        ]
        body = "\n".join(lines)
        with args.github_output.open("a", encoding="utf-8") as out:
            out.write("changed=true\n")
            out.write(f"title={title}\n")
            out.write(f"body<<PIN_TOOLS_EOF\n{body}\nPIN_TOOLS_EOF\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

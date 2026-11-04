#!/usr/bin/env python3
"""Check that every package version matches the release tag."""
import json
import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def workspace_version(path):
    with path.open("rb") as file:
        return tomllib.load(file)["workspace"]["package"]["version"]


def json_version(path):
    return json.loads(path.read_text())["version"]


def props_version(path):
    node = ET.parse(path).getroot().find(".//PackageVersion")
    if node is None or not node.text:
        raise SystemExit(f"{path}: <PackageVersion> not found")
    return node.text.strip()


def main():
    tag = sys.argv[1].strip() if len(sys.argv) > 1 else ""
    versions = {
        "Cargo.toml": workspace_version(ROOT / "Cargo.toml"),
        "beatd-js/package.json": json_version(ROOT / "beatd-js/package.json"),
        "beatd-dotnet/Directory.Build.props": props_version(
            ROOT / "beatd-dotnet/Directory.Build.props"
        ),
        "Unity package.json": json_version(
            ROOT
            / "beatd-dotnet/Beatd.Unity/Packages/com.nuskey8.beatd.unity/package.json"
        ),
    }
    expected = versions["Cargo.toml"]
    errors = [
        f"{name}: {version} != {expected}"
        for name, version in versions.items()
        if version != expected
    ]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", expected):
        errors.append(f"version {expected!r} is not semver")
    if tag:
        if not tag.startswith("v"):
            errors.append(f"tag {tag!r} must start with 'v'")
        elif tag[1:] != expected:
            errors.append(f"tag {tag!r} != version {expected}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"version {expected} OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

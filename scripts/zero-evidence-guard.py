#!/usr/bin/env python3
"""Scan the workspace for blocked legacy-provider markers."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


BLOCKED_MARKER_HEX = [
    "6769746c6162",
    "676974206c6162",
    "6769742d6c6162",
    "2e6769746c61622d63692e796d6c",
    "676c6162",
]

SKIP_DIRS = {
    ".git",
    "target",
}


def blocked_markers() -> list[bytes]:
    return [bytes.fromhex(value) for value in BLOCKED_MARKER_HEX]


def iter_files(root: Path):
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(root)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        yield rel, path


def line_number(data: bytes, index: int) -> int:
    return data.count(b"\n", 0, index) + 1


def scan(root: Path) -> list[str]:
    findings: list[str] = []
    markers = blocked_markers()
    for rel, path in iter_files(root):
        data = path.read_bytes().lower()
        for marker in markers:
            index = data.find(marker)
            if index >= 0:
                findings.append(f"{rel}:{line_number(data, index)}: blocked marker")
                break
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description="scan for blocked legacy-provider markers")
    parser.add_argument("root", nargs="?", default=".", help="workspace root to scan")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    findings = scan(root)
    if findings:
        for finding in findings:
            print(finding, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

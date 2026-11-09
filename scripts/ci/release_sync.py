#!/usr/bin/env python3
"""Bring detached lockfiles' path packages to the root workspace's member versions.

release-plz bumps root members and the root `Cargo.lock` only, so every detached workspace that
reaches a bumped member by path then fails `--locked`. A path package's lock entry holds only
its name, version and dependency names (no source or checksum), so rewriting its version is
what `cargo update` would do, without the registry the release job does not have.

Usage: release_sync.py ROOT   (prints each lockfile it rewrote)
"""

import json
from pathlib import Path
import re
import subprocess
import sys

from plan import DETACHED

ENTRY = re.compile(r'^name = "([^"]+)"\nversion = "([^"]+)"$', re.M)


def member_versions(root):
    """The root workspace's members by name; `--no-deps` needs no registry or lockfile."""
    result = subprocess.run(["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1",
                             "--manifest-path", str(root / "Cargo.toml")],
                            check=True, stdout=subprocess.PIPE)
    return {package["name"]: package["version"] for package in json.loads(result.stdout)["packages"]}


def sync_text(text, versions):
    """`text` with each path (sourceless) package of a root member at the member's version.

    Lock v4 names a dependency `"name"` while one entry has that name, so with one entry per
    root member no reference changes; any other shape fails closed for a human to resolve."""
    if "\r" in text or not re.search(r'^version = 4$', text, re.M):
        raise ValueError("expected an LF, lock-format 4 Cargo.lock")
    blocks = text.split("\n\n")
    entries = [(index, ENTRY.search(block)) for index, block in enumerate(blocks)
               if block.startswith("[[package]]")]
    names = [entry.group(1) for _, entry in entries if entry]
    for name in versions.keys() & {name for name in names if names.count(name) > 1}:
        raise ValueError(f"{name} is locked more than once")
    for index, entry in entries:
        if not entry or "\nsource = " in blocks[index]:
            continue
        name, old = entry.groups()
        new = versions.get(name)
        if new is not None and new != old:
            blocks[index] = blocks[index].replace(entry.group(0), f'name = "{name}"\nversion = "{new}"', 1)
    return "\n\n".join(blocks)


def sync(root):
    """Rewrite every registered detached lockfile under `root`; return those that changed."""
    root = Path(root).resolve()
    versions = member_versions(root)
    changed = []
    for directory in DETACHED:
        lockfile = root / directory / "Cargo.lock"
        if not lockfile.is_file():
            continue
        text = lockfile.read_text()
        synced = sync_text(text, versions)
        if synced != text:
            lockfile.write_text(synced)
            changed.append(lockfile.relative_to(root).as_posix())
    return changed


if __name__ == "__main__":
    print("\n".join(sync(sys.argv[1])))

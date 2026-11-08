#!/usr/bin/env python3
"""Reduce Cargo target directories to third-party dependency artifacts before they are cached.

CircleCI bills a cache by size times retention. Owned (path) packages rebuild whenever they
change, so their libraries, test and example executables, fingerprints and build-script output
only inflate the cache. A target whose workspace cannot be read is dropped whole: an uncached
target only costs a cold build.

Usage: prune.py TARGET_DIR...   (run from the root workspace; each TARGET_DIR's parent holds its
workspace's Cargo.toml)
"""

import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

# Cargo's per-unit artifact names: `<crate>-<16 hex>` with an optional `lib` prefix and extension.
UNIT = re.compile(r"(?:lib)?([A-Za-z0-9_-]+?)-[0-9a-f]{16}(?:\..*)?$")


def normal(name):
    return name.replace("-", "_")


def owned_names(*workspaces):
    """The workspaces' member packages as (package and library names, every target name). Units with
    a `lib` prefix, an extension, or a fingerprint/build directory are named after the former; bare
    executables after the latter. Test targets may share a third-party crate's name (crypto's
    `chacha20` oracle tests), so library artifacts are never matched against them."""
    # `--no-deps` needs no registry: a job holds only the crates it built, and dependency metadata
    # would need them all. A detached workspace's owned path dependencies are root members.
    owned = []
    for workspace in workspaces:
        result = subprocess.run(["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1",
                                 "--manifest-path", str(workspace / "Cargo.toml")],
                                check=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        owned += json.loads(result.stdout)["packages"]
    libraries = {normal(name) for package in owned for name in
                 (package["name"], *(target["name"] for target in package["targets"]
                                     if {"lib", "rlib", "proc-macro", "dylib", "cdylib", "staticlib"}
                                     & set(target["kind"])))}
    return libraries, libraries | {normal(target["name"]) for package in owned for target in package["targets"]}


def owned(unit, names):
    """Whether a profile entry belongs to an owned package (`names` from `owned_names`)."""
    libraries, executables = names
    match = UNIT.match(unit.name)
    if match:
        bare = unit.is_file() and not unit.name.startswith("lib") and "." not in unit.name
        return normal(match.group(1)) in (executables if bare else libraries)
    return False


def prune(target, names):
    """Remove owned units from every profile directory (host and per-triple) under `target`."""
    # A profile directory holds both `deps` and `.fingerprint`; a build script's OUT_DIR does not.
    for profile in {path.parent for path in target.rglob(".fingerprint") if (path.parent / "deps").is_dir()}:
        for directory in ("examples", "incremental"):
            shutil.rmtree(profile / directory, ignore_errors=True)
        for directory in ("deps", ".fingerprint", "build"):
            for unit in (profile / directory).glob("*"):
                if owned(unit, names):
                    shutil.rmtree(unit) if unit.is_dir() else unit.unlink()
        # Top-level copies of owned binaries and libraries, and their dep-info.
        for unit in profile.iterdir():
            stem = normal(re.sub(r"\..*$", "", unit.name))
            library = unit.name.startswith("lib") and normal(stem[3:]) in names[0]
            if unit.is_file() and (library or stem in (names[0] if "." in unit.name else names[1])):
                unit.unlink()
    for directory in ("doc", "nextest", "tmp", "tests"):
        shutil.rmtree(target / directory, ignore_errors=True)


def main(targets):
    for target in map(Path, targets):
        if not target.is_dir():
            continue
        # The checks already passed: no pruning failure may fail the job, only cost a cold build.
        try:
            names = owned_names(Path.cwd(), target.parent)
            prune(target, names)
        except Exception as error:  # noqa: BLE001 - see above
            print(f"{target}: not pruned ({error!r}); leaving it out of the cache")
            shutil.rmtree(target, ignore_errors=True)
            continue
        size = sum(path.stat().st_size for path in target.rglob("*") if path.is_file())
        print(f"{target}: {size / 2**20:.0f} MiB after pruning {len(names[1])} owned names")


if __name__ == "__main__":
    main(sys.argv[1:])

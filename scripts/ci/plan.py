#!/usr/bin/env python3
"""Conservative package-impact selection; no builds, network, or third-party modules.

Cargo supplies declared dependencies (including optional/dev/build/target edges).
Git supplies complete trees and NUL-delimited paths, never a truncated forge diff.
Unknown ownership expands coverage; an invalid graph is an error, not a docs plan.
"""

import argparse
from collections import defaultdict, deque
from dataclasses import dataclass
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
import tomllib

SCHEMA = 1
BNB = "bitsandbytes/bnb"
MACROS = "bitsandbytes/bnb-macros"
SOCKS = "protocols/session/socks"
NOSTD = f"{BNB}/nostd-check"
PKI = {"pki/asn1", "pki/x509", "pki/validation"}
FUZZ = {
    "bitsandbytes/fuzz": ["-max_total_time=60", "-runs=2000000"],
    "crypto/fuzz": ["-max_total_time=45", "-runs=500000"],
    "pki/fuzz": ["-max_total_time=45", "-runs=500000"],
    f"{SOCKS}/fuzz": ["-runs=2000000", "-max_len=2048"],
}
DETACHED = (*FUZZ, NOSTD, "usdr", "rust-dsdcc", "tools/rust-skills")
GLOBAL_FILES = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rust-toolchain",
    "rustfmt.toml", "deny.toml", "release-plz.toml", "AGENTS.md",
    ".github/workflows/ci.yml", ".github/workflows/release-plz.yml",
    "scripts/ci-act.sh",
}
GLOBAL_PREFIXES = (".cargo/", "scripts/ci/")
PROSE_NAMES = {"README.md", "CHANGELOG.md", "DESIGN.md", "ROADMAP.md",
               "LICENSE-MIT", "LICENSE-APACHE"}
PROFILES = {
    "workspace", "bnb", "socks", "network", "facades", "msrv", "no-std",
    "public-api", "semver", "deny", "blessed", "usdr", "rust-dsdcc", "rust-skills",
    "package",
}


@dataclass
class Package:
    name: str
    dependencies: set[str]
    workspace: bool = True
    bins: tuple[str, ...] = ()


def command(root, *args):
    return subprocess.run(args, cwd=root, check=True, stdout=subprocess.PIPE).stdout


def metadata_graph(root):
    """Use unfiltered declarations: feature/platform selection must not hide consumers."""
    root = Path(root).resolve()
    graph = {}
    for directory in (".", *DETACHED):
        manifest = root / directory / "Cargo.toml"
        if not manifest.exists():
            if directory == ".":
                raise ValueError("workspace Cargo.toml is missing")
            continue  # Historical revisions can predate a registered detached workspace.
        data = json.loads(command(root, "cargo", "metadata", "--manifest-path", str(manifest),
                                  "--locked", "--offline", "--no-deps", "--format-version", "1"))
        if directory not in {".", "tools/rust-skills"}:
            package_roots = {Path(p["manifest_path"]).parent.relative_to(root).as_posix()
                             for p in data["packages"]}
            if package_roots != {directory}:
                raise ValueError(f"register validation for additional detached members: {directory}")
        for package in data["packages"]:
            path = Path(package["manifest_path"]).parent.relative_to(root).as_posix()
            dependencies = {
                Path(dep["path"]).resolve().relative_to(root).as_posix()
                for dep in package["dependencies"] if "path" in dep
            }
            graph[path] = Package(package["name"], dependencies, directory == ".",
                                  tuple(sorted(t["name"] for t in package["targets"]
                                               if "bin" in t["kind"])))
    missing = {dep for package in graph.values() for dep in package.dependencies} - graph.keys()
    if missing:
        raise ValueError(f"unregistered owned dependencies: {sorted(missing)}")
    if not graph:
        raise ValueError("Cargo returned an empty graph")
    return graph


def graph_at(root, revision):
    """Historical metadata must see historical manifests AND auto-discovered targets."""
    with tempfile.TemporaryDirectory(prefix="rsl-ci-graph-") as directory:
        with tempfile.TemporaryFile() as archive:
            subprocess.run(["git", "archive", revision], cwd=root, stdout=archive, check=True)
            archive.seek(0)
            with tarfile.open(fileobj=archive) as tree:
                tree.extractall(directory, filter="data")
        return metadata_graph(directory)


def changed_paths(root, base, head):
    # --no-renames represents both sides as paths; ownership needs both, not a similarity score.
    raw = command(root, "git", "diff", "--no-renames", "--name-only", "-z", base, head, "--")
    if raw and not raw.endswith(b"\0"):
        raise ValueError("unterminated Git path list")
    return sorted(set(os.fsdecode(path) for path in raw.split(b"\0") if path))


def is_prose(path):
    parts = PurePosixPath(path).parts
    # These Markdown files are executable/generated fixtures, not prose exemptions.
    if any(part in {"tests", "test", "fixtures", "fixture", "fuzz"} for part in parts):
        return False
    if path.startswith("tools/rust-skills/"):
        return False
    return (PurePosixPath(path).name in PROSE_NAMES
            or (path.endswith(".md") and (path.startswith("docs/")
                or path.startswith("bitsandbytes/docs/")
                or path in {".github/CONTRIBUTING.md", ".github/SECURITY.md"}))
            or (path.startswith("bitsandbytes/docs/assets/")
                and PurePosixPath(path).suffix in {".png", ".svg"}))


def owners(path, graph):
    if path.startswith("tools/rust-skills/"):
        return {root for root in graph if root.startswith("tools/rust-skills/")}
    if path.startswith("pki/tests/"):
        return (PKI | {"pki/fuzz"}) & graph.keys()
    if path == ".github/ci/bnb.toml":
        return {BNB, MACROS} & graph.keys()
    if path in {"tools/gen-blessed.py", "tools/check-drift.py", ".github/actions/blessed-drift/action.yml"}:
        return {"rsl-deps"} & graph.keys()
    matches = [root for root in graph if path.startswith(root + "/")]
    if matches:
        return {max(matches, key=len)}
    if path.startswith("bitsandbytes/"):
        return {BNB, MACROS} & graph.keys()
    return set()


def affected_packages(seeds, old, new):
    reverse = defaultdict(set)
    for graph in (old, new):
        for root, package in graph.items():
            for dependency in package.dependencies:
                reverse[dependency].add(root)
    reasons = {root: set(why) for root, why in seeds.items()}
    pending = deque(sorted(seeds))
    while pending:
        root = pending.popleft()
        for consumer in sorted(reverse[root]):
            if consumer not in reasons:
                reasons[consumer] = set()
                pending.append(consumer)
            reasons[consumer].add(f"depends on {root}")
    return reasons


def select(paths, old, new, *, full=False, release=False, full_reason="explicit full run"):
    seeds = defaultdict(set)
    global_reasons = {full_reason} if full else set()
    docs = []
    for path in sorted(set(paths)):
        parent = PurePosixPath(path).parent.as_posix()
        if (PurePosixPath(path).name == "Cargo.toml" and parent != "."
                and parent not in old and parent not in new and parent not in DETACHED
                and not path.startswith("tools/rust-skills/evals/")):
            raise ValueError(f"register CI ownership for the Cargo manifest: {path}")
        if path in GLOBAL_FILES or path.startswith(GLOBAL_PREFIXES):
            global_reasons.add(f"shared CI/build policy: {path}")
        elif is_prose(path):
            docs.append(path)
        else:
            roots = owners(path, old) | owners(path, new)
            if not roots:
                global_reasons.add(f"unknown ownership: {path}")
            for root in roots:
                seeds[root].add(f"changed {path}")
                if root not in new:
                    global_reasons.add(f"removed owned package: {root}")
    if release:
        # Scope intentionally follows release-plz.toml's reviewed allowlist, checked below.
        for root in (BNB, MACROS):
            if root not in new:
                raise ValueError(f"release package is missing: {root}")
            seeds[root].add("release qualification")
    full = bool(global_reasons)
    if full:
        seeds = {root: global_reasons.copy() for root in old.keys() | new.keys()}
    reasons = affected_packages(seeds, old, new)
    roots = set(reasons) & new.keys()
    packages = sorted(new[root].name for root in roots if new[root].workspace)
    profiles = set()
    if roots:
        profiles.add("deny")
    if packages:
        profiles.update({"workspace", "msrv"})
    if roots & {BNB, MACROS}:
        profiles.update({"bnb", "public-api", "semver"})
    if SOCKS in roots:
        profiles.add("socks")
    if set(packages) & {"rawsock", "dns", "ip", "icmp", "tcp", "udp", "ethernet", "arp"}:
        profiles.add("network")
    if set(packages) & {"rsl", "rsl-deps", "demos"}:
        profiles.add("facades")
    if roots & (PKI | {BNB, NOSTD}):
        profiles.add("no-std")
    if NOSTD in roots:
        profiles.add("msrv")
    if "rsl-deps" in roots or full:
        profiles.add("blessed")
    profiles.update(roots & {"usdr", "rust-dsdcc"})
    if any(root.startswith("tools/rust-skills/") for root in roots):
        profiles.add("rust-skills")
    if release or full:
        profiles.add("package")
    fuzz = [{"root": root, "target": target, "args": FUZZ[root]}
            for root in sorted(roots & FUZZ.keys()) for target in new[root].bins]
    for root in roots & FUZZ.keys():
        if not new[root].bins:
            raise ValueError(f"fuzz workspace has no targets: {root}")
    return {"schema": SCHEMA, "full": full, "release": release,
            "paths": sorted(set(paths)), "prose": docs, "packages": packages,
            "roots": sorted(roots), "profiles": sorted(profiles), "fuzz": fuzz,
            "reasons": {root: sorted(reasons[root]) for root in sorted(roots)},
            "full_reasons": sorted(global_reasons)}


def release_policy(root, revision=None):
    text = (command(root, "git", "show", revision + ":release-plz.toml").decode()
            if revision else (root / "release-plz.toml").read_text())
    policy = tomllib.loads(text)
    workspace = policy["workspace"]
    if workspace.get("release", True) or workspace.get("publish", True) or workspace.get("release_always", True):
        raise ValueError("release qualification requires opt-in, release-PR-only publishing")
    enabled = {p["name"] for p in policy["package"] if p.get("release", False)}
    if enabled != {"bitsandbytes", "bitsandbytes-macros"}:
        raise ValueError("update CI release qualification when changing the release allowlist")
    return workspace.get("pr_branch_prefix", "release-plz-")


def build_plan(root, *, base=None, head="HEAD", full=False, release=False):
    head = command(root, "git", "rev-parse", "--verify", head + "^{commit}").decode().strip()
    requested_base = base
    base = None
    if requested_base:
        result = subprocess.run(["git", "rev-parse", "--verify", requested_base + "^{commit}"],
                                cwd=root, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        if result.returncode == 0:
            base = result.stdout.decode().strip()
    paths = changed_paths(root, base, head) if base else []
    full_reason = "explicit full run" if full else "comparison base unavailable"
    full = full or base is None
    # Prose selection needs no Cargo or toolchain installation. Policy validation still runs.
    if all(is_prose(path) for path in paths) and not full and not release:
        old = new = {}
    else:
        # Immutable trees prevent unstaged manifests from changing a named commit's plan.
        new = graph_at(root, head)
        global_change = any(path in GLOBAL_FILES or path.startswith(GLOBAL_PREFIXES) for path in paths)
        old = new if full or global_change or base == head else graph_at(root, base)
    release_policy(root, head)
    plan = select(paths, old, new, full=full, release=release, full_reason=full_reason)
    plan.update({"base": base, "head": head, "release_candidate": False})
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="comparison base; absent/unavailable selects full CI")
    parser.add_argument("--head", default="HEAD", help="tested tree (defaults to HEAD)")
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--release", action="store_true", help="force release checks; not publication authority")
    parser.add_argument("--output", type=Path, help="JSON output (otherwise stdout)")
    args = parser.parse_args()
    root = Path(command(Path.cwd(), "git", "rev-parse", "--show-toplevel").decode().strip())
    plan = build_plan(root, base=args.base, head=args.head, full=args.full, release=args.release)
    output = json.dumps(plan, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError, tarfile.TarError) as error:
        print(f"CI planning failed: {error}", file=sys.stderr)
        sys.exit(1)

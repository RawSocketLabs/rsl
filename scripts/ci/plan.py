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

from dependencies import DependencyState, LockGraph, dependency_delta

SCHEMA = 2
# Per-path selection detail, which grows with the diff. No job reads it, and every step gets
# the plan as one environment variable, which Linux caps at 128 KiB (MAX_ARG_STRLEN), so it
# stays in the plan step's log and the coverage report, out of the plan jobs pass on.
DETAIL = frozenset({"full_reasons", "paths", "prose", "reasons"})
BNB = "codecs/bitsandbytes/bnb"
MACROS = "codecs/bitsandbytes/bnb-macros"
SOCKS = "protocols/session/socks"
NOSTD = f"{BNB}/nostd-check"
BLADERF = "bindings/bladerf"
PKI = {"pki/asn1", "pki/x509", "pki/validation"}
FUZZ = {
    "codecs/bitsandbytes/fuzz": ["-max_total_time=60", "-runs=2000000"],
    "crypto/fuzz": ["-max_total_time=45", "-runs=500000"],
    "pki/fuzz": ["-max_total_time=45", "-runs=500000"],
    # Release runs stop at five minutes; the Mio target cannot reach 2M inputs inside the job timeout.
    f"{SOCKS}/fuzz": ["-max_total_time=300", "-runs=2000000", "-max_len=2048"],
}
# The vendored libusdr C subset and its refresh script, which every device oracle compiles.
VENDOR_LIBUSDR = "devices/vendor"
# Compiles vendored libusdr C; the reference for the pure-Rust uSDR port.
USDR_ORACLE = "devices/sdr/usdr/oracle"
USDR = "devices/sdr/usdr"
USDR_PORTABLE = f"{USDR}/nostd-check"
# Shared no_std device crates (the bus seam, per-chip register values and sequences, the
# SDR front-end probe) by root. Like every cross-target check, their bare-metal gate is release-only: an explicit
# release of a driver with a portable fixture selects it; a codec release reaching them does
# not. They are not release-enabled.
HAL = "devices/hal"
LP8758 = "devices/sdr/chips/lp8758"
LMS7002M = "devices/sdr/chips/lms7002m"
LMK05318 = "devices/chips/lmk05318"
FRONTEND = "devices/sdr/frontend"
STREAM = "devices/sdr/stream"
GEARBOX = "devices/sdr/gearbox"
FORMATS = "devices/sdr/formats"
BARE_METAL_DEVICES = {HAL: "hal", LP8758: "lp8758", LMS7002M: "lms7002m",
                      LMK05318: "lmk05318", FRONTEND: "sdr-frontend", STREAM: "sdr-stream",
                      GEARBOX: "sdr-gearbox", FORMATS: "sdr-formats"}
# The LMS7 family driver; its oracle runs libusdr's LMS7 family against xsdr-sim,
# compiling the vendored C.
XSDR = "devices/sdr/xsdr"
XSDR_ORACLE = f"{XSDR}/oracle"
XSDR_PORTABLE = f"{XSDR}/nostd-check"
# The LSDr driver; its oracle runs libusdr's LSDr family against lsdr-sim, compiling the
# vendored C.
LSDR = "devices/sdr/lsdr"
LSDR_ORACLE = f"{LSDR}/oracle"
LSDR_PORTABLE = f"{LSDR}/nostd-check"
# The LimeSDR-mini driver; its oracle runs libusdr's LimeSDR-mini against limesdr-sim,
# compiling the same vendored C.
LIMESDR = "devices/sdr/limesdr"
LIMESDR_ORACLE = f"{LIMESDR}/oracle"
LIMESDR_PORTABLE = f"{LIMESDR}/nostd-check"
# The clean-room RTL-SDR driver; no oracle or portable fixture. Its capture policy is prose; its
# evidence ledger is not (tests/provenance.rs checks it against the code).
RTLSDR = "devices/sdr/rtlsdr"
# The PE_SYNC clock-board driver; its oracle runs libusdr's PE_SYNC against pe-sync-sim,
# compiling the same vendored C.
PE_SYNC = "devices/sync/pe_sync"
PE_SYNC_ORACLE = f"{PE_SYNC}/oracle"
PE_SYNC_PORTABLE = f"{PE_SYNC}/nostd-check"
# Each driver with a detached exact-source no_std fixture, and the fixture.
PORTABLE_FIXTURES = {USDR: USDR_PORTABLE, XSDR: XSDR_PORTABLE, LSDR: LSDR_PORTABLE,
                     LIMESDR: LIMESDR_PORTABLE, PE_SYNC: PE_SYNC_PORTABLE}
# Retain former FFI roots so comparisons against pre-move revisions resolve their dependencies.
DETACHED = (*FUZZ, NOSTD, BLADERF, "bindings/usdr", "bindings/dsdcc", "bindings/rust-dsdcc",
            "usdr", "rust-dsdcc", USDR_ORACLE, USDR_PORTABLE, XSDR_ORACLE, XSDR_PORTABLE,
            LSDR_ORACLE, LSDR_PORTABLE,
            PE_SYNC_ORACLE, PE_SYNC_PORTABLE, LIMESDR_ORACLE, LIMESDR_PORTABLE,
            "tools/rust-skills")
# Former paths of moved detached workspaces, read only from historical (base) trees, so a plan
# across the move still finds the old owner; a head tree may not use them. An entry can go
# once no comparison base predates its move (rsl-usdr/oracle: d22ca2c9; the codec's fuzz and
# no_std workspaces moved under codecs/ with the domain-group layout).
MOVED_DETACHED = ("rsl-usdr/oracle", "bitsandbytes/fuzz", "bitsandbytes/bnb/nostd-check")
GLOBAL_FILES = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rust-toolchain",
    "rustfmt.toml", "deny.toml", "release-plz.toml", "AGENTS.md",
    "scripts/ci-local.sh",
}
GLOBAL_PREFIXES = (".cargo/", ".circleci/", "scripts/ci/")
PROSE_NAMES = {"README.md", "CHANGELOG.md", "DESIGN.md", "ROADMAP.md",
               "LICENSE-MIT", "LICENSE-APACHE"}
PROFILES = {
    "workspace", "bnb", "socks", "network", "facades", "msrv", "no-std",
    "public-api", "semver", "deny", "blessed", "usdr-ffi", "rust-dsdcc", "usdr-oracle", "rust-skills",
    "package", "bladerf", "usdr-portable", "xsdr-portable", "xsdr-oracle", "lsdr-portable", "lsdr-oracle",
    "pe-sync-oracle", "pe-sync-portable", "limesdr-oracle", "limesdr-portable",
}
RELEASE_PROFILES = {"msrv", "bnb", "socks", "no-std", "public-api", "semver", "package"}


@dataclass
class Package:
    name: str
    dependencies: set[str]
    workspace: bool = True
    bins: tuple[str, ...] = ()
    version: str = "0.0.0"
    state: DependencyState | None = None


def command(root, *args, env=None):
    return subprocess.run(args, cwd=root, check=True, stdout=subprocess.PIPE, env=env).stdout


def metadata_graph(root, *, historical=False, toolchain=None):
    """Use unfiltered declarations: feature/platform selection must not hide consumers.

    A historical (base) tree also reads MOVED_DETACHED, skips a registered detached path that
    was still a root-workspace member then, and runs Cargo on `toolchain` (the head's) so the
    base's own pin cannot trigger a toolchain download.
    """
    root = Path(root).resolve()
    if not historical:
        for directory in MOVED_DETACHED:
            if (root / directory / "Cargo.toml").exists():
                raise ValueError(f"register CI ownership for the reused former path: {directory}")
    env = {**os.environ, "RUSTUP_TOOLCHAIN": toolchain} if toolchain else None
    graph = {}
    for directory in (".", *DETACHED, *(MOVED_DETACHED if historical else ())):
        manifest = root / directory / "Cargo.toml"
        if not manifest.exists():
            if directory == ".":
                raise ValueError("workspace Cargo.toml is missing")
            continue  # Historical revisions can predate a registered detached workspace.
        data = json.loads(command(root, "cargo", "metadata", "--manifest-path", str(manifest),
                                  "--locked", "--offline", "--no-deps", "--format-version", "1",
                                  env=env))
        if historical and directory != "." and Path(data["workspace_root"]) == root:
            continue  # Then a root-workspace member, already read with ".".
        if directory not in {".", "tools/rust-skills"}:
            package_roots = {Path(p["manifest_path"]).parent.relative_to(root).as_posix()
                             for p in data["packages"]}
            expected_roots = {directory, f"{directory}/sys"} if directory == BLADERF else {directory}
            if package_roots != expected_roots:
                raise ValueError(f"register validation for additional detached members: {directory}")
        lockfile = (Path(data["workspace_root"]) / "Cargo.lock").relative_to(root).as_posix()
        locked = LockGraph(root / lockfile)
        for package in data["packages"]:
            path = Path(package["manifest_path"]).parent.relative_to(root).as_posix()
            dependencies = {
                Path(dep["path"]).resolve().relative_to(root).as_posix()
                for dep in package["dependencies"] if "path" in dep
            }
            graph[path] = Package(package["name"], dependencies, directory == ".",
                                  tuple(sorted(t["name"] for t in package["targets"]
                                               if "bin" in t["kind"])), package["version"],
                                  locked.snapshot(package, root, lockfile))
    for driver, fixture in PORTABLE_FIXTURES.items():
        if fixture in graph:
            # #[path] imports are production-source dependencies invisible to Cargo metadata.
            # Never depend on the full std driver merely to make this edge visible to Cargo.
            graph[fixture].dependencies.add(driver)
    for oracle in (XSDR_ORACLE, LSDR_ORACLE, PE_SYNC_ORACLE, LIMESDR_ORACLE):
        if oracle in graph and USDR_ORACLE in graph:
            # These oracles compile the vendored libusdr the uSDR oracle compiles, by path;
            # Cargo metadata cannot see a C source dependency. The tree used to sit inside
            # the uSDR oracle; the edge keeps selection as it was when it moved out.
            graph[oracle].dependencies.add(USDR_ORACLE)
    missing = {dep for package in graph.values() for dep in package.dependencies} - graph.keys()
    if missing:
        raise ValueError(f"unregistered owned dependencies: {sorted(missing)}")
    if not graph:
        raise ValueError("Cargo returned an empty graph")
    return graph


def toolchain_of(root):
    """The channel `rust-toolchain.toml` pins at `root`, if any."""
    pin = Path(root) / "rust-toolchain.toml"
    return tomllib.loads(pin.read_text())["toolchain"]["channel"] if pin.exists() else None


def graph_at(root, revision, *, historical=False):
    """Historical metadata must see historical manifests AND auto-discovered targets."""
    with tempfile.TemporaryDirectory(prefix="rsl-ci-graph-") as directory:
        with tempfile.TemporaryFile() as archive:
            subprocess.run(["git", "archive", revision], cwd=root, stdout=archive, check=True)
            archive.seek(0)
            with tarfile.open(fileobj=archive) as tree:
                tree.extractall(directory, filter="data")
        return metadata_graph(directory, historical=historical,
                              toolchain=toolchain_of(root) if historical else None)


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
            or path in {f"{USDR}/PORTING.md", f"{XSDR}/PORTING.md", f"{LSDR}/PORTING.md",
                        f"{LIMESDR}/PORTING.md", f"{PE_SYNC}/PORTING.md",
                        f"{RTLSDR}/CAPTURES.md"}
            or (path.endswith(".md") and (path.startswith("docs/")
                or path.startswith("codecs/bitsandbytes/docs/")
                or path in {".github/CONTRIBUTING.md", ".github/SECURITY.md"}))
            or (path.startswith("codecs/bitsandbytes/docs/assets/")
                and PurePosixPath(path).suffix in {".png", ".svg"}))


def owners(path, graph):
    if path.startswith("tools/rust-skills/"):
        return {root for root in graph if root.startswith("tools/rust-skills/")}
    if path.startswith("pki/tests/"):
        return (PKI | {"pki/fuzz"}) & graph.keys()
    if path == ".github/ci/bnb.toml":
        return {BNB, MACROS} & graph.keys()
    if path.startswith(VENDOR_LIBUSDR + "/"):
        # The vendored C belongs to every oracle that compiles it.
        oracles = {USDR_ORACLE, XSDR_ORACLE, LSDR_ORACLE, PE_SYNC_ORACLE, LIMESDR_ORACLE}
        return oracles & graph.keys()
    if path in {"tools/gen-blessed.py", "tools/check-drift.py", ".github/actions/blessed-drift/action.yml"}:
        return {root for root, package in graph.items() if package.name == "deps"}
    matches = [root for root in graph if path.startswith(root + "/")]
    if matches:
        return {max(matches, key=len)}
    directory = PurePosixPath(path).parent.as_posix()
    if directory.startswith("devices/"):
        # A device directory holding only its sim/oracle crates (a family guide above them)
        # belongs to those crates, not to unknown ownership.
        nested = {root for root in graph if root.startswith(directory + "/")}
        if nested:
            return nested
    if path.startswith("codecs/bitsandbytes/"):
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


def released_portable_drivers(release_members):
    """The explicitly released drivers that have a portable fixture."""
    return {member["root"] for member in release_members} & PORTABLE_FIXTURES.keys()


def bare_metal_devices(release_roots, release_members):
    """Shared no_std device crates whose bare-metal gate an explicit release of a driver with
    a portable fixture (uSDR, XSDR, LSDr, LimeSDR) selects."""
    if not released_portable_drivers(release_members):
        return set()
    return set(release_roots) & BARE_METAL_DEVICES.keys()


def select(paths, old, new, *, full=False, release_members=(), full_reason="explicit full run",
           dependency_changes=None):
    for driver in sorted(released_portable_drivers(release_members)):
        if PORTABLE_FIXTURES[driver] not in new:
            raise ValueError(f"{driver} release qualification requires its registered portable fixture")
    seeds = defaultdict(set)
    global_reasons = {full_reason} if full else set()
    docs = []
    dependency_changes = dependency_changes or {}
    dependency_audit = False
    for path in sorted(set(paths)):
        parent = PurePosixPath(path).parent.as_posix()
        if (PurePosixPath(path).name == "Cargo.toml" and parent != "."
                and parent not in old and parent not in new and parent not in DETACHED
                and not path.startswith("tools/rust-skills/evals/")):
            raise ValueError(f"register CI ownership for the Cargo manifest: {path}")
        if path in dependency_changes:
            dependency_audit = True
            if dependency_changes[path] is None:
                global_reasons.add(f"dependency impact unavailable: {path}")
            else:
                for root in dependency_changes[path]:
                    seeds[root].add(f"dependency delta: {path}")
        elif path in GLOBAL_FILES or path.startswith(GLOBAL_PREFIXES):
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
    # Release breadth is independent of changed-files/full breadth. Forward dependencies
    # are included, but do not seed their unrelated consumers.
    release_seeds = {member["root"]: {"release qualification"} for member in release_members}
    release_reasons = affected_packages(release_seeds, old, new)
    pending = list(release_seeds)
    dependencies = set(pending)
    while pending:
        for dependency in new[pending.pop()].dependencies:
            if dependency not in dependencies:
                dependencies.add(dependency)
                pending.append(dependency)
    release_roots = (set(release_reasons) | dependencies) & new.keys()
    full = bool(global_reasons)
    if full:
        seeds = {root: global_reasons.copy() for root in old.keys() | new.keys()}
    reasons = affected_packages(seeds, old, new)
    for root in release_roots:
        reasons.setdefault(root, set()).add("release qualification")
    roots = set(reasons) & new.keys()
    packages = sorted(new[root].name for root in roots if new[root].workspace)
    profiles = set()
    if roots or dependency_audit:
        profiles.add("deny")
    if packages:
        profiles.add("workspace")
    if release_roots:
        profiles.add("msrv")
    if release_roots & {BNB, MACROS}:
        profiles.update({"bnb", "public-api", "semver"})
    if SOCKS in release_roots:
        profiles.add("socks")
    if set(packages) & {"rawsock", "dns", "ip", "icmp", "tcp", "udp", "ethernet", "arp"}:
        profiles.add("network")
    if set(packages) & {"rsl", "deps", "protocol-examples"}:
        profiles.add("facades")
    if release_roots & (PKI | {BNB, NOSTD}):
        profiles.add("no-std")
    if any(PORTABLE_FIXTURES[driver] in release_roots
           for driver in released_portable_drivers(release_members)):
        profiles.add("no-std")
    if bare_metal_devices(release_roots, release_members):
        profiles.add("no-std")
    if "deps" in packages or full:
        profiles.add("blessed")
    profiles.update(new[root].name for root in roots
                    if root in {"bindings/usdr", "bindings/dsdcc", "bindings/rust-dsdcc", "usdr", "rust-dsdcc"})
    if roots & {BLADERF, f"{BLADERF}/sys"}:
        profiles.add("bladerf")
    if USDR_ORACLE in roots:
        profiles.add("usdr-oracle")
    if USDR_PORTABLE in roots:
        profiles.add("usdr-portable")
    if XSDR_PORTABLE in roots:
        profiles.add("xsdr-portable")
    if XSDR_ORACLE in roots:
        profiles.add("xsdr-oracle")
    if LSDR_ORACLE in roots:
        profiles.add("lsdr-oracle")
    if LSDR_PORTABLE in roots:
        profiles.add("lsdr-portable")
    if PE_SYNC_PORTABLE in roots:
        profiles.add("pe-sync-portable")
    if PE_SYNC_ORACLE in roots:
        profiles.add("pe-sync-oracle")
    if LIMESDR_ORACLE in roots:
        profiles.add("limesdr-oracle")
    if LIMESDR_PORTABLE in roots:
        profiles.add("limesdr-portable")
    if any(root.startswith("tools/rust-skills/") for root in roots):
        profiles.add("rust-skills")
    if release_members:
        profiles.add("package")
    fuzz = [{"root": root, "target": target, "args": FUZZ[root]}
            for root in sorted(release_roots & FUZZ.keys()) for target in new[root].bins]
    for root in release_roots & FUZZ.keys():
        if not new[root].bins:
            raise ValueError(f"fuzz workspace has no targets: {root}")
    return {"schema": SCHEMA, "full": full, "release": bool(release_members),
            "products": sorted({member["product"] for member in release_members}),
            "release_members": list(release_members), "release_roots": sorted(release_roots),
            "release_packages": sorted(new[root].name for root in release_roots if new[root].workspace),
            "paths": sorted(set(paths)), "prose": docs, "packages": packages,
            "roots": sorted(roots), "profiles": sorted(profiles), "fuzz": fuzz,
            "reasons": {root: sorted(reasons[root]) for root in sorted(roots)},
            "full_reasons": sorted(global_reasons)}


def handed_on(plan):
    """The plan as jobs pass it on: everything a job reads, without the per-path detail."""
    return {key: value for key, value in plan.items() if key not in DETAIL}


def release_config(root, revision=None):
    text = (command(root, "git", "show", revision + ":release-plz.toml").decode()
            if revision else (root / "release-plz.toml").read_text())
    policy = tomllib.loads(text)
    workspace = policy["workspace"]
    if workspace.get("release", True) or workspace.get("publish", True) or workspace.get("release_always", True):
        raise ValueError("release qualification requires opt-in, release-PR-only publishing")
    return policy


def release_policy(root, revision=None):
    return release_config(root, revision)["workspace"].get("pr_branch_prefix", "release-plz-")


def released_members(policy, old, new, products=None, *, required=True):
    """Resolve enabled products and bind qualification to exact manifest versions.

    An optional qualification (a trunk push) with no version change selects no product."""
    catalog = defaultdict(set)
    for package in policy["package"]:
        if package.get("release", False):
            catalog[package.get("version_group", package["name"])].add(package["name"])
    by_name = {package.name: root for root, package in new.items()}
    members = []
    selected = set(products) if products is not None else set()
    if selected - catalog.keys():
        raise ValueError(f"unknown or disabled release products: {sorted(selected - catalog.keys())}")
    for product, names in sorted(catalog.items()):
        if names - by_name.keys():
            raise ValueError(f"release product has missing packages: {product}")
        roots = sorted(by_name[name] for name in names)
        changed = [root for root in roots if root in old and new[root].version != old[root].version]
        if products is None and changed:
            selected.add(product)
        if product not in selected:
            continue
        if products is None and len(changed) != len(roots):
            raise ValueError(f"release product requires every member's version change: {product}")
        if len({new[root].version for root in roots}) != 1:
            raise ValueError(f"release product has inconsistent versions: {product}")
        members.extend({"product": product, "root": root, "name": new[root].name,
                        "version": new[root].version} for root in roots)
    if not members and required:
        raise ValueError("release qualification requires a product with version changes or explicit --product")
    return members


def dependency_changes(root, base, head, paths, old, new):
    """Narrow only proven dependency-only edits; everything else retains its existing fallback."""
    result = {}
    if not base:
        return result
    for path in paths:
        name = PurePosixPath(path).name
        if name not in {"Cargo.toml", "Cargo.lock"}:
            continue
        if name == "Cargo.toml" and path != "Cargo.toml":
            delta = dependency_delta(old, new)
            result[path] = delta | owners(path, old) | owners(path, new) if delta is not None else None
            continue
        try:
            before, after = (tomllib.loads(command(root, "git", "show", f"{ref}:{path}").decode())
                             for ref in (base, head))
            if path == "Cargo.toml":
                before.get("workspace", {}).pop("dependencies", None)
                after.get("workspace", {}).pop("dependencies", None)
                if before != after:
                    continue  # Resolver/membership/lints/profiles/patch/package policy is shared.
                result[path] = dependency_delta(old, new)
            elif ({key: value for key, value in before.items() if key != "package"}
                  == {key: value for key, value in after.items() if key != "package"}):
                result[path] = dependency_delta(old, new, lockfile=path)
            else:
                result[path] = None  # Unsupported lock header/format changes are not a narrow proof.
        except (ValueError, OSError, subprocess.CalledProcessError):
            result[path] = None
    return result


def build_plan(root, *, base=None, head="HEAD", full=False, release=False, products=None,
               release_optional=False):
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
        # The base graph is read even when a shared file or --full forces full coverage: a manifest moved
        # in the same change must still resolve to its old owner, not look unregistered.
        old = new if base in (None, head) else graph_at(root, base, historical=True)
    policy = release_config(root, head)
    members = released_members(policy, old, new, products, required=not release_optional) if release else []
    if products and not release:
        raise ValueError("--product requires release qualification")
    deltas = dependency_changes(root, base, head, paths, old, new)
    plan = select(paths, old, new, full=full, release_members=members, full_reason=full_reason,
                  dependency_changes=deltas)
    plan.update({"base": base, "head": head, "release_candidate": False})
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="comparison base; absent/unavailable selects full CI")
    parser.add_argument("--head", default="HEAD", help="tested tree (defaults to HEAD)")
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--release", action="store_true", help="force release checks; not publication authority")
    parser.add_argument("--product", action="append", help="enabled product/version group to qualify (repeatable)")
    parser.add_argument("--output", type=Path, help="JSON output (otherwise stdout)")
    args = parser.parse_args()
    root = Path(command(Path.cwd(), "git", "rev-parse", "--show-toplevel").decode().strip())
    plan = build_plan(root, base=args.base, head=args.head, full=args.full, release=args.release,
                      products=args.product)
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

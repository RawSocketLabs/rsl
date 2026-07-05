#!/usr/bin/env python3
"""Alert when a consumer's DIRECT dependency on a blessed crate drifts to a different major line
than rsl-deps blesses. Governs the crates that can't route through the facade (serde, thiserror,
utoipa, …) — the version the consumer actually controls — without the false positives cargo-deny's
whole-graph checks produce from transitive version diversity.

Usage:
  check-drift.py --blessed <path-or-URL to blessed-versions.toml> --repo <consumer repo root>
                 [--strict]   # exit non-zero on drift (default: warn only)

Scans every Cargo.toml under --repo for direct version requirements (skips `workspace = true`,
git/path-only deps) and compares each governed crate's major line to the blessed one.
"""
import argparse, pathlib, sys, tomllib, urllib.request

DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies", "workspace.dependencies")

def load_blessed(src):
    data = urllib.request.urlopen(src).read() if src.startswith(("http://", "https://")) else pathlib.Path(src).read_bytes()
    return tomllib.loads(data.decode()).get("versions", {})

def line(req):
    # major line of a simple caret req: "1.4"->"1", "0.5.2"->"0.5", "=2.0.0"->"2"
    nums = req.lstrip("^~=><").split(",")[0].strip().split(".")
    try:
        major = int(nums[0]); minor = int(nums[1]) if len(nums) > 1 else 0
    except ValueError:
        return None
    return str(major) if major > 0 else f"0.{minor}"

def direct_reqs(cargo):
    """crate -> req for every explicit-version direct dep in one Cargo.toml."""
    doc = tomllib.loads(cargo.read_text())
    tables = []
    for t in ("dependencies", "dev-dependencies", "build-dependencies"):
        tables.append(doc.get(t, {}))
    tables.append(doc.get("workspace", {}).get("dependencies", {}))
    for tgt in doc.get("target", {}).values():
        for t in ("dependencies", "dev-dependencies", "build-dependencies"):
            tables.append(tgt.get(t, {}))
    out = {}
    for tbl in tables:
        for name, spec in tbl.items():
            if isinstance(spec, str):
                out[name] = spec
            elif isinstance(spec, dict) and "version" in spec:
                out[name] = spec["version"]
    return out

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--blessed", required=True)
    ap.add_argument("--repo", default=".")
    ap.add_argument("--strict", action="store_true")
    a = ap.parse_args()
    blessed = load_blessed(a.blessed)
    drifts = []
    for cargo in pathlib.Path(a.repo).rglob("Cargo.toml"):
        if "target" in cargo.parts:
            continue
        for name, req in direct_reqs(cargo).items():
            if name in blessed and line(req) and line(blessed[name]) and line(req) != line(blessed[name]):
                drifts.append((name, req, blessed[name], cargo))
    if not drifts:
        print("blessed-version drift: none — all governed direct deps match rsl-deps.")
        return 0
    print(f"blessed-version drift: {len(drifts)} governed direct dep(s) off the blessed line:")
    for name, req, bless, cargo in sorted(set((n, r, b, str(c)) for n, r, b, c in drifts)):
        print(f"  {name}: you={req}  blessed={bless}   ({cargo})")
    print("\nEither align these to the blessed major line, or bless the new version in rsl-deps.")
    return 1 if a.strict else 0

if __name__ == "__main__":
    sys.exit(main())

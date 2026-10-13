"""Validate the exact-source USDR fixture without builds, network or third-party modules."""

import argparse
from pathlib import Path
import re
import tomllib


USDR = "devices/sdr/usdr"
FIXTURE = f"{USDR}/nostd-check"
# Every dependency of the portable core, with the features only the std driver may add to it.
# The fixture declares each with the workspace's path and version, without default features,
# and enables exactly the driver's remaining features (none today); never `std`.
DEPENDENCIES = {"bnb": set(), "rsl-lowlevel": {"std"}, "rsl-lp8758": set()}


def check_dependency(root, name, host_only, shared, member, candidate):
    """One fixture dependency must be the production declaration minus host-only features."""
    if not isinstance(member, dict) or member.get("workspace") is not True:
        raise ValueError(f"production {name} must use the shared workspace declaration")
    if not isinstance(shared, dict) or not isinstance(candidate, dict):
        raise ValueError(f"portable {name} must use the production path dependency")
    if set(candidate) - {"package", "path", "version", "default-features", "features"}:
        raise ValueError(f"portable {name} has an unsupported dependency override")
    if (root / shared["path"]).resolve() != (root / FIXTURE / candidate["path"]).resolve():
        raise ValueError(f"portable {name} path differs from the production dependency")
    for field in ("package", "version"):
        if candidate.get(field) != shared.get(field):
            raise ValueError(f"portable {name} {field} differs from production")
    if (shared.get("default-features", True) is not False
            or member.get("default-features", False) is not False
            or candidate.get("default-features", True) is not False):
        raise ValueError(f"portable {name} must not enable default/std features")
    expected = (set(shared.get("features", [])) | set(member.get("features", []))) - host_only
    features = set(candidate.get("features", []))
    if features != expected or expected:
        raise ValueError(f"portable {name} must retain the production core-only feature set")


def check_owned_edges(root, name, shared):
    """An owned dependency's own edges to registered dependencies stay core-only as well, so
    `rsl-lp8758` cannot enable `rsl-lowlevel/std` (or defaults) behind the fixture's back."""
    manifest = tomllib.loads((root / shared["path"] / "Cargo.toml").read_text())
    tables = [manifest.get("dependencies", {}),
              *(target.get("dependencies", {}) for target in manifest.get("target", {}).values())]
    for table in tables:
        for dependency, spec in table.items():
            if dependency not in DEPENDENCIES:
                continue
            if not isinstance(spec, dict) or spec.get("workspace") is not True:
                raise ValueError(f"{name}'s {dependency} must use the shared workspace declaration")
            if spec.get("default-features", False) is not False or spec.get("features"):
                raise ValueError(f"{name} must not enable default/std features of {dependency}")


def validate(root):
    """Keep the detached proof's source, dependency features and versions in sync."""
    root = Path(root).resolve()
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    driver = tomllib.loads((root / USDR / "Cargo.toml").read_text())
    fixture = tomllib.loads((root / FIXTURE / "Cargo.toml").read_text())
    if set(fixture) - {"package", "workspace", "features", "dependencies", "lints"}:
        raise ValueError("portable fixture must not add build/target/dev dependencies or patches")
    if fixture.get("features") != {"default": ["usb"], "usb": []}:
        raise ValueError("portable fixture must enable only the pure usb module feature")
    package = fixture["package"]
    if package.get("publish") is not False or package.get("build") is not False:
        raise ValueError("portable fixture must stay unpublished and have no build script")
    for field in ("edition", "rust-version", "license"):
        if package.get(field) != workspace["package"][field]:
            raise ValueError(f"portable fixture {field} differs from production workspace")
    dependencies = fixture.get("dependencies", {})
    if set(dependencies) != DEPENDENCIES.keys():
        raise ValueError("register any new portable-core dependencies in the consistency check")
    for name, host_only in DEPENDENCIES.items():
        check_dependency(root, name, host_only, workspace["dependencies"].get(name),
                         driver["dependencies"].get(name), dependencies[name])
        check_owned_edges(root, name, workspace["dependencies"][name])
    source = root / FIXTURE / "src/lib.rs"
    text = source.read_text()
    imports = re.findall(r'#\[path\s*=\s*"([^"]+)"\]\s*mod\s+portable\s*;', text)
    if len(imports) != 1 or (source.parent / imports[0]).resolve() != (
            root / USDR / "src/portable/mod.rs").resolve():
        raise ValueError("fixture must import the exact production portable module tree")
    if "#![no_std]" not in text or "#![forbid(unsafe_code)]" not in text:
        raise ValueError("portable fixture must enforce no_std and forbid unsafe code")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    validate(arguments.root)
    print("USDR portable source/dependency contract passed")


if __name__ == "__main__":
    main()

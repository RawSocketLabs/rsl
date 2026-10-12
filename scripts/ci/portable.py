"""Validate the exact-source USDR fixture without builds, network or third-party modules."""

import argparse
from pathlib import Path
import re
import tomllib


USDR = "devices/sdr/usdr"
FIXTURE = f"{USDR}/nostd-check"


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
    if set(dependencies) != {"bnb"}:
        raise ValueError("register any new portable-core dependencies in the consistency check")
    member = driver["dependencies"]["bnb"]
    if not isinstance(member, dict) or member.get("workspace") is not True:
        raise ValueError("production bnb must use the shared workspace declaration")
    shared = workspace["dependencies"]["bnb"]
    candidate = dependencies["bnb"]
    if not isinstance(shared, dict) or not isinstance(candidate, dict):
        raise ValueError("portable bnb must use the production path dependency")
    if set(candidate) - {"package", "path", "version", "default-features", "features"}:
        raise ValueError("portable bnb has an unsupported dependency override")
    if (root / shared["path"]).resolve() != (root / FIXTURE / candidate["path"]).resolve():
        raise ValueError("portable bnb path differs from the production dependency")
    for field in ("package", "version"):
        if candidate.get(field) != shared.get(field):
            raise ValueError(f"portable bnb {field} differs from production")
    if (shared.get("default-features", True) is not False
            or member.get("default-features", False) is not False
            or candidate.get("default-features", True) is not False):
        raise ValueError("portable bnb must not enable default/std features")
    expected = set(shared.get("features", [])) | set(member.get("features", []))
    if set(candidate.get("features", [])) != expected or expected:
        raise ValueError("portable bnb must retain the production core-only feature set")
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

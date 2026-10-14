"""Validate the exact-source portable fixtures without builds, network or third-party modules."""

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import tomllib


USDR = "devices/sdr/usdr"
XSDR = "devices/sdr/xsdr"
LSDR = "devices/sdr/lsdr"
LIMESDR = "devices/sdr/limesdr"
PE_SYNC = "devices/sync/pe_sync"


@dataclass(frozen=True)
class Fixture:
    """A driver's detached no_std fixture: where it is, what it may depend on, its features.

    `dependencies` maps every dependency of the driver's portable core to the features only
    the std driver may add to it. The fixture declares each with the workspace's path and
    version, without default features, and enables exactly the driver's remaining features
    (none today); never `std`.
    """

    driver: str
    dependencies: dict
    features: dict | None

    @property
    def path(self):
        return f"{self.driver}/nostd-check"


FIXTURES = {
    "usdr": Fixture(USDR, {"bnb": set(), "hal": {"std"}, "lp8758": set()},
                    {"default": ["usb"], "usb": []}),
    "xsdr": Fixture(XSDR, {"lms7002m": set(), "hal": {"std"}, "lp8758": set()},
                    None),
    "lsdr": Fixture(LSDR, {"bnb": set(), "hal": {"std"}}, None),
    "limesdr": Fixture(LIMESDR, {"lms7002m": set(), "hal": {"std"}}, None),
    "pe-sync": Fixture(PE_SYNC, {"bnb": set(), "lmk05318": set(), "hal": {"std"}},
                       None),
}
# Every crate a fixture compiles without std, directly or through an owned dependency (the
# chip crates' `bnb`, listed explicitly: the XSDR and LimeSDR fixtures reach it only through
# them): an owned dependency may enable none of their default or std features behind a
# fixture's back.
CORE_ONLY = {"bnb"} | {name for fixture in FIXTURES.values() for name in fixture.dependencies}


def check_dependency(root, fixture, name, host_only, shared, member, candidate):
    """One fixture dependency must be the production declaration minus host-only features."""
    if not isinstance(member, dict) or member.get("workspace") is not True:
        raise ValueError(f"production {name} must use the shared workspace declaration")
    if not isinstance(shared, dict) or not isinstance(candidate, dict):
        raise ValueError(f"portable {name} must use the production path dependency")
    if set(candidate) - {"package", "path", "version", "default-features", "features"}:
        raise ValueError(f"portable {name} has an unsupported dependency override")
    if (root / shared["path"]).resolve() != (root / fixture.path / candidate["path"]).resolve():
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
    """An owned dependency's own edges to core-only crates stay core-only as well, so
    `lp8758` cannot enable `hal/std` (or defaults) behind a fixture's back."""
    manifest = tomllib.loads((root / shared["path"] / "Cargo.toml").read_text())
    tables = [manifest.get("dependencies", {}),
              *(target.get("dependencies", {}) for target in manifest.get("target", {}).values())]
    for table in tables:
        for dependency, spec in table.items():
            if dependency not in CORE_ONLY:
                continue
            if not isinstance(spec, dict) or spec.get("workspace") is not True:
                raise ValueError(f"{name}'s {dependency} must use the shared workspace declaration")
            if spec.get("default-features", False) is not False or spec.get("features"):
                raise ValueError(f"{name} must not enable default/std features of {dependency}")


def validate(root, names=None):
    """Keep each detached proof's source, dependency features and versions in sync."""
    root = Path(root).resolve()
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    for name in names or FIXTURES:
        validate_fixture(root, workspace, FIXTURES[name])


def validate_fixture(root, workspace, fixture):
    """One fixture against its driver and the workspace."""
    driver = tomllib.loads((root / fixture.driver / "Cargo.toml").read_text())
    manifest = tomllib.loads((root / fixture.path / "Cargo.toml").read_text())
    if set(manifest) - {"package", "workspace", "features", "dependencies", "lints"}:
        raise ValueError("portable fixture must not add build/target/dev dependencies or patches")
    if manifest.get("features") != fixture.features:
        raise ValueError("portable fixture must enable only its registered pure features")
    package = manifest["package"]
    if package.get("publish") is not False or package.get("build") is not False:
        raise ValueError("portable fixture must stay unpublished and have no build script")
    for field in ("edition", "rust-version", "license"):
        if package.get(field) != workspace["package"][field]:
            raise ValueError(f"portable fixture {field} differs from production workspace")
    dependencies = manifest.get("dependencies", {})
    if set(dependencies) != fixture.dependencies.keys():
        raise ValueError("register any new portable-core dependencies in the consistency check")
    for name, host_only in fixture.dependencies.items():
        check_dependency(root, fixture, name, host_only, workspace["dependencies"].get(name),
                         driver["dependencies"].get(name), dependencies[name])
        check_owned_edges(root, name, workspace["dependencies"][name])
    source = root / fixture.path / "src/lib.rs"
    text = source.read_text()
    imports = re.findall(r'#\[path\s*=\s*"([^"]+)"\]\s*mod\s+portable\s*;', text)
    if len(imports) != 1 or (source.parent / imports[0]).resolve() != (
            root / fixture.driver / "src/portable/mod.rs").resolve():
        raise ValueError("fixture must import the exact production portable module tree")
    if "#![no_std]" not in text or "#![forbid(unsafe_code)]" not in text:
        raise ValueError("portable fixture must enforce no_std and forbid unsafe code")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixtures", nargs="*", choices=sorted(FIXTURES),
                        help="fixtures to validate (default: all)")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    validate(arguments.root, arguments.fixtures)
    print(f"portable source/dependency contract passed: {', '.join(arguments.fixtures or FIXTURES)}")


if __name__ == "__main__":
    main()

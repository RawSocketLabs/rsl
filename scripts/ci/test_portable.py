"""Exact-source/dependency consistency regressions; no builds, network or packages."""

from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest

from portable import FIXTURE, USDR, validate


class PortableContractTests(unittest.TestCase):
    def fixture(self, directory):
        source = Path(__file__).resolve().parents[2]
        root = Path(directory)
        for relative in ("Cargo.toml", f"{USDR}/Cargo.toml", f"{FIXTURE}/Cargo.toml",
                         f"{FIXTURE}/src/lib.rs", f"{USDR}/src/portable/mod.rs"):
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, destination)
        return root

    def test_actual_contract_matches_the_production_sources(self):
        validate(Path(__file__).resolve().parents[2])

    def test_std_features_version_drift_and_copied_sources_fail_closed(self):
        source = Path(__file__).resolve().parents[2]
        version = tomllib.loads((source / FIXTURE / "Cargo.toml").read_text())["dependencies"]["bnb"]["version"]
        cases = [
            (f"{FIXTURE}/Cargo.toml", "default-features = false", "default-features = true"),
            (f"{FIXTURE}/Cargo.toml", f'version = "{version}"', 'version = "0.0.1"'),
            (f"{FIXTURE}/Cargo.toml", 'usb = []', 'usb = ["bnb/std"]'),
            (f"{FIXTURE}/Cargo.toml", 'build = false', ''),
            (f"{FIXTURE}/src/lib.rs", '../../src/portable/mod.rs', 'copied.rs'),
            (f"{FIXTURE}/src/lib.rs", '#![no_std]', ''),
        ]
        for relative, before, after in cases:
            with self.subTest(change=after), tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory:
                root = self.fixture(directory)
                path = root / relative
                text = path.read_text()
                self.assertIn(before, text)
                path.write_text(text.replace(before, after))
                with self.assertRaises(ValueError):
                    validate(root)

    def test_production_dependency_feature_changes_cannot_silently_change_the_proof(self):
        with tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory:
            root = self.fixture(directory)
            path = root / USDR / "Cargo.toml"
            path.write_text(path.read_text().replace('bnb.workspace = true',
                                                    'bnb = { workspace = true, features = ["std"] }'))
            with self.assertRaisesRegex(ValueError, "core-only feature"):
                validate(root)


if __name__ == "__main__":
    unittest.main()

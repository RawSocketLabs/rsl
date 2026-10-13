"""Exact-source/dependency consistency regressions; no builds, network or packages."""

from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest

from portable import DEPENDENCIES, FIXTURE, USDR, validate

# The registered dependencies' own manifests, which the contract also reads.
OWNED = ("bitsandbytes/bnb/Cargo.toml", "devices/lowlevel/Cargo.toml",
         "devices/sdr/chips/lp8758/Cargo.toml")


class PortableContractTests(unittest.TestCase):
    def fixture(self, directory):
        source = Path(__file__).resolve().parents[2]
        root = Path(directory)
        for relative in ("Cargo.toml", f"{USDR}/Cargo.toml", f"{FIXTURE}/Cargo.toml",
                         f"{FIXTURE}/src/lib.rs", f"{USDR}/src/portable/mod.rs", *OWNED):
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, destination)
        return root

    def test_actual_contract_matches_the_production_sources(self):
        validate(Path(__file__).resolve().parents[2])

    def test_std_features_version_drift_and_copied_sources_fail_closed(self):
        source = Path(__file__).resolve().parents[2]
        cases = [
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

    def test_every_portable_dependency_is_held_to_the_same_contract(self):
        self.assertEqual(DEPENDENCIES["rsl-lowlevel"], {"std"})
        source = Path(__file__).resolve().parents[2]
        declared = tomllib.loads((source / FIXTURE / "Cargo.toml").read_text())["dependencies"]
        for name in DEPENDENCIES:
            declaration = f"{name} = {{ "
            version = declared[name]["version"]
            cases = [
                ("default-features = false }", 'default-features = false, features = ["std"] }'),
                ("default-features = false }", "default-features = true }"),
                (f'version = "{version}"', 'version = "0.0.1"'),
                ('path = "../', 'path = "../../'),
            ]
            for before, after in cases:
                with (self.subTest(dependency=name, change=after),
                      tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                    root = self.fixture(directory)
                    path = root / FIXTURE / "Cargo.toml"
                    lines = path.read_text().splitlines(keepends=True)
                    index = next(i for i, line in enumerate(lines) if line.startswith(declaration))
                    self.assertIn(before, lines[index])
                    lines[index] = lines[index].replace(before, after)
                    path.write_text("".join(lines))
                    with self.assertRaises(ValueError):
                        validate(root)

    def test_a_shared_crate_cannot_enable_std_on_another_behind_the_fixture(self):
        for after in ('rsl-lowlevel = { workspace = true, features = ["std"] }',
                      "rsl-lowlevel = { workspace = true, default-features = true }",
                      'rsl-lowlevel = { path = "../../../lowlevel" }'):
            with (self.subTest(change=after),
                  tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                root = self.fixture(directory)
                path = root / "devices/sdr/chips/lp8758/Cargo.toml"
                text = path.read_text()
                self.assertIn("rsl-lowlevel.workspace = true\n", text)
                path.write_text(text.replace("rsl-lowlevel.workspace = true\n", after + "\n"))
                with self.assertRaisesRegex(ValueError, "rsl-lp8758"):
                    validate(root)

    def test_an_unregistered_portable_dependency_fails_closed(self):
        with tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory:
            root = self.fixture(directory)
            path = root / FIXTURE / "Cargo.toml"
            text = path.read_text()
            self.assertIn("[dependencies]\n", text)
            path.write_text(text.replace(
                "[dependencies]\n", '[dependencies]\nother = { path = "../../../other" }\n'))
            with self.assertRaisesRegex(ValueError, "register any new portable-core"):
                validate(root)

    def test_the_driver_may_enable_only_host_only_features_on_a_shared_crate(self):
        with tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory:
            root = self.fixture(directory)
            path = root / USDR / "Cargo.toml"
            text = path.read_text()
            self.assertIn('rsl-lowlevel = { workspace = true, features = ["std"] }', text)
            path.write_text(text.replace('features = ["std"] }', 'features = ["std", "extra"] }'))
            with self.assertRaisesRegex(ValueError, "core-only feature"):
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

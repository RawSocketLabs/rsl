"""Exact-source/dependency consistency regressions; no builds, network or packages."""

from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest

from portable import FIXTURES, validate

# The registered dependencies' own manifests, which the contract also reads.
OWNED = ("codecs/bitsandbytes/bnb/Cargo.toml", "devices/hal/Cargo.toml",
         "devices/sdr/chips/lp8758/Cargo.toml", "devices/sdr/chips/lms7002m/Cargo.toml",
         "devices/chips/lmk05318/Cargo.toml")
USDR = FIXTURES["usdr"]
XSDR = FIXTURES["xsdr"]
LIMESDR = FIXTURES["limesdr"]
PE_SYNC = FIXTURES["pe-sync"]


class PortableContractTests(unittest.TestCase):
    def fixture(self, directory):
        source = Path(__file__).resolve().parents[2]
        root = Path(directory)
        files = ["Cargo.toml", *OWNED]
        for fixture in FIXTURES.values():
            files += [f"{fixture.driver}/Cargo.toml", f"{fixture.path}/Cargo.toml",
                      f"{fixture.path}/src/lib.rs", f"{fixture.driver}/src/portable/mod.rs"]
        for relative in files:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, destination)
        return root

    def test_actual_contract_matches_the_production_sources(self):
        validate(Path(__file__).resolve().parents[2])

    def test_std_features_version_drift_and_copied_sources_fail_closed(self):
        cases = [
            (USDR, f"{USDR.path}/Cargo.toml", 'usb = []', 'usb = ["bnb/std"]'),
            (USDR, f"{USDR.path}/Cargo.toml", 'build = false', ''),
            (USDR, f"{USDR.path}/src/lib.rs", '../../src/portable/mod.rs', 'copied.rs'),
            (USDR, f"{USDR.path}/src/lib.rs", '#![no_std]', ''),
            (XSDR, f"{XSDR.path}/Cargo.toml", '[workspace]', '[features]\nstd = []\n\n[workspace]'),
            (XSDR, f"{XSDR.path}/Cargo.toml", 'build = false', ''),
            (XSDR, f"{XSDR.path}/src/lib.rs", '../../src/portable/mod.rs',
             '../../../usdr/src/portable/mod.rs'),
            (XSDR, f"{XSDR.path}/src/lib.rs", '#![no_std]', ''),
            (LIMESDR, f"{LIMESDR.path}/Cargo.toml", 'build = false', ''),
            (LIMESDR, f"{LIMESDR.path}/src/lib.rs", '../../src/portable/mod.rs',
             '../../../xsdr/src/portable/mod.rs'),
            (LIMESDR, f"{LIMESDR.path}/src/lib.rs", '#![forbid(unsafe_code)]', ''),
            (PE_SYNC, f"{PE_SYNC.path}/Cargo.toml", 'lmk05318 = { path',
             'lmk05318 = { features = ["std"], path'),
            (PE_SYNC, f"{PE_SYNC.path}/src/lib.rs", '../../src/portable/mod.rs',
             '../../../../sdr/lsdr/src/portable/mod.rs'),
        ]
        for fixture, relative, before, after in cases:
            with (self.subTest(fixture=fixture.driver, change=after),
                  tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                root = self.fixture(directory)
                path = root / relative
                text = path.read_text()
                self.assertIn(before, text)
                path.write_text(text.replace(before, after))
                with self.assertRaises(ValueError):
                    validate(root)

    def test_every_portable_dependency_is_held_to_the_same_contract(self):
        self.assertEqual(USDR.dependencies["hal"], {"std"})
        self.assertEqual(XSDR.dependencies["hal"], {"std"})
        self.assertEqual(LIMESDR.dependencies["hal"], {"std"})
        self.assertEqual(PE_SYNC.dependencies["hal"], {"std"})
        source = Path(__file__).resolve().parents[2]
        for fixture in FIXTURES.values():
            declared = tomllib.loads((source / fixture.path / "Cargo.toml").read_text())
            for name in fixture.dependencies:
                declaration = f"{name} = {{ "
                version = declared["dependencies"][name]["version"]
                cases = [
                    ("default-features = false }",
                     'default-features = false, features = ["std"] }'),
                    ("default-features = false }", "default-features = true }"),
                    (f'version = "{version}"', 'version = "0.0.1"'),
                    ('path = "../', 'path = "../../'),
                ]
                for before, after in cases:
                    with (self.subTest(fixture=fixture.driver, dependency=name, change=after),
                          tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                        root = self.fixture(directory)
                        path = root / fixture.path / "Cargo.toml"
                        lines = path.read_text().splitlines(keepends=True)
                        index = next(i for i, line in enumerate(lines)
                                     if line.startswith(declaration))
                        self.assertIn(before, lines[index])
                        lines[index] = lines[index].replace(before, after)
                        path.write_text("".join(lines))
                        with self.assertRaises(ValueError):
                            validate(root, [name_of(fixture)])

    def test_a_shared_crate_cannot_enable_std_on_another_behind_the_fixture(self):
        for crate, owner in (("devices/sdr/chips/lp8758", "lp8758"),
                             ("devices/sdr/chips/lms7002m", "lms7002m")):
            for after in ('hal = { workspace = true, features = ["std"] }',
                          "hal = { workspace = true, default-features = true }",
                          'hal = { path = "../../../hal" }'):
                with (self.subTest(crate=owner, change=after),
                      tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                    root = self.fixture(directory)
                    path = root / crate / "Cargo.toml"
                    text = path.read_text()
                    self.assertIn("hal.workspace = true\n", text)
                    path.write_text(text.replace("hal.workspace = true\n", after + "\n"))
                    with self.assertRaisesRegex(ValueError, owner):
                        validate(root)

    def test_a_chip_crate_cannot_enable_bnb_std_for_a_fixture_without_bnb(self):
        with tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory:
            root = self.fixture(directory)
            path = root / "devices/sdr/chips/lms7002m/Cargo.toml"
            text = path.read_text()
            self.assertIn("bnb.workspace = true\n", text)
            path.write_text(text.replace("bnb.workspace = true\n",
                                         'bnb = { workspace = true, features = ["std"] }\n'))
            with self.assertRaisesRegex(ValueError, "lms7002m"):
                validate(root, ["xsdr"])

    def test_an_unregistered_portable_dependency_fails_closed(self):
        for fixture in FIXTURES.values():
            with (self.subTest(fixture=fixture.driver),
                  tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                root = self.fixture(directory)
                path = root / fixture.path / "Cargo.toml"
                text = path.read_text()
                self.assertIn("[dependencies]\n", text)
                path.write_text(text.replace(
                    "[dependencies]\n", '[dependencies]\nother = { path = "../../../other" }\n'))
                with self.assertRaisesRegex(ValueError, "register any new portable-core"):
                    validate(root, [name_of(fixture)])

    def test_the_driver_may_enable_only_host_only_features_on_a_shared_crate(self):
        for fixture in FIXTURES.values():
            with (self.subTest(fixture=fixture.driver),
                  tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                root = self.fixture(directory)
                path = root / fixture.driver / "Cargo.toml"
                text = path.read_text()
                self.assertIn('hal = { workspace = true, features = ["std"] }', text)
                path.write_text(text.replace('features = ["std"] }',
                                             'features = ["std", "extra"] }'))
                with self.assertRaisesRegex(ValueError, "core-only feature"):
                    validate(root, [name_of(fixture)])

    def test_production_dependency_feature_changes_cannot_silently_change_the_proof(self):
        for fixture, line in ((USDR, "bnb.workspace = true"),
                              (XSDR, "lms7002m.workspace = true"),
                              (LIMESDR, "lms7002m.workspace = true")):
            crate = line.split(".")[0]
            with (self.subTest(fixture=fixture.driver),
                  tempfile.TemporaryDirectory(prefix="rsl-portable-contract-") as directory):
                root = self.fixture(directory)
                path = root / fixture.driver / "Cargo.toml"
                text = path.read_text()
                self.assertIn(line, text)
                path.write_text(text.replace(
                    line, f'{crate} = {{ workspace = true, features = ["std"] }}'))
                with self.assertRaisesRegex(ValueError, "core-only feature"):
                    validate(root, [name_of(fixture)])


def name_of(fixture):
    """The registry name of `fixture`."""
    return next(name for name, registered in FIXTURES.items() if registered == fixture)


if __name__ == "__main__":
    unittest.main()

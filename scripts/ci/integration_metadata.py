"""Full-tier selector tests against Cargo itself; no compilation or registry access."""

from pathlib import Path
import tempfile
import unittest

from plan import BNB, SOCKS, metadata_graph, select


class CargoMetadataTests(unittest.TestCase):
    def test_optional_build_dev_and_other_platform_dependencies_are_not_filtered(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-cargo-test-") as directory:
            root = Path(directory)
            names = ["consumer", "normal", "optional", "build", "dev", "windows"]
            root.joinpath("Cargo.toml").write_text(
                '[workspace]\nresolver = "2"\nmembers = ['
                + ", ".join(f'"{name}"' for name in names) + "]\n")
            for name in names:
                crate = root / name
                (crate / "src").mkdir(parents=True)
                (crate / "src/lib.rs").touch()
                manifest = f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n'
                if name == "consumer":
                    manifest += '''
[dependencies]
normal = { path = "../normal" }
optional = { path = "../optional", optional = true }
[build-dependencies]
build = { path = "../build" }
[dev-dependencies]
dev = { path = "../dev" }
[target.'cfg(windows)'.dependencies]
windows = { path = "../windows" }
'''
                (crate / "Cargo.toml").write_text(manifest)
            graph = metadata_graph(root)
            self.assertEqual(graph["consumer"].dependencies, set(names) - {"consumer"})
            for dependency in names[1:]:
                with self.subTest(dependency=dependency):
                    plan = select([f"{dependency}/src/lib.rs"], graph, graph)
                    self.assertEqual(set(plan["packages"]), {dependency, "consumer"})

    def test_unregistered_detached_manifest_cannot_be_omitted_by_full_mode(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-unregistered-test-") as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/lib.rs").touch()
            (root / "Cargo.toml").write_text('[package]\nname="owned"\nversion="0.1.0"\n')
            (root / "new-tool").mkdir()
            (root / "new-tool/Cargo.toml").write_text('[workspace]\n')
            graph = metadata_graph(root)
            with self.assertRaisesRegex(ValueError, "register CI ownership"):
                select(["new-tool/Cargo.toml"], graph, graph, full=True)

    def test_actual_repository_matches_the_reviewed_blast_radius(self):
        graph = metadata_graph(Path(__file__).resolve().parents[2])
        cases = [
            (f"{SOCKS}/src/blocking.rs", {"socks"}, set(), {f"{SOCKS}/fuzz"}),
            ("compression/src/lib.rs", {"rsl-compression", "rsl"}, {"socks", "rsl-crypto"}, set()),
            ("crypto/src/lib.rs", {"rsl-crypto", "rsl-crypto-legacy", "rsl-pki", "rsl"},
             {"socks", "bitsandbytes"}, {"crypto/fuzz", "pki/fuzz"}),
            (f"{BNB}/src/lib.rs", {"bitsandbytes", "socks", "rsl-asn1", "rsl-x509", "rsl-pki"},
             {"rsl-crypto", "rsl-crypto-legacy"}, {"bitsandbytes/fuzz", "pki/fuzz", f"{SOCKS}/fuzz"}),
        ]
        for path, required, unrelated, fuzz in cases:
            with self.subTest(path=path):
                plan = select([path], graph, graph)
                self.assertTrue(required <= set(plan["packages"]), plan["packages"])
                self.assertFalse(unrelated & set(plan["packages"]), plan["packages"])
                self.assertEqual({item["root"] for item in plan["fuzz"]}, fuzz)

    def test_nested_detached_member_requires_explicit_validation_registration(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-detached-test-") as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/lib.rs").touch()
            (root / "Cargo.toml").write_text('[package]\nname="owned"\nversion="0.1.0"\n')
            fuzz = root / "crypto/fuzz"
            (fuzz / "src").mkdir(parents=True)
            (fuzz / "src/main.rs").touch()
            (fuzz / "helper/src").mkdir(parents=True)
            (fuzz / "helper/src/lib.rs").touch()
            (fuzz / "Cargo.toml").write_text(
                '[package]\nname="fuzzer"\nversion="0.1.0"\n[workspace]\nmembers=["helper"]\n')
            (fuzz / "helper/Cargo.toml").write_text('[package]\nname="helper"\nversion="0.1.0"\n')
            with self.assertRaisesRegex(ValueError, "additional detached members"):
                metadata_graph(root)


if __name__ == "__main__":
    unittest.main()

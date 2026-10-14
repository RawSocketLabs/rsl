"""Full-tier selector tests against Cargo itself; no compilation or registry access."""

from pathlib import Path
import tempfile
import unittest

from plan import BLADERF, BNB, MOVED_DETACHED, SOCKS, USDR, USDR_PORTABLE, metadata_graph, select


def crate(root, path, name):
    (root / path / "src").mkdir(parents=True)
    (root / path / "src/lib.rs").touch()
    (root / path / "Cargo.toml").write_text(f'[package]\nname="{name}"\nversion="0.1.0"\n')


class CargoMetadataTests(unittest.TestCase):
    def test_exact_source_fixture_has_a_non_cargo_edge_to_its_production_owner(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-portable-") as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(f'[workspace]\nresolver="2"\nmembers=["{USDR}"]\n')
            crate(root, USDR, "rsl-usdr")
            crate(root, USDR_PORTABLE, "rsl-usdr-nostd-check")
            manifest = root / USDR_PORTABLE / "Cargo.toml"
            manifest.write_text(manifest.read_text() + '[workspace]\n')
            graph = metadata_graph(root)
            self.assertEqual(graph[USDR_PORTABLE].dependencies, {USDR})
            self.assertFalse(graph[USDR_PORTABLE].workspace)
            plan = select([f"{USDR}/src/portable/temperature.rs"], graph, graph)
            self.assertIn("usdr-portable", plan["profiles"])
            self.assertNotIn("no-std", plan["profiles"])

    def test_a_portable_fixture_without_its_production_owner_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-orphan-portable-") as directory:
            root = Path(directory)
            crate(root, ".", "other")
            crate(root, USDR_PORTABLE, "rsl-usdr-nostd-check")
            manifest = root / USDR_PORTABLE / "Cargo.toml"
            manifest.write_text(manifest.read_text() + '[workspace]\n')
            with self.assertRaisesRegex(ValueError, "unregistered owned dependencies"):
                metadata_graph(root)

    def test_bladerf_nested_workspace_preserves_sys_and_downstream_ownership(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-bladerf-") as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text(
                f'[workspace]\nresolver="2"\nmembers=["consumer"]\nexclude=["{BLADERF}"]\n')
            for path, name in (("consumer", "consumer"), (BLADERF, "bladerf"),
                               (f"{BLADERF}/sys", "bladerf-sys")):
                crate = root / path
                (crate / "src").mkdir(parents=True)
                (crate / "src/lib.rs").touch()
                manifest = f'[package]\nname="{name}"\nversion="0.1.2"\n'
                if name == "consumer":
                    manifest += f'[dependencies]\nbladerf={{path="../{BLADERF}", optional=true}}\n'
                elif name == "bladerf":
                    manifest += ('[workspace]\nmembers=[".", "sys"]\n'
                                 '[dependencies]\nbladerf-sys={path="sys", version="0.1.2"}\n')
                (crate / "Cargo.toml").write_text(manifest)
            graph = metadata_graph(root)
            self.assertFalse(graph[BLADERF].workspace)
            self.assertFalse(graph[f"{BLADERF}/sys"].workspace)
            self.assertEqual(graph[BLADERF].dependencies, {f"{BLADERF}/sys"})
            for path, required_roots in ((BLADERF, {BLADERF, "consumer"}),
                                         (f"{BLADERF}/sys", {BLADERF, f"{BLADERF}/sys", "consumer"})):
                with self.subTest(path=path):
                    plan = select([f"{path}/src/lib.rs"], graph, graph)
                    self.assertFalse(plan["full"])
                    self.assertEqual(set(plan["roots"]), required_roots)
                    self.assertEqual(plan["packages"], ["consumer"])
                    self.assertIn("bladerf", plan["profiles"])
                    self.assertEqual(plan["fuzz"], [])

    def test_ffi_ownership_resolves_current_and_historical_layouts(self):
        for ffi_roots in (("usdr", "rust-dsdcc"), ("bindings/usdr", "bindings/rust-dsdcc"),
                          ("bindings/usdr", "bindings/dsdcc")):
            with self.subTest(roots=ffi_roots), tempfile.TemporaryDirectory(prefix="rsl-ci-ffi-test-") as directory:
                root = Path(directory)
                (root / "Cargo.toml").write_text(
                    '[workspace]\nresolver="2"\nmembers=["facade"]\nexclude=['
                    + ", ".join(f'"{path}"' for path in ffi_roots) + "]\n")
                for path, name in [("facade", "facade"), *zip(ffi_roots, ("usdr", "rust-dsdcc"))]:
                    crate = root / path
                    (crate / "src").mkdir(parents=True)
                    (crate / "src/lib.rs").touch()
                    manifest = f'[package]\nname="{name}"\nversion="0.1.0"\n'
                    if name == "facade":
                        manifest += "[dependencies]\n" + "".join(
                            f'{ffi}={{path="../{path}", optional=true}}\n'
                            for path, ffi in zip(ffi_roots, ("usdr", "rust-dsdcc")))
                    (crate / "Cargo.toml").write_text(manifest)
                graph = metadata_graph(root)
                self.assertEqual(graph["facade"].dependencies, set(ffi_roots))
                for path, profile in zip(ffi_roots, ("usdr", "rust-dsdcc")):
                    self.assertFalse(graph[path].workspace)
                    plan = select([f"{path}/src/lib.rs"], graph, graph)
                    self.assertFalse(plan["full"])
                    self.assertEqual(plan["packages"], ["facade"])
                    self.assertIn(profile, plan["profiles"])
                    self.assertNotIn("rust-dsdcc" if profile == "usdr" else "usdr", plan["profiles"])

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
            ("deps/src/lib.rs", {"rsl-deps", "dns"}, {"socks", "rsl-compression"}, set()),
            (f"{SOCKS}/src/blocking.rs", {"socks"}, set(), {f"{SOCKS}/fuzz"}),
            ("codecs/compression/src/lib.rs", {"rsl-compression", "rsl"}, {"socks", "rsl-crypto"}, set()),
            ("crypto/core/src/lib.rs", {"rsl-crypto", "rsl-crypto-legacy", "rsl-pki", "rsl"},
             {"socks", "bitsandbytes"}, {"crypto/fuzz", "pki/fuzz"}),
            (f"{BNB}/src/lib.rs", {"bitsandbytes", "socks", "rsl-asn1", "rsl-x509", "rsl-pki"},
             {"rsl-crypto", "rsl-crypto-legacy"}, {"codecs/bitsandbytes/fuzz", "pki/fuzz", f"{SOCKS}/fuzz"}),
        ]
        for path, required, unrelated, fuzz in cases:
            with self.subTest(path=path):
                plan = select([path], graph, graph)
                self.assertTrue(required <= set(plan["packages"]), plan["packages"])
                self.assertFalse(unrelated & set(plan["packages"]), plan["packages"])
                self.assertEqual(plan["fuzz"], [])
                self.assertTrue(fuzz <= set(plan["roots"]))

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

    def test_a_member_moved_with_a_shared_file_change_plans_full_coverage(self):
        # build_plan reads the base tree as historical; this is that read plus the selection.
        with tempfile.TemporaryDirectory(prefix="rsl-ci-move-test-") as directory:
            base, head = Path(directory, "base"), Path(directory, "head")
            for tree, member in ((base, "old"), (head, "new")):
                tree.mkdir()
                (tree / "Cargo.toml").write_text(
                    f'[workspace]\nresolver = "2"\nmembers = ["{member}"]\n')
                crate(tree, member, "moved")
            old = metadata_graph(base, historical=True)
            new = metadata_graph(head)
            for full in (False, True):
                with self.subTest(full=full):
                    plan = select(["Cargo.toml", "new/Cargo.toml", "old/Cargo.toml"],
                                  old, new, full=full)
                    self.assertTrue(plan["full"])
                    self.assertEqual(plan["packages"], ["moved"])

    def test_a_base_member_now_registered_as_detached_is_read_once(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-detach-test-") as directory:
            base = Path(directory)
            (base / "Cargo.toml").write_text(
                '[workspace]\nresolver = "2"\nmembers = ["usdr", "other"]\n')
            crate(base, "usdr", "usdr")
            crate(base, "other", "other")
            self.assertEqual(set(metadata_graph(base, historical=True)), {"usdr", "other"})

    def test_a_former_detached_path_cannot_be_reused_at_head(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-reuse-test-") as directory:
            root = Path(directory)
            crate(root, ".", "owned")
            (root / MOVED_DETACHED[0]).mkdir(parents=True)
            (root / MOVED_DETACHED[0] / "Cargo.toml").write_text("[workspace]\n")
            with self.assertRaisesRegex(ValueError, "reused former path"):
                metadata_graph(root)


if __name__ == "__main__":
    unittest.main()

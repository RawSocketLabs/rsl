"""Dependency impact through real offline Cargo metadata and immutable Git archives."""

from pathlib import Path
import subprocess
import tempfile
import unittest

from dependencies import dependency_delta
from integration_metadata import crate
from plan import dependency_changes, graph_at, metadata_graph, select


class DependencyMetadataTests(unittest.TestCase):
    def test_stale_lock_cannot_introduce_unplanned_resolution_in_a_locked_command(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-stale-lock-") as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[workspace]\nresolver="2"\nmembers=["owner","helper"]\n')
            crate(root, "owner", "owner")
            crate(root, "helper", "helper")
            with (root / "owner/Cargo.toml").open("a") as file:
                file.write('[dependencies]\nhelper={path="../helper"}\n')
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root, check=True,
                           capture_output=True)
            lock_before = (root / "Cargo.lock").read_bytes()
            manifest = root / "helper/Cargo.toml"
            manifest.write_text(manifest.read_text().replace('version="0.1.0"', 'version="0.2.0"'))
            result = subprocess.run(
                ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
                cwd=root, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--locked", result.stderr)
            self.assertEqual((root / "Cargo.lock").read_bytes(), lock_before)

    def test_workspace_inheritance_and_detached_resolution_use_distinct_locks(self):
        with tempfile.TemporaryDirectory(prefix="rsl-ci-inherited-") as directory:
            graphs = []
            for version in ("1", "2"):
                root = Path(directory, version)
                root.mkdir()
                (root / "Cargo.toml").write_text(
                    '[workspace]\nresolver="2"\nmembers=["owner","peer","unrelated"]\nexclude=["usdr"]\n'
                    f'[workspace.dependencies]\nalias={{package="wire",version="{version}"}}\n')
                for name in ("owner", "peer", "unrelated", "usdr"):
                    crate(root, name, name)
                    manifest = root / name / "Cargo.toml"
                    if name in {"owner", "peer"}:
                        with manifest.open("a") as file:
                            file.write('[dependencies]\nalias.workspace=true\n')
                    if name == "usdr":
                        with manifest.open("a") as file:
                            file.write('[workspace]\n[dependencies]\nwire="1"\n')
                # Cargo metadata expands workspace inheritance, rename, requirements and
                # workspace_root without resolving/fetching registry dependencies.
                source = 'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                (root / "Cargo.lock").write_text(
                    'version=4\n' + "".join(
                        f'[[package]]\nname="{name}"\nversion="0.1.0"\n'
                        + ('dependencies=["wire"]\n' if name != "unrelated" else "")
                        for name in ("owner", "peer", "unrelated"))
                    + f'[[package]]\nname="wire"\nversion="{version}.0.0"\n' + source)
                (root / "usdr/Cargo.lock").write_text(
                    'version=4\n[[package]]\nname="usdr"\nversion="0.1.0"\ndependencies=["wire"]\n'
                    '[[package]]\nname="wire"\nversion="1.0.0"\n' + source)
                graphs.append(metadata_graph(root))
            old, new = graphs
            delta = dependency_delta(old, new)
            self.assertEqual(delta, {"owner", "peer"})
            plan = select(["Cargo.toml", "Cargo.lock"], old, new,
                          dependency_changes={"Cargo.toml": delta,
                                              "Cargo.lock": dependency_delta(old, new, lockfile="Cargo.lock")})
            self.assertFalse(plan["full"])
            self.assertEqual(plan["packages"], ["owner", "peer"])
            self.assertNotIn("usdr", plan["profiles"])

    def test_missing_ignored_locks_in_actual_git_tree_do_not_poison_unrelated_deltas(self):
        root = Path(__file__).resolve().parents[2]
        graph = graph_at(root, "HEAD")
        self.assertEqual(dependency_delta(graph, graph), set())
        # Parse exact committed TOML/locks, not mutable working-tree lockfiles.
        deltas = dependency_changes(root, "HEAD", "HEAD",
                                    ["Cargo.toml", "Cargo.lock"], graph, graph)
        plan = select(["Cargo.toml", "Cargo.lock"], graph, graph, dependency_changes=deltas)
        self.assertFalse(plan["full"])
        self.assertEqual(plan["packages"], [])
        self.assertEqual(plan["profiles"], ["deny"])

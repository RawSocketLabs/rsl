"""Dependency impact through real offline Cargo metadata and immutable Git archives."""

from pathlib import Path
import tempfile
import unittest

from dependencies import dependency_delta
from integration_metadata import crate
from plan import dependency_changes, graph_at, metadata_graph, select


class DependencyMetadataTests(unittest.TestCase):
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

"""Independent lock/declaration impact oracles; standard library only, no Cargo/network."""

import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from dependencies import LockGraph, dependency_delta
from plan import Package, dependency_changes, select


def locked(name, *, version="1.0.0", dependencies=(), source=None, checksum=None):
    row = {"name": name, "version": version}
    if dependencies:
        row["dependencies"] = list(dependencies)
    if source is not None:
        row["source"] = source
    if checksum is not None:
        row["checksum"] = checksum
    return row


def snapshot(rows, *, declarations=None, lockfile="Cargo.lock", features=None):
    """Metadata-shaped declarations plus independently authored Cargo lock-format fixtures."""
    graph = {}
    with tempfile.TemporaryDirectory(prefix="rsl-ci-dependencies-") as directory:
        root = Path(directory)
        path = root / "Cargo.lock"
        path.write_text("version = 4\n" + "".join(
            "\n[[package]]\n" + "".join(f"{key} = {json.dumps(value)}\n" for key, value in row.items())
            for row in rows))
        lock = LockGraph(path)
        for name in ("owner", "peer", "consumer", "unrelated"):
            deps = (declarations or {}).get(name, [])
            package = {"name": name, "version": "1.0.0", "dependencies": deps,
                       "features": (features or {}).get(name, {})}
            state = lock.snapshot(package, root, lockfile)
            owned = {"owner"} if name == "consumer" else set()
            graph[name] = Package(name, owned, state=state)
    return graph


def topology():
    # owner -> renamed wire -> leaf. consumer -> owner. peer -> other. unrelated -> none.
    return [locked("owner", dependencies=["wire"]), locked("consumer", dependencies=["owner"]),
            locked("peer", dependencies=["other"]), locked("unrelated"),
            locked("wire", source="registry+fixture", dependencies=["leaf"]),
            locked("leaf", source="registry+fixture", checksum="before"),
            locked("other", source="registry+fixture")]


class DependencyTests(unittest.TestCase):
    def test_transitive_checksum_change_selects_only_users_and_owned_consumers(self):
        rows = topology()
        old = snapshot(rows, declarations={"owner": [{"name": "wire", "rename": "alias"}]})
        rows[-2]["checksum"] = "after"
        new = snapshot(rows, declarations={"owner": [{"name": "wire", "rename": "alias"}]})
        delta = dependency_delta(old, new, lockfile="Cargo.lock")
        plan = select(["Cargo.lock"], old, new, dependency_changes={"Cargo.lock": delta})
        self.assertFalse(plan["full"])
        self.assertEqual(plan["packages"], ["consumer", "owner"])
        self.assertEqual(plan["fuzz"], [])

    def test_source_version_and_added_or_removed_matching_versions_are_impacts(self):
        base = topology()
        declarations = {"owner": [{"name": "wire", "rename": "alias"}]}
        for field, value in (("version", "2.0.0"), ("source", "git+fixture#revision"),
                             ("dependencies", [])):
            with self.subTest(field=field):
                changed = copy.deepcopy(base)
                changed[4][field] = value
                self.assertEqual(dependency_delta(snapshot(base, declarations=declarations),
                                                 snapshot(changed, declarations=declarations)),
                                 {"owner", "consumer"})
        # Direct declarations conservatively include both versions/sources, even though
        # a precisely versioned lock edge uses only the first.
        base[0]["dependencies"] = ["wire 1.0.0 (registry+fixture)"]
        added = [*base, locked("wire", version="2.0.0", source="git+fixture#revision")]
        for before, after in ((base, added), (added, base)):
            old = snapshot(before, declarations=declarations)
            new = snapshot(after, declarations=declarations)
            delta = dependency_delta(old, new, lockfile="Cargo.lock")
            plan = select(["Cargo.lock"], old, new, dependency_changes={"Cargo.lock": delta})
            self.assertEqual(plan["packages"], ["consumer", "owner"])

    def test_unreachable_nodes_and_lock_reordering_do_not_retest_packages(self):
        rows = topology()
        old = snapshot(rows)
        new = snapshot([*reversed(rows), locked("unused", source="registry+fixture")])
        self.assertEqual(dependency_delta(old, new, lockfile="Cargo.lock"), set())
        plan = select(["Cargo.lock"], old, new, dependency_changes={"Cargo.lock": set()})
        self.assertEqual(plan["packages"], [])
        self.assertEqual(plan["profiles"], ["deny"])  # Supply-chain checks remain mandatory.

    def test_feature_changes_include_shared_users_but_not_isolated_workspace_users(self):
        rows = topology()
        rows[2]["dependencies"] = ["wire"]
        before = {"owner": [{"name": "wire", "features": []}]}
        after = {"owner": [{"name": "wire", "features": ["fast"]}]}
        old, new = snapshot(rows, declarations=before), snapshot(rows, declarations=after)
        self.assertEqual(dependency_delta(old, new), {"owner", "peer", "consumer"})
        # Same package name in an independent lockfile does not feature-unify.
        old["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        new["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        self.assertNotIn("isolated", dependency_delta(old, new))

    def test_feature_changes_include_shared_transitive_users_without_owned_edges(self):
        rows = topology()
        rows[2]["dependencies"] = ["leaf"]  # peer has no edge to wire or owner.
        before = {"owner": [{"name": "wire", "features": []}]}
        after = {"owner": [{"name": "wire", "features": ["fast"]}]}
        old, new = snapshot(rows, declarations=before), snapshot(rows, declarations=after)
        old["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        new["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        self.assertEqual(dependency_delta(old, new), {"owner", "peer", "consumer"})

    def test_locked_version_updates_include_unchanged_transitive_feature_peers(self):
        rows = topology()
        rows[2]["dependencies"] = ["leaf"]
        old = snapshot(rows)
        old["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        rows[4]["version"] = "1.1.0"  # New wire code may request new leaf features.
        new = snapshot(rows)
        new["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        delta = dependency_delta(old, new, lockfile="Cargo.lock")
        self.assertEqual(delta, {"owner", "peer", "consumer"})

    def test_feature_changes_cross_resolution_contexts_via_owned_path_consumers(self):
        rows = topology()
        rows[2]["dependencies"] = ["leaf"]
        old, new = snapshot(rows), snapshot(rows)
        before = {"owner": [{"name": "wire", "features": []}]}
        after = {"owner": [{"name": "wire", "features": ["fast"]}]}
        old["binding"] = snapshot(rows, declarations=before, lockfile="binding/Cargo.lock")["owner"]
        new["binding"] = snapshot(rows, declarations=after, lockfile="binding/Cargo.lock")["owner"]
        old["consumer"].dependencies = {"binding"}
        new["consumer"].dependencies = {"binding"}
        old["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        new["isolated"] = snapshot(rows, lockfile="tool/Cargo.lock")["peer"]
        delta = dependency_delta(old, new)
        self.assertIn("peer", delta)
        self.assertNotIn("isolated", delta)

    def test_unrelated_missing_locks_do_not_disable_manifest_narrowing(self):
        old, new = snapshot(topology()), snapshot(topology())
        rows = topology()
        rows[4]["dependencies"] = ["missing"]
        old["unproven"] = snapshot(rows, lockfile="ignored/Cargo.lock")["peer"]
        new["unproven"] = snapshot(rows, lockfile="ignored/Cargo.lock")["peer"]
        new["owner"] = snapshot(topology(), declarations={"owner": [{"name": "wire", "features": ["fast"]}]})["owner"]
        self.assertEqual(dependency_delta(old, new), {"owner", "consumer"})

    def test_detached_locks_use_their_own_resolution_and_old_consumer_edges(self):
        rows = topology()
        old, new = snapshot(rows), snapshot(rows)
        old["tool"] = snapshot(rows, lockfile="tool/Cargo.lock")["owner"]
        rows[-2]["checksum"] = "after"
        new["tool"] = snapshot(rows, lockfile="tool/Cargo.lock")["owner"]
        old["consumer"].dependencies = {"tool"}
        new["consumer"].dependencies = set()
        delta = dependency_delta(old, new, lockfile="tool/Cargo.lock")
        plan = select(["tool/Cargo.lock"], old, new, dependency_changes={"tool/Cargo.lock": delta})
        self.assertFalse(plan["full"])
        self.assertEqual(plan["packages"], ["consumer", "owner"])  # tool's package name is owner.
        self.assertEqual(set(plan["roots"]), {"tool", "consumer"})

    def test_missing_references_or_snapshots_force_full_not_empty_coverage(self):
        old = snapshot(topology())
        changed = topology()
        changed[4]["dependencies"] = ["missing"]
        new = snapshot(changed)
        delta = dependency_delta(old, new, lockfile="Cargo.lock")
        self.assertIsNone(delta)
        plan = select(["Cargo.lock"], old, new, dependency_changes={"Cargo.lock": delta})
        self.assertTrue(plan["full"])
        self.assertIn("dependency impact unavailable: Cargo.lock", plan["full_reasons"])
        self.assertIsNone(dependency_delta(old, new, lockfile="unknown/Cargo.lock"))
        del new["owner"]
        self.assertIsNone(dependency_delta(old, new))

    def test_root_manifest_narrows_only_workspace_dependency_edits(self):
        old, new = snapshot(topology()), snapshot(topology())
        declarations = {"owner": [{"name": "wire", "features": ["fast"]}]}
        new = snapshot(topology(), declarations=declarations)
        with tempfile.TemporaryDirectory(prefix="rsl-ci-manifest-") as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout.decode().strip()
            git("init", "--quiet")
            manifest = root / "Cargo.toml"
            manifest.write_text('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire="1"\n')
            git("add", "Cargo.toml")
            base = git("write-tree")
            for content, narrow in (
                ('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire={version="1",features=["fast"]}\n', True),
                ('[workspace]\nresolver="3"\n[workspace.dependencies]\nwire="1"\n', False),
                ('[workspace]\nresolver="2"\nmembers=["added"]\n[workspace.dependencies]\nwire="1"\n', False),
                ('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire="1"\n[profile.dev]\nopt-level=1\n', False),
                ('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire="1"\n[workspace.lints.rust]\nunsafe_code="deny"\n', False),
                ('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire="1"\n[workspace.package]\nrust-version="1.85"\n', False),
                ('[workspace]\nresolver="2"\n[workspace.dependencies]\nwire="1"\n[patch.crates-io]\nwire={path="patched"}\n', False),
            ):
                with self.subTest(narrow=narrow, content=content):
                    manifest.write_text(content)
                    git("add", "Cargo.toml")
                    head = git("write-tree")
                    deltas = dependency_changes(root, base, head, ["Cargo.toml"], old, new)
                    plan = select(["Cargo.toml"], old, new, dependency_changes=deltas)
                    self.assertEqual(plan["full"], not narrow)
                    if narrow:
                        self.assertEqual(plan["packages"], ["consumer", "owner"])

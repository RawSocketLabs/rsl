"""Offline dependency snapshots for conservative manifest/lockfile impact selection."""

from collections import defaultdict
from dataclasses import dataclass, field
import json
from pathlib import Path
import re
import tomllib


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


@dataclass(frozen=True)
class DependencyState:
    lockfile: str
    declarations: tuple
    features: str
    locked: frozenset
    names: frozenset
    error: str | None = None
    feature_names: tuple = ()
    resolver: object = field(default=None, compare=False, repr=False)


class LockGraph:
    """Include owned/path nodes, checksums, versions, sources and transitive edges.

    Name-only declarations conservatively include every locked version/source of that name.
    A missing reference never becomes an empty (apparently unchanged) dependency set.
    """

    def __init__(self, path):
        self.nodes = {}
        self.by_name = defaultdict(set)
        self.edges = {}
        self.error = None
        try:
            data = tomllib.loads(Path(path).read_text())
            if data.get("version") not in {3, 4} or set(data) != {"version", "package"}:
                raise ValueError("unsupported lockfile shape/version")
            for row in data["package"]:
                if set(row) - {"name", "version", "source", "checksum", "dependencies"}:
                    raise ValueError("unsupported lockfile package fields")
                if (not isinstance(row["name"], str) or not isinstance(row["version"], str)
                        or not isinstance(row.get("dependencies", []), list)
                        or not all(isinstance(dep, str) for dep in row.get("dependencies", []))
                        or any(value is not None and not isinstance(value, str)
                               for value in (row.get("source"), row.get("checksum")))):
                    raise ValueError("invalid lockfile package values")
                key = (row["name"], row["version"], row.get("source"))
                if key in self.nodes:
                    raise ValueError("duplicate locked package identity")
                row = {**row, "dependencies": sorted(row.get("dependencies", []))}
                self.nodes[key] = canonical(row)
                self.by_name[key[0]].add(key)
            for row in data["package"]:
                key = (row["name"], row["version"], row.get("source"))
                self.edges[key] = set()
                for reference in row.get("dependencies", []):
                    match = re.fullmatch(r"([^ ()]+)(?: ([^ ()]+))?(?: \(([^()]+)\))?", reference)
                    if match is None:
                        raise ValueError(f"unsupported lock dependency: {reference}")
                    name, version, source = match.groups()
                    candidates = {item for item in self.by_name[name]
                                  if (version is None or item[1] == version)
                                  and (source is None or item[2] == source)}
                    if not candidates:
                        raise ValueError(f"unresolved lock dependency: {reference}")
                    self.edges[key].update(candidates)
        except (OSError, ValueError, KeyError, TypeError) as error:
            self.error = str(error)

    def snapshot(self, package, root, lockfile):
        grouped = defaultdict(list)
        for dependency in package["dependencies"]:
            dependency = dict(dependency)
            if "path" in dependency:
                dependency["path"] = Path(dependency["path"]).resolve().relative_to(root).as_posix()
            grouped[dependency["name"]].append(canonical(dependency))
        declarations = tuple((name, canonical(sorted(values))) for name, values in sorted(grouped.items()))
        features = canonical(package["features"])
        if self.error:
            return DependencyState(lockfile, declarations, features, frozenset(), frozenset(), self.error,
                                   resolver=self)
        seeds = {(package["name"], package["version"], None)}
        if not seeds <= self.nodes.keys():
            return DependencyState(lockfile, declarations, features, frozenset(), frozenset(),
                                   "owned package is missing from its lockfile", resolver=self)
        for name in grouped:
            if not self.by_name[name]:
                return DependencyState(lockfile, declarations, features, frozenset(), frozenset(),
                                       f"declared dependency is missing from lockfile: {name}", resolver=self)
            seeds.update(self.by_name[name])
        reached = self.closure(seeds)
        feature_names = tuple((name, frozenset(key[0] for key in self.closure(self.by_name[name])))
                              for name in sorted(grouped))
        return DependencyState(lockfile, declarations, features,
                               frozenset(self.nodes[key] for key in reached),
                               frozenset(key[0] for key in reached), feature_names=feature_names,
                               resolver=self)

    def closure(self, seeds):
        reached = set(seeds)
        pending = list(seeds)
        while pending:
            for dependency in self.edges[pending.pop()]:
                if dependency not in reached:
                    reached.add(dependency)
                    pending.append(dependency)
        return reached


def dependency_delta(old, new, *, lockfile=None):
    """Return changed owners and shared feature users, or None when proof is unavailable."""
    selected = set()
    shared = set()
    changed_owners = {}
    matched = False
    for root in old.keys() | new.keys():
        before = old.get(root)
        after = new.get(root)
        a = before.state if before else None
        b = after.state if after else None
        if lockfile is not None and not any(state and state.lockfile == lockfile for state in (a, b)):
            continue
        matched = True
        if not a or not b:
            return None
        if a.error or b.error:
            # Ignored/untracked detached locks are not evidence against an unrelated
            # manifest edit. Changed declarations or an edited lock still require proof.
            if lockfile is None and (a.declarations, a.features, a.lockfile) == (
                    b.declarations, b.features, b.lockfile):
                continue
            return None
        if a != b:
            selected.add(root)
        if lockfile is not None:
            # A new locked version/checksum can change requested/default transitive
            # features even when those dependencies' own lock rows stay unchanged.
            changed_names = {json.loads(row)["name"] for row in a.locked ^ b.locked}
            for state in (a, b):
                if state.lockfile == lockfile:
                    for name in changed_names:
                        shared.update((lockfile, key[0]) for key in
                                      state.resolver.closure(state.resolver.by_name[name]))
        if lockfile is None:
            bindings_a, bindings_b = dict(a.declarations), dict(b.declarations)
            changed = {name for name in bindings_a.keys() | bindings_b.keys()
                       if bindings_a.get(name) != bindings_b.get(name)}
            if a.features != b.features:
                changed.update(bindings_a.keys() | bindings_b.keys())
            if changed:
                changed_owners[root] = (changed, a, b)
    reverse = defaultdict(set)
    for graph in (old, new):
        for root, package in graph.items():
            for dependency in package.dependencies:
                reverse[dependency].add(root)
    for owner, (changed, a, b) in changed_owners.items():
        consumers = {owner}
        pending = [owner]
        while pending:
            for consumer in reverse[pending.pop()]:
                if consumer not in consumers:
                    consumers.add(consumer)
                    pending.append(consumer)
        contexts = {}
        for graph in (old, new):
            for consumer in consumers & graph.keys():
                state = graph[consumer].state
                if state is None or state.error:
                    return None
                contexts.setdefault(state.lockfile, []).append(state.resolver)
        own_names = set()
        for state in (a, b):
            impacts = dict(state.feature_names)
            for name in changed:
                own_names.update(impacts.get(name, ()))
        for context, resolvers in contexts.items():
            names = own_names.copy()
            for resolver in resolvers:
                for name in changed:
                    names.update(key[0] for key in resolver.closure(resolver.by_name[name]))
            shared.update((context, name) for name in names)
    # Declaration or locked-code changes may unify transitive features without an owned edge.
    for graph in (old, new):
        for root, package in graph.items():
            state = package.state
            if state and state.error and any(context == state.lockfile for context, _ in shared):
                return None
            if state and any((state.lockfile, name) in shared for name in state.names):
                selected.add(root)
    return selected if matched else None

#!/usr/bin/env python3
"""Inspect a Rust repository without changing it and emit structured JSON."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

try:
    import tomllib
except ModuleNotFoundError as error:  # pragma: no cover - version diagnostic
    raise SystemExit("Python 3.11 or newer is required (missing tomllib)") from error


SKIP_DIRECTORIES = {
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "vendor",
    "generated",
}


def relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def repository_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for path in root.rglob("*"):
        if any(part in SKIP_DIRECTORIES for part in path.relative_to(root).parts):
            continue
        if path.is_file():
            files.append(path)
    return sorted(files)


def load_manifest(path: Path, root: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": relative(path, root)}
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        result["error"] = str(error)
        return result

    package = document.get("package", {})
    workspace = document.get("workspace", {})
    lib = document.get("lib", {})
    package_root = path.parent
    result.update(
        {
            "package": package.get("name"),
            "version": package.get("version"),
            "edition": package.get("edition"),
            "rust_version": package.get("rust-version"),
            "publish": package.get("publish"),
            "build_script": package.get("build"),
            "workspace": bool(workspace),
            "workspace_members": workspace.get("members", []),
            "workspace_exclude": workspace.get("exclude", []),
            "lib_crate_types": lib.get("crate-type", []),
            "has_library": bool(lib) or (package_root / "src" / "lib.rs").is_file(),
            "features": sorted(document.get("features", {}).keys()),
        }
    )
    dependencies: dict[str, list[str]] = {}
    for section in (
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "target",
    ):
        value = document.get(section)
        if isinstance(value, dict):
            dependencies[section] = sorted(value.keys())
    result["dependency_sections"] = dependencies
    result["has_bins"] = bool(document.get("bin")) or (
        package_root / "src" / "main.rs"
    ).is_file()
    return result


def text_if_small(path: Path) -> str:
    try:
        if path.stat().st_size > 2_000_000:
            return ""
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def workspace_manifests(manifests: list[dict[str, Any]]) -> list[dict[str, Any]]:
    root_manifest = next(
        (manifest for manifest in manifests if manifest.get("path") == "Cargo.toml"),
        None,
    )
    if root_manifest is None:
        return []
    if root_manifest.get("package"):
        selected = [root_manifest]
    else:
        selected = []
    members = root_manifest.get("workspace_members", [])
    excludes = root_manifest.get("workspace_exclude", [])
    for manifest in manifests:
        if not manifest.get("package") or manifest is root_manifest:
            continue
        package_directory = PurePosixPath(manifest["path"]).parent
        included = any(package_directory.match(pattern) for pattern in members)
        excluded = any(package_directory.match(pattern) for pattern in excludes)
        if included and not excluded:
            selected.append(manifest)
    return selected


def infer_profiles(
    manifests: list[dict[str, Any]],
    texts: dict[str, str],
    benches: list[str],
) -> dict[str, Any]:
    package_manifests = workspace_manifests(manifests)
    has_library = any(manifest.get("has_library") for manifest in package_manifests)
    has_binary = any(manifest.get("has_bins") for manifest in package_manifests)
    public = any(manifest.get("publish") is not False for manifest in package_manifests)
    if has_library and not has_binary:
        base = "public-library" if public else "internal-library"
    elif has_binary:
        base = "application"
    else:
        base = "experimental"

    corpus = "\n".join(texts.values()).lower()
    path_and_package_signals = " ".join(
        [
            *texts,
            *(
                str(manifest.get("package", ""))
                for manifest in package_manifests
            ),
        ]
    ).lower()
    dependency_names = {
        dependency
        for manifest in package_manifests
        for names in manifest.get("dependency_sections", {}).values()
        for dependency in names
    }
    capabilities: set[str] = set()
    if len(package_manifests) > 1 or any(manifest.get("workspace") for manifest in manifests):
        capabilities.add("workspace")
    if "#![no_std]" in corpus or "#![cfg_attr" in corpus and "no_std" in corpus:
        capabilities.add("no-std")
    if (
        re.search(r'extern\s+"(?:C|system)"', corpus)
        or dependency_names.intersection({"bindgen", "cbindgen"})
        or any(
            crate_type in {"cdylib", "staticlib"}
            for manifest in package_manifests
            for crate_type in manifest.get("lib_crate_types", [])
        )
    ):
        capabilities.add("ffi")
    if dependency_names.intersection({"tokio", "async-std", "smol", "futures"}):
        capabilities.add("async")
    if any(
        token in path_and_package_signals
        for token in ("protocol", "packet", "codec", "parser", "serializer")
    ):
        capabilities.add("protocol")
    if (
        dependency_names.intersection({"rustfft", "realfft", "dasp", "fundsp"})
        or any(token in path_and_package_signals for token in ("dsp", "sdr", "signal"))
    ):
        capabilities.add("dsp")
    if benches or dependency_names.intersection({"criterion", "iai-callgrind"}):
        capabilities.add("performance-sensitive")
    if dependency_names.intersection(
        {"cortex-m", "cortex-m-rt", "embedded-hal", "riscv", "riscv-rt"}
    ) or any(token in path_and_package_signals for token in ("firmware", "embedded")):
        capabilities.add("embedded")
    return {
        "base": base,
        "capabilities": sorted(capabilities),
        "confidence": "tentative-until-interview",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("repository", nargs="?", default=".")
    parser.add_argument("--pretty", action="store_true")
    arguments = parser.parse_args()

    root = Path(arguments.repository).resolve()
    if not root.is_dir():
        parser.error(f"{root} is not a directory")

    files = repository_files(root)
    manifests = [
        load_manifest(path, root) for path in files if path.name == "Cargo.toml"
    ]
    rust_files = [path for path in files if path.suffix == ".rs"]
    rust_text = {relative(path, root): text_if_small(path) for path in rust_files}
    non_product_parts = {"benches", "evals", "examples", "fixtures", "fuzz", "tests"}
    product_rust_text = {
        relative(path, root): text_if_small(path)
        for path in rust_files
        if not non_product_parts.intersection(path.relative_to(root).parts)
    }
    selected_text = {
        relative(path, root): text_if_small(path)
        for path in files
        if path.name
        in {
            "AGENTS.md",
            "CLAUDE.md",
            "README.md",
            "CONTRIBUTING.md",
            "Cargo.toml",
            "rustfmt.toml",
            "clippy.toml",
            "deny.toml",
        }
        or ".github/workflows" in relative(path, root)
    }
    command_pattern = re.compile(r"\bcargo\s+(?:\+\S+\s+)?(?:fmt|check|clippy|test|doc|deny|audit|miri|fuzz|semver-checks)\b[^\n]*")
    commands = sorted(
        {
            match.group(0).strip()
            for text in selected_text.values()
            for match in command_pattern.finditer(text)
        }
    )
    relevant_manifest_paths = {
        manifest["path"] for manifest in workspace_manifests(manifests)
    }
    profile_text = {
        **{
            path: text
            for path, text in selected_text.items()
            if path in relevant_manifest_paths
            or Path(path).name in {"rustfmt.toml", "clippy.toml"}
        },
        **product_rust_text,
    }
    benches = [
        relative(path, root)
        for path in files
        if "benches" in path.relative_to(root).parts
    ]
    result = {
        "schema": 1,
        "repository": str(root),
        "manifests": manifests,
        "instructions": [
            relative(path, root)
            for path in files
            if path.name in {"AGENTS.md", "CLAUDE.md"}
        ],
        "tooling": [
            relative(path, root)
            for path in files
            if path.name
            in {
                "rust-toolchain.toml",
                "rust-toolchain",
                "rustfmt.toml",
                "clippy.toml",
                "deny.toml",
                "Cross.toml",
            }
        ],
        "ci": [
            relative(path, root)
            for path in files
            if relative(path, root).startswith(".github/workflows/")
        ],
        "tests": [
            relative(path, root)
            for path in files
            if "tests" in path.relative_to(root).parts
        ],
        "benches": benches,
        "fuzz": [
            relative(path, root)
            for path in files
            if "fuzz" in path.relative_to(root).parts
        ],
        "unsafe_occurrences": sum(
            len(re.findall(r"\bunsafe\b", text)) for text in rust_text.values()
        ),
        "no_std_crates": sorted(
            path for path, text in rust_text.items() if "#![no_std]" in text
        ),
        "observed_cargo_commands": commands,
        "suggested_profile": infer_profiles(manifests, profile_text, benches),
        "question_topics": [
            "purpose-and-consumers",
            "compatibility-and-platforms",
            "api-errors-and-maintainability",
            "performance-and-execution",
            "testing-and-validation",
            "conditional-domain-contracts",
            "dependencies-security-and-workflow",
        ],
    }
    json.dump(result, sys.stdout, indent=2 if arguments.pretty else None, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

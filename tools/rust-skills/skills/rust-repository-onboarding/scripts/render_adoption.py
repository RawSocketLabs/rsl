#!/usr/bin/env python3
"""Render an explicitly approved Rust-skills adoption without overwriting files."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path, PurePosixPath
from typing import Any

try:
    import tomllib
except ModuleNotFoundError as error:  # pragma: no cover - version diagnostic
    raise SystemExit("Python 3.11 or newer is required (missing tomllib)") from error


VALID_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
VALID_BASES = {
    "public-library",
    "internal-library",
    "application",
    "service",
    "experimental",
}
VALID_TIERS = {"required", "optional"}
VALID_APPLICABILITY = {"applicable", "inapplicable"}
VALID_ADAPTERS = {"common": "agent-skills", "claude": "claude-skills"}
VALID_INSTALLATION_MODES = {"copy", "generate", "reference", "symlink", "vendor"}


def require_string(document: dict[str, Any], key: str) -> str:
    value = document.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"answers require non-empty string {key!r}")
    return value.strip()


def require_string_array(document: dict[str, Any], key: str) -> list[str]:
    value = document.get(key)
    if not isinstance(value, list) or not all(
        isinstance(item, str) and item.strip() for item in value
    ):
        raise ValueError(f"answers require string array {key!r}")
    result = [item.strip() for item in value]
    if len(set(result)) != len(result):
        raise ValueError(f"answers contain duplicate values in {key!r}")
    return result


def quote(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def string_array(values: list[str]) -> str:
    return "[" + ", ".join(quote(value) for value in values) + "]"


def validate_answers(document: dict[str, Any]) -> None:
    if document.get("schema") != 1:
        raise ValueError("answers must use schema 1")
    if document.get("approved") is not True:
        raise ValueError("answers must contain approved = true")
    base = require_string(document, "base")
    if base not in VALID_BASES:
        raise ValueError(f"unknown base profile {base!r}")
    for key in (
        "capabilities",
        "enabled_skills",
        "local_instruction_paths",
        "unresolved",
    ):
        require_string_array(document, key)
    for value in document["capabilities"] + document["enabled_skills"]:
        if not VALID_ID.fullmatch(value):
            raise ValueError(f"invalid ID {value!r}")
    decisions = document.get("decisions", {})
    if not isinstance(decisions, dict) or not all(
        isinstance(key, str)
        and VALID_ID.fullmatch(key)
        and isinstance(value, str)
        for key, value in decisions.items()
    ):
        raise ValueError("decisions must map lowercase hyphenated IDs to Markdown")
    checks = document.get("validation")
    if not isinstance(checks, list) or not checks:
        raise ValueError("answers require at least one validation check")
    identifiers: set[str] = set()
    for check in checks:
        if not isinstance(check, dict):
            raise ValueError("every validation check must be an object")
        identifier = check.get("id")
        if not isinstance(identifier, str) or not VALID_ID.fullmatch(identifier):
            raise ValueError(f"invalid validation check ID {identifier!r}")
        if identifier in identifiers:
            raise ValueError(f"duplicate validation check ID {identifier!r}")
        identifiers.add(identifier)
        if check.get("tier") not in VALID_TIERS:
            raise ValueError(f"check {identifier!r} needs required or optional tier")
        command = check.get("command")
        if not isinstance(command, list) or not command or not all(
            isinstance(part, str) and part for part in command
        ):
            raise ValueError(f"check {identifier!r} needs a command string array")
        for field in ("prerequisites", "required_tools"):
            values = check.get(field, [])
            if not isinstance(values, list) or not all(
                isinstance(item, str) and item.strip() for item in values
            ):
                raise ValueError(f"check {identifier!r} needs {field} strings")
        if check.get("applicability", "applicable") not in VALID_APPLICABILITY:
            raise ValueError(f"check {identifier!r} has invalid applicability")
        if check.get("tier") == "required" and check.get("enabled") is False:
            raise ValueError(f"required check {identifier!r} cannot be disabled")
    if document.get("agents_mode", "preserve") not in {"create", "preserve"}:
        raise ValueError("agents_mode must be create or preserve")
    if document.get("adapter", "common") not in VALID_ADAPTERS:
        raise ValueError("adapter must be common or claude")
    if require_string(document, "installation_mode") not in VALID_INSTALLATION_MODES:
        raise ValueError(
            "installation_mode must be copy, generate, reference, symlink, or vendor"
        )

    components = document.get("components")
    if not isinstance(components, dict):
        raise ValueError("answers require a components object")
    for component_path, component in components.items():
        if not isinstance(component_path, str):
            raise ValueError(f"invalid component path {component_path!r}")
        path = PurePosixPath(component_path)
        if (
            not component_path
            or path.is_absolute()
            or "." in path.parts
            or ".." in path.parts
        ):
            raise ValueError(f"invalid component path {component_path!r}")
        if not isinstance(component, dict):
            raise ValueError(f"component {component_path!r} must be an object")
        base = require_string(component, "base")
        if base not in VALID_BASES:
            raise ValueError(
                f"component {component_path!r} has unknown base profile {base!r}"
            )
        capabilities = require_string_array(component, "capabilities")
        component_skills = require_string_array(component, "enabled_skills")
        require_string(component, "rationale")
        for value in capabilities + component_skills:
            if not VALID_ID.fullmatch(value):
                raise ValueError(
                    f"component {component_path!r} has invalid ID {value!r}"
                )
        uninstalled = set(component_skills) - set(document["enabled_skills"])
        if uninstalled:
            raise ValueError(
                f"component {component_path!r} enables uninstalled skills "
                f"{sorted(uninstalled)!r}"
            )


def validation_toml(checks: list[dict[str, Any]]) -> str:
    lines = ["schema = 1"]
    for check in checks:
        lines.extend(
            [
                "",
                "[[checks]]",
                f"id = {quote(check['id'])}",
                f"tier = {quote(check['tier'])}",
                f"description = {quote(check.get('description', ''))}",
                f"command = {string_array(check['command'])}",
                f"prerequisites = {string_array(check.get('prerequisites', []))}",
                f"required_tools = {string_array(check.get('required_tools', []))}",
                f"enabled = {'true' if check.get('enabled', True) else 'false'}",
                f"applicability = {quote(check.get('applicability', 'applicable'))}",
                f"working_directory = {quote(check.get('working_directory', '.'))}",
                f"timeout_seconds = {int(check.get('timeout_seconds', 900))}",
            ]
        )
        if check.get("reason"):
            lines.append(f"reason = {quote(check['reason'])}")
    return "\n".join(lines) + "\n"


def manifest_provenance(
    manifest_path: Path,
    standards: str,
    adapter: str,
    skills: list[str],
) -> tuple[str, list[str]]:
    document = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    if document.get("schema") != 1 or document.get("standards") != standards:
        raise ValueError(
            f"{manifest_path} must use schema 1 and standards {standards!r}"
        )
    if document.get("hash_algorithm") != "fnv1a64":
        raise ValueError(f"{manifest_path} uses an unsupported hash algorithm")
    files = document.get("files")
    if not isinstance(files, list):
        raise ValueError(f"{manifest_path} has no generated file entries")

    prefix = VALID_ADAPTERS[adapter]
    bundle_hashes: list[str] = []
    for skill in skills:
        skill_prefix = f"{prefix}/{skill}/"
        entries = sorted(
            (
                entry.get("path"),
                entry.get("hash"),
            )
            for entry in files
            if isinstance(entry, dict)
            and isinstance(entry.get("path"), str)
            and entry["path"].startswith(skill_prefix)
        )
        if not entries or not all(
            isinstance(path, str) and isinstance(file_hash, str)
            for path, file_hash in entries
        ):
            raise ValueError(
                f"{manifest_path} has no complete {adapter} entries for {skill!r}"
            )
        digest = hashlib.sha256()
        for path, file_hash in entries:
            relative = path.removeprefix(skill_prefix)
            digest.update(relative.encode("utf-8"))
            digest.update(b"\0")
            digest.update(file_hash.encode("ascii"))
            digest.update(b"\n")
        bundle_hashes.append(f"{skill}=sha256:{digest.hexdigest()}")

    manifest_hash = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    return f"sha256:{manifest_hash}", bundle_hashes


def adoption_toml(
    document: dict[str, Any],
    standards: str,
    source: str,
    adopted: str,
    manifest_hash: str,
    skill_hashes: list[str],
) -> str:
    text = (
        "schema = 1\n"
        f"standards = {quote(standards)}\n"
        f"source = {quote(source)}\n"
        f"adopted = {quote(adopted)}\n"
        f"base = {quote(document['base'])}\n"
        f"capabilities = {string_array(document['capabilities'])}\n"
        f"skills = {string_array(document['enabled_skills'])}\n"
        f"organization = {quote(document.get('organization', ''))}\n"
        f"adapter = {quote(document.get('adapter', 'common'))}\n"
        f"installation_mode = {quote(document['installation_mode'])}\n"
        f"migration = {quote(document.get('migration', 'new-adoption'))}\n"
        f"local_instruction_paths = {string_array(document['local_instruction_paths'])}\n"
        f"generated_manifest_hash = {quote(manifest_hash)}\n"
        'skill_hash_algorithm = "sha256-over-sorted-fnv1a64-manifest-entries"\n'
        f"skill_hashes = {string_array(skill_hashes)}\n"
    )
    for path, component in sorted(document["components"].items()):
        text += (
            f"\n[components.{quote(path)}]\n"
            f"base = {quote(component['base'])}\n"
            f"capabilities = {string_array(component['capabilities'])}\n"
            f"skills = {string_array(component['enabled_skills'])}\n"
            f"rationale = {quote(component['rationale'])}\n"
        )
    return text


def agents_text(document: dict[str, Any]) -> str:
    skills = ", ".join(f"`{skill}`" for skill in document["enabled_skills"])
    capabilities = ", ".join(document["capabilities"]) or "none"
    components = ", ".join(f"`{path}`" for path in sorted(document["components"]))
    return f"""# Repository Rust Engineering Contract

Repository purpose: {document.get("purpose", "See repository documentation.")}

Adopted Rust profile: `{document["base"]}` with capabilities: {capabilities}.
Enabled shared skills: {skills}.
Component overlays: {components or "none"}.

Read `.rust-skills/repository-profile.md`, `.rust-skills/enabled-skills.md`,
`.rust-skills/validation.md`, and the indexed decision files before material
Rust work. Run `scripts/validate-rust.py` from the repository root for the
declared validation workflow.

Apply current user instructions, then closest directory instructions,
repository decisions and mechanical configuration, confirmed profiles,
organization preferences, applicable skills, authoritative references,
approved guidance, examples, advisory material, and general knowledge.
Surface any apparent correctness, soundness, or safety conflict.
"""


def profile_text(document: dict[str, Any]) -> str:
    capabilities = "\n".join(f"- `{value}`" for value in document["capabilities"])
    components = []
    for path, component in sorted(document["components"].items()):
        component_capabilities = ", ".join(component["capabilities"]) or "none"
        components.append(
            f"- `{path}`: `{component['base']}` + {component_capabilities} — "
            f"{component['rationale']}"
        )
    component_text = "\n".join(components) or "- None"
    return f"""# Repository Profile

- Base: `{document["base"]}`
- Purpose: {document.get("purpose", "Not recorded.")}
- Consumers: {document.get("consumers", "Not recorded.")}
- Lifecycle: {document.get("lifecycle", "Not recorded.")}

## Capabilities

{capabilities or "- None"}

## Local profile rationale

{document.get("profile_rationale", "See approved onboarding record.")}

## Component overlays

{component_text}
"""


def enabled_skills_text(document: dict[str, Any]) -> str:
    return "# Enabled Skills\n\n" + "\n".join(
        f"- `{skill}`" for skill in document["enabled_skills"]
    ) + "\n"


def validation_markdown(checks: list[dict[str, Any]]) -> str:
    lines = [
        "# Rust Validation",
        "",
        "Run `scripts/validate-rust.py` from the repository root.",
        "",
        "| ID | Tier | Purpose |",
        "|---|---|---|",
    ]
    for check in checks:
        lines.append(
            f"| `{check['id']}` | {check['tier']} | {check.get('description', '')} |"
        )
        prerequisites = check.get("prerequisites", [])
        if prerequisites:
            lines.append(
                f"|  | prerequisites | {'; '.join(prerequisites)} |"
            )
    lines.extend(
        [
            "",
            "Unavailable required tools fail the workflow. Optional unavailable",
            "tools are reported as unavailable, never passed.",
        ]
    )
    return "\n".join(lines) + "\n"


def unresolved_text(values: list[str]) -> str:
    if not values:
        return "# Unresolved Decisions\n\nNone.\n"
    return "# Unresolved Decisions\n\n" + "\n".join(f"- {value}" for value in values) + "\n"


def plan_outputs(
    target: Path,
    document: dict[str, Any],
    standards: str,
    source: str,
    adopted: str,
    manifest_hash: str,
    skill_hashes: list[str],
    validator_source: Path,
) -> dict[Path, str | Path]:
    rust_root = target / ".rust-skills"
    outputs: dict[Path, str | Path] = {
        rust_root / "adoption.toml": adoption_toml(
            document,
            standards,
            source,
            adopted,
            manifest_hash,
            skill_hashes,
        ),
        rust_root / "repository-profile.md": profile_text(document),
        rust_root / "enabled-skills.md": enabled_skills_text(document),
        rust_root / "validation.toml": validation_toml(document["validation"]),
        rust_root / "validation.md": validation_markdown(document["validation"]),
        rust_root / "unresolved.md": unresolved_text(document["unresolved"]),
        target / "scripts" / "validate-rust.py": validator_source,
    }
    for identifier, content in sorted(document.get("decisions", {}).items()):
        outputs[rust_root / "decisions" / f"{identifier}.md"] = content.rstrip() + "\n"
    if document.get("agents_mode", "preserve") == "create":
        outputs[target / "AGENTS.md"] = agents_text(document)
    else:
        outputs[rust_root / "agents-proposal.md"] = agents_text(document)
    return outputs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--answers", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--standards", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--manifest")
    parser.add_argument("--write", action="store_true")
    arguments = parser.parse_args()

    answers_path = Path(arguments.answers).resolve()
    target = Path(arguments.target).resolve()
    if not target.is_dir():
        parser.error(f"{target} is not a directory")
    for label, value in (
        ("standards", arguments.standards),
        ("source", arguments.source),
    ):
        if not value.strip() or any(character in value for character in "*^~<>"):
            parser.error(f"{label} must be an exact non-range tag, version, or commit")
    try:
        document = json.loads(answers_path.read_text(encoding="utf-8"))
        if not isinstance(document, dict):
            raise ValueError("answers root must be an object")
        validate_answers(document)
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        print(f"invalid approved answers: {error}", file=sys.stderr)
        return 2

    validator_source = Path(__file__).resolve().parent.parent / "assets" / "validate-rust.py"
    manifest_path = (
        Path(arguments.manifest).resolve()
        if arguments.manifest
        else Path(__file__).resolve().parents[3] / "generated" / "manifest.toml"
    )
    try:
        manifest_hash, skill_hashes = manifest_provenance(
            manifest_path,
            arguments.standards,
            document.get("adapter", "common"),
            document["enabled_skills"],
        )
    except (OSError, UnicodeError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"invalid generated manifest: {error}", file=sys.stderr)
        return 2
    adopted = datetime.date.today().isoformat()
    outputs = plan_outputs(
        target,
        document,
        arguments.standards,
        arguments.source,
        adopted,
        manifest_hash,
        skill_hashes,
        validator_source,
    )
    existing = [path for path in outputs if path.exists()]
    if existing:
        print("refusing to overwrite existing adoption files:", file=sys.stderr)
        for path in existing:
            print(f"- {path}", file=sys.stderr)
        return 2
    for path in outputs:
        print(path)
    if not arguments.write:
        print("dry run only; pass --write after reviewing these targets")
        return 0

    for destination, content in outputs.items():
        destination.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, Path):
            shutil.copyfile(content, destination)
            destination.chmod(0o755)
        else:
            destination.write_text(content, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

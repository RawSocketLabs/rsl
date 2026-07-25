#!/usr/bin/env python3
"""Run repository-declared Rust checks without treating missing tools as passed."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError as error:  # pragma: no cover - version diagnostic
    raise SystemExit("Python 3.11 or newer is required (missing tomllib)") from error


VALID_TIERS = {"required", "optional"}
VALID_APPLICABILITY = {"applicable", "inapplicable"}


def tool_available(command: str, root: Path) -> bool:
    candidate = Path(command)
    if candidate.is_absolute():
        return candidate.is_file()
    if "/" in command:
        return (root / candidate).is_file()
    return shutil.which(command) is not None


def load_checks(config: Path) -> list[dict[str, Any]]:
    document = tomllib.loads(config.read_text(encoding="utf-8"))
    if document.get("schema") != 1:
        raise ValueError(f"{config} must use schema 1")
    checks = document.get("checks")
    if not isinstance(checks, list) or not checks:
        raise ValueError(f"{config} must define at least one [[checks]] entry")
    seen: set[str] = set()
    for check in checks:
        identifier = check.get("id")
        tier = check.get("tier")
        command = check.get("command")
        if not isinstance(identifier, str) or not identifier:
            raise ValueError("every check requires a non-empty id")
        if identifier in seen:
            raise ValueError(f"duplicate check id {identifier!r}")
        seen.add(identifier)
        if tier not in VALID_TIERS:
            raise ValueError(f"check {identifier!r} has invalid tier {tier!r}")
        if not isinstance(command, list) or not command or not all(
            isinstance(part, str) and part for part in command
        ):
            raise ValueError(f"check {identifier!r} requires a command string array")
        for field in ("prerequisites", "required_tools"):
            values = check.get(field, [])
            if not isinstance(values, list) or not all(
                isinstance(item, str) and item.strip() for item in values
            ):
                raise ValueError(
                    f"check {identifier!r} requires {field} strings"
                )
        applicability = check.get("applicability", "applicable")
        if applicability not in VALID_APPLICABILITY:
            raise ValueError(
                f"check {identifier!r} has invalid applicability {applicability!r}"
            )
        if tier == "required" and check.get("enabled") is False:
            raise ValueError(f"required check {identifier!r} cannot be disabled")
        timeout = check.get("timeout_seconds", 900)
        if not isinstance(timeout, int) or timeout <= 0:
            raise ValueError(f"check {identifier!r} has invalid timeout")
    return checks


def run_check(check: dict[str, Any], root: Path) -> dict[str, Any]:
    identifier = check["id"]
    result: dict[str, Any] = {
        "id": identifier,
        "tier": check["tier"],
        "description": check.get("description", ""),
        "command": check["command"],
        "prerequisites": check.get("prerequisites", []),
        "required_tools": check.get("required_tools", []),
    }
    applicability = check.get("applicability", "applicable")
    if applicability == "inapplicable":
        result["status"] = "inapplicable"
        result["reason"] = check.get("reason", "repository configuration")
        return result
    if not check.get("enabled", True):
        result["status"] = "skipped"
        result["reason"] = check.get("reason", "disabled by repository configuration")
        return result

    command = check["command"]
    unavailable = [
        tool
        for tool in [command[0], *check.get("required_tools", [])]
        if not tool_available(tool, root)
    ]
    if unavailable:
        result["status"] = "unavailable"
        result["reason"] = f"command not found: {', '.join(unavailable)}"
        return result

    working_directory = (root / check.get("working_directory", ".")).resolve()
    if root not in (working_directory, *working_directory.parents):
        result["status"] = "failed"
        result["reason"] = "working directory escapes repository root"
        return result
    if not working_directory.is_dir():
        result["status"] = "failed"
        result["reason"] = f"working directory does not exist: {working_directory}"
        return result
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=working_directory,
            text=True,
            capture_output=True,
            timeout=check.get("timeout_seconds", 900),
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        result.update(
            {
                "status": "failed",
                "reason": "timeout",
                "elapsed_seconds": time.monotonic() - started,
                "stdout": error.stdout or "",
                "stderr": error.stderr or "",
            }
        )
        return result
    result.update(
        {
            "status": "passed" if completed.returncode == 0 else "failed",
            "returncode": completed.returncode,
            "elapsed_seconds": time.monotonic() - started,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--config", default=".rust-skills/validation.toml")
    parser.add_argument("--json", action="store_true")
    arguments = parser.parse_args()

    root = Path(arguments.root).resolve()
    config = root / arguments.config
    try:
        checks = load_checks(config)
    except (OSError, UnicodeError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"validation configuration error: {error}", file=sys.stderr)
        return 2

    results = [run_check(check, root) for check in checks]
    failed = [
        result
        for result in results
        if result["tier"] == "required"
        and result["status"] in {"failed", "skipped", "unavailable"}
    ]
    if arguments.json:
        json.dump(
            {"schema": 1, "root": str(root), "results": results, "success": not failed},
            sys.stdout,
            indent=2,
            sort_keys=True,
        )
        sys.stdout.write("\n")
    else:
        for result in results:
            print(f"[{result['status'].upper():12}] {result['tier']:8} {result['id']}")
            if result.get("reason"):
                print(f"  {result['reason']}")
            if result["status"] == "failed":
                if result.get("stdout"):
                    print(result["stdout"], end="" if result["stdout"].endswith("\n") else "\n")
                if result.get("stderr"):
                    print(result["stderr"], file=sys.stderr, end="" if result["stderr"].endswith("\n") else "\n")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())

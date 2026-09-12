#!/usr/bin/env python3
"""Translate a GitHub/act event into the same explicit-base local CI plan."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

from gate import validate_plan
from plan import build_plan, changed_paths, command, is_prose, release_policy


def is_release_pr(pr, repository, prefix):
    return (pr["head"]["ref"].startswith(prefix)
            and pr["head"]["repo"] is not None
            and pr["head"]["repo"]["full_name"] == repository
            and pr["base"]["repo"]["full_name"] == repository
            and pr["base"]["ref"] == "main")


def associated_release(root, repository, sha, prefix):
    pages = json.loads(command(root, "gh", "api", "--paginate", "--slurp",
                               f"repos/{repository}/commits/{sha}/pulls"))
    return any(is_release_merge(pr, repository, sha, prefix)
               for page in pages for pr in page)


def is_release_merge(pr, repository, sha, prefix):
    return bool(pr["merged_at"] and pr["merge_commit_sha"] == sha
                and is_release_pr(pr, repository, prefix))


def event_options(root, event, name, *, base, full, release, repository, prefix):
    candidate = False
    if name == "pull_request":
        pr = event["pull_request"]
        # A stale PR is tested as a merge, but select all changes since its merge base.
        result = subprocess.run(["git", "merge-base", pr["base"]["sha"], pr["head"]["sha"]],
                                cwd=root, stdout=subprocess.PIPE)
        base = result.stdout.decode().strip() if result.returncode == 0 else ""
        release |= is_release_pr(pr, repository, prefix)
    elif name == "push":
        if event["ref"] != "refs/heads/main":
            raise ValueError("push CI is only configured for main")
        base = event["before"]
        candidate = associated_release(root, repository, event["after"], prefix)
        release |= candidate
    elif name == "schedule":
        full = True
    elif name != "workflow_dispatch":
        raise ValueError(f"unsupported CI event: {name}")
    return base, full or not base, release, candidate


def emit_output(name, value):
    encoded = value if isinstance(value, str) else json.dumps(value, separators=(",", ":"))
    if "\n" in encoded or "\r" in encoded:
        raise ValueError("GitHub output must occupy one line")
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"{name}={encoded}\n")


def planning_inputs():
    root = Path.cwd()
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    base, full, release, candidate = event_options(
        root, event, os.environ["GITHUB_EVENT_NAME"],
        base=os.environ.get("INPUT_BASE", ""), full=os.environ.get("INPUT_FULL") == "true",
        release=os.environ.get("INPUT_RELEASE") == "true",
        repository=os.environ["GITHUB_REPOSITORY"], prefix=release_policy(root))
    return root, base, full, release, candidate


def needs_cargo(root, base, full, release):
    if full or release or not base:
        return True
    try:
        return not all(is_prose(path) for path in changed_paths(root, base, "HEAD"))
    except subprocess.CalledProcessError:
        return True  # Missing history expands to full coverage, including metadata setup.


def plan_for_event():
    root, base, full, release, candidate = planning_inputs()
    plan = build_plan(root, base=base, full=full, release=release)
    plan["release_candidate"] = candidate
    if plan["head"] != os.environ["GITHUB_SHA"]:
        raise ValueError("checkout does not match the workflow's tested SHA")
    validate_plan(plan)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true", help="decide whether Cargo setup is needed, without Cargo")
    args = parser.parse_args()
    if args.probe:
        root, base, full, release, _ = planning_inputs()
        emit_output("needs_cargo", needs_cargo(root, base, full, release))
        return
    plan = plan_for_event()
    output = Path(os.environ["RUNNER_TEMP"]) / "ci-plan.json"
    output.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    bnb = tomllib.loads(Path(".github/ci/bnb.toml").read_text())
    checks = [{"profile": profile,
               "toolchain": ("1.85.0" if profile == "msrv" else
                             bnb["public_api_toolchain"] if profile == "public-api" else "stable")}
              for profile in plan["profiles"]]
    emit_output("plan", plan)
    emit_output("checks", {"include": checks or [{"profile": "none", "toolchain": "stable"}]})
    emit_output("fuzz", {"include": plan["fuzz"] or [{"root": "none", "target": "none", "args": []}]})
    emit_output("has_checks", bool(checks))
    emit_output("has_fuzz", bool(plan["fuzz"]))
    emit_output("release", plan["release"])
    emit_output("public_api_tool", "cargo-public-api@" + bnb["public_api_version"])
    emit_output("semver_tool", "cargo-semver-checks@" + bnb["semver_version"])
    print(json.dumps(plan, indent=2))
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write("## CI coverage\n\n")
            summary.write(f"Mode: {'full' if plan['full'] else 'affected'}; "
                          f"release qualification: {plan['release']}.\n\n")
            summary.write(f"{len(plan['packages'])} workspace packages, {len(checks)} check profiles, "
                          f"{len(plan['fuzz'])} fuzz targets.\n\n")
            summary.write("Selection reasons are in the plan step and exact-run coverage artifact.\n")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"CI event planning failed: {error}", file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Validate selected CI jobs and exact-run release qualification, failing closed."""

import argparse
import json
import os
from pathlib import Path
import re
import sys

from plan import FUZZ, PROFILES, SCHEMA, command, metadata_graph, release_policy, select


def validate_plan(plan):
    if plan["schema"] != SCHEMA:
        raise ValueError("unsupported coverage schema")
    if not re.fullmatch(r"[0-9a-f]{40}", plan["head"]):
        raise ValueError("coverage must identify the tested commit")
    if type(plan["full"]) is not bool or type(plan["release"]) is not bool:
        raise ValueError("invalid coverage mode")
    if type(plan["release_candidate"]) is not bool or (plan["release_candidate"] and not plan["release"]):
        raise ValueError("invalid release candidate qualification")
    if not plan["full"] and not re.fullmatch(r"[0-9a-f]{40}", plan["base"] or ""):
        raise ValueError("scoped coverage requires a comparison base")
    profiles = plan["profiles"]
    if profiles != sorted(set(profiles)) or not set(profiles) <= PROFILES:
        raise ValueError("invalid validation profiles")
    if plan["roots"] and not profiles:
        raise ValueError("affected packages have no validation")
    if plan["packages"] and not {"workspace", "msrv"} <= set(profiles):
        raise ValueError("workspace packages require tests, lint, and MSRV")
    seen = set()
    for item in plan["fuzz"]:
        key = (item["root"], item["target"])
        if (item["root"] not in FUZZ or item["args"] != FUZZ[item["root"]]
                or not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_-]*", item["target"])
                or key in seen):
            raise ValueError("invalid fuzz selection or budget")
        seen.add(key)


def check_results(plan, needs):
    validate_plan(plan)
    expected = {"plan": True, "policy": True,
                "checks": bool(plan["profiles"]), "fuzz": bool(plan["fuzz"])}
    if set(needs) != set(expected):
        raise ValueError("missing or unexpected CI dependency")
    for job, selected in expected.items():
        required = "success" if selected else "skipped"
        if needs[job]["result"] != required:
            raise ValueError(f"{job}: expected {required}, got {needs[job]['result']}")


def check_plan(actual, expected):
    if actual != expected:
        raise ValueError("coverage differs from the plan recomputed from the event and immutable Git trees")


def check_release(report, *, sha, run_id, attempt, graph):
    if (report["head"] != sha or report["run_id"] != run_id
            or report["run_attempt"] != attempt or report["plan"]["head"] != sha):
        raise ValueError("coverage belongs to another commit, run, or attempt")
    check_results(report["plan"], report["needs"])
    actual = report["plan"]
    if report["event"] != "push" or not actual["release_candidate"] or not actual["release"]:
        raise ValueError("CI did not qualify a merged release candidate")
    required = select([], graph, graph, release=True)
    for key in ("profiles", "roots", "packages"):
        if not set(required[key]) <= set(actual[key]):
            raise ValueError(f"release coverage is missing required {key}")
    selected_fuzz = {(item["root"], item["target"]) for item in actual["fuzz"]}
    if not {(item["root"], item["target"]) for item in required["fuzz"]} <= selected_fuzz:
        raise ValueError("release coverage is missing downstream fuzz targets")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("ci", "release", "candidate"))
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if args.mode == "candidate":
        from prepare import associated_release, emit_output

        sha = os.environ["CANDIDATE_SHA"]
        root = Path.cwd()
        if command(root, "git", "rev-parse", "HEAD").decode().strip() != sha:
            raise ValueError("candidate checkout is not the verified CI commit")
        candidate = associated_release(root, os.environ["GITHUB_REPOSITORY"], sha, release_policy(root))
        emit_output("publish", candidate)
        print("Exact merged release candidate" if candidate else "No publish operation for this commit")
        return
    if args.report is None:
        parser.error("--report is required for ci/release qualification")
    if args.mode == "ci":
        from prepare import plan_for_event

        plan = json.loads(os.environ["CI_PLAN"])
        needs = json.loads(os.environ["CI_NEEDS"])
        check_plan(plan, plan_for_event())
        check_results(plan, needs)
        report = {"schema": SCHEMA, "head": plan["head"], "plan": plan, "needs": needs,
                  "event": os.environ["GITHUB_EVENT_NAME"],
                  "run_id": int(os.environ["GITHUB_RUN_ID"]),
                  "run_attempt": int(os.environ.get("GITHUB_RUN_ATTEMPT", "1"))}
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        print(f"Passed {len(plan['profiles'])} check profiles and {len(plan['fuzz'])} fuzz targets")
    else:
        check_release(json.loads(args.report.read_text()), sha=os.environ["CANDIDATE_SHA"],
                      run_id=int(os.environ["CI_RUN_ID"]), attempt=int(os.environ["CI_RUN_ATTEMPT"]),
                      graph=metadata_graph(Path.cwd()))
        print("Exact-run release qualification verified")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, TypeError, OSError) as error:
        print(f"CI qualification failed: {error}", file=sys.stderr)
        sys.exit(1)

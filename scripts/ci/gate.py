#!/usr/bin/env python3
"""Validate selected CI jobs and exact-run release qualification, failing closed."""

import argparse
import json
import os
from pathlib import Path
import re
import sys

from plan import (FUZZ, PROFILES, RELEASE_PROFILES, SCHEMA, graph_at, handed_on,
                  metadata_graph, release_config, released_members, select)


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
    if plan["packages"] and "workspace" not in profiles:
        raise ValueError("workspace packages require tests and lint")
    if plan["products"] != sorted(set(plan["products"])) or bool(plan["products"]) != plan["release"]:
        raise ValueError("invalid release products")
    if bool(plan["release_members"]) != plan["release"]:
        raise ValueError("invalid release members")
    if sorted({member["product"] for member in plan["release_members"]}) != plan["products"]:
        raise ValueError("release members differ from selected products")
    if not plan["release"] and (plan["release_roots"] or plan["release_packages"]
                               or plan["fuzz"] or set(profiles) & RELEASE_PROFILES):
        raise ValueError("heavy qualification requires a release product")
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


def coverage_report(handed, needs, recomputed, *, event, run_id, attempt):
    """The exact-run coverage receipt: the jobs' plan must equal the recomputed one as handed
    on, every selected job must have succeeded, and the receipt keeps the recomputed plan with
    the per-path detail the jobs were not handed."""
    check_plan(handed, handed_on(recomputed))
    check_results(handed, needs)
    return {"schema": SCHEMA, "head": recomputed["head"], "plan": recomputed, "needs": needs,
            "event": event, "run_id": run_id, "run_attempt": attempt}


def check_release(report, *, sha, run_id, attempt, graph, members):
    if (report["head"] != sha or report["run_id"] != run_id
            or report["run_attempt"] != attempt or report["plan"]["head"] != sha):
        raise ValueError("coverage belongs to another commit, run, or attempt")
    check_results(report["plan"], report["needs"])
    actual = report["plan"]
    if report["event"] != "push" or not actual["release_candidate"] or not actual["release"]:
        raise ValueError("CI did not qualify a merged release candidate")
    required = select([], graph, graph, release_members=members)
    for key in ("products", "release_members", "release_roots", "release_packages", "fuzz"):
        if required[key] != actual[key]:
            raise ValueError(f"release coverage differs from required {key}")
    for key in ("profiles", "roots", "packages"):
        if not set(required[key]) <= set(actual[key]):
            raise ValueError(f"release coverage is missing required {key}")
    selected_fuzz = {(item["root"], item["target"]) for item in actual["fuzz"]}
    if not {(item["root"], item["target"]) for item in required["fuzz"]} <= selected_fuzz:
        raise ValueError("release coverage is missing downstream fuzz targets")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("release",))
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    root = Path.cwd()
    graph = metadata_graph(root)
    sha = os.environ["CANDIDATE_SHA"]
    # Publication checks the actual merged commit's parent, never a receipt-supplied base.
    members = released_members(release_config(root, sha),
                               graph_at(root, sha + "^", historical=True), graph)
    check_release(json.loads(args.report.read_text()), sha=sha,
                  run_id=os.environ["CI_RUN_ID"], attempt=int(os.environ["CI_RUN_ATTEMPT"]),
                  graph=graph, members=members)
    print("Exact-run release qualification verified")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, TypeError, OSError) as error:
        print(f"CI qualification failed: {error}", file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Plan a CircleCI pipeline and continue it with only the selected jobs.

The static `.circleci/config.yml` is a setup workflow: it runs `plan`, which selects coverage
with the same `plan.py` contract as the Actions workflow and continues the pipeline with one
job per selected check profile and fuzz target. Unselected work is never scheduled. Every
generated job runs concurrently after `policy`; the final `CI` job recomputes the plan and
requires the handed-on plan to match before it writes the coverage receipt.

Publication stays with the Actions workflow: this adapter never marks a release candidate.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
import urllib.request

from gate import coverage_report, validate_plan
from plan import build_plan, changed_paths, handed_on, is_prose, release_policy

TRUNK = "main"
IMAGE = "ubuntu-2404:current"
# The Actions runner equivalent: 4 vCPU, 15 GB.
DEFAULT_CLASS = "large"
SETUP = "bash scripts/ci/circleci-setup.sh"
# trybuild panic snapshots need rust-src. Never silently switch their compiler.
RUST_SRC = {"workspace", "bnb", "socks", "usdr-oracle", "usdr-portable", "xsdr-portable",
            "xsdr-oracle", "lsdr-portable", "lsdr-oracle", "pe-sync-oracle", "pe-sync-portable",
            "limesdr-oracle", "limesdr-portable", "rust-skills", "bladerf"}
NATIVE_COMPILER = {"usdr-ffi", "rust-dsdcc", "usdr-oracle", "xsdr-oracle", "lsdr-oracle",
                   "pe-sync-oracle", "limesdr-oracle"}
NO_RUST = {"deny", "blessed"}
DENY_VERSION = "0.20.2"
FUZZ_VERSION = "0.13.2"
ACTIONLINT = "rhysd/actionlint:1.7.12"
# Cargo tools live in their own per-version roots (circleci-setup.sh), never in ~/.cargo/bin.
CACHE_PATHS = ["~/.cargo/registry/index", "~/.cargo/registry/cache", "~/.cargo/git/db", "target"]
DETACHED_TARGETS = [
    "codecs/bitsandbytes/bnb/nostd-check", "tools/rust-skills", "bindings/usdr", "bindings/dsdcc",
    "bindings/bladerf", "devices/sdr/usdr/oracle", "devices/sdr/usdr/nostd-check",
    "devices/sdr/xsdr/oracle", "devices/sdr/xsdr/nostd-check", "devices/sdr/lsdr/oracle",
    "devices/sdr/lsdr/nostd-check", "devices/sync/pe_sync/oracle", "devices/sync/pe_sync/nostd-check",
    "devices/sdr/limesdr/oracle", "devices/sdr/limesdr/nostd-check",
]
# Pipeline inputs the setup job receives; generated jobs get the same literal values so the
# gate's recomputation sees the event the plan saw.
INPUTS = ("CI_INPUT_FULL", "CI_INPUT_BASE", "CI_INPUT_RELEASE", "CI_INPUT_PRODUCTS",
          "CI_INPUT_CLASS", "CI_TRIGGER_SOURCE", "CI_BASE_REVISION")
# Declared again in the continuation: trigger-time parameters carry over into it.
PARAMETERS = {"full": {"type": "boolean", "default": False}, "base": {"type": "string", "default": ""},
              "release": {"type": "boolean", "default": False},
              "products": {"type": "string", "default": ""},
              "resource_class": {"type": "string", "default": DEFAULT_CLASS}}


def merge_base(root, revision):
    result = subprocess.run(["git", "merge-base", revision, "HEAD"], cwd=root,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.decode().strip() if result.returncode == 0 else ""


def event_options(root, env, prefix):
    """(event, base, full, release, products) in the Actions vocabulary `gate.py` records.

    Explicit pipeline parameters are a manual dispatch; a schedule is a full run; the trunk is
    a push compared against the previous pipeline's revision; any other branch is compared
    from its merge base with the trunk, as a pull request would be."""
    base = env.get("CI_INPUT_BASE", "")
    if not re.fullmatch(r"(?:[A-Za-z0-9._][A-Za-z0-9._/-]*)?", base):
        raise ValueError("comparison base is not a Git revision")
    full = env.get("CI_INPUT_FULL") == "true"
    release = env.get("CI_INPUT_RELEASE") == "true"
    products = env.get("CI_INPUT_PRODUCTS", "").split()
    branch = env.get("CIRCLE_BRANCH", "")
    if env.get("CI_TRIGGER_SOURCE") == "scheduled_pipeline":
        event, full = "schedule", True
    elif full or release or products or base:
        event = "workflow_dispatch"
    elif branch == TRUNK:
        event, base = "push", env.get("CI_BASE_REVISION", "")
    elif branch:
        # The plan job pins the merge base so the gate cannot see a trunk that moved since.
        event, base = "pull_request", env.get("CI_MERGE_BASE") or merge_base(root, f"origin/{TRUNK}")
        if base == env.get("CIRCLE_SHA1"):
            base = ""  # A branch at a trunk commit has no diff; never post an empty green there.
        # Fork pull requests set CIRCLE_PR_USERNAME; only same-repository release branches qualify.
        release |= branch.startswith(prefix) and not env.get("CIRCLE_PR_USERNAME")
    else:
        raise ValueError("CircleCI CI is only configured for branch pipelines")
    if products and event != "workflow_dispatch":
        raise ValueError("explicit products are only allowed for manual qualification")
    return event, base, full or not base, release, products or None


def needs_cargo(root, env):
    _, base, full, release, _ = event_options(root, env, release_policy(root))
    if full or release:
        return True
    try:
        return not all(is_prose(path) for path in changed_paths(root, base, "HEAD"))
    except subprocess.CalledProcessError:
        return True  # Missing history expands to full coverage, including metadata setup.


def plan_for_event(root, env):
    event, base, full, release, products = event_options(root, env, release_policy(root))
    plan = build_plan(root, base=base, full=full, release=release, products=products)
    plan["release_candidate"] = False
    if plan["head"] != env["CIRCLE_SHA1"]:
        raise ValueError("checkout does not match the pipeline's tested SHA")
    validate_plan(plan)
    return event, plan


def executor(resource_class):
    if "/" in resource_class:  # A self-hosted runner class, provisioned with native libraries.
        return {"machine": True, "resource_class": resource_class}
    return {"machine": {"image": IMAGE}, "resource_class": resource_class}


def run(name, command, **extra):
    return {"run": {"name": name, "command": command, **extra}}


def cached(key, paths, steps, *, save, lock=True):
    exact = key + '-{{ checksum "Cargo.lock" }}' if lock else key
    restore = {"restore_cache": {"keys": [exact, key + "-"] if lock else [exact]}}
    result = [restore, *steps]
    # Only trunk pipelines write caches. CircleCI does not scope caches by branch, so a branch
    # that edits this generator could still write one: keep that in mind before requiring CI.
    if save:
        result.append({"save_cache": {"key": exact + "-{{ epoch }}" if lock else exact,
                                      "paths": paths, "when": "on_success"}})
    return result


def tool(crate, version, *, save):
    root = f"~/ci-tools/{crate}-{version}"
    return cached(f"v1-tool-{crate}-{version}", [root],
                  [run(f"Install {crate}", f"{SETUP} tool {crate} {version}")], save=save, lock=False)


def slug(text):
    return re.sub(r"[^A-Za-z0-9_-]+", "-", text).strip("-")


def check_job(profile, toolchain, plan, bnb, *, hosted, save):
    steps = []
    if profile != "blessed":
        components = "rustfmt,clippy,rust-src" if profile in RUST_SRC else ""
        targets = "thumbv7em-none-eabi" if profile == "no-std" else ""
        steps.append(run("Install Rust", f"{SETUP} rust {toolchain} '{components}' '{targets}'"))
    if profile == "rust-skills" and any(root.startswith("tools/rust-skills/") for root in plan["release_roots"]):
        steps.append(run("Install the MSRV toolchain", f"{SETUP} rust 1.85.0"))
    if profile == "public-api":
        steps.extend(tool("cargo-public-api", bnb["public_api_version"], save=save))
    if profile == "semver":
        steps.extend(tool("cargo-semver-checks", bnb["semver_version"], save=save))
    if profile == "deny":
        steps.extend(tool("cargo-deny", DENY_VERSION, save=save))
    if profile in NATIVE_COMPILER and hosted:
        steps.append(run("Native compiler for FFI bridge checks", f"{SETUP} apt build-essential"))
    if profile == "rust-dsdcc":
        steps.append(run("Fetch DSDcc headers (compile check only, no hardware/native linking)",
                         'rm -rf "$DSDCC_INCLUDE" && '
                         'git clone --depth 1 https://github.com/f4exb/dsdcc.git "$DSDCC_INCLUDE"'))
    if profile == "bladerf" and hosted:
        steps.append(run("Native development libraries for bladeRF bindings",
                         f"{SETUP} apt libbladerf-dev libclang-dev"))
    if profile == "deny":
        steps.append(run("cargo deny", "cargo deny --all-features check"))
    else:
        # Actions' timeout-minutes bounds the whole profile, not only a silent stretch.
        steps.append(run("Run the complete selected profile", 'timeout 30m python3 scripts/ci/run.py "$PROFILE"',
                         no_output_timeout="30m"))
    if profile in NO_RUST:
        return steps
    return cached(f"v1-checks-{profile}-{toolchain}", CACHE_PATHS + [f"{root}/target" for root in DETACHED_TARGETS],
                  steps, save=save)


def fuzz_job(item, *, save):
    steps = [run("Install Rust", f"{SETUP} rust nightly"),
             *tool("cargo-fuzz", FUZZ_VERSION, save=save),
             # Separate processes preserve each target's original budget. GNU is required for ASan.
             run("Fuzz", "timeout 15m python3 scripts/ci/run.py fuzz", no_output_timeout="15m")]
    return cached(f"v1-fuzz-{slug(item['root'])}-{item['target']}",
                  CACHE_PATHS[:-1] + [f"{item['root']}/target"], steps, save=save)


def continuation(plan, bnb, env):
    """The continued pipeline: policy, the selected checks and fuzz targets, then the gate."""
    resource_class = env.get("CI_INPUT_CLASS") or DEFAULT_CLASS
    hosted = "/" not in resource_class
    save = env.get("CIRCLE_BRANCH") == TRUNK
    handed = json.dumps(handed_on(plan), separators=(",", ":"), sort_keys=True)
    common = {"CI_PLAN": handed, "CI_TEMP": "/tmp/ci", "RUNNER_TEMP": "/tmp/ci",
              "CARGO_TERM_COLOR": "always", "CI_MERGE_BASE": plan["base"] or "",
              **{name: env.get(name, "") for name in INPUTS}}
    checkout = ["checkout", run("Prepare", 'mkdir -p "$CI_TEMP"')]

    def job(steps, *, resource=resource_class, **environment):
        return {**executor(resource), "environment": {**common, **environment},
                "steps": [*checkout, *steps]}

    jobs = {"policy": job([
        # Fast tier has no Cargo invocation, builds, registry access, or extra Python dependencies.
        run("CI tooling tests", "python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v"),
        run("Shell syntax", "bash -n scripts/ci-act.sh scripts/ci/circleci-setup.sh"),
        run("Actions workflow syntax", f'docker run --rm -v "$PWD:/repo" -w /repo {ACTIONLINT}'),
    ], resource="medium")}
    selected = []
    for profile in plan["profiles"]:
        toolchain = ("1.85.0" if profile == "msrv" else
                     bnb["public_api_toolchain"] if profile == "public-api" else "stable")
        environment = {"PROFILE": profile, "DSDCC_INCLUDE": "/tmp/ci/dsdcc"}
        if profile not in NO_RUST:
            environment["RUSTUP_TOOLCHAIN"] = toolchain
        name = f"checks-{profile}"
        jobs[name] = job(check_job(profile, toolchain, plan, bnb, hosted=hosted, save=save), **environment)
        selected.append(name)
    for item in plan["fuzz"]:
        name = f"fuzz-{slug(item['root'])}-{item['target']}"
        jobs[name] = job(fuzz_job(item, save=save), RUSTUP_TOOLCHAIN="nightly",
                         FUZZ_ITEM=json.dumps(item, separators=(",", ":")))
        selected.append(name)
    needs = {"checks": "success" if plan["profiles"] else "skipped",
             "fuzz": "success" if plan["fuzz"] else "skipped"}
    jobs["CI"] = job([
        run("Fetch the trunk", f"git fetch --no-tags origin +refs/heads/{TRUNK}:refs/remotes/origin/{TRUNK}"),
        run("Install Rust when the plan needs Cargo metadata",
            f'needs=$(python3 scripts/ci/circleci.py probe); if [ "$needs" = true ]; then {SETUP} rust stable; fi'),
        run("Recompute coverage and reject any divergence", "python3 scripts/ci/circleci.py gate"),
        {"store_artifacts": {"path": "/tmp/ci/ci-coverage.json", "destination": "ci-coverage.json"}},
    ], resource="medium", CI_NEEDS=json.dumps(needs, sort_keys=True))
    workflow = [{"policy": {}},
                *({name: {"requires": ["policy"]}} for name in selected),
                {"CI": {"requires": ["policy", *selected]}}]
    return {"version": 2.1, "parameters": PARAMETERS, "jobs": jobs, "workflows": {"ci": {"jobs": workflow}}}


def continue_pipeline(configuration):
    body = json.dumps({"continuation-key": os.environ["CIRCLE_CONTINUATION_KEY"],
                       "configuration": configuration}).encode()
    host = os.environ.get("CIRCLECI_HOST", "https://circleci.com")
    request = urllib.request.Request(f"{host}/api/v2/pipeline/continue", data=body, method="POST",
                                     headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=60) as response:
        print(response.read().decode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("probe", "plan", "gate"))
    parser.add_argument("--dry-run", action="store_true", help="write the continuation without continuing")
    args = parser.parse_args()
    root, env = Path.cwd(), dict(os.environ)
    temp = Path(env.get("CI_TEMP", "/tmp/ci"))
    if args.mode == "probe":
        print("true" if needs_cargo(root, env) else "false")
        return
    if args.mode == "plan":
        _, plan = plan_for_event(root, env)
        temp.mkdir(parents=True, exist_ok=True)
        (temp / "ci-plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
        print(json.dumps(plan, indent=2))
        bnb = tomllib.loads(Path(".github/ci/bnb.toml").read_text())
        configuration = json.dumps(continuation(plan, bnb, env), indent=1)
        (temp / "continuation.json").write_text(configuration + "\n")
        print(f"Selected {len(plan['profiles'])} check profiles and {len(plan['fuzz'])} fuzz targets")
        if not args.dry_run:
            continue_pipeline(configuration)
        return
    event, recomputed = plan_for_event(root, env)
    needs = {"plan": {"result": "success"}, "policy": {"result": "success"},
             **{job: {"result": result} for job, result in json.loads(env["CI_NEEDS"]).items()}}
    report = coverage_report(json.loads(env["CI_PLAN"]), needs, recomputed, event=event,
                             run_id=env["CIRCLE_WORKFLOW_ID"], attempt=1)
    (temp / "ci-coverage.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"Passed {len(recomputed['profiles'])} check profiles and {len(recomputed['fuzz'])} fuzz targets")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"CircleCI planning failed: {error}", file=sys.stderr)
        sys.exit(1)

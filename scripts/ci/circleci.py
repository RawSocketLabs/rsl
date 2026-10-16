#!/usr/bin/env python3
"""Plan a CircleCI pipeline and continue it with only the selected jobs.

The static `.circleci/config.yml` is a setup workflow: its `plan` job selects coverage with
`plan.py` and continues the pipeline with one job per selected check profile and fuzz target.
Unselected work is never scheduled. Every generated job runs concurrently after `policy`; the
final `CI` job recomputes the plan and requires the handed-on plan to match before it writes
the coverage receipt. On the trunk, `release` publishes a qualified release-plz merge and
`release-pr` refreshes the release PR, both serialised and holding the only secrets.
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
from plan import build_plan, handed_on, release_policy

TRUNK = "main"
REPOSITORY = "RawSocketLabs/rsl"
# The job image (.circleci/image/Dockerfile), by digest. Its toolchains and tools are the only
# ones jobs use: nothing is installed at run time. test_circleci.py holds these pins in step.
IMAGE = "ghcr.io/rawsocketlabs/rsl-ci@sha256:232f958976b7cd76da1dd54f4ee7f95e2376feb538cd909cb2cb38bd9590cee9"
# The upstream commitlint image (21.2.3), which carries @commitlint/config-conventional.
COMMITLINT = ("docker.io/commitlint/commitlint@"
              "sha256:33f225c84111a38ebac0c81fa21c488a79709b8a8e04056dfaaf4f80ef20be25")
STABLE = "1.99.0"
MSRV = "1.85.0"
# 4 vCPU, 8 GB: the heaviest trial job peaked at 1.5 GiB and saturated its CPUs.
DEFAULT_CLASS = "large"
NO_RUST = {"deny", "blessed"}
# Every job's budget: a slow job fails rather than stretching the pipeline past it.
BUDGET = "10m"
# nextest's `ci` profile (.config/nextest.toml) writes JUnit here for CircleCI's test insights.
JUNIT = "target/nextest/ci/junit.xml"
# The critical path; its parts run on separate machines (run.py workspace_parts).
WORKSPACE_PARTS = ("lint", "test", "extra")
CACHE_PATHS = ["~/.cargo/registry/index", "~/.cargo/registry/cache", "~/.cargo/git/db", "target"]
DETACHED_TARGETS = [
    "codecs/bitsandbytes/bnb/nostd-check", "tools/rust-skills", "bindings/usdr", "bindings/dsdcc",
    "bindings/bladerf", "devices/sdr/usdr/oracle", "devices/sdr/usdr/nostd-check",
    "devices/sdr/xsdr/oracle", "devices/sdr/xsdr/nostd-check", "devices/sdr/lsdr/oracle",
    "devices/sdr/lsdr/nostd-check", "devices/sync/pe_sync/oracle", "devices/sync/pe_sync/nostd-check",
    "devices/sdr/limesdr/oracle", "devices/sdr/limesdr/nostd-check",
]
IMAGE_SOURCE = ".circleci/image/"
# Contexts hold the only secrets, each restricted to trunk with SSH reruns denied: `github`
# holds RELEASE_PLZ_TOKEN and `crates` CARGO_REGISTRY_TOKEN (this project only), and `images`
# the GHCR push credential (shared with libsdr). Each job gets only the contexts it uses.
GITHUB_CONTEXT = "github"
CRATES_CONTEXT = "crates"
IMAGE_CONTEXT = "images"
RELEASE_GROUP = "<< pipeline.project.slug >>/release"
# The gate's job name is the required status. A manual run with an explicit `base` may cover less
# than the commit's own diff, so its gate reports under another name and cannot satisfy it.
GATE, SCOPED_GATE = "rsl", "rsl-scoped"
# Pipeline inputs the setup job receives; generated jobs get the same literal values so the
# gate's recomputation sees the event the plan saw.
INPUTS = ("CI_INPUT_FULL", "CI_INPUT_BASE", "CI_INPUT_RELEASE", "CI_INPUT_PRODUCTS",
          "CI_INPUT_CLASS", "CI_INPUT_AUDIT", "CI_TRIGGER_SOURCE", "CI_BASE_REVISION")
# Declared again in the continuation: trigger-time parameters carry over into it.
PARAMETERS = {"full": {"type": "boolean", "default": False}, "base": {"type": "string", "default": ""},
              "release": {"type": "boolean", "default": False},
              "products": {"type": "string", "default": ""},
              "audit": {"type": "boolean", "default": False},
              "resource_class": {"type": "string", "default": DEFAULT_CLASS}}
SHA = re.compile(r"[0-9a-f]{40}")
# `current` and `candidate` exit with SKIP when a release job has nothing to do; any other
# failure fails the job.
SKIP = 3
# Jobs receive the plan as an environment variable. Linux refuses to start a process with one
# longer than 128 KiB (MAX_ARG_STRLEN, name included), so a larger plan fails at generation.
ENV_LIMIT = 128 * 1024 - 256


def merge_base(root, revision):
    result = subprocess.run(["git", "merge-base", revision, "HEAD"], cwd=root,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.decode().strip() if result.returncode == 0 else ""


def first_parent(root):
    result = subprocess.run(["git", "rev-parse", "--verify", "HEAD^1"], cwd=root,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.decode().strip() if result.returncode == 0 else ""


def event_options(root, env, prefix):
    """(event, base, full, release, products) in the vocabulary `gate.py` records.

    Explicit pipeline parameters are a manual dispatch. A schedule passes `full`. The trunk is
    a push compared against the previous pipeline's revision, qualifying any release whose
    versions it changes (without one: full coverage, releases judged against the first parent); any other branch is compared from its merge base with the trunk."""
    base = env.get("CI_INPUT_BASE", "")
    if not re.fullmatch(r"(?:[A-Za-z0-9._][A-Za-z0-9._/-]*)?", base):
        raise ValueError("comparison base is not a Git revision")
    full = env.get("CI_INPUT_FULL") == "true"
    release = env.get("CI_INPUT_RELEASE") == "true"
    products = env.get("CI_INPUT_PRODUCTS", "").split()
    branch = env.get("CIRCLE_BRANCH", "")
    if full or release or products or base:
        event = "workflow_dispatch"
    elif branch == TRUNK:
        revision = env.get("CI_BASE_REVISION", "")
        if revision == env.get("CIRCLE_SHA1"):
            revision = ""  # A re-run of the same commit has no diff; never post an empty green.
        if SHA.fullmatch(revision):
            base = revision
        else:
            # CircleCI renders an absent previous revision as "<nil>", and GitHub App pipelines
            # never set one. An admin push can bypass the branch run, so coverage stays full; the
            # first parent (gate.py's `sha^`) only decides which versions this push changed. A
            # version bump below the tip of a multi-commit push qualifies nothing.
            base, full = first_parent(root), True
        event, release = "push", True
    elif branch:
        # The plan job pins the merge base so the gate cannot see a trunk that moved since.
        event, base = "pull_request", env.get("CI_MERGE_BASE") or merge_base(root, f"origin/{TRUNK}")
        if base == env.get("CIRCLE_SHA1"):
            base = ""  # A branch at a trunk commit has no diff; never post an empty green there.
        # Fork pull requests set CIRCLE_PR_USERNAME; only same-repository release branches qualify.
        release |= branch.startswith(prefix) and not env.get("CIRCLE_PR_USERNAME")
    else:
        raise ValueError("CircleCI CI is only configured for branch pipelines")
    return event, base, full or not base, release, products or None


def plan_for_event(root, env):
    event, base, full, release, products = event_options(root, env, release_policy(root))
    # A trunk push qualifies only the products whose versions it changes; most change none.
    plan = build_plan(root, base=base, full=full, release=release, products=products,
                      release_optional=event == "push")
    # A trunk push that qualifies a release is a publication candidate; `release` confirms it
    # came from a merged release-plz PR before anything is published.
    plan["release_candidate"] = event == "push" and plan["release"]
    if plan["head"] != env["CIRCLE_SHA1"]:
        raise ValueError("checkout does not match the pipeline's tested SHA")
    validate_plan(plan)
    return event, plan


def executor(resource_class, image=IMAGE):
    # A namespace/name class is a self-hosted container runner; it runs the same image.
    return {"docker": [{"image": image}], "resource_class": resource_class}


def run(name, command, **extra):
    return {"run": {"name": name, "command": command, **extra}}


def cached(key, paths, steps, *, save, toolchain, lock="Cargo.lock", enabled=True):
    if not enabled:
        return steps
    # The cached value is the dependency build: the image (native libraries), toolchain and
    # lockfile decide it. Each key is written once, not on every trunk push.
    key += f"-{toolchain}-{IMAGE.rsplit(':', 1)[1][:12]}"
    exact = key + f'-{{{{ checksum "{lock}" }}}}'
    # A saving job restores only its exact key, so a new key holds a clean build rather than
    # accumulating every earlier lockfile's dependencies.
    result = [{"restore_cache": {"keys": [exact] if save else [exact, key + "-"]}}, *steps]
    # Only full-coverage trunk pipelines write caches, so a scoped push cannot fix a partial
    # build under a key it never rewrites. CircleCI does not scope caches by branch, so a
    # collaborator's branch could still write one: release candidates restore none.
    if save:
        result.append({"save_cache": {"key": exact, "paths": paths, "when": "on_success"}})
    return result


def slug(text):
    return re.sub(r"[^A-Za-z0-9_-]+", "-", text).strip("-")


def toolchain_of(profile, bnb):
    return (MSRV if profile == "msrv" else
            bnb["public_api_toolchain"] if profile in {"public-api", "fuzz"} else STABLE)


def check_job(profile, *, save, caches, toolchain, part=None):
    if profile == "deny":
        steps = [run("cargo deny", "cargo deny --all-features check")]
    else:
        # The budget bounds the whole profile, not only a silent stretch.
        selection = ' --part "$PART"' if part else ""
        steps = [run("Run the complete selected profile",
                     f'timeout {BUDGET} python3 scripts/ci/run.py "$PROFILE"{selection}', no_output_timeout=BUDGET)]
    if part == "test":
        steps.append({"store_test_results": {"path": JUNIT}})
    return cached(f"v4-checks-{profile}{'-' + part if part else ''}",
                  CACHE_PATHS + [f"{root}/target" for root in DETACHED_TARGETS], steps,
                  save=save, toolchain=toolchain, enabled=caches and profile not in NO_RUST)


def fuzz_job(item, *, save, caches, toolchain):
    # Separate processes preserve each target's original budget. GNU is required for ASan.
    steps = [run("Fuzz", f"timeout {BUDGET} python3 scripts/ci/run.py fuzz", no_output_timeout=BUDGET)]
    return cached(f"v4-fuzz-{slug(item['root'])}-{item['target']}",
                  CACHE_PATHS[:-1] + [f"{item['root']}/target"], steps, save=save, toolchain=toolchain,
                  lock=f"{item['root']}/Cargo.lock", enabled=caches)


def commitlint_job(base):
    """Conventional Commits for every commit since the base: release-plz versions from them.
    The upstream image is run, not used as the executor: its entrypoint is commitlint itself."""
    lint = (f'docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/repo" -w /repo --entrypoint commitlint {COMMITLINT} '
            f'--config .config/commitlint.config.mjs --from {base} --to "$CIRCLE_SHA1"')
    return {"machine": {"image": "ubuntu-2404:current"}, "resource_class": "medium",
            "steps": ["checkout", run("Lint commit messages", lint)]}


def skip_unless(name, command):
    """Run a decision: success continues, SKIP halts the job successfully, anything else fails."""
    return run(name, f"status=0\n{command} || status=$?\n"
                     f"case $status in 0) ;; {SKIP}) circleci-agent step halt ;; *) exit $status ;; esac")


def release_jobs(release):
    """`release` publishes a verified release-plz merge; `release-pr` then refreshes the PR.
    Both stop unless the trunk still points at this commit before any token is used."""
    # The token reaches Git through a credential helper reading the environment, never .git/config.
    authenticate = run("Authenticate Git with the release token",
                       f"git remote set-url origin https://github.com/{REPOSITORY}.git\n"
                       "git config credential.helper "
                       "'!f() { echo username=x-access-token; echo \"password=$RELEASE_PLZ_TOKEN\"; }; f'\n"
                       "git config user.name release-plz\n"
                       "git config user.email release-plz@users.noreply.github.com")
    current = skip_unless("Stop unless the trunk still points at this commit",
                          "python3 scripts/ci/circleci.py current")
    jobs = {"release-pr": {**executor("medium"), "environment": {"CI_TEMP": "/tmp/ci"}, "steps": [
        "checkout", current, authenticate,
        run("release-plz release-pr", 'release-plz release-pr --git-token "$RELEASE_PLZ_TOKEN"'),
    ]}}
    if release:
        jobs["release"] = {**executor("medium"), "environment": {"CI_TEMP": "/tmp/ci"}, "steps": [
            "checkout", {"attach_workspace": {"at": "/tmp/ci"}}, current,
            skip_unless("Stop unless this is the exact merged release PR",
                        "python3 scripts/ci/circleci.py candidate"),
            run("Require complete exact-run release and downstream qualification",
                'CANDIDATE_SHA="$CIRCLE_SHA1" CI_RUN_ID="$CIRCLE_WORKFLOW_ID" CI_RUN_ATTEMPT=1 '
                "python3 scripts/ci/gate.py release --report /tmp/ci/ci-coverage.json"),
            run("Fail closed before release side effects",
                'test -n "$CARGO_REGISTRY_TOKEN" && test -n "$RELEASE_PLZ_TOKEN"'),
            authenticate,
            run("release-plz release", 'release-plz release --git-token "$RELEASE_PLZ_TOKEN"'),
        ]}
    return jobs


def image_job():
    """Publish the job image for a changed Dockerfile; adopting it is a reviewed digest change."""
    publish = ('echo "$GHCR_TOKEN" | docker login ghcr.io --username "$GHCR_USER" --password-stdin\n'
               'image="ghcr.io/rawsocketlabs/rsl-ci:$CIRCLE_SHA1"\n'
               f'docker build --tag "$image" {IMAGE_SOURCE}\n'
               'docker push "$image"\n'
               "echo \"Pin: $(docker inspect --format '{{index .RepoDigests 0}}' \"$image\")\"")
    return {"machine": {"image": "ubuntu-2404:current"}, "resource_class": "large",
            "steps": ["checkout", run("Build and publish rsl-ci", publish, no_output_timeout="45m")]}


def continuation(plan, bnb, env, event):
    """The continued pipeline: policy, the selected checks and fuzz targets, the gate, then
    for a trunk push the release jobs."""
    resource_class = env.get("CI_INPUT_CLASS") or DEFAULT_CLASS
    trunk = env.get("CIRCLE_BRANCH") == TRUNK
    caches = not plan["release"]  # Evidence that can authorise publication restores no cache.
    save = trunk and caches and plan["full"]
    handed = json.dumps(handed_on(plan), separators=(",", ":"), sort_keys=True)
    if len(handed.encode()) > ENV_LIMIT:
        raise ValueError("the handed-on plan exceeds one environment variable's limit")
    # Incremental state and full debuginfo are most of a cached target directory; line tables
    # keep panic locations.
    common = {"CI_PLAN": handed, "CI_TEMP": "/tmp/ci", "RUNNER_TEMP": "/tmp/ci",
              "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "line-tables-only",
              "CI_MERGE_BASE": plan["base"] or "", **{name: env.get(name, "") for name in INPUTS}}
    checkout = ["checkout", run("Prepare", 'mkdir -p "$CI_TEMP"')]

    def job(steps, *, resource=resource_class, **environment):
        return {**executor(resource), "environment": {**common, **environment},
                "steps": [*checkout, *steps]}

    jobs = {"policy": job([
        # Fast tier has no Cargo invocation, builds, registry access, or extra Python dependencies.
        run("CI tooling tests", "python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v"),
        run("Shell syntax", "bash -n scripts/ci-local.sh"),
    ], resource="medium")}
    selected = []
    if plan["base"]:
        jobs["commitlint"] = commitlint_job(plan["base"])
        selected.append("commitlint")
    for profile in plan["profiles"]:
        environment = {"PROFILE": profile}
        if profile not in NO_RUST:
            environment["RUSTUP_TOOLCHAIN"] = toolchain_of(profile, bnb)
        for part in WORKSPACE_PARTS if profile == "workspace" else (None,):
            name = f"checks-{profile}" + (f"-{part}" if part else "")
            jobs[name] = job(check_job(profile, save=save, caches=caches,
                                       toolchain=toolchain_of(profile, bnb), part=part),
                             **environment, **({"PART": part} if part else {}))
            selected.append(name)
    for item in plan["fuzz"]:
        name = f"fuzz-{slug(item['root'])}-{item['target']}"
        toolchain = toolchain_of("fuzz", bnb)
        jobs[name] = job(fuzz_job(item, save=save, caches=caches, toolchain=toolchain), RUSTUP_TOOLCHAIN=toolchain,
                         FUZZ_ITEM=json.dumps(item, separators=(",", ":")))
        selected.append(name)
    needs = {"checks": "success" if plan["profiles"] else "skipped",
             "fuzz": "success" if plan["fuzz"] else "skipped"}
    gate = SCOPED_GATE if env.get("CI_INPUT_BASE") else GATE
    jobs[gate] = job([
        run("Recompute coverage and reject any divergence", "python3 scripts/ci/circleci.py gate"),
        {"persist_to_workspace": {"root": "/tmp/ci", "paths": ["ci-coverage.json"]}},
        {"store_artifacts": {"path": "/tmp/ci/ci-coverage.json", "destination": "ci-coverage.json"}},
    ], resource="medium", CI_NEEDS=json.dumps(needs, sort_keys=True))
    workflow = [{"policy": {}},
                *({name: {} if name == "commitlint" else {"requires": ["policy"]}} for name in selected),
                {gate: {"requires": ["policy", *selected]}}]
    if event == "push":
        # Only reviewed trunk commits reach a secret. The image publishes after CI passes.
        if any(path.startswith(IMAGE_SOURCE) for path in plan.get("paths", ())):
            jobs["image"] = image_job()
            workflow.append({"image": {"requires": [GATE], "context": [IMAGE_CONTEXT]}})
        jobs.update(release_jobs(plan["release_candidate"]))
        if plan["release_candidate"]:
            workflow.append({"release": {"requires": [GATE], "context": [GITHUB_CONTEXT, CRATES_CONTEXT],
                                         "serial-group": RELEASE_GROUP}})
        workflow.append({"release-pr": {"requires": ["release" if plan["release_candidate"] else GATE],
                                        "context": [GITHUB_CONTEXT], "serial-group": RELEASE_GROUP}})
    return {"version": 2.1, "parameters": PARAMETERS, "jobs": jobs, "workflows": {"rsl": {"jobs": workflow}}}


def audit_continuation():
    """The weekly scan for advisories disclosed against unchanged, already-pinned dependencies."""
    job = {**executor("medium"), "steps": ["checkout", run("cargo deny advisories", "cargo deny check advisories")]}
    return {"version": 2.1, "parameters": PARAMETERS, "jobs": {"audit": job},
            "workflows": {"audit": {"jobs": ["audit"]}}}


def github(path):
    """Every page of a public GitHub REST list."""
    items, url = [], f"https://api.github.com/{path}?per_page=100"
    while url:
        request = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
        with urllib.request.urlopen(request, timeout=60) as response:
            items.extend(json.loads(response.read()))
            links = re.findall(r'<([^>]+)>;\s*rel="next"', response.headers.get("Link", ""))
            url = links[0] if links else None
    return items


def is_release_pr(pr, prefix):
    return (pr["head"]["ref"].startswith(prefix)
            and pr["head"]["repo"] is not None
            and pr["head"]["repo"]["full_name"] == REPOSITORY
            and pr["base"]["repo"]["full_name"] == REPOSITORY
            and pr["base"]["ref"] == TRUNK)


def is_release_merge(pr, sha, prefix):
    return bool(pr["merged_at"] and pr["merge_commit_sha"] == sha and is_release_pr(pr, prefix))


def trunk_tip():
    out = subprocess.run(["git", "ls-remote", f"https://github.com/{REPOSITORY}.git", f"refs/heads/{TRUNK}"],
                         check=True, stdout=subprocess.PIPE).stdout.decode().split()
    return out[0] if out else ""


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
    parser.add_argument("mode", choices=("plan", "gate", "current", "candidate"))
    parser.add_argument("--dry-run", action="store_true", help="write the continuation without continuing")
    args = parser.parse_args()
    root, env = Path.cwd(), dict(os.environ)
    temp = Path(env.get("CI_TEMP", "/tmp/ci"))
    if args.mode == "current":
        if trunk_tip() != env["CIRCLE_SHA1"]:
            print("Superseded: the trunk has moved past this commit")
            sys.exit(SKIP)
        return
    if args.mode == "candidate":
        # A read of a public repository: no write-scoped token is needed or used.
        sha = env["CIRCLE_SHA1"]
        pulls = github(f"repos/{REPOSITORY}/commits/{sha}/pulls")
        if not any(is_release_merge(pr, sha, release_policy(root)) for pr in pulls):
            print("No publish operation for this commit")
            sys.exit(SKIP)
        print("Exact merged release candidate")
        return
    if args.mode == "plan":
        temp.mkdir(parents=True, exist_ok=True)
        if env.get("CI_INPUT_AUDIT") == "true":
            configuration = json.dumps(audit_continuation(), indent=1)
        else:
            event, plan = plan_for_event(root, env)
            (temp / "ci-plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
            print(json.dumps(plan, indent=2))
            bnb = tomllib.loads(Path(".github/ci/bnb.toml").read_text())
            configuration = json.dumps(continuation(plan, bnb, env, event), indent=1)
            print(f"Selected {len(plan['profiles'])} check profiles and {len(plan['fuzz'])} fuzz targets")
        (temp / "continuation.json").write_text(configuration + "\n")
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
    except Exception as error:  # Any failure must fail its job, never read as a decision (SKIP).
        print(f"CircleCI planning failed: {error!r}", file=sys.stderr)
        sys.exit(2)

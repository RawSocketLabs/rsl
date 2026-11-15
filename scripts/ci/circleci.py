#!/usr/bin/env python3
"""Plan a CircleCI pipeline and continue it with only the selected jobs.

The static `.circleci/config.yml` is a setup workflow: its `plan` job selects coverage with
`plan.py` and continues the pipeline with jobs for the selected check profiles (short ones share
a job) and fuzz targets.
Unselected work is never scheduled. Every generated job runs concurrently after `policy` (with
it, on a self-hosted runner); the
final `CI` job recomputes the plan and requires the handed-on plan to match before it writes
the coverage receipt. On the trunk, `release` publishes a qualified release-plz merge and
`release-pr` refreshes the release PR, both serialised and holding the only secrets.
"""

import argparse
import base64
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

from gate import coverage_report, validate_plan
from plan import build_plan, handed_on, release_policy
import release_sync

TRUNK = "main"
REPOSITORY = "RawSocketLabs/rsl"
# The job image (.circleci/image/Dockerfile), by digest. Its toolchains and tools are the only
# ones jobs use: nothing is installed at run time. test_circleci.py holds these pins in step.
IMAGE = "ghcr.io/rawsocketlabs/rsl-ci@sha256:7a7593791aa5b42cf1e180fb274834bb61f0f685b4fd37ddeed6e9c6b5582a31"
# The upstream commitlint image (21.2.3), which carries @commitlint/config-conventional.
COMMITLINT = ("docker.io/commitlint/commitlint@"
              "sha256:33f225c84111a38ebac0c81fa21c488a79709b8a8e04056dfaaf4f80ef20be25")
STABLE = "1.100.0"
# A stable bump takes two changes, because jobs run the image pinned above, not the Dockerfile at
# HEAD. First the Dockerfile's `ARG STABLE` moves alone and this names its version: the main push
# publishes that image while every job still runs STABLE on the pinned one. Then one change
# adopts the new digest, moves STABLE and rust-toolchain.toml to it, and sets this back to None.
NEXT_STABLE = None
MSRV = "1.85.0"
# 2 vCPU, 4 GB at half large's credit rate: on large most jobs took 1-2 minutes (insights,
# 2026-11) and the heaviest trial job peaked at 1.5 GiB. Pass resource_class=large to go faster.
DEFAULT_CLASS = "medium"
# The `auto` class picks the MS-01 runner while mini's ci-heartbeat timer keeps the project's
# CI_RUNNER_SEEN (epoch seconds, written each minute while CircleCI sees the agent connected)
# fresh, and DEFAULT_CLASS otherwise, so a home outage falls back to the cloud. A rerun keeps the
# generated class, so jobs stranded on a dropped runner need a new pipeline instead.
AUTO_CLASS, RUNNER_CLASS, RUNNER_FRESH = "auto", "rawsocketlabs/ms01", 180
NO_RUST = {"deny", "blessed"}
# Every job's budget: a slow job fails rather than stretching the pipeline past it.
BUDGET = "10m"
# nextest's `ci` profile (.config/nextest.toml) writes JUnit here for CircleCI's test insights.
JUNIT = "target/nextest/ci/junit.xml"
# The critical path; its parts run on separate machines (run.py workspace_parts).
WORKSPACE_PARTS = ("lint", "test", "extra")
# Short profiles share a job: each job pays ~30 s of spin-up, checkout and cache restore, and most
# profiles take a minute or two (insights, 2026-10..11). A bucket's summed median stays near the
# critical path's (`workspace` test, ~2.2 min) and within one toolchain; a profile not listed runs
# alone. A job's name and cache key stay the bucket's whatever subset a plan selects.
BUCKETS = {
    "crates": ("deny", "blessed", "network", "facades"),
    "bindings": ("bladerf", "usdr-ffi", "rust-dsdcc", "rust-skills"),
    "oracles-a": ("usdr-oracle", "pe-sync-oracle"),
    "oracles-b": ("xsdr-oracle", "lsdr-oracle", "limesdr-oracle"),
    "portable": ("usdr-portable", "xsdr-portable", "lsdr-portable", "pe-sync-portable", "limesdr-portable"),
    # Release-only profiles stay unbucketed: release runs restore no cache, so a shared job would
    # be several cold builds under one 10-minute budget, and a timeout blocks a release.
}
# Buckets only save cloud spin-ups. A runner's jobs cost nothing to start and its CPU idles while a
# bucket's oracles build one after another (measured 2026-11-15), so there each oracle is a job.
RUNNER_UNBUCKETED = {"oracles-a", "oracles-b"}
# Caches are written by full-coverage trunk runs: the weekly schedule (Sundays), and a push that
# changes a key's inputs. CircleCI never overwrites a live key, so each key names the week of its
# save (cache_week) and lives one CACHE_RETENTION, under 7 days: the next Sunday writes a new one.
CACHE_RETENTION = "6d"
# What a cache key is made of: the lockfile, toolchain and job image (followed through GENERATOR).
CACHE_INPUTS = {"Cargo.lock", "rust-toolchain.toml"}
GENERATOR = "scripts/ci/circleci.py"  # Holds IMAGE, whose digest every cache key carries.
CACHE_PATHS = ["~/.cargo/registry/index", "~/.cargo/registry/cache", "~/.cargo/git/db", "target"]
DETACHED_TARGETS = [
    "codecs/bitsandbytes/bnb/nostd-check", "tools/rust-skills", "bindings/usdr",
    "bindings/dsdcc",
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
CIRCLECI_BOT = "circleci-app[bot]"
# Pipeline inputs the setup job receives; generated jobs get the same literal values so the
# gate's recomputation sees the event the plan saw.
INPUTS = ("CI_INPUT_FULL", "CI_INPUT_BASE", "CI_INPUT_RELEASE", "CI_INPUT_PRODUCTS",
          "CI_INPUT_CLASS", "CI_INPUT_AUDIT", "CI_TRIGGER_SOURCE", "CI_BASE_REVISION",
          "CI_PIPELINE_NUMBER", "CI_PROJECT_SLUG")
# Declared again in the continuation: trigger-time parameters carry over into it.
PARAMETERS = {"full": {"type": "boolean", "default": False}, "base": {"type": "string", "default": ""},
              "release": {"type": "boolean", "default": False},
              "products": {"type": "string", "default": ""},
              "audit": {"type": "boolean", "default": False},
              "resource_class": {"type": "string", "default": AUTO_CLASS}}
SHA = re.compile(r"[0-9a-f]{40}")
# `current` and `candidate` exit with SKIP when a release job has nothing to do; any other
# failure fails the job.
SKIP = 3
# Jobs receive the plan as an environment variable. Linux refuses to start a process with one
# longer than 128 KiB (MAX_ARG_STRLEN, name included), so a larger plan fails at generation.
ENV_LIMIT = 128 * 1024 - 256


def fetch(request, attempts=3, delay=5):
    """urlopen a read, retrying a lost connection or a server error. A refusal (rate limit,
    not found) is an answer, and the caller's own failure handling decides it."""
    for attempt in range(attempts):
        try:
            return urllib.request.urlopen(request, timeout=60)
        except urllib.error.HTTPError as error:
            if error.code < 500 or attempt == attempts - 1:
                raise
        except (urllib.error.URLError, TimeoutError):
            if attempt == attempts - 1:
                raise
        time.sleep(delay * (attempt + 1))


def merge_base(root, revision):
    result = subprocess.run(["git", "merge-base", revision, "HEAD"], cwd=root,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.decode().strip() if result.returncode == 0 else ""


def first_parent(root):
    result = subprocess.run(["git", "rev-parse", "--verify", "HEAD^1"], cwd=root,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.decode().strip() if result.returncode == 0 else ""


def descends(root, revision):
    """Whether HEAD's history contains `revision`; a force-push leaves the old trunk outside it."""
    result = subprocess.run(["git", "merge-base", "--is-ancestor", revision, "HEAD"], cwd=root,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return result.returncode == 0


def gate_passed(sha):
    """Whether the CircleCI app's latest required gate on this commit succeeded; on any error, no."""
    try:
        statuses = github(f"repos/{REPOSITORY}/commits/{sha}/statuses")
    except (OSError, ValueError):  # URLError and HTTPError are OSErrors; ValueError is bad JSON.
        return False
    # Only the CircleCI app posts as its bot; the scoped gate never stands in for the required one.
    # GitHub lists statuses newest first, and the ruleset judges the latest: a later red overrides.
    latest = next((status for status in statuses if status.get("context") == f"ci/circleci: {GATE}"
                   and (status.get("creator") or {}).get("login") == CIRCLECI_BOT), {})
    return latest.get("state") == "success"


def previous_trunk_revision(root, env):
    """What `pipeline.git.base_revision` would hold, when it is a proven base: the commit of this
    project's latest earlier trunk pipeline (CircleCI's public pipeline list), if HEAD descends from
    it and its required gate passed; else "". A red or unfinished predecessor proves nothing."""
    number, project = env.get("CI_PIPELINE_NUMBER", ""), env.get("CI_PROJECT_SLUG", "")
    if not number.isdigit() or not project:
        return ""
    host = env.get("CIRCLECI_HOST", "https://circleci.com")
    try:
        with fetch(f"{host}/api/v2/project/{project}/pipeline?branch={TRUNK}") as response:
            items = json.loads(response.read())["items"]
        # Newest first, one page: if 20 later trunk pipelines overtook this one, coverage is full.
        revision = next((item["trigger_parameters"]["git"]["checkout_sha"] for item in items
                         if item["number"] < int(number)), "")
    except (OSError, ValueError, KeyError, TypeError):  # Unreadable history proves nothing: full.
        return ""
    if not isinstance(revision, str) or not SHA.fullmatch(revision):
        return ""
    return revision if descends(root, revision) and gate_passed(revision) else ""


def head_tested(env):
    """Whether the pushed commit already carries the CircleCI app's green required gate. The plan
    pins the answer as CI_HEAD_TESTED: by the gate's recomputation this workflow's own gate is
    pending, and the gate must not depend on the network."""
    if "CI_HEAD_TESTED" not in env:
        env["CI_HEAD_TESTED"] = str(gate_passed(env["CIRCLE_SHA1"])).lower()
    return env["CI_HEAD_TESTED"] == "true"


def image_changed(plan):
    return any(path.startswith(IMAGE_SOURCE) for path in plan.get("paths", ()))


def pinned_image(source):
    match = re.search(r'^IMAGE = "([^"]+)"$', source, re.M)
    return match and match.group(1)


def image_pin_at(revision, root="."):
    """The job image `revision` pinned, or None when that revision cannot be read."""
    result = subprocess.run(["git", "show", f"{revision}:{GENERATOR}"], cwd=root, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, text=True)
    return pinned_image(result.stdout) if result.returncode == 0 else None


def cache_inputs_changed(plan):
    paths = set(plan.get("paths", ()))
    # Paths are only listed against a base, so one exists when GENERATOR changed.
    return bool(CACHE_INPUTS & paths) or (GENERATOR in paths and image_pin_at(plan["base"]) != IMAGE)


def cache_week(now=None):
    """The date of the weekly full schedule (Sundays 09:17 UTC) a cache saved now belongs to."""
    shifted = (now or datetime.now(timezone.utc)) - timedelta(hours=9, minutes=17)
    return (shifted.date() - timedelta(days=(shifted.weekday() + 1) % 7)).isoformat()


def event_options(root, env, prefix):
    """(event, base, full, release, products) in the vocabulary `gate.py` records.

    Explicit pipeline parameters are a manual dispatch. A schedule passes `full`. The trunk is
    a push compared against the previous trunk pipeline's revision, qualifying any release whose
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
        if not SHA.fullmatch(revision):
            # CircleCI renders an absent previous revision as "<nil>", and GitHub App pipelines
            # never set one, so read it from the project's pipeline history instead.
            # The plan pins its answer for the generated jobs, so the gate's recomputation reads
            # the same base instead of asking the network again.
            pinned = env.get("CI_TRUNK_BASE")
            if pinned is None:
                revision = previous_trunk_revision(root, env)
            else:
                revision = pinned if SHA.fullmatch(pinned) and descends(root, pinned) else ""
        if revision == env.get("CIRCLE_SHA1"):
            revision = ""  # A re-run of the same commit has no diff; never post an empty green.
        if SHA.fullmatch(revision):
            base = revision
        else:
            # No previous trunk pipeline this commit descends from (or its history is
            # unreadable). An admin push can bypass the branch run, so coverage stays full; the
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
    if event == "push":
        # Pinned for the gate, which recomputes from this and never from the plan's own fullness.
        # A push that is full because of what it changed (a global file) still has its proven
        # base, and the gate's fallback, the first parent, is another commit whenever the push
        # carries more than one: the recomputed plan then differs and the gate fails.
        env.setdefault("CI_TRUNK_BASE", "none" if full else base)
    # `full` here is the event's: a push without a proven green trunk base. A plan made full by its
    # content (central CI, toolchain) still compares from that base.
    # A trunk push qualifies only the products whose versions it changes; most change none.
    plan = build_plan(root, base=base, full=full, release=release, products=products,
                      release_optional=event == "push")
    # A trunk push that qualifies a release is a publication candidate; `release` confirms it
    # came from a merged release-plz PR before anything is published.
    plan["release_candidate"] = event == "push" and plan["release"]
    if (event == "push" and not full and not plan["release"] and not image_changed(plan)
            and not (plan["full"] and cache_inputs_changed(plan)) and head_tested(env)):
        # The branch pipeline already passed the required gate on this exact commit, and its merge
        # base is at or before the proven green trunk base, so its diff held every path this push
        # changed and its coverage was at least this plan's: main re-tests nothing. Without a green
        # predecessor the branch's scope could hide a red trunk; a release or image push needs this
        # workflow's own run, and a full push changing a cache key runs to save the new key. The
        # gate recomputes from the same pinned trunk base.
        plan = build_plan(root, base=env["CIRCLE_SHA1"])
    if plan["head"] != env["CIRCLE_SHA1"]:
        raise ValueError("checkout does not match the pipeline's tested SHA")
    validate_plan(plan)
    return event, plan


def executor(resource_class, image=IMAGE):
    # A namespace/name class is a self-hosted container runner; it runs the same image.
    return {"docker": [{"image": image}], "resource_class": resource_class}


def run(name, command, **extra):
    return {"run": {"name": name, "command": command, **extra}}


def cached(key, paths, steps, *, save, toolchain, week, lock="Cargo.lock", enabled=True):
    if not enabled:
        return steps
    # The cached value is the dependency build: the image (native libraries), toolchain and
    # lockfile decide it; the week names its save, so a mid-week key gives way to Sunday's.
    key += f"-{toolchain}-{IMAGE.rsplit(':', 1)[1][:12]}"
    locked = key + f'-{{{{ checksum "{lock}" }}}}'
    exact = f"{locked}-{week}"
    # A saving job restores only its exact key, so a new key holds a clean build rather than
    # accumulating every earlier lockfile's dependencies. Others take the newest for their
    # lockfile, else the newest of any.
    result = [{"restore_cache": {"keys": [exact] if save else [exact, locked + "-", key + "-"]}}, *steps]
    # Only full-coverage trunk runs write caches, so a scoped push cannot store a partial build.
    # CircleCI does not scope caches by branch, so a collaborator's branch could still write one:
    # release candidates restore none.
    if save:
        # CircleCI bills size times retention: only third-party artifacts are worth keeping.
        targets = " ".join(path for path in paths if path.endswith("target"))
        result += [run("Prune owned artifacts before caching", f"python3 scripts/ci/prune.py {targets}"),
                   {"save_cache": {"key": exact, "paths": paths, "when": "on_success"}}]
    return result


# A self-hosted runner mounts its own disk here. CircleCI bills cache restores to a self-hosted
# runner as network egress, so its build state stays local: trunk and branches keep separate
# directories, so main never builds on a branch's output, and the host wipes them weekly.
RUNNER_CACHE = "/ci-cache/target"
# Downloaded .crate files are shared across the runner's jobs (cargo checksums them on unpack); the
# rest of the registry stays private, because cargo's lock for it lives in each job's CARGO_HOME.
RUNNER_CRATES = "/ci-cache/crates"


def routed_class(env, now=None, *, cloud=False):
    chosen = env.get("CI_INPUT_CLASS") or AUTO_CLASS
    if chosen != AUTO_CLASS:
        return chosen
    seen = env.get("CI_RUNNER_SEEN", "").strip()
    age = (time.time() if now is None else now) - int(seen) if seen.isascii() and seen.isdigit() else None
    if cloud:
        return DEFAULT_CLASS
    # A minute of tolerance for clock skew between mini and CircleCI.
    return RUNNER_CLASS if age is not None and -60 <= age <= RUNNER_FRESH else DEFAULT_CLASS


def self_hosted(resource_class):
    return "/" in resource_class


def linked(name, roots, steps, *, toolchain, trunk):
    base = f"{RUNNER_CACHE}/{'trunk' if trunk else 'branch'}/{name}-{toolchain}-{IMAGE.rsplit(':', 1)[1][:12]}"
    links = " && ".join([f"mkdir -p ~/.cargo/registry && ln -sfn {RUNNER_CRATES} ~/.cargo/registry/cache",
                         *(f"mkdir -p {base}/{slug(root) or 'root'} && ln -sfn {base}/{slug(root) or 'root'} {root}/target"
                           for root in roots)])
    return [run("Link the runner's persistent build directories", links), *steps]


def slug(text):
    return re.sub(r"[^A-Za-z0-9_-]+", "-", text).strip("-")


def toolchain_of(profile, bnb):
    return (MSRV if profile == "msrv" else
            bnb["public_api_toolchain"] if profile in {"public-api", "fuzz"} else STABLE)


def bucket_of(profile):
    return next((name for name, members in BUCKETS.items() if profile in members), profile)


def check_job(name, profiles, *, save, caches, toolchain, week, part=None, local=None):
    # The budget bounds the whole job's profiles, not only a silent stretch.
    selection = ' --part "$PART"' if part else ""
    steps = [run("Run the complete selected profiles",
                 f'timeout {BUDGET} python3 scripts/ci/run.py $PROFILES{selection}', no_output_timeout=BUDGET)]
    if part == "test":
        steps.append({"store_test_results": {"path": JUNIT}})
    if local is not None and caches and not set(profiles) <= NO_RUST:
        return linked(name, [".", *DETACHED_TARGETS], steps, toolchain=toolchain, trunk=local)
    return cached(f"v5-{name}", CACHE_PATHS + [f"{root}/target" for root in DETACHED_TARGETS], steps,
                  save=save, toolchain=toolchain, week=week,
                  enabled=caches and not set(profiles) <= NO_RUST)


def fuzz_job(item, *, save, caches, toolchain, week, local=None):
    # Separate processes preserve each target's original budget. GNU is required for ASan.
    steps = [run("Fuzz", f"timeout {BUDGET} python3 scripts/ci/run.py fuzz", no_output_timeout=BUDGET)]
    if local is not None and caches:
        return linked(f"fuzz-{slug(item['root'])}-{item['target']}", [item["root"]], steps,
                      toolchain=toolchain, trunk=local)
    return cached(f"v4-fuzz-{slug(item['root'])}-{item['target']}",
                  CACHE_PATHS[:-1] + [f"{item['root']}/target"], steps, save=save, toolchain=toolchain,
                  week=week, lock=f"{item['root']}/Cargo.lock", enabled=caches)


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
        run("Sync the release PR's detached lockfiles", "python3 scripts/ci/circleci.py sync-release"),
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
    """Publish the job and plan images for a changed Dockerfile; adopting either is a reviewed
    digest change."""
    publish = ('echo "$GHCR_TOKEN" | docker login ghcr.io --username "$GHCR_USER" --password-stdin\n'
               'for pair in rsl-ci:Dockerfile rsl-ci-plan:plan.Dockerfile; do\n'
               '  image="ghcr.io/rawsocketlabs/${pair%%:*}:$CIRCLE_SHA1"\n'
               f'  docker build --tag "$image" --file "{IMAGE_SOURCE}${{pair#*:}}" {IMAGE_SOURCE}\n'
               '  docker push "$image"\n'
               "  echo \"Pin: $(docker inspect --format '{{index .RepoDigests 0}}' \"$image\")\"\n"
               'done')
    return {"machine": {"image": "ubuntu-2404:current"}, "resource_class": "large",
            "steps": ["checkout", run("Build and publish rsl-ci and rsl-ci-plan", publish, no_output_timeout="45m")]}


def continuation(plan, bnb, env, event):
    """The continued pipeline: policy, the selected checks and fuzz targets, the gate, then
    for a trunk push the release jobs."""
    trunk = env.get("CIRCLE_BRANCH") == TRUNK
    caches = not plan["release"]  # Evidence that can authorise publication restores no cache.
    # Only full-coverage trunk runs save: the weekly schedule, and a push that changed a key's
    # inputs, so a mid-week lockfile, toolchain or image change is warm before Sunday.
    save = trunk and caches and plan["full"] and (env.get("CI_TRIGGER_SOURCE") == "scheduled_pipeline"
                                                  or cache_inputs_changed(plan))
    week = cache_week()
    # Runs that refresh the cloud caches (which keep the fallback warm) or produce release
    # evidence stay on CircleCI's machines.
    resource_class = routed_class(env, cloud=save or not caches)
    local = trunk if self_hosted(resource_class) else None  # Which runner-local directory set, if any.
    # A runner's tasks cost no credits, so policy and the gate run there too, and checks need not
    # wait for policy to save credits on a policy failure; the gate still requires it.
    light = resource_class if local is not None else "medium"
    handed = json.dumps(handed_on(plan), separators=(",", ":"), sort_keys=True)
    if len(handed.encode()) > ENV_LIMIT:
        raise ValueError("the handed-on plan exceeds one environment variable's limit")
    # Incremental state and full debuginfo are most of a cached target directory; line tables
    # keep panic locations.
    common = {"CI_PLAN": handed, "CI_TEMP": "/tmp/ci", "RUNNER_TEMP": "/tmp/ci",
              # A runner's linked target directory never leaves its disk, so Cargo's own default
              # (incremental dev and test builds) spares recompiling unchanged owned crates after each
              # checkout; elsewhere incremental state would only bloat a cache or a release build.
              **({} if local is not None and caches else {"CARGO_INCREMENTAL": "0"}),
              "CARGO_PROFILE_DEV_DEBUG": "line-tables-only",
              "CI_MERGE_BASE": plan["base"] or "",
              # The plan job's pin (`plan_for_event`): the previous pipeline's revision, or "none"
              # when there was no proven one and coverage is full against the first parent. Only a
              # trunk push reads it.
              "CI_TRUNK_BASE": env.get("CI_TRUNK_BASE") or "none",
              "CI_HEAD_TESTED": env.get("CI_HEAD_TESTED", "false"),
              **{name: env.get(name, "") for name in INPUTS}}
    checkout = ["checkout", run("Prepare", 'mkdir -p "$CI_TEMP"')]

    def job(steps, *, resource=resource_class, **environment):
        saves = any(isinstance(step, dict) and "save_cache" in step for step in steps)
        return {**executor(resource), "environment": {**common, **environment},
                **({"retention": {"caches": CACHE_RETENTION}} if saves else {}),
                "steps": [*checkout, *steps]}

    jobs = {"policy": job([
        # Fast tier has no Cargo invocation, builds, registry access, or extra Python dependencies.
        run("CI tooling tests", "python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v"),
        run("Shell syntax", "bash -n scripts/ci-local.sh"),
    ], resource=light)}
    selected = []
    if plan["base"] and plan["base"] != plan["head"]:  # A tested trunk commit has no new commits.
        jobs["commitlint"] = commitlint_job(plan["base"])
        selected.append("commitlint")
    buckets = {}
    for profile in plan["profiles"]:
        if profile == "workspace":
            buckets.update({f"workspace-{part}": ([profile], part) for part in WORKSPACE_PARTS})
        else:
            bucket = bucket_of(profile)
            if local is not None and bucket in RUNNER_UNBUCKETED:
                bucket = profile
            buckets.setdefault(bucket, ([], None))[0].append(profile)
    for bucket, (profiles, part) in buckets.items():
        profiles = sorted(profiles, key=BUCKETS.get(bucket, profiles).index)  # The listed order: deny first.
        toolchains = {toolchain_of(profile, bnb) for profile in profiles if profile not in NO_RUST}
        if len(toolchains) > 1:
            raise ValueError(f"bucket {bucket} mixes toolchains: {sorted(toolchains)}")
        toolchain = next(iter(toolchains), STABLE)
        environment = {"PROFILES": " ".join(profiles), **({"RUSTUP_TOOLCHAIN": toolchain} if toolchains else {})}
        name = f"checks-{bucket}"
        jobs[name] = job(check_job(name, profiles, save=save, caches=caches, toolchain=toolchain,
                                   week=week, part=part, local=local),
                         **environment, **({"PROFILE": "workspace", "PART": part} if part else {}))
        selected.append(name)
    for item in plan["fuzz"]:
        name = f"fuzz-{slug(item['root'])}-{item['target']}"
        toolchain = toolchain_of("fuzz", bnb)
        jobs[name] = job(fuzz_job(item, save=save, caches=caches, toolchain=toolchain, week=week, local=local),
                         RUSTUP_TOOLCHAIN=toolchain,
                         FUZZ_ITEM=json.dumps(item, separators=(",", ":")))
        selected.append(name)
    needs = {"checks": "success" if plan["profiles"] else "skipped",
             "fuzz": "success" if plan["fuzz"] else "skipped"}
    gate = SCOPED_GATE if env.get("CI_INPUT_BASE") else GATE
    jobs[gate] = job([
        run("Recompute coverage and reject any divergence", "python3 scripts/ci/circleci.py gate"),
        {"persist_to_workspace": {"root": "/tmp/ci", "paths": ["ci-coverage.json"]}},
        {"store_artifacts": {"path": "/tmp/ci/ci-coverage.json", "destination": "ci-coverage.json"}},
    ], resource=light, CI_NEEDS=json.dumps(needs, sort_keys=True))
    workflow = [{"policy": {}},
                *({name: {} if name == "commitlint" or local is not None else {"requires": ["policy"]}}
                  for name in selected),
                {gate: {"requires": ["policy", *selected]}}]
    if event == "push":
        # Only reviewed trunk commits reach a secret. The image publishes after CI passes.
        if image_changed(plan):
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


def github(path, token=None):
    """Every page of a public GitHub REST list; a token only lifts the anonymous rate limit."""
    items, url = [], f"https://api.github.com/{path}?per_page=100"
    headers = {"Accept": "application/vnd.github+json",
               **({"Authorization": f"Bearer {token}"} if token else {})}
    while url:
        request = urllib.request.Request(url, headers=headers)
        with fetch(request) as response:
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


COMMIT_ON_BRANCH = """mutation($input: CreateCommitOnBranchInput!) {
  createCommitOnBranch(input: $input) { commit { oid } }
}"""


def graphql(query, variables, token):
    """One GitHub GraphQL call; GraphQL reports failures in the body, so they raise here."""
    request = urllib.request.Request("https://api.github.com/graphql", method="POST",
                                     data=json.dumps({"query": query, "variables": variables}).encode(),
                                     headers={"Authorization": f"Bearer {token}",
                                              "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            body = json.loads(response.read())
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"GitHub GraphQL HTTP {error.code}: {error.read().decode(errors='replace')}") from error
    if body.get("errors"):
        raise RuntimeError(f"GitHub GraphQL: {body['errors']}")
    return body["data"]


def sync_release(root, temp):
    """Commit the open release PR's detached lockfile versions onto its branch (see
    `release_sync`). Runs the trunk's tooling over the branch's files. The commit goes through
    GitHub's API, as release-plz's does: CircleCI refused (`block-unregistered-user`) pipelines
    for pushed sync commits, while release-plz's API commits (committer GitHub) run. The
    API applies it only on the fetched tip, so a branch release-plz moved meanwhile fails this
    run, not the branch."""
    token = os.environ["RELEASE_PLZ_TOKEN"]
    pulls = [pr for pr in github(f"repos/{REPOSITORY}/pulls", token) if is_release_pr(pr, release_policy(root))]
    if not pulls:
        print("No open release PR")
        return
    if len(pulls) > 1:
        raise RuntimeError(f"several open release PRs: {[pr['number'] for pr in pulls]}")
    # The API names the branch; its tip comes from Git, since a PR's head SHA lags a fresh push.
    branch = pulls[0]["head"]["ref"]
    tree = temp / "release-branch"
    subprocess.run(["git", "fetch", "origin", f"refs/heads/{branch}"], check=True)
    subprocess.run(["git", "worktree", "add", "--detach", str(tree), "FETCH_HEAD"], check=True)
    head = subprocess.run(["git", "-C", str(tree), "rev-parse", "HEAD"], check=True,
                          stdout=subprocess.PIPE, text=True).stdout.strip()
    changed = release_sync.sync(tree)
    if not changed:
        print(f"{branch}: detached lockfiles already match")
        return
    additions = [{"path": path, "contents": base64.b64encode((tree / path).read_bytes()).decode()}
                 for path in changed]
    data = graphql(COMMIT_ON_BRANCH, {"input": {
        "branch": {"repositoryNameWithOwner": REPOSITORY, "branchName": branch},
        "message": {"headline": "chore: sync detached lockfiles to the released versions"},
        "fileChanges": {"additions": additions},
        "expectedHeadOid": head,
    }}, token)
    print(f"{branch}: synced {', '.join(changed)} as {data['createCommitOnBranch']['commit']['oid']}")


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
    parser.add_argument("mode", choices=("plan", "gate", "current", "candidate", "sync-release"))
    parser.add_argument("--dry-run", action="store_true", help="write the continuation without continuing")
    args = parser.parse_args()
    root, env = Path.cwd(), dict(os.environ)
    temp = Path(env.get("CI_TEMP", "/tmp/ci"))
    if args.mode == "current":
        if trunk_tip() != env["CIRCLE_SHA1"]:
            print("Superseded: the trunk has moved past this commit")
            sys.exit(SKIP)
        return
    if args.mode == "sync-release":
        sync_release(root, temp)
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

#!/usr/bin/env python3
"""Run the CircleCI pipeline locally: the same plan, generated jobs and image, under Podman.

The plan runs in the pinned `rsl-ci` image and writes the continuation CircleCI would run.
Each generated job then runs its own `run` steps, unchanged, in a fresh clone of the tested
commit inside that image; `policy` and `commitlint` first, the checks and fuzz targets
concurrently, and `CI` last, which recomputes the plan and rejects any divergence. Release
jobs never run locally, and a changed job image is built but not published.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import fcntl
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

import circleci

HOME = "/home/ci"
PROJECT = f"{HOME}/project"
CACHE = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "rsl-ci-local"
# Machine jobs call `docker run`; rootless Podman needs SELinux labelling off for their mounts.
# `--userns=keep-id` keeps the host user's ID, so a job's `--user "$(id -u)"` owns its checkout.
DOCKER = ('docker() { if [ "$1" = run ]; then shift; '
          'podman run --security-opt label=disable --userns=keep-id "$@"; '
          'else podman "$@"; fi; }; export -f docker\n')
# Never run locally: they publish, or need secrets.
REMOTE_ONLY = {"release", "release-pr"}


def git(*args, cwd=None):
    result = subprocess.run(["git", *args], cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    return result.stdout.strip() if result.returncode == 0 else ""


def pipeline_inputs(args, root):
    """The pipeline parameters a manual CircleCI run would receive for this request."""
    base = args.base or ""
    full = args.full
    if not base:
        # The trunk recorded by repo-guard; nothing is fetched to choose it.
        remote, trunk = git("config", "--get", "repoGuard.remote", cwd=root), git("config", "--get", "repoGuard.trunk", cwd=root)
        if remote and trunk:
            base = git("merge-base", f"refs/remotes/{remote}/{trunk}", "HEAD", cwd=root)
        if not base:
            print("No configured trunk comparison available; selecting full coverage.")
            full = True
    release = args.mode == "release" or bool(args.product)
    branch = git("symbolic-ref", "--quiet", "--short", "HEAD", cwd=root) or "detached"
    release |= branch.startswith(circleci.release_policy(root))
    return {"CI_INPUT_BASE": base, "CI_INPUT_FULL": str(full).lower(), "CI_INPUT_RELEASE": str(release).lower(),
            "CI_INPUT_PRODUCTS": " ".join(args.product or ()), "CI_INPUT_CLASS": "local",
            "CI_INPUT_AUDIT": "false", "CI_TRIGGER_SOURCE": "local", "CI_BASE_REVISION": "",
            "CIRCLE_BRANCH": branch}


def job_script(name, job):
    """A job's run steps as one fail-fast script; CircleCI-only steps have local equivalents
    (the clone is the checkout, caches are mounted, results stay in the run directory)."""
    lines = []
    for step in job["steps"]:
        if isinstance(step, dict) and "run" in step:
            lines += [f"echo {shlex.quote('==> ' + step['run']['name'])}", step["run"]["command"]]
    return "\n".join(lines)


def podman(image, workdir, temp, environment, script, *, registry, target=None):
    # Shared `z` labels: job clones hardlink the snapshot's objects, and a private `Z` relabel
    # by one container would revoke them from the others. Each job has its own registry, as on
    # CircleCI; concurrent containers would otherwise race to unpack the same crate.
    command = ["podman", "run", "--rm", "--userns=keep-id:uid=1001,gid=1001",
               "-v", f"{workdir}:{PROJECT}:z", "-v", f"{temp}:/tmp/ci:z",
               "-v", f"{registry}:{HOME}/.cargo/registry:z", "-w", PROJECT]
    if target:
        command += ["-v", f"{target}:{PROJECT}/target:z"]
    for key, value in environment.items():
        command += ["-e", f"{key}={value}"]
    return [*command, image, "bash", "-eo", "pipefail", "-c", script]


def run_job(name, job, *, image, snapshot, run_dir, environment):
    workdir = run_dir / "jobs" / name
    # Hardlinked, self-contained clones: alternates would point at host paths a container cannot see.
    subprocess.run(["git", "clone", "--quiet", "--local", str(snapshot), str(workdir)], check=True)
    log = run_dir / "logs" / f"{name}.log"
    started = time.monotonic()
    env = {**environment, **job.get("environment", {})}
    with log.open("w") as output:
        if name == "image":
            # Build the changed image to prove the Dockerfile; publication is CircleCI's.
            command = ["podman", "build", "--format", "docker", str(workdir / circleci.IMAGE_SOURCE)]
        elif "machine" in job:
            # Machine jobs drive containers themselves (commitlint): their script runs on the host
            # with Podman standing in for Docker, and the container they start does the work.
            command = ["bash", "-eo", "pipefail", "-c",
                       DOCKER + job_script(name, job)]
            return finish(name, subprocess.run(command, cwd=workdir, env={**os.environ, **env},
                                               stdout=output, stderr=subprocess.STDOUT), started, log)
        else:
            cached = any(isinstance(step, dict) and "restore_cache" in step for step in job["steps"])
            target = None
            if cached:
                target = CACHE / "target" / name
                target.mkdir(parents=True, exist_ok=True)
            registry = CACHE / "registry" / name
            registry.mkdir(parents=True, exist_ok=True)
            command = podman(image, workdir, run_dir / "tmp", env, job_script(name, job),
                             registry=registry, target=target)
            # One job's caches serve one run at a time: concurrent runs (two worktrees) would
            # race to unpack crates and share a target directory.
            with (CACHE / f"{name}.lock").open("w") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
                result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT)
            return finish(name, result, started, log)
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT)
    return finish(name, result, started, log)


def finish(name, result, started, log):
    elapsed = time.monotonic() - started
    status = "PASS" if result.returncode == 0 else "FAIL"
    print(f"{status} {name} ({elapsed:.0f}s) {log}", flush=True)
    return name, result.returncode == 0, log


def stages(workflow):
    """policy and commitlint, then everything that requires only them, then the gate."""
    entries = [next(iter(entry.items())) for entry in workflow]
    names = [name for name, _ in entries if name not in REMOTE_ONLY]
    first = [name for name, spec in entries if name in names and not (spec or {}).get("requires")]
    last = [name for name in names if name == "CI"]
    middle = [name for name in names if name not in first and name not in last]
    return [stage for stage in (first, middle, last) if stage]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pre-push", "release"))
    parser.add_argument("--base", help="comparison base (default: merge base with repo-guard's trunk)")
    parser.add_argument("--full", action="store_true", help="every package's routine checks")
    parser.add_argument("--product", action="append", help="enabled product to qualify (repeatable)")
    parser.add_argument("-j", "--jobs", type=int, default=int(os.environ.get("CI_LOCAL_JOBS", "2")))
    parser.add_argument("--image", default=circleci.IMAGE, help="job image (default: the pinned digest)")
    args = parser.parse_args()
    root = Path(git("rev-parse", "--show-toplevel"))
    if git("status", "--porcelain", cwd=root):
        parser.error("requires a clean, committed worktree")
    head = git("rev-parse", "HEAD", cwd=root)
    # Not /tmp, which may be RAM-backed: clones and uncached targets are large.
    (CACHE / "runs").mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="run.", dir=CACHE / "runs"))
    for directory in ("logs", "jobs", "tmp"):
        (run_dir / directory).mkdir()
    (CACHE / "registry" / "plan").mkdir(parents=True, exist_ok=True)
    # A self-contained clone: linked worktrees and unstaged files never reach the tested tree.
    snapshot = run_dir / "snapshot"
    subprocess.run(["git", "clone", "--quiet", "--no-local", "--no-checkout", str(root), str(snapshot)], check=True)
    subprocess.run(["git", "-C", str(snapshot), "checkout", "--quiet", "--detach", head], check=True)
    environment = {**pipeline_inputs(args, root), "CIRCLE_SHA1": head,
                   "CIRCLE_WORKFLOW_ID": f"local-{uuid.uuid4()}", "CI_TEMP": "/tmp/ci"}
    print(f"Validating {head} in {run_dir}")
    plan = subprocess.run(podman(args.image, snapshot, run_dir / "tmp", environment,
                                 "python3 scripts/ci/circleci.py plan --dry-run", registry=CACHE / "registry" / "plan"),
                          stdout=subprocess.DEVNULL)
    if plan.returncode:
        sys.exit("planning failed")
    config = json.loads((run_dir / "tmp" / "continuation.json").read_text())
    workflow = next(iter(config["workflows"].values()))["jobs"]
    failed = []
    with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        for stage in stages(workflow):
            results = pool.map(lambda name: run_job(name, config["jobs"][name], image=args.image,
                                                    snapshot=snapshot, run_dir=run_dir, environment=environment), stage)
            failed += [(name, log) for name, ok, log in results if not ok]
            if failed:
                break
    if failed:
        for name, log in failed:
            print(f"\n--- {name}: last lines of {log}")
            print("".join(log.read_text().splitlines(keepends=True)[-30:]))
        sys.exit(1)
    shutil.rmtree(run_dir, ignore_errors=True)  # Failed runs keep their logs and clones.
    print(f"Local CI passed for {head}")


if __name__ == "__main__":
    main()

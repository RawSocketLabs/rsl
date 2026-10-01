#!/usr/bin/env bash
# Run the checked-in GitHub Actions workflow locally through rootless Podman.
#
# Usage:
#   scripts/ci-act.sh                 # list jobs
#   scripts/ci-act.sh pre-push                 # affected coverage against configured trunk
#   scripts/ci-act.sh pre-push --base REF      # explicit comparison base
#   scripts/ci-act.sh pre-push --full          # every validation surface
#   scripts/ci-act.sh -j checks                # check profiles and their dependencies
# Extra arguments pass straight through to act.
set -euo pipefail

cd "$(dirname "$0")/.."

socket="$(podman info --format '{{.Host.RemoteSocket.Path}}' 2>/dev/null || true)"
if [[ -z "${socket}" || ! -S "${socket}" ]]; then
  socket="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock"
fi
if [[ ! -S "${socket}" ]]; then
  echo "error: no podman API socket at ${socket}" >&2
  echo "start it with: systemctl --user start podman.socket" >&2
  exit 1
fi
export DOCKER_HOST="unix://${socket}"

act_bin="$(command -v act 2>/dev/null || true)"
[[ -z "${act_bin}" && -x "${HOME}/.local/bin/act" ]] && act_bin="${HOME}/.local/bin/act"
if [[ -z "${act_bin}" ]]; then
  echo "error: act not found on PATH or at ~/.local/bin/act" >&2
  exit 1
fi

if [[ $# -eq 0 ]]; then
  "${act_bin}" -l
  echo
  echo "run affected validation with: scripts/ci-act.sh pre-push (or --full)"
  exit 0
fi

if [[ $1 == "pre-push" ]]; then
  shift
  ci_full=false
  ci_base=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --full) ci_full=true; shift ;;
      --base)
        [[ $# -ge 2 && -n "$2" ]] || { echo "error: --base requires a ref" >&2; exit 2; }
        ci_base="$2"
        shift 2
        ;;
      *) echo "error: unknown pre-push argument: $1" >&2; exit 2 ;;
    esac
  done
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: pre-push requires a clean, committed worktree" >&2
    exit 2
  fi

  if [[ "$ci_full" == false && -z "$ci_base" ]]; then
    ci_remote="$(git config --get repoGuard.remote || true)"
    ci_trunk="$(git config --get repoGuard.trunk || true)"
    if [[ -n "$ci_remote" && -n "$ci_trunk" ]]; then
      ci_base="$(git merge-base "refs/remotes/$ci_remote/$ci_trunk" HEAD || true)"
    fi
    if [[ -z "$ci_base" ]]; then
      echo "No configured trunk comparison available; selecting full coverage."
      ci_full=true
    fi
  fi
  ci_branch="$(git symbolic-ref --quiet --short HEAD || true)"
  ci_release_prefix="$(python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, "scripts/ci"); from plan import release_policy; print(release_policy(Path.cwd()))')"
  ci_release=false
  [[ "$ci_branch" == "$ci_release_prefix"* ]] && ci_release=true

  # act copies a linked worktree's .git file, not its external object database.
  # The selector needs immutable history, so validate a self-contained local clone.
  # Retain the snapshot as evidence; never delete a caller's checkout or rewrite its refs.
  if [[ -f .git ]]; then
    ci_head="$(git rev-parse HEAD)"
    ci_origin="$(git remote get-url origin)"
    ci_snapshot="$(mktemp -d /tmp/rsl-ci-act.XXXXXX)"
    git clone --quiet --no-local --no-checkout . "$ci_snapshot"
    git -C "$ci_snapshot" remote set-url origin "$ci_origin"
    git -C "$ci_snapshot" checkout --quiet --detach "$ci_head"
    echo "Validating self-contained snapshot: $ci_snapshot ($ci_head)"
    cd "$ci_snapshot"
  fi

  exec "${act_bin}" \
    --pull=false \
    --concurrent-jobs "${ACT_CONCURRENT_JOBS:-2}" \
    workflow_dispatch \
    --input runner=ubuntu-latest \
    --input "full=$ci_full" \
    --input "base=$ci_base" \
    --input "release=$ci_release" \
    -j ci-ok
fi

exec "${act_bin}" --pull=false --input runner=ubuntu-latest "$@"

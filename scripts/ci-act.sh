#!/usr/bin/env bash
# Run the checked-in GitHub Actions workflow locally through rootless Podman.
#
# Usage:
#   scripts/ci-act.sh                 # list jobs
#   scripts/ci-act.sh pre-push        # run every validation surface
#   scripts/ci-act.sh -j test         # run one job and its dependencies
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
  echo "run all validation with: scripts/ci-act.sh pre-push"
  exit 0
fi

if [[ $1 == "pre-push" ]]; then
  shift
  if [[ $# -ne 0 ]]; then
    echo "error: pre-push accepts no additional arguments" >&2
    exit 2
  fi
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: pre-push requires a clean, committed worktree" >&2
    exit 2
  fi

  exec "${act_bin}" \
    --pull=false \
    --concurrent-jobs "${ACT_CONCURRENT_JOBS:-2}" \
    workflow_dispatch \
    --input runner=ubuntu-latest \
    --input full=true \
    -j ci-ok
fi

exec "${act_bin}" --pull=false --input runner=ubuntu-latest "$@"

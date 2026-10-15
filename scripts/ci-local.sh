#!/usr/bin/env bash
# Run the CircleCI pipeline locally, in the same rsl-ci image, under rootless Podman.
#
# Usage:
#   scripts/ci-local.sh pre-push                  # affected coverage against repo-guard's trunk
#   scripts/ci-local.sh pre-push --base REF       # explicit comparison base
#   scripts/ci-local.sh pre-push --full           # every package's routine checks
#   scripts/ci-local.sh release --product NAME    # product release tier, never publication
#   scripts/ci-local.sh pre-push -j 4             # concurrent jobs (default CI_LOCAL_JOBS or 2)
#   scripts/ci-local.sh pre-push --image IMAGE    # an unpublished image, e.g. localhost/rsl-ci:trial
set -euo pipefail

cd "$(dirname "$0")/.."
command -v podman >/dev/null || { echo "error: podman is required" >&2; exit 1; }
exec python3 scripts/ci/local.py "$@"

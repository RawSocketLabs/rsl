#!/usr/bin/env bash
# Explicitly reviewed 0.4.0 -> 0.5 API delta. Do not replace this with a broad lint waiver.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
mode="${1:?usage: check-api-delta.sh all|default|none}"
case "$mode" in
  all) features=(--all-features) ;;
  default) features=() ;;
  none) features=(--no-default-features) ;;
  *) printf 'unknown feature mode: %s\n' "$mode" >&2; exit 2 ;;
esac
actual="$(mktemp /tmp/bnb-api-delta.XXXXXX)"
# Drop the tool's final formatting-only blank line so snapshots pass git diff --check.
cargo +nightly-2026-06-17 public-api -p bitsandbytes --color never "${features[@]}" diff 0.4.0 |
  sed '${/^$/d;}' > "$actual"
diff -u "bitsandbytes/bnb/api-delta-0.5-${mode}.txt" "$actual"

#!/usr/bin/env bash
# Explicitly reviewed 0.5.0 -> 0.6 API delta; temporary breaking-release gate.
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
# Drop the tool's final formatting-only blank line, not any API content.
cargo +nightly-2026-06-17 public-api -p bitsandbytes --color never "${features[@]}" diff 0.5.0 |
  sed '${/^$/d;}' > "$actual"
diff -u "bitsandbytes/bnb/api-delta-0.6-${mode}.txt" "$actual"

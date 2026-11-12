#!/usr/bin/env bash
# Per-unit instruction and cycle counts for every mode of benches/probe.rs.
#
#   benches/probe.sh [executable]
#
# Without an executable it builds the probe with FEATURES (default "tokio": every mode, and
# the build the gate uses) into CARGO_TARGET_DIR or the workspace target. Runs pinned to
# CORE (default 61). Compare two builds by running this on each. Process startup (about
# 0.4M instructions, measured with BNB_PROBE=list) is subtracted before dividing.
set -euo pipefail

core="${CORE:-61}"
exe="${1:-}"
if [[ -z "$exe" ]]; then
    build="$(cargo bench -p bitsandbytes --bench probe --features "${FEATURES-tokio}" \
        --no-run 2>&1)" || { echo "$build" >&2; exit 1; }
    exe="$(sed -n 's/.*Executable .*(\(.*\))$/\1/p' <<<"$build")"
fi
[[ -x "$exe" ]] || { echo "probe executable not found" >&2; exit 1; }
modes="$(BNB_PROBE=list "$exe")"

# The modes below measure the attempt that the incremental and transport readers share
# (`decode_attempt`) as a consumer compiles it: out of line. Whether it is depends on the
# build, so look for the call in each mode's code rather than trust it (DESIGN.md §22).
calls="$(objdump -d -C --no-show-raw-insn "$exe" | awk '
    /^[0-9a-f]+ <.*>:$/ { fn = $0; sub(/^[0-9a-f]+ </, "", fn); sub(/>:$/, "", fn) }
    /call.*decode_attempt::</ {
        to = $0; sub(/.*decode_attempt::<probe::/, "", to); sub(/,.*/, "", to)
        print to " " fn
    }')"
covered() { # covered <message type> <caller pattern> <call sites needed>
    [[ "$(grep -c "^$1 .*$2" <<<"$calls" || true)" -ge "$3" ]]
}
inlined=()
covered Envelope 'probe::repeat::<' 3 || inlined+=("prefix_*envelope")
covered Fixed44 'probe::repeat::<' 2 || inlined+=("prefix_*fixed44")
covered Envelope 'pull' 1 || inlined+=("pull_short")
if grep -q '^stream_' <<<"$modes"; then
    covered Envelope 'read_message::<' 1 || inlined+=("stream_*")
fi
if grep -q '^codec_decode ' <<<"$modes"; then
    covered Envelope 'BinCodec<' 1 || inlined+=("codec_decode")
fi

# count <mode>: "instructions cycles" for one run, or fail loudly.
count() {
    local out
    out="$(BNB_PROBE="$1" taskset -c "$core" \
        perf stat -x, -e instructions:u,cycles:u "$exe" 2>&1 >/dev/null)" ||
        { echo "mode $1 failed: $out" >&2; exit 1; }
    awk -F, '
        $3 ~ /^instructions/ { i = $1 }
        $3 ~ /^cycles/ { c = $1 }
        END { if (i !~ /^[0-9]+$/ || c !~ /^[0-9]+$/) exit 1; print i, c }' <<<"$out" ||
        { echo "mode $1: could not read perf output: $out" >&2; exit 1; }
}

counts="$(count list)"
read -r start_ins start_cyc <<<"$counts"
echo "# $exe"
if [[ ${#inlined[@]} -eq 0 ]]; then
    echo "# core $core; decode_attempt out of line in every attempt mode"
else
    echo "# core $core; decode_attempt INLINED in: ${inlined[*]} (not the consumer's path)"
fi
printf '%-24s %-9s %14s %12s\n' mode tier ins/unit cycles/unit
while read -r mode tier units; do
    counts="$(count "$mode")"
    read -r instructions cycles <<<"$counts"
    awk -v m="$mode" -v t="$tier" -v u="$units" \
        -v i="$((instructions - start_ins))" -v c="$((cycles - start_cyc))" \
        'BEGIN { printf "%-24s %-9s %14.2f %12.2f\n", m, t, i / u, c / u }'
done <<<"$modes"

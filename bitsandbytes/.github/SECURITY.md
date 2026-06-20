# Security Policy

`bitsandbytes` (imported as `bnb`) is a binary codec — **its core job is to parse
untrusted bytes**. That makes its decode path security-relevant, and it's designed and
tested accordingly. This document is the threat model and how to report a problem.

## Reporting a vulnerability

**Please report privately — do not open a public issue.**

Use GitHub's private vulnerability reporting:
**[Security ▸ Report a vulnerability](https://github.com/RawSocketLabs/bitsandbytes/security/advisories/new)**.
That opens an advisory visible only to the maintainers.

A good report includes:

- the affected version or commit (and `rustc` version / target if relevant),
- a **minimal reproducer** — ideally the byte slice plus the `#[bin]`/decode call that
  triggers it (a failing `#[test]` is perfect),
- the impact you observed (panic, abort, hang, over-allocation, memory unsafety, …).

We aim to acknowledge a report within a few business days and to keep you updated as we
triage and fix. Coordinated disclosure is appreciated; we'll credit reporters who want it.

## Supported versions

The crate is pre-1.0, so security fixes land on the **latest released `0.x`** line.

| Version | Supported |
| ------- | --------- |
| latest `0.x` | ✅ |
| older `0.x` | ❌ (please upgrade) |

This table will change once `1.0` ships (see [`bnb/ROADMAP.md`](../bnb/ROADMAP.md), "Road to
1.0").

## What is — and isn't — a vulnerability

`bnb` is **dual-use** (see [`bnb/DESIGN.md`](../bnb/DESIGN.md)): it parses real-world and
deliberately-malformed wire formats, and it does **not** sanitize what it decodes. Knowing
which side of that line a behavior falls on avoids noise reports.

### In scope (please report)

- A **panic, abort, infinite loop, or unbounded memory/CPU** reachable from decoding
  *any* byte input. The decoder's contract is that hostile bytes yield `Ok`/`Err`, never a
  crash — see the `cargo-fuzz` target (`fuzz/fuzz_targets/decode.rs`) and the
  `decode_arbitrary_bytes_never_panics` property.
- Any **memory unsafety** (this should be impossible — see *Security properties* — so a
  report here is high-signal).
- A crafted length/`count` that causes an **over-read past the buffer** or an
  **over-allocation** disproportionate to the input size.
- Incorrect codec output that a peer could weaponize (e.g. a decode/encode asymmetry that
  smuggles bytes).

### Not a vulnerability (intentional, dual-use design)

- **The parser accepting "invalid" or non-compliant input.** By design the decoder is
  permissive and never rejects representable bytes: `#[catch_all]` preserves unknown enum
  discriminants, reserved/flag bits are *retained* rather than zeroed, and `validate` is
  **construction-side only** (it gates `build()`, not decode). If you need strict
  acceptance, enforce it on your own values after decoding — don't expect the parser to.
- A **round-trip faithfully preserving bytes you consider malformed** — that's the dual-use
  guarantee working as intended.
- Resource usage **proportional to a legitimately-sized input** (a large but well-formed
  message producing a large value).

## Security properties

These are the load-bearing guarantees behind the threat model:

- **No `unsafe`.** `unsafe_code = "forbid"` is set workspace-wide — a guarantee an
  `#[allow]` cannot locally reopen. The proc-macros emit no `unsafe` either, so the
  guarantee carries into generated code: there are no memory-safety bugs from `unsafe`
  *by construction*.
- **Continuously fuzzed decode path.** A `cargo-fuzz` target runs in CI under
  ASan/UBSan, asserting that decoding arbitrary bytes never panics and that fixed-length
  parsers are exact bijections.
- **Bounded allocation under a hostile length.** A `#[br(count = N)]` `Vec` grows by
  *pushing* — it never pre-allocates from the attacker-controlled `N`. A huge count on a
  short input errors with `UnexpectedEof` instead of reserving gigabytes (asserted in
  `bnb/tests/bin_count_adversarial.rs`).
- **Graceful, position-aware errors.** Truncated or oversized frames surface a `BitError`
  carrying the bit offset and field name — never a panic or a silent partial parse.

## Scope

This policy covers the crates in this repository: **`bitsandbytes`** (the runtime) and
**`bitsandbytes-macros`** (the proc-macros). Vulnerabilities in *downstream* code that
merely uses `bnb` belong to that project, unless they stem from a defect in `bnb` itself.

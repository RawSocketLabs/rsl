# rsl-netlink — agent & contributor guide

> Inherits the workspace-root `../../../AGENTS.md` and protocol-wide `../../AGENTS.md`.

**What this is.** A strict, typed implementation of the Linux Netlink kernel/userspace protocol.
It covers core `nlmsghdr`/attribute framing, `NETLINK_ROUTE`, Generic Netlink discovery, and the
WireGuard Generic Netlink family. Its controlling specifications are the Linux UAPI headers and
the Linux kernel Netlink documentation; its future refcheck corpus name is `netlink`.

## Start here

- `src/core.rs` owns message and TLV attribute framing.
- `src/transport.rs` owns blocking socket I/O, sequence matching, multipart replies, ACKs,
  overrun handling, timeouts, strict checking, and extended acknowledgements.
- `src/route.rs` owns typed route-netlink messages and operations.
- `src/generic.rs` owns Generic Netlink controller-family discovery.
- `src/wireguard.rs` owns WireGuard family encoding, decoding, updates, and secret-bearing types.
- `DESIGN.md` records why this non-OSI protocol lives under `protocols/system` and why its
  transport remains in the same crate.

## Rules

- Preserve the public package/import names `rsl-netlink` / `rsl_netlink`; the filesystem category
  is not part of the consumer API.
- Treat native endianness, four-byte alignment, stored lengths, nested attribute flags, sequence
  numbers, port IDs, multipart termination, and signed kernel errno conversion as wire semantics.
  Keep the corresponding operations explicit and test them at their boundary.
- Decode only structurally invalid input as an error. Do not turn kernel policy or capability
  constraints into codec restrictions.
- Never accept a response for the wrong request sequence. Surface kernel extended acknowledgement
  text when present, and never silently treat interrupted or overrun dumps as complete state.
- Keep socket creation separable from `Client`; callers such as `nesos` must be able to open a
  socket inside a temporary network namespace and use it after returning to the original one.
- `SecretKey` remains non-`Clone`, redacts `Debug`, and zeroizes on drop. Apply the same posture to
  any new secret-bearing type.
- Keep codecs and state machines in safe Rust. The existing `libc::setsockopt` call is one
  isolated, documented transport boundary for Netlink-specific options that `rustix` does not
  expose; do not spread `unsafe` beyond that boundary.

## Testing

- Fixed framing and attribute tests are standard-derived evidence from the Linux UAPI layout.
- Add negative tests for malformed lengths/alignment and mismatched response state.
- Linux integration tests may query live route and Generic Netlink state but must not mutate host
  networking unless explicitly privileged and isolated.
- Run `cargo test -p rsl-netlink` and `cargo clippy -p rsl-netlink --all-targets` after changes.

## Scope

Protocol codecs, reply-contract validation, and narrowly typed family clients belong here.
Namespace lifecycle, daemon policy, reconciliation ownership, and application-specific workflows
belong in consumers such as `nesos`.

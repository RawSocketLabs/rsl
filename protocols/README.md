# protocols

A Rust workspace of **from-scratch network-protocol implementations**, built for learning the
protocols deeply and for shipping fast, RFC-grounded tooling — a typed, compiled, dual-use
alternative to Scapy / Impacket. Each protocol is its own crate, organized by OSI layer; the
wire codec throughout is [`bnb`](https://github.com/RawSocketLabs/bitsandbytes) (the
`bitsandbytes` bit-aware binary codec).

## Dual-use by default

Every crate is **compliant by default, but deliberately violatable.** The guided path emits and
parses RFC-correct traffic; the raw path lets you forge non-conformant traffic (fuzzing,
red-teaming, interop testing). Parsers accept representable-but-non-compliant input — unknown
values become `Custom(..)`, never a hard error — and never enforce policy. See
[`AGENTS.md`](AGENTS.md) for the full philosophy.

## What's here

- **Protocols** — one crate per protocol, by OSI layer (`link/`, `network/`, `transport/`,
  `session/`, `application/`). Current surface and the adoption roadmap are in the crate status
  table in [`AGENTS.md`](AGENTS.md); the first landed crate is `link/ethertype`, with
  `application/dns` the next (flagship) port.
- **Sibling crates** in the [`rsl` monorepo](https://github.com/RawSocketLabs/rsl) (workspace
  members, path deps): `bnb` (the codec, published as `bitsandbytes`) and `rawsock` (dual-use
  raw-packet I/O, used by the crates that put frames on the wire). `refcheck` (an RFC-compliance
  *observer*, not an enforcer) remains a separate external tool, wired when compliance tracking
  begins.

## Standards

Centralized and CI-enforced: one dependency version each (`[workspace.dependencies]` +
`cargo deny`), aggressive lints (`clippy::all = deny` via `[workspace.lints]`), `rustfmt`,
edition 2024, MSRV 1.85. Independent per-crate SemVer driven by Conventional Commits (scope =
crate name) — see [`VERSIONING.md`](VERSIONING.md).

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without
any additional terms or conditions.

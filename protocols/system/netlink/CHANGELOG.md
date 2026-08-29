# Changelog

## Unreleased

## [0.1.1](https://github.com/RawSocketLabs/rsl/compare/rsl-netlink-v0.1.0...rsl-netlink-v0.1.1) - 2026-08-29

### Other

- updated the following local packages: bitsandbytes

- Replace the Tokio transport with a blocking one: netlink is a local kernel
  round-trip, so every client API is now synchronous and the `tokio` feature and
  dependency are gone. A bounded `poll` (5 s) guards against a kernel that never
  answers.
- Support `not` (inverted) policy rules and `suppress_prefixlength`.

## 0.1.0 - 2026-08-25

- Add strict native-endian netlink and attribute framing using `bitsandbytes`.
- Add serialized Tokio route and generic-netlink transports with sequence,
  multipart, strict-checking, ACK, overflow, and extended-error handling.
- Add typed link, address, route, policy-rule, and WireGuard APIs, including
  multipart peer merging and zeroized, redacted secret keys.

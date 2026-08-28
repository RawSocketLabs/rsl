# Changelog

## 0.1.0 - 2026-08-25

- Add strict native-endian netlink and attribute framing using `bitsandbytes`.
- Add serialized Tokio route and generic-netlink transports with sequence,
  multipart, strict-checking, ACK, overflow, and extended-error handling.
- Add typed link, address, route, policy-rule, and WireGuard APIs, including
  multipart peer merging and zeroized, redacted secret keys.

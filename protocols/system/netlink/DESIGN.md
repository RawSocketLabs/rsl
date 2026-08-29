# rsl-netlink — design

## Placement

Netlink is the Linux kernel/userspace socket protocol family. It has a fixed message header,
aligned TLV attributes, protocol-specific headers, nested families, request sequence rules,
multipart completion, acknowledgements, and structured kernel errors. Those are protocol
semantics, so the crate lives with the monorepo's other protocol implementations.

Netlink does not traverse an OSI network stack. Placing it under `link`, `network`, `transport`,
or `application` would imply a false layering relationship, so `protocols/system` is the explicit
category for local kernel/userspace protocols.

The package remains independently versioned and published as `rsl-netlink`. Relocation within the
monorepo does not rename its Cargo package or Rust crate and therefore does not force an API
migration on downstream crates.

## Boundaries

The crate has four conceptual layers:

1. `core` represents `nlmsghdr`, message framing, and aligned attributes.
2. `transport` owns `AF_NETLINK` socket I/O and validates request/reply state.
3. `route` and `generic` implement the classic route family and Generic Netlink discovery.
4. `wireguard` implements the WireGuard Generic Netlink family and its secret-bearing values.

The blocking transport stays in this crate. Unlike generic packet capture or raw injection,
Netlink transport must interpret sequence numbers, multipart termination, ACK/error messages,
extended acknowledgements, strict checking, and buffer overruns. Splitting those rules from the
codec would divide one protocol state machine across crates. Socket construction remains separate
from `Client` so callers can control the network namespace in which a socket is opened. Direct
FFI is confined to the documented `setsockopt` boundary for Netlink-specific options not exposed
by `rustix`; codecs and protocol state remain safe Rust.

## Non-goals

- Namespace lifecycle, process entry, and bind mounts.
- Application policy, desired-state reconciliation, or ownership of configured resources.
- An async runtime abstraction; the current contract is bounded blocking request/response I/O.
- A generic replacement for every Linux Netlink family before a concrete consumer requires it.

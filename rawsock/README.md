# rawsock

**Dual-use, layered raw-packet I/O.** Transmit *exactly the bytes you give it* at a chosen
layer — an ordinary L4 payload, a hand-built IP header (L3), or a forged Ethernet frame
(L2) — for spoofing, fuzzing, and interop testing.

`rawsock` is the socket-layer half of the [RawSocketLabs](https://github.com/RawSocketLabs)
dual-use philosophy. The protocol crates *encode* bytes (compliant by default, deliberately
violatable, on the [`bnb`](https://github.com/RawSocketLabs/bitsandbytes) codec); `rawsock`
*transmits* those bytes verbatim. It is the sink; the protocol crates are the source.

```rust
use rawsock::{Loopback, Layer, RawIo};

let mut sink = Loopback::new(Layer::Link);
sink.send_raw(&[0xde, 0xad, 0xbe, 0xef]).unwrap(); // verbatim — no validation
assert_eq!(sink.last_sent(), Some(&[0xde, 0xad, 0xbe, 0xef][..]));
```

## The contract

- [`RawIo::send_raw`] puts bytes on the wire **verbatim** — no validation, header
  synthesis, checksum, or length fixing. The escape hatch.
- The [`compose`] model (`Protocol` / `ProtocolExt`) stacks layers with `.payload()` and
  computes derived fields (lengths, checksums) on the *compliant* `encode()`, or skips them
  on the *verbatim* `encode_raw()`. Encoding is lazy + top-down so cross-layer fields (a UDP
  checksum over the IP pseudo-header) work.

## Layers are opt-in

The dual-use power is gated by Cargo feature, per layer — a consumer that doesn't enable a
layer **cannot construct that socket** (misuse is a compile error, not a warning). This
first cut ships the **unprivileged core**:

| feature | layer | backend | status |
|---|---|---|---|
| `transport` *(default)* | L4 | ordinary UDP via `rustix` | ✅ shipped |
| `compute` *(default)* | — | derived-field computation signal | ✅ shipped |
| `network` | L3 | raw IP (`IPPROTO_RAW`/`IP_HDRINCL`) | 🔜 with IP/ICMP |
| `link` | L2 | raw Ethernet (`AF_PACKET`) | 🔜 with ARP/Ethernet |

Plus the always-available [`Loopback`] in-memory backend (tests, any OS, no privilege) and
[`capabilities()`] host probing.

## Upstream crates

Syscalls go through **[`rustix`](https://docs.rs/rustix)** (safe, Linux-only, optional) —
not `libc`/`socket2`/`pnet`/`pcap`. The core here is **100% safe** (`#![forbid(unsafe_code)]`).
The future `link` backend is the sole planned FFI (`libc`, only for the `AF_PACKET`
`sockaddr_ll` bind + `if_nametoindex`), isolated to that module — and to be re-checked
against rustix's `netdevice`/link support first, which may remove the need entirely. See
[`DESIGN.md`](DESIGN.md).

## Not this

- **Not `bnb`.** [`bnb`](https://github.com/RawSocketLabs/bitsandbytes)'s `net` feature
  already does typed whole-message exchange over *ordinary kernel sockets* (UDP/TCP, sync +
  async). `rawsock` never re-implements that; it forges and injects raw bytes below the
  kernel's help. A protocol crate depends on both.
- Not a packet-capture framework, not a compliance enforcer.

## License

MIT OR Apache-2.0.

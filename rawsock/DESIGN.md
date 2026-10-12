# rawsock — design

`rawsock` is the **socket-layer half of the workspace's dual-use philosophy**. The protocol
crates *encode* bytes (compliant by default, deliberately violatable); `rawsock` *transmits
those bytes verbatim at a chosen layer*. It is the sink; the protocol crates are the source.
This design is carried over from the proven predecessor implementation (asyio `rawsock`
rev 3), re-homed as its own bnb-independent repo.

## Confirmed decisions

- **Scope:** Linux-first; macOS/Windows are *additive backends* behind the same `RawIo` trait
  (designed-for, not shipped).
- **Sockets via `rustix`, not `socket2`/`libc`/`pnet`/`pcap`.** rustix gives safe `AF_PACKET`
  and `SOCK_RAW` syscalls with no libc linkage; `IP_HDRINCL` comes free by opening the L3
  socket as `IPPROTO_RAW`. Re-verified against rustix 1.1.4: `netdevice::name_to_index`
  replaces `if_nametoindex`, but there is still no safe link-layer socket address, so the L2
  bind builds a `sockaddr_ll` via `SocketAddrAny::read` in one confined `unsafe` fn. No `libc`.
- **Sync-first**; non-blocking handles leave async additive.
- **Misuse resistance is first-class:** lower layers are opt-in Cargo features, the default
  path uses maximum kernel help (L4), and sends are typed per layer with an explicit raw
  escape hatch.

## Composition model

Stack protocols with each container's `.payload(impl Protocol)` (no operator overloading;
`/` was rejected). A `Protocol` (`encode_with`/`encode_raw_with`/`protocol_id`/`layer`) is
the unit; `Vec<u8>` is a leaf (raw-bytes payload). Derived fields (lengths/checksums) are
computed on `encode` (gated by the `compute` feature, default on), **lazily** so cross-layer
checksums work: `encode` walks the nest top-down handing each layer a `Context` (the
enclosing IP pseudo-header). A typed payload auto-sets the container's demux field (IP
`Protocol` / Ethernet `EtherType`) so packets parse normally by default — overridable for the
dual-use "lying field". Mixing levels (inner `encode()`, outer `encode_raw()`) is supported
via bytes-as-payload.

## Layer ladder

| Layer | caller supplies | kernel does | backend | privilege |
|---|---|---|---|---|
| Transport (L4) | transport payload | IP + Ethernet + checksums | UDP via rustix | none |
| Network (L3) | full IP header + payload | Ethernet | `SOCK_RAW`/`IPPROTO_RAW` | `CAP_NET_RAW` |
| Link (L2) | full Ethernet frame | nothing | `AF_PACKET` | `CAP_NET_RAW` |

L4 is the honest top rung (maximum kernel help); dropping lower is a deliberate opt-down.
bnb's `net` feature already covers *typed* L4 message exchange over ordinary sockets —
`rawsock`'s L4 rung is the thin raw-bytes sink for ladder symmetry, distinct from that.

## What ships

All three rungs: `transport` (L4, default), `network` (L3, `NetworkSocket`), and `link` (L2,
`LinkSocket`), plus `compose`/`loopback`/`capability`. Default features compile and test
anywhere in CI with no `CAP_NET_RAW`. The privileged tests check `open()` against
`capabilities()` everywhere and only perform a real send when the probe reports the layer
available, so they skip cleanly when unprivileged. Running the suite inside
`unshare --user --map-root-user --net` (with `lo` up) yields full `CapEff` and exercises the
real sends.

## Boundary with bnb

`rawsock` is bnb-**independent**: pure raw I/O + `Vec<u8>` composition, no codec. A protocol
crate depends on *both* — it uses `bnb` `#[bin]` to encode its own header bytes and implements
`rawsock`'s `Protocol` to compose + inject:

```rust
impl rawsock::Protocol for Datagram {
    fn encode_with(&self, ctx: &Context) -> Vec<u8> { /* bnb to_bytes + derived fields */ }
}
```

## Windows (deferred)

The concrete backend is deferred; the `RawIo` trait must not preclude WinDivert *or* Npcap.
`OpenError::DriverMissing` is reserved for it.

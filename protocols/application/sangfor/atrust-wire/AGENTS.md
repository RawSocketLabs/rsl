# application/sangfor/atrust-wire

The data-plane wire formats of the Sangfor aTrust client protocol, as `bnb` codecs:
specification §7 and §9 ([`../SPEC-atrust.md`](../SPEC-atrust.md)). refcheck protocol name:
none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — wire layer complete for the TCP tunnel, unverified against a node

What a client and a node exchange when a tunnel connection opens, and the frame header of a
TCP tunnel's data phase. The layouts match the specification's vectors V12, V14, V16 and V17
and the header of V15. Nothing here has been checked against a real node. The L3 tunnel's
frames (specification §8) are not written.

## Architecture

Byte layout only: no I/O, no JSON, no status checks. What both kinds of tunnel connection
share is at the crate root; what only a TCP tunnel uses is in `tcp`. The L3 frames will be a
sibling module.

- `negotiation.rs` — `Negotiation`: the client's greeting and sub-negotiation in one message,
  carrying a JSON document as opaque bytes. `session` and `tcp` build the two openings.
- `request.rs` — `TunnelRequest`: the SOCKS5-shaped request that follows. `connect` for a TCP
  tunnel, `query_vip` for a session-authentication connection.
- `reply.rs` — what a node answers on either connection, one type per step: `MethodReply`,
  `AuthReply`, and for a virtual-address query `VipHead` then `VipAddresses`.
- `tcp/reply.rs` — `ReplyHead` then `ConnectReply`, the answer to a connect request.
- `tcp/frame.rs` — `FrameHeader` and `FrameKind`: the four-byte header of the framed data
  phase. The data is the client's to read, straight into its caller's buffer.

`Endpoint` (address type, address, port) and `tcp::ReplyCode` are `socks::v5`'s, re-exported:
the layouts and the code registry are SOCKS5's, so the workspace keeps one codec for each.
`socks` is built without its I/O features.

## Rules

- **Every field is stored.** Version, marker, method, status, command and flag bytes are
  plain fields, not magics, so an unexpected value decodes and round-trips. Refusing it is
  the client's job (`sangfor-atrust`), which knows what it asked for. Only an address type
  with no defined length fails to decode.
- **A reply is read step by step.** Each type is one step, decoded with `decode_prefix` from
  the front of what has arrived: a short buffer is `Incomplete` (read more and retry), and
  the count returned says where the next step starts. Bytes after the last step are stream
  data and must not be dropped.
- **A head before what a failure may omit.** After a failure code a node may send nothing
  more, so `ReplyHead` is decoded alone and `ConnectReply` only after
  `ReplyCode::Succeeded`. `VipHead` and `VipAddresses` are split the same way: the
  specification does not say what follows a failed query, and this must be settled against
  a real node.
- **On a session-authentication connection `AuthReply` is decoded whole before its status
  is looked at**, so a failure leaves the stream framed (§7.2). On a TCP tunnel a refusing
  node sends the marker and status and nothing more (§9.1), so a client looks at those two
  bytes before it waits for the rest.
- **The JSON is not this crate's.** Building, signing and parsing it belong to
  `sangfor-atrust`; here it is bytes with a 16-bit length, and a longer document does not
  encode.
- **The bound address may decode as a name.** The specification allows only IPv4 and IPv6
  there, but a name has the SOCKS5 layout, so `ConnectReply` keeps it and the client
  refuses it.
- **One deliberate difference from the reference client:** a destination name longer than
  255 bytes does not encode. The reference writes it with a wrapped length byte, which no
  node can parse (specification V14).
- **No validity predicates yet.** What a compliant node sends is checked in `sangfor-atrust`
  against the constants on `Negotiation`, `TunnelRequest`, `ConnectReply` and `FrameHeader`.
  Add predicates here when a second caller (the L3 tunnel) repeats the checks.

## Testing

- **`contract`** — `tests/contract.rs`: the specification's vectors, byte for byte, and the
  tables and failure rules of §7.2 and §9.1.
- **`adversarial`** — `tests/adversarial.rs`: every truncation is `Incomplete`; lengths larger
  than the input; address types with no length; trailing bytes; an off-nominal value in
  every stored field; the two values that do not encode; and 2000 generated inputs per type,
  each of which must settle once and encode back to the bytes it took.

There is no `unit` or `api` layer: the crate has no private logic and no operations beyond
the derived codecs. The fuzz target is `wire` in [`../fuzz`](../fuzz/README.md), which holds
arbitrary input to the same relation.

Run: `cargo test -p sangfor-atrust-wire`.

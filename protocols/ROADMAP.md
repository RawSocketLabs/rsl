# protocols — roadmap

Status and the plan of record. Each protocol lands as its own crate on `bnb`; the crate status
table in [`AGENTS.md`](AGENTS.md) tracks per-crate stage.

## Built

- [x] **Workspace scaffold** — conventions (dual-use doctrine, standards-in-config, per-crate
      SemVer), CI (fmt/clippy/test/deny/MSRV), release automation (tags only, no publish yet),
      onboarding docs, and the per-crate template.
- [x] **`link/ethertype`** — the harness proof: `EtherType` as a `bnb` `#[derive(BitEnum)]` with a
      `catch_all` for unknown values. First dogfood of bnb on a protocol type.
- [x] **`application/dns` — Increment 1 (the pure codec)** — decode (following compression
      pointers inline via `seek`) + uncompressed encode. Header (flat `#[bitfield]` State),
      `Name` as a `#[bin(codec)]` newtype, `RType`/`RClass`/`QType`/`QClass` BitEnums with
      catch-alls, `RData` as a `tag`-dispatched union with a raw-bytes `Custom` fallback (fixing
      the reference crate's 36 misparsing stubbed types), `Message`. Golden wire vectors
      (uncompressed round-trip byte-identical; compressed decoded with names resolved inline) +
      an adversarial suite. Dogfooded, on real DNS: codec newtypes, bulk `read_bytes`, ctx-`tag`
      dispatch + aux ctx + bytes catch-all, `count = expr`, nested-bitfield flattening, absolute
      `seek`. **Increment 2** (encode compression, needs the bnb mutable-state gap) and a
      client (needs `rawsock`) remain.
- [x] **`system/netlink`** — strict native-endian Netlink and attribute framing, route and
      generic-netlink clients, typed link/address/route/rule operations, and WireGuard family
      support. Its blocking `rustix` transport owns sequence/ACK/multipart/extack validation and
      stays with the protocol because those rules define the kernel/userspace exchange.

## Protocol adoption order

Pull each protocol in as a `bnb` rewrite. Order favors dogfooding value and low coupling:

1. [~] **`application/dns`** — the pure codec (Increments 1 + 2: decode, uncompressed +
       compressed encode) **and a synchronous UDP resolver client** (the `client` feature,
       `dns::Resolver`) built on bnb's `net` `MessageDatagram` — **not** `rawsock` (a normal
       resolver needs no raw sockets; a dual-use spoofing client is the `rawsock` case).
       The resolver now also does **DNS-over-TCP fallback** (UDP → TCP on a truncated response). **Remaining**: EDNS(0), caching.
2. [~] **`transport/tcp`** done (header codec: `Control` `#[bitfield]` flags word, raw options
       sized by `data_offset` + a `TcpOption` structured view, dual-use stored checksum/offset).
       **`transport/udp`** done (header codec **plus** the `inject` feature: a `rawsock::Protocol`
       `Udp<P>` layer + pseudo-header `udp_checksum` — the socket layer's first on-the-wire
       consumer, the `rawsock` extraction trigger, now live). **TCP has the same `inject` layer**
       (`Tcp<P>` + `tcp_checksum`). Remaining transport follow-up: privileged L3 send (needs
       rawsock's `network` backend — in progress).
3. [x] **`network/ip`** + **`network/icmp`** done. `ip`: IPv4 header codec + the `inject` `Ip<P>`
       layer that supplies the L4 pseudo-header and computes the header checksum. `icmp`: RFC 792
       header codec + an `Icmp<P>` inject layer whose checksum is **self-contained** (no
       pseudo-header). A full `Ip(Udp/Tcp/Icmp(..))` stack emits a correct datagram — every
       checksum verifies (a composed ping is a real, valid packet).
4. [ ] **`link/ethertype` consumers: `link/arp`, `link/ethernet`** — the one real
       protocol-to-protocol chain.
5. [~] **`session/socks`** — RFC 1928/1929 wire slices implemented on `bnb`: method negotiation,
       complete method/CMD/REP/ATYP registries, IPv4/domain/IPv6 endpoints, requests, replies, and
       username/password authentication messages. Blocking and optional Tokio CONNECT clients,
       embedded server handshakes, and bounded listening proxies now share strict session
       validation, explicit authentication/destination policy, and half-close-aware relay.
       An optional Mio backend adds resumable client/server handshakes and an owned-poll proxy.
       Blocking embedded BIND now has two-reply client/server stages, explicit inbound-peer
       approval, and TCP helpers with bounded accept and separate phase deadlines.
       Remaining: managed/Tokio/Mio BIND, explicit raw/malformed codec, GSS-API,
       UDP ASSOCIATE, SOCKS4, and SOCKS4A. Windows/macOS Mio runtime qualification is deferred.
       Application protocols then continue as demand dictates: `tftp`, `smb`, `nbt`, `ssh`, …

## Draft harvest

The `draft/protocols-*` branches hold binrw-era code migrated from a retired org. None of it is
kept as code: each protocol is rewritten on `bnb` to the crate template in `AGENTS.md`. Following
the socks rule (*recover behavior and evidence at the smallest owning layer*), each ledger below
lists what a draft is worth as evidence — golden vectors, edge cases, registries, behaviours,
decisions, and draft bugs to guard against — with its spec clause and where it lands. A row is
evidence, not truth: verify it against the pinned spec when porting. `Draft source` paths are on
the named branch, which stays as read-only reference until its protocol's port lands (its PR is
closed); then the branch is deleted and the ledger's remaining rows move into the crate.

- **ssh** (`draft/protocols-ssh`, PR #63) — nothing recovered: a partial RFC 4253 §6.3 cipher-name
  list (with `edes-cbc` for `3des-cbc`) and a packet struct, both re-derivable from the spec.
- **dcerpc** (`draft/protocols-dcerpc`, PR #62) — nothing recovered: a bare C706 PDU-type enum,
  flags and header struct, re-derivable from the spec.

### tftp (`draft/protocols-tftp`, PR #59)

The draft is worth mining for behaviour and edge cases, not for vectors or code. The lock-step core is mostly right: Sorcerer's Apprentice guard, final block judged by payload length, the empty block after an exact multiple, OACK→ACK(0) on reads and OACK→DATA(1) on writes, and netascii translated over the whole stream. But its "RFC" contract vectors are invented, its decoder breaks byte identity in several places, it has no `Custom` packet, and neither the client nor the server enforces the option RFCs' OACK rules (blksize ≤ requested, timeout must match, ERROR 8 for unrequested options). Next step: open the bnb rewrite with the RFC-figure vectors and the adversarial rows below in `tests/contract.rs`/unit tests, and record the bug rows as ROADMAP items for the high-level layer. Retransmission and option behaviour should be rebuilt sans-I/O and tested there. Do not port `transfer.rs`, `RawClient` or `tests/common`.

| Item | Kind | Draft source | Spec | Verdict |
|---|---|---|---|---|
| Contract "vectors taken straight from RFC 1350" are invented (`rfc1350.txt`, `out.bin`, `abc`). RFC 1350 has no byte examples. Use the RFC figure packets instead: 2347 RRQ `foofile` blksize 1432 + OACK; WRQ `barfile` blksize 2048 + OACK (erratum 1258, Verified: the WRQ example's arrow is labelled "RRQ"); 2348 RRQ `foobar` blksize 1428; 2349 RRQ `foobar` timeout 1 and WRQ `foobar` tsize 673312; 7440 RRQ `foobar` windowsize 16 | vector | `tests/contract.rs:1-3,9-89` | `rfc2347` Examples; `rfc2348` Blocksize Option Spec; `rfc2349` Timeout/Tsize Spec; `rfc7440#3` | keep (replace) → `tests/contract.rs`: rebuild from the RFC figures (verified against figure text) |
| Stale-ACK guard: the sender ignores a duplicate or older ACK and never resends the current DATA because of one | behaviour | `src/transfer.rs:150-171` (`:162`) | `rfc1123#4.2.3.1` (MUST) | keep → `tests/regression.rs` `//~ verifies` (verified; the draft has no test for it) |
| Bug: every stale ACK, wrong-TID datagram or undecodable datagram calls `recv_packet`/`recv_from` again with a fresh timeout and, for stale ACKs, `retries = 0`. A peer that trickles junk keeps the transfer alive forever (livelock/DoS) | bug | `src/transfer.rs:76-110`, `:158-162`, `:193-210` | `rfc1350#7` ("timeouts must also be used") | keep → regression: retries and deadline must be per block, not per datagram (verified by reading) |
| Bug: fixed 3 s timeout, no backoff | bug | `src/transfer.rs:24-28`, `src/client/client.rs:247-250` | `rfc1123#4.2.3.2` (MUST adaptive, at least exponential backoff) | keep → ROADMAP (verified) |
| Bug: the client accepts any OACK. A blksize larger than requested or never requested is adopted, a timeout that differs is applied, unrequested options are ignored instead of ERROR 8, and an RRQ tsize is never checked (ERROR 3 option) | bug | `src/client/client.rs:293-312` | `rfc2347` Negotiation Protocol ("should respond with … error code 8"); `rfc2348` (≤ requested, use it or ERROR 8); `rfc2349` (timeout "must match", tsize ERROR 3) | keep → regression with crafted OACKs, raw-path ERROR 8 (verified) |
| Bug: the server acknowledges `timeout` in its OACK but never applies it; the socket keeps the server default | bug | `src/server/server.rs:165-166`, `:318` | `rfc2349` Timeout Interval ("number of seconds to wait before retransmitting") | keep → regression (verified) |
| The OACK path is correct: RRQ+OACK → client ACK(0); WRQ+OACK → client sends DATA 1; server ignores options → DATA or ACK(0) and blksize stays 512; the server retransmits OACK or ACK(0) on timeout | behaviour | `src/client/client.rs:150-175`, `:228-241`; `src/server/server.rs:199-211`, `:236-262`, `:291-302` | `rfc2347` Negotiation Protocol | keep → `tests/integration.rs` (verified; add the "server ignores options" case, which the draft never tests) |
| Final block is judged by payload length < blksize, not by datagram length. RFC 1350 §6 says datagram length < 516, and two Verified errata contradict each other (4971: 512; 7167: 524) | decision | `src/transfer.rs:139`, `:200` | `rfc1350#6` + errata 4971/7167; `rfc2348` ("data length less than the negotiated blocksize") | keep → DESIGN.md + contract test (verified) |
| Exact multiple of blksize ends with an empty DATA; e2e size matrix `[0,1,511,512,513,1024,1500,4096]` | edge case | `tests/regression.rs:77-84`, `tests/e2e.rs:15` | `rfc2348` ("integral multiple … extra data packet containing no data"); `rfc1350#5` | keep → `tests/e2e.rs` sizes; add blksize-relative sizes (verified) |
| Block-number rollover: the draft wraps 65535 → 0 on both sides (`wrapping_add`, duplicate check `wrapping_sub(1)`). RFC 1350 only says blocks "begin with one and increase by one". Rollover is undefined, so files above 65535 × blksize (≈32 MiB at 512) need a declared policy (0 vs 1) | decision | `src/transfer.rs:141`, `:201`, `:207` | `rfc1350#4`, `rfc1350#5` (silent); `rfc7440` search for rollover/wrap: no match | keep → DESIGN.md decision (configurable, wrap to 0 by default) + a regression at block 65535 (verified absent from spec) |
| Bug: the first reply is accepted from any source (`expected = None`), so any off-path host can claim the transfer. RFC 1350 §4 fixes only the port as new; the server IP is known | bug | `src/client/client.rs:139-146`, `:220-227` | `rfc1350#4` | keep → regression: first reply must come from the server's IP (verified by reading) |
| Undecodable datagrams from the locked TID are silently dropped. RFC 1350 lists an incorrectly formed packet as a terminating error (ERROR 4) | bug | `src/transfer.rs:92-98` | `rfc1350#2` | keep → ROADMAP: guided path sends ERROR 4 and stops; raw path free (verified) |
| Wrong-TID datagram → ERROR(5) to that source, transfer continues | behaviour | `src/transfer.rs:80-91`; `tests/regression.rs:36-64` | `rfc1350#4`, `rfc1350#2` | keep → `tests/regression.rs` `//~ verifies` (verified) |
| `Packet::decode` returns an error on an unknown opcode even though `Opcode::Custom` exists; requests reject opcodes other than 1/2. This violates the doctrine: there must be `Packet::Custom { opcode, bytes }` | bug | `src/wire/packet.rs:59-71`; `src/wire/request.rs:104-105` | `rfc1350#5` registry; protocols/AGENTS.md dual-use | keep → contract: opcode 0, 7, 99, 0xFFFF round-trip as `Custom` (doctrine check) |
| Mode and option names are case-folded on decode (`NETASCII` → `netascii`, `BLKSIZE` → `blksize`, `Other.name` lower-cased), and non-UTF-8 values go through `from_utf8_lossy`, so decode → encode is not byte-identical | edge case | `src/wire/mode.rs:44-51`; `src/wire/option.rs:57-76`; `src/wire/request.rs:116-119` | `rfc1350#5` (any case); `rfc2347` (case-insensitive; OACK name "copied from the original request") | keep → contract: `\x00\x02file\0NETASCII\0` and `BlkSize\01024\0` re-encode byte-identically (verified) |
| Option value `"+1024"` / `"01024"` parse to `BlkSize(1024)` (Rust `u16::from_str` accepts `+`) and re-encode as `"1024"`. The raw value text must be kept beside the typed view | edge case | `src/wire/option.rs:60-62` | `rfc2348` ("specified in ASCII") | keep → unit adversarial (draft behaviour not run) |
| Option ranges: blksize 8..=65464, timeout 1..=255, tsize unbounded, windowsize 1..=65535 (missing from the draft). Out-of-range values (`blksize 4`, `timeout 0`) are kept as raw, not rejected | registry | `src/wire/option.rs:14-29`, `:57-76`, `:137-151` | `rfc2348`; `rfc2349`; `rfc7440#3` (MUST 1..=65535) | keep → unit + contract (verified); add windowsize |
| Duplicate options are decoded and kept, the lookup helpers take the first, and the server echoes every copy into the OACK | edge case | `src/wire/option.rs:93-114`; `src/server/server.rs:313-329` | `rfc2347` ("an option may only be specified once") | keep → decode keeps duplicates; builder `validate` rejects them; server acknowledges one (verified) |
| The request builder does not enforce the 512-octet request limit | edge case | `src/wire/request.rs:134-145` | `rfc2347` Packet Formats ("maximum size of a request packet is 512 octets") | keep → builder `validate` + raw override test (verified) |
| Trailing bytes are mishandled: a lone NUL or odd field after the mode fails the whole RRQ under `options`; without `options` the extra bytes are ignored and lost on re-encode. ACK and ERROR bytes after the fixed fields are probably dropped too (binrw does not check EOF) | edge case | `src/wire/request.rs:109-122`; `src/wire/ack.rs:15-21`; `src/wire/error_packet.rs:19-27` | `rfc1350#5`; `rfc2347` | keep → adversarial: `…octet\0\0`, `…octet\0blksize\0`, `\x00\x04\x00\x01\xff` must decode losslessly (ACK/ERROR behaviour not run) |
| Server silently serves `mail` and any `Custom` mode as octet | bug | `src/server/server.rs:190-194`, `:267-271` | `rfc1350#1` (mail "should not be implemented"); `rfc1123#4.2.2.1` (SHOULD NOT) | keep → ROADMAP: guided server rejects unsupported modes with ERROR (code is policy; spec names none) |
| netascii: LF ↔ CR LF, lone CR ↔ CR NUL, any other CR passed through. Translation runs over the whole stream, so CR/LF pairs split across blocks are safe. Vectors `a\rb` ↔ `a\r\0b`, `line1\nline2\n` ↔ `line1\r\nline2\r\n` | vector | `src/netascii.rs:23-66`, `:72-86`; `src/client/client.rs:254-260` | `rfc1350#1`, `rfc1350#5` (→ RFC 854, not in corpus) | keep → unit vectors + a block-boundary CR test (CR NUL rule not verified against RFC 854) |
| Server WRQ: `handler.write` runs after the final ACK has been sent, so a refusal at that point goes out as an ERROR the client never sees; it believes the upload succeeded | bug | `src/server/server.rs:264-275` | `rfc1350#6`, `rfc1350#7` | keep → ROADMAP: commit or refuse before the final ACK (verified by reading) |
| No dally after sending the final ACK: `recv_file` returns at once and cannot re-ACK a retransmitted final DATA | behaviour | `src/transfer.rs:196-204` | `rfc1350#6` ("dallying is encouraged") | keep → ROADMAP (verified) |
| IPv4-only: the client, server-transfer and raw sockets all bind `0.0.0.0:0`, so IPv6 peers fail; non-UTF-8 filenames reach `Handler` through `filename_lossy` and different names collide on U+FFFD | bug | `src/client/client.rs:248`; `src/server/server.rs:165`, `:168`; `src/client/raw.rs:36` | `rfc1350#5` (filename "sequence of bytes") | keep → ROADMAP (verified by reading). `malformed` byte strings (`raw.rs:112-135`) → adversarial unit tests; drop `RawClient`, `Handler(Vec)`, the semaphore and `tests/common` as transport/test infrastructure |

### socks (`draft/protocols-socks`, PR #64)

Main already covers everything the draft has for SOCKS5 CONNECT, blocking BIND, method/CMD/REP/ATYP registries, RFC 1929, truncation and unknown-code cases, wrong-version and unknown-command rejection, silent-client and BIND accept timeouts. The draft's remaining value is in SOCKS4/4A vectors and registries, the RFC 1928 §7 UDP header and its size arithmetic, the UDP relay's source-pinning behaviour, and a list of malformed generators. It also has several bugs that main should not reproduce: unbounded USERID reads, a 4A BIND that can never succeed, a UDP relay with no destination policy, and a 65 535-byte datagram ceiling. Recommended next step: land a SOCKS4/4A wire slice with the rows marked `tests/contract.rs`, using the design rows below as its acceptance criteria, before any exchange code; UDP ASSOCIATE follows as its own slice.

Spec sources: RFC 1928 and RFC 1929 come from the `stds` corpus (published text, which is not a project baseline; errata checked 2026-09-19). The SOCKS4, SOCKS4A (openssh.com `socks4.protocol`, `socks4a.protocol`) and RFC 1961 texts were checked against a local copy; they are not in `stds` yet. `socks4#1` means the memo's "1) CONNECT" section and `socks4#2` its "2) BIND" section; both documents are a single `front` section.

| Item | Kind | Draft source | Spec | Verdict |
|---|---|---|---|---|
| SOCKS4A CONNECT vector `04 01 00 50 00 00 00 01 00 "www.example.com" 00`: marker 0.0.0.1, empty USERID, NUL-terminated name | vector | `tests/v4_contract.rs:78` | `socks4a#front` | keep → `tests/contract.rs` (verified) |
| SOCKS4 CONNECT vector `04 01 00 17 42 66 07 63 "Fred" 00`. The draft says it is "the canonical worked example from the memo", but the memo has no byte example | vector | `tests/v4_contract.rs:13` | `socks4#1` | keep → `tests/contract.rs` (layout verified). Drop the provenance claim and label it synthetic |
| SOCKS4 reply vectors: granted `00 5A 00 00 00 00 00 00`, and a BIND-style reply `00 5A 04 38 7F 00 00 01`. Reply VN is 0, not 4 | vector | `tests/v4_contract.rs:51`, `src/v4/reply.rs:92` | `socks4#1`, `socks4#2` | keep → `tests/contract.rs` (verified) |
| SOCKS4 CD-reply registry: 90 granted, 91 rejected/failed, 92 identd unreachable, 93 identd mismatch, any other value preserved, all 256 values round-trip | registry | `src/v4/reply_code.rs:12` | `socks4#1` | keep → v4 `codes.rs` unit (verified). Keep 92/93 in the registry; IDENT (RFC 1413) itself is out of scope |
| SOCKS4 CD-request registry: 1 CONNECT, 2 BIND, any other value preserved, all 256 values round-trip | registry | `src/v4/command.rs:14` | `socks4#1`, `socks4a#front` | keep → v4 `codes.rs` unit (verified) |
| 4A marker predicate: 0.0.0.x with x≠0. 0.0.0.0, 1.0.0.1 and 127.0.0.1 are not markers. The parser keeps x as received | edge case | `src/v4/request.rs:108`, `:186` | `socks4a#front` | keep → `tests/contract.rs` (verified) |
| Framing: parsing stops exactly at the USERID or DOMAIN NUL and leaves trailing bytes unread | edge case | `src/v4/request.rs:220`, `:248` | `socks4#1` | keep → `tests/adversarial.rs` handoff case (verified) |
| USERID/DOMAIN are read one byte at a time with no length limit, so a peer that never sends NUL makes the server buffer without bound | bug | `src/v4/request.rs:167` | `socks4#1` (no length limit in the spec) | keep → ROADMAP: bound by `MAX_FRAME_LEN` as a resource limit. It is not a parser rule, so representable input still decodes |
| Whether the 4A domain is parsed depends on a `v4a` cfg flag. With the flag off, a marker request's domain bytes are left in the stream and relayed to the target as application data | bug | `src/v4/request.rs:136` | `socks4a#front` ("must read in the domain name") | keep → ROADMAP decision: always decode the 4A tail. Settles DESIGN's open question that SOCKS4/4A should not be feature-gated at the wire layer |
| `domain: Option` sits next to a free `dest_ip`. A domain with a non-marker IP encodes but does not decode back, the same mismatch main fixed with `Endpoint` | decision | `src/v4/request.rs:60` | `socks4a#front` | keep → ROADMAP: model the target as an enum `{Ip, Domain{x, name}}`, and keep the mismatched form only on the raw path |
| 4A BIND bug: the server compares the inbound peer against the 0.0.0.x marker, so a BIND by domain is always rejected. The client's `bind()` cannot send a domain at all | bug | `src/server/v4/mod.rs:347`, `src/client/v4/mod.rs:196` | `socks4a#front` ("used for both CONNECT and BIND") | keep → `tests/regression.rs`: compare against the resolved address |
| The BIND peer check is skipped when DSTIP is 0.0.0.0. The memo has no such exception (a mismatch means CD 91 and closing both connections) | bug | `src/server/v4/mod.rs:347` | `socks4#2` | keep → ROADMAP: any wildcard must be explicit policy, not a silent skip |
| First BIND reply with DSTIP 0.0.0.0: the client substitutes the proxy IP (verified). The draft also substitutes in the second reply, which the memo does not specify | behaviour | `src/v4/reply.rs:64`, `src/client/v4/mod.rs:239` | `socks4#2` | keep → `tests/contract.rs` for the first reply only (verified). Drop the second-reply substitution |
| Time limit: the memo sets one for both CONNECT and BIND (2 min in CSTC). The draft bounds only BIND (120 s); its CONNECT dial is unbounded | bug | `src/server/v4/mod.rs:53`, `:295` | `socks4#2` (closing paragraph) | keep → ROADMAP: reuse main's dial and accept budgets for v4 |
| USERID is a claimed identity. The draft's authorizer grants everything by default and treats the userid as access control | decision | `src/server/v4/mod.rs:85`, `:202` | `socks4#1` (server may check userid/IDENT) | keep → ROADMAP: pass USERID to `Policy` as operation input, never as authentication. Drop the grant-all default |
| The guided client sends USERID/host without rejecting an interior NUL, which misframes the request. It also imposes a 1–255 host limit that the 4A spec does not have | bug | `src/client/v4/mod.rs:96`, `:167` | `socks4a#front` | keep → ROADMAP: the guided builder rejects interior NUL and the raw path allows it. Drop the 255 limit, or document it as `deviates` |
| A SOCKS4 BIND listener must be IPv4: on an IPv6 control socket the reply degrades to 0.0.0.0 and the client cannot substitute | edge case | `src/server/v4/mod.rs:308`, `:366` | `socks4#2` | keep → ROADMAP |
| UDP request-header vector `00 00 00 01 7F 00 00 01 1F 90`: RSV, FRAG 0, IPv4, port 8080 | vector | `src/v5/udp_header.rs:46` | `rfc1928#7` | keep → `tests/contract.rs` (verified) |
| UDP header size per ATYP: 10 (IPv4), 22 (IPv6), 7+len (domain), worst case 262. The published RFC says 20 for IPv6, which Verified erratum 3198 corrects to 22, and the draft matches the erratum | edge case | `src/client/udp.rs:11`, `:170` | `rfc1928#7` + erratum 3198 | keep → `tests/contract.rs` (verified) |
| The datagram ceiling is set to 65 535. The real UDP payload limit is lower (65 507 over IPv4), so `max_payload` overstates it and large sends fail. Wrapping a full-size reply also overflows silently | bug | `src/client/udp.rs:14`, `src/server/udp.rs:183` | `rfc1928#7` ("smaller than the actual space provided by the OS") | keep → ROADMAP: compute from the socket's real limit |
| FRAG handling: the relay drops FRAG≠0 (correct), but the client returns `NotSupported` to the caller instead of dropping. No reassembly anywhere | behaviour | `src/server/udp.rs:143`, `src/client/udp.rs:122` | `rfc1928#7` ("MUST drop"; reassembly timer ≥5 s) | keep → ROADMAP: drop on both sides, no reassembly. The raw path must still emit any FRAG value, including the high bit |
| Relay source pinning: the client IP comes from the control connection, never from a datagram; the client port is learned from the first datagram; replies are relayed only from contacted targets, capped at 1024. The draft's regression test cites the "other source IP" MUST, but its rogue sender shares 127.0.0.1, so it actually tests the stricter port and solicited-only rule | behaviour | `src/server/udp.rs:46`, `:125`, `:163`; `tests/regression.rs:18` | `rfc1928#7` | keep → ROADMAP + `tests/regression.rs` (IP MUST verified; port/solicited rules are deliberate extras). Fix the citation |
| The UDP relay resolves each datagram's DST (including domains) inline with blocking DNS and no destination policy, a policy bypass. The client also accepts datagrams from any source, not only the relay | bug | `src/server/udp.rs:150`, `src/client/udp.rs:118` | `rfc1928#6` UDP ASSOCIATE (server "MAY" limit) | keep → ROADMAP: run `Policy` on every datagram's resolved target and filter the client's receive by relay address |
| ASSOCIATE request and reply: the client sends 0.0.0.0 with its real port, but the server ignores the port whenever the address is zero. The client replaces an unspecified BND.ADDR with the proxy IP (common practice; not in the RFC). The association lasts as long as the control TCP connection, and control bytes are discarded | decision | `src/client/udp.rs:46`, `src/server/udp.rs:51`, `:67` | `rfc1928#6` UDP ASSOCIATE | keep → ROADMAP: decide whether to honour the port when the address is zero, and record BND substitution as `deviates` |
| Malformed generators: v5 VN=4 greeting, overstated NMETHODS (3 vs 1), RSV=0xFF request, CMD 0x09; v4 VN=5, CD 3, arbitrary USERID | registry | `src/client/raw.rs:145`, `src/client/v4/raw.rs:104` | `rfc1928#3`, `rfc1928#4`, `socks4#1` | keep → ROADMAP (raw/malformed list). Main already tests the v5 rejections. Drop the draft's tolerance of non-zero RSV, which contradicts main's server |
| GSS-API: the draft has only the 0x01 code and an "extension trait over the raw stream, separate crate" note; no RFC 1961 framing | decision | `src/auth.rs:49`, `src/lib.rs:102` | `rfc1961#3.8` (ver 0x01, mtyp 0xff abort) | drop: no evidence. Main already has `AuthMethod::GssApi`; start from RFC 1961 directly |

Skipped because main covers it or it is draft infrastructure: v5 codecs and registries, RFC 1929, CONNECT/BIND transcripts, silent-client and BIND timeouts, the v4 version-mismatch reply (main closes without guessing a reply), the thread pool, relay, benches, observability and e2e harnesses.

### nbt (`draft/protocols-nbt`, PR #60)

Keep the draft as evidence only: RFC golden vectors, registries, field order and about a dozen bugs worth turning into regression tests. Its wire code is wrong where it matters (LSB-first `modular_bitfield` in label, NB_FLAGS and session FLAGS; names never first-level encoded; wrong builder opcodes and flags; NBSTAT names parsed as labels; no datagram service). Share only main's `dns::Name` + `CompressionDict` and the TCP length framing, since NBT's header, Op, RCode and RType collide with DNS. Next step: a bnb `nbt` crate that depends on `dns` for `Name`, seeded from rows 1–4. (Checked against the draft's vendored RFC 1001/1002 text, not `stds`, which lacks both; vectors checked with python3; errata not checked; paths relative to `protocols/application/nbt/`.)

| Item | Kind | Draft source | Spec | Verdict |
|---|---|---|---|---|
| `FRED` + scope `NETBIOS.COM` → 46 bytes `20 4547464345464545 4341…41 07 4E455442494F53 03 434F4D 00` | vector | `resources/rfc1002_netbios_tcp_udp_detailed_sepcs.txt:276-330` | `rfc1002#4.1` | keep → `tests/contract.rs` (verified: diagram bytes extracted and compared with computed encoding; equal) |
| Broadcast name `*` + 15×`0x00` → `CKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.NETBIOS.SCOPE`. The draft pads every name with spaces (`format!("{:16}")`), giving the wrong `CKCACA…` | vector / bug | `src/name/name.rs:140` | `rfc1001#17.2` | keep → `tests/contract.rs` (verified by computation); padding byte is the caller's choice, not hardcoded space |
| RFC's own example `FEGHGFCAEOGFHEECEJEPFDCAHEGBGNGF` decodes to `"Tge NetBIOS tame"`; correct encoding of `"The NetBIOS name"` is `FEGIGFCAEOGFHEECEJEPFDCAGOGBGNGF` (scope `SCOPE.ID.COM`) | vector / bug (RFC) | `resources/rfc1001_netbios_tcp_udp_concepts_and_methods.txt:1537-1540` | `rfc1001#14.1` | keep → `tests/contract.rs` with corrected string and a note (verified by computation; erratum status not checked) |
| `Workstation001` (space-padded to 16) → `FHGPHCGLHDHEGBHEGJGPGODADADBCACA`; mixed case is preserved (RFC has no uppercasing) | vector | `src/name/name.rs:133-136` | `rfc1001#14.1` | keep → inline `mod unit` (verified by computation) |
| First-level encoding covers all 16 raw bytes 0x00–0xFF, including the suffix. Draft truncates with `char as u8`, mixes byte `len()` with char padding, and `NameType::to_string` panics on suffixes ≥0x80 (0x87/0xBE/0xBF) through `from_utf8().unwrap()` | edge case / bug | `src/name/name.rs:147, 272-274` | `rfc1001#14.1` | keep → `mod unit` regression (suffix 0xBE, non-ASCII byte) |
| `first_level_decode` panics on odd length (`c[1]`), bytes <0x41 (u8 underflow) and non-UTF-8 output | bug | `src/name/name.rs:159-169` | `rfc1001#14.1` | keep → `tests/adversarial.rs`: a non-A–P or odd-length first label decodes as a raw label and never panics |
| Scope ID is any DNS domain name with no leading/trailing dot. Draft `scope_checks` requires exactly 2 labels and so rejects the RFC's `SCOPE.ID.COM` | edge case / bug | `src/name/name.rs:115-123` | `rfc1001#14.1`, `rfc1002#6` (SCOPE_ID) | keep → `tests/contract.rs` (3-label scope, empty scope) |
| First name byte must be `0x20` (32-byte label) or `11xxxxxx` pointer, with `01`/`10` reserved. Label pointers are allowed only in Name Service packets, never in Session or Datagram | decision | `src/name/label.rs:75-120` | `rfc1002#4.1` | keep → `DESIGN.md`: a decoded non-32 first label stays raw (Custom, not rejected); session/datagram names encode through the uncompressed `Name` path |
| Second-level codec = `dns::Name` (raw byte labels, bounded pointer following, 63/255 limits, opt-in `CompressionDict`). TCP NBNS uses the same 2-byte length prefix as DNS/TCP | decision (share) | `src/name/label.rs` (whole), `src/name/name.rs:171-219` | `rfc1002#4.1`, `rfc1002#4.2.1`, `rfc1035#4.1.4` | keep → dns (already `pub use name::{CompressionDict, Name}`); nbt depends on it. Drop draft `Label`/`parse_labels` |
| Label, pointer, NB_FLAGS, NAME_FLAGS and STATISTICS use upstream `modular_bitfield` (LSB-first, little-endian multi-byte), so the marker lands in the low 2 bits: `0x20` parses as length 8, and length 32 encodes as `0x80` (reserved marker) | bug | `src/name/label.rs:2, 79-84, 179-187`; `src/name/resource.rs:5` | `rfc1002#4.1`, `rfc1002#4.2.1.3` | drop: wire-incorrect (reasoned from the `modular-bitfield-msb` README's stated MSB/LSB difference, not executed) |
| NB_FLAGS `G\|ONT(2)\|res(13)` and NAME_FLAGS `G\|ONT\|DRG\|CNF\|ACT\|PRM\|res(9)`, ONT 00=B 01=P 10=M 11=reserved. Draft field order is right, packing wrong | registry | `src/name/resource.rs:153-168, 199-209` | `rfc1002#4.2.1.3`, `rfc1002#4.2.18` | keep → `mod unit` (G=0x8000, ONT M=0x4000, PRM=0x0200; verified against clause bit tables) |
| NB RDATA is an ADDR_ENTRY array (6 bytes each, RDLENGTH=6n, TC set when truncated). Draft reads one entry of `len - 2` bytes (underflow panic when len<2) and admits 16-byte IPv6 | bug / edge case | `src/name/resource.rs:98-105, 170-189` | `rfc1002#4.2.13` | keep → `tests/contract.rs` (2-entry group response) and `tests/adversarial.rs` (RDLENGTH 0/5/7: raw tail kept) |
| NBSTAT NODE_NAME entries are 16 raw bytes + NAME_FLAGS: the protocol's only name that is not second-level encoded. Draft parses them as labels | bug | `src/name/resource.rs:191-197` | `rfc1001#14`, `rfc1002#4.2.18` | keep → `tests/contract.rs` node-status response vector |
| STATISTICS layout: UNIT_ID(6) JUMPERS TEST_RESULT, six u16s, GOOD_SENDS/RECEIVES u32, nine u16s = 46 bytes. Draft order matches the clause | registry | `src/name/resource.rs:128-151` | `rfc1002#4.2.18` | keep → ROADMAP; decode bounded by RDLENGTH with a short/long tail kept raw (interop with real stacks not verified) |
| Opcodes 0/5/6/7/8, but §4.2.4's REFRESH diagram shows `0x9` against the table's 8. Draft `AltRefresh = 9` records this | registry / edge case | `src/name/opcode.rs:42-60` | `rfc1002#4.2.1.1`, `rfc1002#4.2.4` | keep → registry: decode both 8 and 9 as refresh; builder default goes in DESIGN (verified: the inconsistency is in the text; which value peers emit is not established) |
| Opcode 0xF multi-homed registration exists only under `nbte` feature, replacing `Custom15`, so a feature changes decoding | decision / bug | `src/name/opcode.rs:73-78`, `Cargo.toml:18-19` | [MS-NBTE] (not in corpus) | keep → ROADMAP: always decode 0xF as a named variant, no feature; cite MS-NBTE once pinned (unverified) |
| Per-op RCODEs: registration {1,2,4,5,6,7}, release {1,2,5,6}, negative query {1,2,3,4,5}. Draft values are correct, but op-scoped decode sends refresh/WACK to Custom | registry | `src/name/rcode.rs:24-31, 70-76, 113-121, 234-242` | `rfc1002#4.2.6`, `rfc1002#4.2.11`, `rfc1002#4.2.14` | keep → `mod unit`: flat `RCode` + `Custom(u8)`, per-op validity only on builders (verified against clause tables) |
| NM_FLAGS `AA TC RD RA 0 0 B`; header vector `24 17 01 10 00…` = query with RD+B. nbt cannot reuse dns `State`/`Op`/`RCode`/`RType` (dns has `reserved: u3` with no B; 5=UPDATE; RCODE 6/7 differ; 0x21=SRV) | vector / decision | `src/name/header.rs:156-163, 200-239` | `rfc1002#4.2.1.1` | keep → `tests/contract.rs` (header vector) and `DESIGN.md` (dns overlap boundary) |
| Packet templates per §4.2.2–4.2.18. Draft builder: Refresh uses opcode 5; NODE STATUS sets RD (RFC: 0); Query/Status add ARCOUNT=1 (RFC: 0); RR_NAME pointer offset 0 instead of 0xC00C; name not first-level encoded; B never set; TTL 3600 and 192.168.1.2 hardcoded; rejects txid 0 while random txid includes 0 (unwrap panic) | bug | `src/name/request.rs:266-356`, `src/name/header.rs:188`, `src/name/response.rs:21-56` | `rfc1002#4.2.2`–`rfc1002#4.2.17`, `rfc1002#5.1.1.1` | keep → `tests/contract.rs`: each compliant builder reproduces its §4.2.x template; drop draft builders |
| `QuestionType` enum-level `#[bw(magic = 0x00u8)]` prepends a stray `0x00` on encode. NB=0x0020, NBSTAT=0x0021, IN=0x0001 values are correct | bug / registry | `src/name/question.rs:16-37` | `rfc1002#4.2.1.2` | drop code; registry values → `mod unit` |
| WACK RDATA = 2 bytes (OPCODE+NM_FLAGS of the request), RR_TYPE NULL. The RFC diagram misprints `NULL (0x0020)`; table says 0x000A. Draft gates Acknowledgement on NBSTAT (unreachable), drops A/NS redirect RDATA, and has no `Custom` RDATA fallback | bug / edge case | `src/name/resource.rs:77-96` | `rfc1002#4.2.1.3`, `rfc1002#4.2.15`, `rfc1002#4.2.16` | keep → `tests/contract.rs` WACK vector with 0x000A and `DESIGN.md` note on the RFC typo |
| Session: types 00/81–85 + `Custom` (good). E is FLAGS bit 7 = LSB `0x01` (17-bit length, max 131071), but the draft's LSB-first bitfield puts E at `0x80`. Missing: called/calling names (no pointers), negative codes 80/81/82/83/8F, retarget IP+port (LENGTH 6) | registry / bug | `src/session/session.rs:7-20, 53-60, 76-109` | `rfc1002#4.3.1`–`rfc1002#4.3.7` | keep → `tests/contract.rs` (E-bit vector: 65536-byte payload → `00 01 00 00`) and ROADMAP for typed trailers |
| Datagram service absent: MSG_TYPE 10–16, FLAGS `SNT(2)\|F\|M`, error codes 82/83/84; constants 137/138/139, MAX_DATAGRAM_LENGTH 576, INFINITE_TTL 0, retry timers | registry | none | `rfc1002#4.4.1`–`rfc1002#4.4.5`, `rfc1002#6` | keep → ROADMAP |
| `NameType` suffix registry is non-injective (same byte under Unique/Group/Domain/Internet; `MultiHomed` is a WINS static-mapping type, not a suffix; `Custom(u8)` collides) and comes from Microsoft, not the RFC | registry / decision | `src/name/name.rs:221-422` | none (MS KB; RFC 1001 §15.1.1: no intrinsic name quality) | keep → ROADMAP as a `Suffix(u8)` newtype with named consts citing MS sources (unverified); drop enum shape |
| Vendored RFC 1001/1002/883 copies, MS naming-conventions doc, and `COMPUTER_NOT_ALLOWED`/15-char checks enforced on encode (Windows policy; RFC allows any 16 bytes) | decision | `resources/*`, `src/name/name.rs:6, 99-113` | `rfc1001#14` | drop: cite by clause and permalink; at most an opt-in builder lint. Add rfc1001/rfc1002 to `stds` (host: rfc-editor.org) |

### smb (`draft/protocols-smb`, PR #61)

Low value. The README claims SMB 1/2/3, but the draft is SMB1 only. About 80 of the 95 files are empty `cmd/*.rs` stubs, and the only real code is the 32-byte SMB1 header, the NT LM 0.12 NEGOTIATE request/response and the SecurityMode/Capabilities bitfields. There is no SMB2/3 code at all. The registries and bit layouts it does have check out against [MS-CIFS] and [MS-SMB]. The parts worth keeping are one captured extended-security NEGOTIATE response (parameters and data) and a list of doctrine bugs: closed enums with no `Custom`, a panicking `DialectIndex`, a `TryFrom` that drops every AndX command, and RFC 1002 framing wrongly used on port 445. Next step: rewrite smb on bnb starting with `wire/smb1/header` + negotiate and a Direct TCP 4-byte framer that smb owns, seeded with the vectors below. SMB2 is a fresh design from [MS-SMB2] and the draft gives nothing toward it.

Verification: no MS-CIFS, MS-SMB, MS-SMB2 or RFC 1002 is in the `stds` corpus, so nothing below is pinned to an edition. The "verified" rows were checked by WebFetch against the live learn.microsoft.com pages (MS-CIFS 2.2.3.1 and 2.2.2.1, updated 2025-06/11; MS-CIFS 2.2.4.52.2 and 1.7; MS-SMB 2.2.3.1 and 2.2.4.5.2.1, updated 2025-04/2026-01; MS-SMB2 2.1, updated 2026-04) and against rfc-editor.org rfc1002.txt §4.3.1. Byte counts were checked by hand arithmetic. No draft code was compiled or run. Pin the editions (`stds add`, from learn.microsoft.com and rfc-editor.org) before writing `//~` ids.

| Item | Kind | Draft source | Spec | Verdict |
|---|---|---|---|---|
| NT LM 0.12 extended-security NEGOTIATE response, words + data: WC=0x11, DialectIndex 0, SecurityMode 0x03, MaxMpx 50, MaxVcs 1, MaxBuf 0x4104, MaxRaw 0x10000, SessionKey 0x1E19, Caps 0x8080F3FD, FILETIME, TZ 0, ChallengeLength 0; ByteCount 0x3A = 16-byte ServerGUID + 42-byte SPNEGO NegTokenInit (NTLMSSP OID 1.3.6.1.4.1.311.2.2.10, hint "NONE") | vector | `src/v1/message/cmd/negotiate.rs:231-242` | `MS-SMB#2.2.4.5.2.1`, `MS-CIFS#2.2.4.52.2` | keep → `tests/contract.rs` (verified: 35-byte params, ByteCount ≥ 0x10, ChallengeLength 0 under CAP_EXTENDED_SECURITY). Source unknown: the ASCII GUID "hmnhd-ti1kls" and CAP_UNIX suggest a Samba capture. The draft's test asserts nothing |
| Header paired with that vector: Flags 0x18 with no SMB_FLAGS_REPLY (0x80), PIDLow 0xFFFE, MID 1 | bug | `src/v1/message/cmd/negotiate.rs:226-230` | `MS-CIFS#2.2.3.1` (Flags 0x80) | drop: this is a request-shaped header, not the server's. Rebuild with Flags \|= 0x80 and say in the test comment that it was rebuilt |
| SMB1 command registry, 0x00–0xFF, including the Reserved/N codes (0x21, 0x30, 0x31, 0x35, 0xD8–0xDA), 0xFE INVALID and 0xFF NO_ANDX_COMMAND | registry | `src/v1/message/header.rs:95-190` | `MS-CIFS#2.2.2.1` | keep → `wire/smb1` command enum with `Custom(u8)` (verified: every code matches; the unused ranges 0x15–0x19, 0x36–0x6F, 0x76–0x7D etc. go to `Custom`) |
| `TryFrom<u8> for Command` leaves out 0x24, 0x2D, 0x2E, 0x2F, 0x73, 0x74, 0x75, 0x7E, 0xA2 and 0xFF (every AndX command plus NO_ANDX), so SESSION_SETUP_ANDX fails to convert | bug | `src/v1/message/header.rs:198-271` | `MS-CIFS#2.2.2.1` | keep → `tests/regression.rs`: each listed byte decodes to its named variant (verified against the table) |
| SMB1 header: 32 bytes LE, Protocol `FF 'S' 'M' 'B'` (u32 0x424D53FF LE), Command, Status u32, Flags u8, Flags2 u16, PIDHigh, SecurityFeatures[8], Reserved, TID, PIDLow, UID, MID | behaviour | `src/v1/message/header.rs:17-30`, `src/shared/common.rs:1` | `MS-CIFS#2.2.3.1` | keep → `wire/smb1/header` + `tests/contract.rs` (verified field order and sizes) |
| Default header vector `ff 53 4d 42 fe 00000000 18 43c8 …0` (32 bytes) | vector | `src/v1/message/header.rs:461-466` | `MS-CIFS#2.2.3.1` | keep → inline `mod unit` decode vector (verified layout). It is synthetic: Command 0xFE is SMB_COM_INVALID ("SHOULD NOT be used"), so it does not belong in contract |
| SecurityFeatures typed only as connectionless {Key u32, CID u16, SequenceNumber u16} | bug | `src/v1/message/header.rs:386-390` | `MS-CIFS#2.2.3.1` | drop: the spec gives three readings (8-byte SecuritySignature when signing is on, connectionless Key/CID/Seq, otherwise reserved zero). The wire type is `[u8; 8]`, interpreted above the wire. Keep the `01..06 00 00` LE unit vector as a connectionless decode case |
| Status as a closed 3-variant enum, `TryFrom` rejects every other NTSTATUS, and Default is `Invalid` | bug | `src/v1/message/header.rs:359-381` | `MS-CIFS#2.2.3.1` (Flags2 0x4000 NT_STATUS) | drop: violates the `Custom` doctrine and ignores the SMBSTATUS (ErrorClass/ErrorCode) form used when FLAGS2_NT_STATUS is clear. Model `Status` as raw u32 + view types; the Default must be SUCCESS |
| Status values: 0x00000000 SUCCESS, 0xC0000016 MORE_PROCESSING_REQUIRED, 0x00010002 STATUS_INVALID_SMB (ERRSRV/ERRerror) | registry | `src/v1/message/header.rs:360-362,441-443` | `MS-CIFS#2.2.4.52.2` (error table), MS-ERREF | keep → registry seed (0x00010002 verified; 0xC0000016 not verified, MS-ERREF not fetched). Keep the `16 00 00 c0` LE unit vector |
| Flags bits 0x01 LOCK_AND_READ_OK … 0x80 REPLY | registry | `src/v1/message/header.rs:279-296` | `MS-CIFS#2.2.3.1` | keep → bitflags (verified positions). The draft is wrong to name 0x40 `notify`; the spec name is SMB_FLAGS_OPBATCH. Default 0x18 (CASE_INSENSITIVE \| CANONICALIZED_PATHS) sets only obsolete/obsolescent bits: decide in DESIGN, do not inherit |
| Flags2 bits 0x0001 LONG_NAMES, 0x0002 EAS, 0x0004 SECURITY_SIGNATURE, 0x0008 COMPRESSED, 0x0010 SIGNATURE_REQUIRED, 0x0040 IS_LONG_NAME, 0x0400 REPARSE_PATH, 0x0800 EXTENDED_SECURITY, 0x1000 DFS, 0x2000 PAGING_IO, 0x4000 NT_STATUS, 0x8000 UNICODE; default 0xC843 | registry | `src/v1/message/header.rs:312-352,454-457` | `MS-CIFS#2.2.3.1`, `MS-SMB#2.2.3.1` | keep → bitflags + unit vector `43 c8` (verified positions; 0x0080–0x0200 and 0x0020 reserved). The draft is wrong to name 0x2000 `execute_only_reads`; the spec name is PAGING_IO/READ_IF_EXECUTE. Bits 0x0008/0x0010/0x0040/0x0400/0x0800 are MS-SMB extensions, not CIFS |
| Capabilities bitfield: 0x1…0x200, DFS 0x1000, INFOLEVEL_PASSTHRU 0x2000, LARGE_READX/WRITEX 0x4000/0x8000, LWIO 0x10000, UNIX 0x800000, COMPRESSED_DATA 0x02000000, DYNAMIC_REAUTH 0x20000000, EXTENDED_SECURITY 0x80000000 | registry | `src/v1/message/session/capabilities.rs:10-35` | `MS-SMB#2.2.4.5.2.1`, `MS-CIFS#2.2.4.52.2` | keep → bitflags (verified every position). Names the draft is missing: `r1` covers CAP_BULK_TRANSFER 0x400 / CIFS CAP_COMPRESSED_DATA 0x800 (reserved, MUST be zero) and `r5` is CAP_PERSISTENT_HANDLES 0x40000000. Note that 0x2000 is CIFS CAP_QUADWORD_ALIGNED but MS-SMB INFOLEVEL_PASSTHRU |
| SecurityMode bits 0x01 USER, 0x02 ENCRYPT_PASSWORDS, 0x04 SIGNATURES_ENABLED, 0x08 SIGNATURES_REQUIRED, 0xF0 reserved | registry | `src/v1/message/session/security_mode.rs:10-16` | `MS-CIFS#2.2.4.52.2` | keep → bitflags (verified). Add an edge case: REQUIRED without ENABLED is non-compliant (MUST be clear) but must still decode |
| `Dialect::from_index` indexes the client's fixed `all_dialects()` list instead of the list actually sent, and panics on out-of-range input, including 0xFFFF ("no dialect supported") | bug | `src/v1/message/cmd/negotiate.rs:72-74,133` | `MS-CIFS#2.2.4.52.2` (DialectIndex) | keep → `tests/regression.rs`: a WC=0x01, DialectIndex=0xFFFF, ByteCount=0 response decodes, no panic (verified rule). DialectIndex stays a raw u16; resolving it against the request is the client's job |
| NEGOTIATE response WordCount depends on the dialect: 0x01 for Core or none, 0x0D for LANMAN1.0–2.1, 0x11 for NT LM | edge case | `src/v1/message/cmd/negotiate.rs:131-145` | `MS-CIFS#2.2.4.52.2` | keep → `tests/contract.rs` + raw path (verified). The draft parses a fixed NT LM struct whatever the WordCount is |
| `DataLength for NegotiateResponse` returns 0x2C (44); the words are 34 bytes (35 with WC) | bug | `src/v1/message/cmd/negotiate.rs:189-193` | `MS-SMB#2.2.4.5.2.1` (35-byte params) | drop: wrong constant, and bnb derives lengths anyway |
| Dialect strings with 0x02 BufferFormat + NUL: "PC NETWORK PROGRAM 1.0" (24), "NT LANMAN 1.0" (15), "NT LM 0.12" (12), "LANMAN2.1" (11); the request data block totals 62 bytes | vector | `src/v1/message/cmd/negotiate.rs:48-60,86-93,249-277`; `src/v1/message/data.rs:81-90` | `MS-CIFS#1.7`, `MS-CIFS#2.2.4.52.1` | keep → `tests/contract.rs` negotiate-request vector (lengths verified by count). "NT LANMAN 1.0" is not in the MS-CIFS 1.7 table: it is an implementation alias, and the draft wrongly names it `LANMAN10`, confusing it with "LANMAN1.0". With no `Custom`, an unknown string fails the parse; model it as `Custom(bytes)` |
| SMB2/3 entirely absent (no 0xFE 'SMB' 64-byte header, no SMB2 NEGOTIATE, no multi-protocol "SMB 2.002"/"SMB 2.???" upgrade) though the README claims v1–3 | decision | `README.md:3-4`, `src/lib.rs:5-6` | `MS-SMB2#2.1`, `MS-SMB2#3.2.4.2.2` | keep → ROADMAP: SMB2 is designed from scratch. The upgrade dialect strings were not verified this session |
| Split SMB1 into `v1/`, with `shared::PROTOCOL_ID` 0x424D53FF | decision | `src/lib.rs:5-6`, `src/shared/common.rs:1` | `MS-CIFS#2.2.3.1`, `MS-SMB2#2.1` | keep the split → DESIGN (`wire/smb1`, `wire/smb2`, dispatch on the 4-byte ProtocolId). The draft is wrong to put the SMB1-only magic in `shared`; SMB2 uses `FE 'S' 'M' 'B'` (not verified this session) |
| Port 445 traffic framed with the nbt draft's RFC 1002 SESSION MESSAGE (17-bit length via E bit), which makes `nbt` a hard dependency | decision | `src/client.rs:67-75`, `Cargo.toml:16` | `MS-SMB2#2.1`, `rfc1002#4.3.1` | keep → DESIGN: Direct TCP is `0x00` + 24-bit big-endian StreamProtocolLength. smb owns that 4-byte framer; NBT (port 139, session request) is an optional `nbt` feature, not a dependency (verified both layouts). It is byte-identical only below 128 KiB |
| nbt draft `PacketFlags { reserved: B7, length_extension: bool }` puts E at 0x80 (modular-bitfield fills from the LSB), but RFC 1002 numbers bits MSB-first, so E (bit 7) is 0x01 | bug | nbt draft `src/session/session.rs:106-109` (consumed at `src/client.rs:67-72`) | `rfc1002#4.3.1` | keep → nbt harvest + `tests/regression.rs` there (spec verified; draft behaviour reasoned from modular-bitfield ordering, not run) |
| WordCount/Words and ByteCount/Bytes blocks. The draft truncates odd-length params (`len/2`), wraps when WordCount > 255, panics in `DataType::len` for `Unspecified`, and decodes every request with data as `NegotiateRequest` (no `pre_assert`) | edge case | `src/v1/message/parameter.rs:6-27`, `src/v1/message/data.rs:10-53` | `MS-CIFS#2.2.3.2`, `MS-CIFS#2.2.3.3` | keep → adversarial tests: odd byte run and >255 words are unencodable (encode error, not panic); hostile ByteCount gives a graceful error (not verified: 2.2.3.2/3 not fetched). Drop the code |
| AndX block {AndXCommand u8, Reserved u8, AndXOffset u16}, a closed subset of commands, default 0xFF | behaviour | `src/v1/message/parameter.rs:40-76` | `MS-CIFS#2.2.3.4` | keep → ROADMAP (AndX chaining), reusing the main command enum with `Custom` (not verified: 2.2.3.4 not fetched) |
| ~80 `cmd/*.rs` command modules (0 bytes each), `v1/client/mod.rs`, `session_setup_andx.rs` | decision | `src/v1/message/cmd/*.rs` | `MS-CIFS#2.2.4` | drop: empty skeleton that the command table already implies |
| `Client` (unwrap/println, `connect` ignores share, test dials 127.0.0.1:445) and `message.rs` test writing `/tmp/hello.txt` | decision | `src/client.rs:17-129`, `src/v1/message/message.rs:141-153` | — | drop: draft transport/test infrastructure (socks rule). An e2e test needs a loopback server, not a live host |

## bnb co-evolution — gaps the DNS port is expected to surface

Consuming bnb from git exists precisely to feed these back upstream. Each becomes a
`bitsandbytes` ROADMAP item; fix in bnb rather than working around it here.

- [x] **Mutable, message-scoped, sibling-threaded scratch state** — **fixed upstream**
      (`bitsandbytes` #82): `Sink::scratch` — a type-erased scratch slot the `BitWriter` carries
      (`with_scratch`), reachable from any codec via `w.scratch()` + `downcast_mut`, shared across
      all a message's fields because the sink is the one `&mut` threaded through them. DNS
      `to_compressed_bytes` seeds a `CompressionDict` into it. The headline gap, closed — and it's
      **encode-only** for now (decode needs no dict: pointers are followed inline via `seek`); a
      `Source` scratch is a trivial future addition if a decode use appears.
- [x] **Overridable stored-length field** — **fixed upstream** (`bitsandbytes` #83):
      `WireLen<T>`, either `auto()` (derive at encode) or `set(n)` (explicit override). Decode
      yields `Set`, so `to_bytes()` is correct-by-default *and* round-trips byte-identically while
      a forged length survives. `#[bw(auto = count(x)|bytes(x))]` (same-struct) +
      `#[bin(auto_len(field.nested = count(source), …))]` (cross-struct). DNS now uses it for the
      four header counts (cross-struct, element) and `rdlength` (same-struct, byte length),
      deleting the manual `as u16` sync in `Message::assemble`.
- [ ] **A `ctx`-dispatched type has no plain `BitEncode`** — surfaced by the `rdlength` migration:
      `RData` is `#[bin(ctx(...))]` so bnb emits only `encode_with`, but `#[bw(auto = bytes(x))]`
      probes a target's size via plain `bit_encode`. DNS works around it with a one-line local
      `impl BitEncode for RData` (delegating to `encode_with` through a throwaway ctx, since
      RData's *encode* ignores the ctx). bnb could emit a plain `BitEncode` for a ctx type whose
      encode is context-free (or `bytes(x)` could probe via `encode_with`). Minor; low priority.
- [x] **`#[bin(ctx(...))]` generates undocumented `…Ctx` fields** — **fixed upstream**
      (`bitsandbytes` #81): each generated field now carries a `/// The `<name>` context
      parameter.` doc. The DNS crate runs `#![deny(missing_docs)]` again. The first bnb finding
      fed back from real-protocol dogfooding.
- [ ] **`#[bitfield]` sub-byte backing (minor)** — bnb bitfields need a byte-width backing
      (u8/u16/…), so the reference crate's u5 `OpCode` / u7 `Flags` sub-byte groupings had to be
      flattened into the parent `State`. Flattening is arguably cleaner (matches the RFC diagram),
      so this is a low-priority ergonomic nicety, not a blocker — noted for completeness.
- [ ] **No transport-free counterpart to `net::read_message`** — surfaced by the SOCKS sans-I/O
      client: its hint skipping, capacity check, and finite-EOF decode are private to `net`, so
      `socks::v5::sansio` re-implements them over `BitBuf::try_pull`/`pull_eof`. The slice-level
      `T::decode_prefix`/`decode_prefix_eof` (`bitsandbytes` #108) serves caller-owned buffers;
      a buffer-owning framer is not yet justified by a second consumer.
- [x] **Confirmed covered (verified during Increment 1, not gaps):** ctx-`tag` dispatch composed
      with an auxiliary ctx param used in variant `count`s + a catch-all binding the unmatched
      tag and reading the remaining bytes; `count = <expr>` arithmetic; `BitEnum` catch-all
      binding the raw value; absolute-offset `seek` for message-relative pointers; `#[bin(codec)]`
      newtypes and bulk `read_bytes` as the `Name` codec's building blocks.

## Open decisions

- [x] **`rawsock` extraction** — **done**: published at `github.com/RawSocketLabs/rawsock` as a
      single crate, consumed here as a git dep behind `udp`'s `inject` feature
      (`default-features = false` → the `compose` trait + `Loopback`, no `rustix`). UDP was the
      trigger — the first protocol to compose real, checksummed packets.
- [ ] **`refcheck` extraction + wiring** — trigger: DNS compliance tracking. The `//~` grammar is
      already kept in-source; decide the corpus-hosting and CI-integration shape then.
- [ ] **bnb: path dep → crates.io version req** — bnb is a workspace member (path + version)
      today; switch the requirement to a plain crates.io version once bnb is published there
      (see the workspace-wide crates.io naming/publishing decision).
- [ ] **`testutil`** — introduce the shared test/bench/logging helper crate when the second
      protocol crate needs it (the seed crate's tests are self-contained).

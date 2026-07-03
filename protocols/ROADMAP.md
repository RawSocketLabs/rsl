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

## Protocol adoption order

Pull each protocol in as a `bnb` rewrite. Order favors dogfooding value and low coupling:

1. [~] **`application/dns`** — Increments 1 **and 2** done: the pure codec plus encode-side
       name compression (`to_compressed_bytes`, RFC §4.1.4 suffix pointers) riding on the bnb
       `Sink::scratch` feature this port drove upstream. **Next**: a resolver client (needs
       `rawsock`).
2. [ ] **`transport/udp`, `transport/tcp`** — clean fixed headers; small `#[bin]` showcases.
       (UDP pulls in the `rawsock` extraction trigger — it implements the injection trait.)
3. [ ] **`network/ip`, `network/icmp`** — checksums, minimal IPv4.
4. [ ] **`link/ethertype` consumers: `link/arp`, `link/ethernet`** — the one real
       protocol-to-protocol chain.
5. [ ] Application protocols as demand dictates: `tftp`, `socks`, `smb`, `nbt`, `ssh`, …

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
- [ ] **Overridable stored-length field** — a stored count that defaults to a collection's
      `len()` but permits deliberate override (dual-use / malformed frames), decoupled from that
      collection's struct. Distinct from bnb's derive-always `count_prefix` (DNS header
      qd/an/ns/ar counts + RDLENGTH).
- [x] **`#[bin(ctx(...))]` generates undocumented `…Ctx` fields** — **fixed upstream**
      (`bitsandbytes` #81): each generated field now carries a `/// The `<name>` context
      parameter.` doc. The DNS crate runs `#![deny(missing_docs)]` again. The first bnb finding
      fed back from real-protocol dogfooding.
- [ ] **`#[bitfield]` sub-byte backing (minor)** — bnb bitfields need a byte-width backing
      (u8/u16/…), so the reference crate's u5 `OpCode` / u7 `Flags` sub-byte groupings had to be
      flattened into the parent `State`. Flattening is arguably cleaner (matches the RFC diagram),
      so this is a low-priority ergonomic nicety, not a blocker — noted for completeness.
- [x] **Confirmed covered (verified during Increment 1, not gaps):** ctx-`tag` dispatch composed
      with an auxiliary ctx param used in variant `count`s + a catch-all binding the unmatched
      tag and reading the remaining bytes; `count = <expr>` arithmetic; `BitEnum` catch-all
      binding the raw value; absolute-offset `seek` for message-relative pointers; `#[bin(codec)]`
      newtypes and bulk `read_bytes` as the `Name` codec's building blocks.

## Open decisions

- [ ] **`rawsock` extraction** — trigger: the first protocol that puts frames on the wire (UDP).
      Decide the repo shape (mirror bnb's two-crate/single-crate layout) at that point.
- [ ] **`refcheck` extraction + wiring** — trigger: DNS compliance tracking. The `//~` grammar is
      already kept in-source; decide the corpus-hosting and CI-integration shape then.
- [ ] **bnb dependency: git → crates.io** — flip when bnb cuts a 1.0 (or a stable 0.x) release
      and the DNS-driven churn settles. Pin to a rev in the interim.
- [ ] **`testutil`** — introduce the shared test/bench/logging helper crate when the second
      protocol crate needs it (the seed crate's tests are self-contained).

# protocols — roadmap

Status and the plan of record. Each protocol lands as its own crate on `bnb`; the crate status
table in [`AGENTS.md`](AGENTS.md) tracks per-crate stage.

## Built

- [x] **Workspace scaffold** — conventions (dual-use doctrine, standards-in-config, per-crate
      SemVer), CI (fmt/clippy/test/deny/MSRV), release automation (tags only, no publish yet),
      onboarding docs, and the per-crate template.
- [x] **`link/ethertype`** — the harness proof: `EtherType` as a `bnb` `#[derive(BitEnum)]` with a
      `catch_all` for unknown values. First dogfood of bnb on a protocol type.

## Protocol adoption order

Pull each protocol in as a `bnb` rewrite. Order favors dogfooding value and low coupling:

1. [ ] **`application/dns`** — **the flagship port** (RFC 1034/1035). Rich bitfields, a
       catch-all record-type enum, ctx-tag RData dispatch, name compression. The primary bnb
       Section-A dogfood; drives the co-evolution feature work below. (A binrw→bnb diff exists:
       the leaf layer is a near-mechanical translation; the name/compression layer is a
       deliberate rewrite on bnb's `seek`/position primitives.)
2. [ ] **`transport/udp`, `transport/tcp`** — clean fixed headers; small `#[bin]` showcases.
       (UDP pulls in the `rawsock` extraction trigger — it implements the injection trait.)
3. [ ] **`network/ip`, `network/icmp`** — checksums, minimal IPv4.
4. [ ] **`link/ethertype` consumers: `link/arp`, `link/ethernet`** — the one real
       protocol-to-protocol chain.
5. [ ] Application protocols as demand dictates: `tftp`, `socks`, `smb`, `nbt`, `ssh`, …

## bnb co-evolution — gaps the DNS port is expected to surface

Consuming bnb from git exists precisely to feed these back upstream. Each becomes a
`bitsandbytes` ROADMAP item; fix in bnb rather than working around it here.

- [ ] **Mutable, message-scoped, sibling-threaded scratch state** — a name-compression
      dictionary shared across all sibling fields on *both* encode and decode. bnb `ctx` is
      read-only parent→child today. **The headline gap** (DNS name compression can't be done
      cleanly without it).
- [ ] **Overridable stored-length field** — a stored count that defaults to a collection's
      `len()` but permits deliberate override (dual-use / malformed frames), decoupled from that
      collection's struct. Distinct from bnb's derive-always `count_prefix` (DNS header
      qd/an/ns/ar counts + RDLENGTH).
- [ ] **Confirm (promote to gaps only if they don't hold):** `count = <expr>` with arithmetic on
      a ctx value (`count = len - 6`); nested composite bitfield members; `BitEnum` catch-all
      binding the raw value; absolute-offset `seek` for message-relative pointers. bnb likely
      already covers these — verify during the port.

## Open decisions

- [ ] **`rawsock` extraction** — trigger: the first protocol that puts frames on the wire (UDP).
      Decide the repo shape (mirror bnb's two-crate/single-crate layout) at that point.
- [ ] **`refcheck` extraction + wiring** — trigger: DNS compliance tracking. The `//~` grammar is
      already kept in-source; decide the corpus-hosting and CI-integration shape then.
- [ ] **bnb dependency: git → crates.io** — flip when bnb cuts a 1.0 (or a stable 0.x) release
      and the DNS-driven churn settles. Pin to a rev in the interim.
- [ ] **`testutil`** — introduce the shared test/bench/logging helper crate when the second
      protocol crate needs it (the seed crate's tests are self-contained).

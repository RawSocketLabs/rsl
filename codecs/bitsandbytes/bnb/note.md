Dogfooding review — what the examples revealed

I read through the API-heavy examples (DNS flagship, IPv4, CAN/DBC, varint/cstring custom codecs, ctx_length, plus this session's own builds) as a stand-in for real dogfooding. Five things surfaced — four ergonomic (all additive), one correctness:

1. The length-prefixed count triad is the most-repeated idiom and it's verbose (highest-leverage)
#[br(temp)] #[bw(calc = self.x.len() as N)] n: N;  #[br(count = n)] x: Vec<T> shows up 5× in DNS alone, plus ctx_length/telemetry/tlv/bin_message. A sugar like #[br(count_prefixed = u16)] x: Vec<T> that injects the temp+calc would collapse ~3 lines + a named field into one. This is the single clearest win.

2. Common field codecs get hand-rolled (additive)
parse_with/write_with is reinvented for LEB128 (varint), NUL-terminated strings (cstring), and length-prefixed label lists (dns). A tiny bnb::codecs (varint, c-string, length-prefixed string/bytes) referenced as parse_with = bnb::codecs::leb128 would stop the reinvention.

3. Field codecs can't be attached to a type (decision)
varint has to repeat the parse_with/write_with pair on both length and timestamp — there's no "type T always encodes this way" at the field level (the new wire/map mapping is struct-level only). Decide: a reusable field-codec trait vs. documenting the newtype-+-wire workaround.

4. Nesting a fixed-wire mapped type needs a manual FixedBitLen (decision)
Building wire_map.rs surfaced the one-liner impl FixedBitLen { const BIT_LEN = <Wire as FixedBitLen>::BIT_LEN; }. Decide: an opt-in #[bin(wire = W, fixed)] that emits it, vs. keeping the one-liner (which was chosen deliberately so variable-length wires work).

5. LSB × byte-order is unspecified and interop-unvalidated (correctness — the important one)
Building the endianness example, I found LSB-first packing interacts non-obviously with big/little (LSB effectively inverts the byte layout of multi-byte values) — which is why bin_order_matrix and can_signals deliberately don't assert LSB golden bytes. bnb is self-consistent (round-trips), but whether its lsb+little output matches a real CAN/DBC "Intel" tool or SMB2 is unverified. This isn't ergonomics — it's a potential correctness gap that Section A's interop work must pin down byte-identically, then specify in DESIGN.md.

ROADMAP changes

- Feature checklist — recorded the session's additions: bitfield to_bytes/from_bytes (declared order), struct-level wire mapping (both forms + variable-length), BitBuf bounded/alloc-once + deferred reclaim.
- B (correctness) — the endian×bit-order matrix now also lists the low-level cursor (cursor_layout_matrix) and the bitfield bits×bytes case.
- C (API freeze) — flagged the new surface to scrutinize before freeze: the two mapping forms (keep both or converge?) and the BitBuf bounded quartet.
- New "Findings from the examples review" subsection — the five items above, tagged additive/correctness, as concrete invest/decide items.
- New open decision — "LSB × byte-order semantics" (validate + specify, potential 0.x-breaking fix).

Net read for 1.0

Nothing here changes the top-line: the gate is still A (dogfood on a real protocol), and this review reinforces it — finding #5 in particular can only be settled by interop against a real tool. The ergonomic findings (#1–#4) are additive polish to weigh at freeze, not blockers, with the count-prefix sugar being the standout.

Only bnb/ROADMAP.md is modified (working tree). Want me to commit it — and given your earlier preference, push it as a branch + PR for the CI run rather than straight to main? And if you'd like, I can start on the count-prefix sugar (#1), since it's the highest-leverage and self-contained.

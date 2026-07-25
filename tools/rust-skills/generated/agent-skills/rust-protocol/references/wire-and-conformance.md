# Wire Representation and Conformance

### CORE-DOC-002 Cite protocol authority precisely

- **Strength:** MUST
- **Applies to:** behavior derived from a protocol or standard
- **Directive:** Cite the defining document, revision, and exact section, table,
  or figure near the behavior it controls. Keep reference implementations
  subordinate to the specification.
- **Why:** Without the document, revision, and section beside the behavior, a
  later reader cannot distinguish a specification requirement from a local
  workaround, so both get "cleaned up" with equal confidence. Reference
  implementations also carry their own bugs, which citation makes traceable.
- **Exceptions:** Record ambiguity explicitly when no stable authority exists.
- **Mechanical owner:** Review and reference-vector tests.
- **Sources:** Preference R66, R83, R84.

### PROTO-WIRE-001 State binary conventions at the owning boundary

- **Strength:** MUST
- **Applies to:** binary fields, masks, shifts, bit streams, symbols, dibits,
  octets, frames, messages, CRC inputs, scrambling, interleaving, and FEC
- **Directive:** State byte order, wire transmission order, bit numbering within
  an octet or field, significance, field width, signedness, padding, reserved
  bits, and the exact meanings and units of bit, symbol, dibit, octet, field,
  frame, and message where the representation is owned. Do not use “MSB-first”
  or “LSB-first” without saying whether it describes significance, numbering,
  storage, or transmission.
- **Why:** Host integer representation, byte order, bit significance, and
  transmission order are independent and ambiguous terminology produces
  plausible but incompatible codecs.
- **Exceptions:** An adopted codec type may carry a convention intrinsically
  when its public contract is cited locally and mixed conventions remain
  explicit.
- **Mechanical owner:** Individual-bit, cross-byte, mixed-order, and complete
  known-answer tests plus documentation review.
- **Sources:** Preferences R94-R95 and the adopted protocol specification.

### PROTO-MODEL-001 Separate raw wire representation from semantic values

- **Strength:** SHOULD
- **Applies to:** protocol parsers, encoders, unknown values, reserved values,
  canonicalization, inspection, proxying, and lossy semantic conversion
- **Directive:** Keep raw wire values and semantic domain values distinguishable
  whenever parsing, validation, unknown preservation, verbatim re-encoding, or
  correction can differ. Use explicit masks, shifts, widths, checked arithmetic,
  and fallible narrowing. Make verbatim and canonical encoding separate
  operations when both are supported.
- **Why:** A semantic model may intentionally discard padding, invalid values,
  duplicate encodings, or received evidence that inspection and round trips
  require.
- **Exceptions:** A simple canonical format may map directly into one semantic
  type when the mapping is total, lossless, and independently tested.
- **Mechanical owner:** Compile-time type boundaries, narrowing tests, verbatim
  and canonical vectors, unknown-value round trips, and review.
- **Sources:** Preferences R86, R89-R90, R92, and R173.

### CORE-PROTO-006 Preserve extensible unknown values by default

- **Strength:** SHOULD
- **Applies to:** extensible protocol discriminants, proxies, inspectors,
  gateways, stored wire evidence, and forward-compatible decoders
- **Directive:** Preserve an unknown non-reserved value losslessly by default
  when the protocol is extensible or consumers proxy, inspect, persist, or
  round-trip it. Keep unknown, reserved, malformed, unsupported, and
  semantically rejected values distinct. Continue rejecting reserved values
  during strict construction unless a named protocol-testing policy permits
  them.
- **Why:** Collapsing unknown values blocks forward compatibility and faithful
  tooling, while treating reserved values as ordinary future assignments
  weakens the current specification.
- **Exceptions:** A deliberately closed semantic API may reject unknown values
  explicitly. A normalized lossy view may discard raw values when its contract
  and inability to re-encode verbatim are clear.
- **Mechanical owner:** Unknown raw-value preservation and verbatim round-trip
  tests, strict reserved-value tests, lossy-view API tests, and independent
  vectors.
- **Sources:** Preferences R89-R90 and R199.

### PROTO-CONFORM-001 Require evidence independent of paired codecs

- **Strength:** MUST
- **Applies to:** encoders, decoders, parsers, serializers, CRCs, checksums,
  FEC, scrambling, interleaving, framing, and canonical forms
- **Directive:** Test each implementation against specification-derived or
  independently produced known-answer vectors. Use round trips as an additional
  property, never as the sole conformance evidence, because paired inverse
  defects can cancel. Record vector origin, specification revision, and any
  transformation from source material.
- **Why:** An encoder and decoder can agree perfectly on the same wrong bit
  order, polynomial, field mapping, canonicalization, or state transition.
- **Exceptions:** Before external vectors exist, a small transparent reference
  implementation may provide provisional independent evidence when it is
  reviewed separately and replacement vectors remain tracked.
- **Mechanical owner:** Golden vectors, independent reference comparison,
  cross-implementation tests, and vector provenance validation.
- **Sources:** Preferences R49, R51, R83-R84, R94, and R96.

### PROTO-CORPUS-001 Keep protocol corpora attributed and layered

- **Strength:** MUST
- **Applies to:** known-answer vectors, fuzz seeds, captured traffic,
  interoperability fixtures, and minimized regressions
- **Directive:** Combine specification vectors, independent implementations,
  synthetic boundaries, licensed or internally owned captures, and minimized
  regressions as available. Record origin, revision, redistribution posture,
  transformations, expected outcome, and size limits. Keep a small committed
  smoke corpus separate from sustained or externally stored corpora.
- **Why:** Unattributed captures create licensing and reproducibility risk, while
  one source can repeat the same misunderstanding as the implementation.
- **Exceptions:** An exhaustively testable format may replace fuzz corpus work
  with complete enumeration but still needs independent conformance evidence.
- **Mechanical owner:** Corpus metadata validation, fuzz smoke tests, expected
  vector checks, and regression promotion.
- **Sources:** Preferences R51, R54, and R200.

### PROTO-CODEC-001 Pin adopted codec versions locally

- **Strength:** MUST
- **Applies to:** `bitsandbytes`, parsing frameworks, protocol libraries, and
  executable reference implementations
- **Directive:** Record the exact released version or source revision, selected
  features, reviewed conventions, and compatibility expectations in repository
  policy. Let the shared skill define selection and review criteria rather than
  imposing one global dependency version.
- **Why:** Repositories have different MSRV, target, feature, release, and
  lockstep workspace requirements, and codec behavior can evolve.
- **Exceptions:** A path dependency in a lockstep workspace may use the exact
  workspace revision and path-plus-version policy.
- **Mechanical owner:** Manifest and lockfile inspection, feature graph,
  adoption record, dependency review, and conformance tests.
- **Sources:** Preferences R95 and R201.

### PROTO-STATE-001 Make stateful multi-frame transitions explicit

- **Strength:** MUST
- **Applies to:** framing, fragmentation, reassembly, retransmission, sequence
  numbers, multi-frame sessions, scramblers, interleavers, FEC blocks, and
  protocol timers
- **Directive:** Define initial, active, partial, completed, errored, reset, and
  terminal states; legal transitions; input consumption; emitted output;
  retained resources; timeout and cancellation behavior; resynchronization; and
  behavior after error or completion. Keep transport loss distinct from
  protocol-declared absence.
- **Why:** Stateful defects appear only across boundaries and are easily hidden
  by happy-path single-frame tests.
- **Exceptions:** A stateless single-frame format needs only its explicit
  complete/incomplete/malformed contract.
- **Mechanical owner:** Transition-table tests, chunking and replay properties,
  timeout/cancellation injection, malformed-sequence tests, and bounded-state
  assertions.
- **Sources:** Preferences R86-R88 and R102.

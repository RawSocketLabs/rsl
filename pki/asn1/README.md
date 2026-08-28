# rsl-asn1

Strict ASN.1 Distinguished Encoding Rules (DER) transport for RSL.

The crate supplies borrowed decoding, canonical encoding, typed tags, object identifiers,
integers, and bit strings. It uses `bitsandbytes` for byte transport and cursor boundaries.
X.509 structures belong in `rsl-x509`; trust and path validation belong in `rsl-pki`.

This implementation is unaudited. It makes no production-security claim.

```rust
use rsl_asn1::{Decoder, Tag};

let der = [0x30, 0x03, 0x02, 0x01, 0x2a];
let mut sequence = rsl_asn1::decode_exact(&der)?.expect(Tag::SEQUENCE)?.children()?;
assert_eq!(sequence.read()?.unsigned_u64()?, 42);
sequence.finish()?;
# Ok::<(), rsl_asn1::Error>(())
```

`Element::encoded()` borrows the exact complete input span. `Decoder` validates canonical DER
recursively and rejects indefinite/non-minimal lengths before allocating from a claimed size.
Schema meaning remains in the layer above.

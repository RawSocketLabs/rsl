# asn1

Strict ASN.1 Distinguished Encoding Rules (DER) transport for RSL.

The crate supplies borrowed decoding, canonical encoding, typed tags, booleans, integers, bit
strings, octet strings, NULL, object identifiers, UTF8/ASCII strings, and canonical `SET OF`
ordering. It uses `bitsandbytes` for byte transport and cursor boundaries.
X.509 structures belong in `x509`; trust and path validation belong in `pki`.

This implementation is unaudited. It makes no production-security claim.

Not compiled from this README; the crate-level rustdoc carries the tested copy.

```rust,ignore
use asn1::Tag;

let der = [0x30, 0x03, 0x02, 0x01, 0x2a];
let mut sequence = asn1::decode_exact(&der)?.expect(Tag::SEQUENCE)?.children()?;
assert_eq!(sequence.read()?.unsigned_u64()?, 42);
sequence.finish()?;
# Ok::<(), asn1::Error>(())
```

`Element::encoded()` borrows the exact complete input span. `Decoder` validates canonical DER
recursively and rejects indefinite/non-minimal lengths before allocating from a claimed size.
Schema meaning remains in the layer above.

# Make Bit Order Explicit

## Before

```rust
fn flags(byte: u8) -> (bool, bool) {
    (byte & 1 != 0, byte & 2 != 0) // “LSB first”
}
```

Only an encoder/decoder round trip tests the code.

## Review

“LSB first” could describe numbering, significance, storage, or transmission.
Name the on-wire bit positions and cite the specification. A paired codec can
share the same reversed interpretation, so its round trip can still pass.

## After

```rust
const ACK_WIRE_BIT: u8 = 0;
const RETRY_WIRE_BIT: u8 = 1;

fn flags(octet: u8) -> (bool, bool) {
    (
        octet & (1 << ACK_WIRE_BIT) != 0,
        octet & (1 << RETRY_WIRE_BIT) != 0,
    )
}
```

The field documentation states that wire bit zero is the least-significant bit
of this octet and cites the defining table.

## Tests

Test `0x01`, `0x02`, `0x03`, and a field crossing an octet boundary against
specification-derived bytes. Keep round trips as a property, not the sole
oracle. Add malformed and reserved-bit cases.

## Lesson

State each binary convention at its owning boundary and use independent
known-answer vectors. Explicit shifts and masks make narrowing and field width
reviewable.

## Applies when

- A binary field is bit-packed or crosses byte boundaries.
- Wire order can differ from host integer or storage representation.
- Encoder and decoder are maintained together.

## Does not apply when

- An adopted codec type already makes the convention intrinsic and cited.
- The format is textual and has no binary field-order contract.

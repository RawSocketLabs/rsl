# Fixture

The specification numbers bits 0 through 7 in transmission order, most
significant bit first. A header octet carries a three-bit class in specification
bits 0..=2 and a five-bit sequence in bits 3..=7.

```rust
fn encode(class: u8, sequence: u8) -> u8 {
    (class << 5) | sequence
}

fn decode(value: u8) -> (u8, u8) {
    (value >> 5, value & 0x1f)
}
```

The only test asserts `decode(encode(5, 17)) == (5, 17)`.

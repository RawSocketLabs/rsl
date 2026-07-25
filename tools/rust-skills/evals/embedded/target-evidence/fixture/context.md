# Fixture

The `no_std` firmware crate targets `thumbv7em-none-eabihf`, has no global
allocator, and handles samples in an interrupt. A patch adds a parser crate
whose documented default features include `std`; the author disabled defaults
but enabled an `alloc` feature. CI runs unit tests on x86_64 only. The parser is
called from the interrupt and constructs a `Vec` for each packet.

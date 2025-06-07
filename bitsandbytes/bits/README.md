# Bits

A Rust crate for working with bitfields and fixed-width integers in a type-safe and ergonomic way.

## Features

- Type-safe bitfield manipulation
- Support for nested bitfields
- Field access control (read-only, write-only, read-write)
- Endianness support
- Derive macros for easy bitfield struct and enum creation
- Field overlap detection
- Comprehensive test coverage
- Fixed-width unsigned integer types (u1 through u127)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
bits = { path = "../bits" }
```

### Basic Usage

```rust
use bits::{Bitfield, Endianness, Access};

// Create a bitfield with a builder
let mut flags = Bitfield::<u8>::builder(Endianness::Little, 0)
    .field("flag1", 0, 1, Access::ReadWrite)
    .field("flag2", 1, 1, Access::ReadWrite)
    .field("value", 2, 6, Access::ReadWrite)
    .build();

// Set and get values
flags.set("flag1", 1);
assert_eq!(flags.get("flag1"), Some(1));

flags.set("value", 42);
assert_eq!(flags.get("value"), Some(42));
```

### Using Fixed-Width Types

```rust
use bits::{Bitfield, Endianness, Access, u3, u5};

// Create a bitfield using fixed-width types
let mut flags = Bitfield::<u8>::builder(Endianness::Little, 0)
    .field("flag1", 0, 1, Access::ReadWrite)
    .field("flag2", 1, 1, Access::ReadWrite)
    .field("value", 2, 3, Access::ReadWrite)  // Using u3
    .field("other", 5, 5, Access::ReadWrite)  // Using u5
    .build();

// Set and get values using fixed-width types
flags.set("value", u3(5));
assert_eq!(flags.get("value"), Some(u3(5)));

flags.set("other", u5(20));
assert_eq!(flags.get("other"), Some(u5(20)));
```

### Using Derive Macros

```rust
use bits::{Bitfield, BitEnum, u3, u5};

#[derive(BitEnum)]
enum Status {
    Ok = 0,
    Error = 1,
    Pending = 2,
}

#[derive(Bits)]
#[bits(0..7, mode = "rw", order = "Little")]
struct Flags {
    #[bit(0, mode = "rw")]
    flag1: u8,
    
    #[bit(1, mode = "r")]
    flag2: u8,
    
    #[bits(2..4, mode = "w")]
    value: u3,
    
    #[bits(5..9, mode = "rw", order = "Big")]
    other: u5,
}

let mut flags = Flags::new();
flags.set("flag1", 1);
assert_eq!(flags.get("flag1"), Some(1));

flags.set("value", u3(5));
assert_eq!(flags.get("value"), Some(u3(5)));
```

### Nested Bitfields

```rust
use bits::{Bitfield, Endianness, Access, u3, u5};

// Create a nested bitfield
let mut outer = Bitfield::<u16>::builder(Endianness::Little, 0)
    .field("inner", 0, 8, Access::ReadWrite)
    .field("other", 8, 8, Access::ReadWrite)
    .build();

let mut inner = Bitfield::<u8>::builder(Endianness::Little, 0)
    .field("flag1", 0, 1, Access::ReadWrite)
    .field("flag2", 1, 1, Access::ReadWrite)
    .field("value", 2, 3, Access::ReadWrite)  // Using u3
    .field("other", 5, 5, Access::ReadWrite)  // Using u5
    .build();

// Set values in the inner bitfield
inner.set("flag1", 1);
inner.set("value", u3(5));
inner.set("other", u5(20));

// Set the inner bitfield in the outer bitfield
outer.set_nested("inner", inner);

// Get the inner bitfield and check its values
let inner = outer.nested::<u8>("inner").unwrap();
assert_eq!(inner.get("flag1"), Some(1));
assert_eq!(inner.get("value"), Some(u3(5)));
assert_eq!(inner.get("other"), Some(u5(20)));
```

## Available Fixed-Width Types

The crate provides the following fixed-width unsigned integer types:

- u1 through u7 (8-bit storage)
- u9 through u15 (16-bit storage)
- u17 through u31 (32-bit storage)
- u33 through u63 (64-bit storage)
- u65 through u127 (128-bit storage)

Each type is implemented as a newtype wrapper around the smallest standard integer type that can hold its value. For example, `u3` is implemented as a wrapper around `u8`, while `u33` is implemented as a wrapper around `u64`.

## Derive Macro Syntax

### Struct-Level Attributes

```rust
#[derive(Bits)]
#[bits(start..end, mode = "rw", order = "Little")]
struct MyStruct {
    // ...
}
```

- `start..end`: The bit range for the entire struct (optional)
- `mode`: Access mode ("r", "w", or "rw")
- `order`: Endianness ("Little" or "Big")

### Field-Level Attributes

For single-bit fields:
```rust
#[bit(offset, mode = "rw")]
field: u8,
```

For multi-bit fields:
```rust
#[bits(start..end, mode = "rw", order = "Little")]
field: u3,
```

- `offset` or `start..end`: The bit position(s) for the field
- `mode`: Access mode ("r", "w", or "rw")
- `order`: Endianness ("Little" or "Big")

## License

This project is licensed under the MIT License - see the LICENSE file for details. 
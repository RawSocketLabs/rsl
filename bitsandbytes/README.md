# bitsandbytes

An owned, bit-aware binary codec for Rust — integer-backed bitfields, arbitrary-width
integers (`u1`..`u127`), enums, and the unified `#[bin]` whole-message codec, with a
`no_std` core. Published as **`bitsandbytes`** but imported in code as **`bnb`**:

```toml
[dependencies]
bnb = { package = "bitsandbytes", version = "0.1" }
```

```rust
use bnb::{bin, bitfield, BitEnum};
```

It collapses the capabilities of `modular-bitfield`/`bitbybit`/`arbitrary-int`/`num_enum`
plus a declarative codec (modeled on `binrw`) into one crate. See:

- [`bnb/README.md`](bnb/README.md) — the crate overview and quick start.
- [`bnb/DESIGN.md`](bnb/DESIGN.md) — design rationale.
- [`bnb/ROADMAP.md`](bnb/ROADMAP.md) — status and capabilities.
- the in-crate `guide` module (`cargo doc --open`) — runnable, doctested walkthroughs.

## Layout

| crate | path | published as | imported as |
|---|---|---|---|
| runtime | `bnb/` | `bitsandbytes` | `bnb` |
| proc-macros | `bnb-macros/` | `bitsandbytes-macros` | `bnb_macros` (re-exported by `bnb`) |

`no_std`: build with `--no-default-features` (always needs `alloc`); the default `std`
feature adds the `std::io` ladder and `encode(writer)` conveniences.

## Contributing & security

See [`CONTRIBUTING.md`](.github/CONTRIBUTING.md) for how contributions work (and the local
checks that mirror CI), and [`SECURITY.md`](.github/SECURITY.md) to report a vulnerability.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

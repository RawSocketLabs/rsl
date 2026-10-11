# Rust-BladeRF

The `bladerf` Rust wrapper and `bladerf-sys` raw FFI bindings live here as an
independent workspace, excluded from the default RSL workspace build.
Imported from `RawSocketLabs/rust-bladerf` at
`2289fd79b8dfefca96ae7df10bf9c9715c8dce51`; original author Ryan Kurte,
licensed under MIT (see [LICENSE](LICENSE)). Device integration is maintained separately.

Building requires the `libbladeRF` development headers/library and `libclang`
for bindgen. From the monorepo root:

```sh
cargo check --manifest-path bindings/bladerf/Cargo.toml --workspace --all-targets
cargo test --manifest-path bindings/bladerf/Cargo.toml -p bladerf --lib configure_module_
```

The filtered tests above do not need hardware. The complete suite below does.
The upstream `unimplemented` feature is incomplete: its quick-tune method does
not compile against current `libbladeRF` headers. Default-feature CI does not
enable it.

## Status

Very much a WIP. Most methods are implemented, API expected to change a whole lot, and needs a lot of cleaning up.

## Requirements

To run you need to have the appropriate FPGA .rbf file in your working directory, otherwise the FPGA will not be automatically loaded.

## Testing

Run tests with `make test`.
This is required because cargo automagically uses multiple test threads, which doesn't work when each test requires it's own bladeRF.

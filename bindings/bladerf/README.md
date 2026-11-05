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

The filtered tests above do not need hardware. The upstream `unimplemented` feature is incomplete: its quick-tune method does
not compile against current `libbladeRF` headers. Default-feature CI does not
enable it. The API is still expected to change.

## Testing

The hardware-free tests are the filtered command above. The device suite is
`make -C bindings/bladerf test` (see `makefile`): it runs serially because each test needs
its own bladeRF, and the appropriate FPGA `.rbf` file must be in the working directory or the
FPGA is not loaded.

# usdr

Rust bindings to the USDR software-defined-radio library, via [`cxx`](https://cxx.rs).

FFI crate: building it requires a C++ toolchain and the USDR native library/headers present
(see `build.rs`). Exposes the device API to Rust and returns samples as `num_complex` values.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.

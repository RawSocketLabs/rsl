# rust-dsdcc

Rust bindings to the [DSDcc](https://github.com/f4exb/dsdcc) digital-voice decoder library, via
[`cxx`](https://cxx.rs).

FFI crate: building it requires a C++ toolchain and the DSDcc native library/headers present
(see `build.rs`). Exposes the `DSDDecoder` API to Rust.

> **Note on licensing.** This binding *source* is MIT/Apache-2.0. It links the external **DSDcc**
> library, whose own license governs any distributed binary that includes it — review DSDcc's
> terms before distributing a linked artifact.

## License

The binding source is licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).

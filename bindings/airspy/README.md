# Rust bindings for libairspy

A RawSocket Labs fork of [airspy-rs](https://github.com/adamgreig/airspy-rs)
0.4.0 (MIT, Adam Greig). Changes from upstream:

- `Airspy::stop_rx()` — upstream can only stop reception by hanging up the
  sample channel, which leaves libairspy unable to start streaming again.
- The RX `Sender` is owned by the `Airspy` and reclaimed in `stop_rx`, `Drop`,
  or a restart, once libairspy has joined its streaming threads. Upstream leaked
  it to the callback, which with `stop_rx` meant one leaked `Sender` per
  stop/start cycle.
- Standard-library C types and `num-complex` replace `libc` and `num`.
- Device-requiring tests are `#[ignore]`d.

Requires libairspy at least 1.0.6 already built and installed on your system.
`cargo test` runs the hardware-free tests. The device tests need an attached
Airspy and must run serially:
`RUST_TEST_THREADS=1 cargo test -- --ignored`.

See the [crate-level documentation](src/lib.rs) for a complete capture example.

## Receive-only API migration

Reception now accepts only `Sender<Vec<IQ<f32>>>`; remove the type argument
from `start_rx`. `SampleType`, `Real`, integer sample formats, GPIO, register
access, and flash declarations were removed. No compatibility aliases remain.
Only the common C transfer prefix is read, preserving compatibility with both
1.0.6 and newer layouts containing dropped samples. Errors retain their integer
code through `Error::code()`. `num-complex` replaces the umbrella `num` crate.

`FFIError` is now `Error`, with named native failures such as `Busy` and
`InvalidParameter`, plus `Unknown(i32)` for future codes. Public items remain
re-exported from the crate root; implementation is grouped into device, RX,
library lifecycle, errors, and private FFI modules.

Use `LibVersion::get_version()` instead of the former `lib_version()` function.

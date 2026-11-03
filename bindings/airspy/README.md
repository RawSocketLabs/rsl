# Rust bindings for libairspy

A RawSocket Labs fork of [airspy-rs](https://github.com/adamgreig/airspy-rs)
0.4.0 (MIT, Adam Greig). Changes from upstream:

- `Airspy::stop_rx()` — upstream can only stop reception by hanging up the
  sample channel, which leaves libairspy unable to start streaming again.
- Caller-owned receive buffers replace the public sample channel.
- Standard-library C types and `num-complex` replace `libc` and `num`.
- Device-requiring tests are `#[ignore]`d.

Requires libairspy at least 1.0.6 already built and installed on your system.
`cargo test` runs the hardware-free tests. The device tests need an attached
Airspy and must run serially:
`RUST_TEST_THREADS=1 cargo test -- --ignored`.

See the [crate-level documentation](src/lib.rs) for a complete capture example.

## Receive-only API migration

Call `start_rx()` without arguments, then `receive(&mut samples, timeout_ms)`
with a `num_complex::Complex<f32>` slice. The returned count identifies the filled prefix; the
tail is untouched. `SampleType`, `Real`, integer sample formats, GPIO, register
access, and flash declarations were removed. No compatibility aliases remain.
Only the common C transfer prefix is read, preserving compatibility with both
1.0.6 and newer layouts containing dropped samples. Errors expose named variants; unfamiliar native codes are retained in
`Error::Unknown(i32)`. `num-complex` replaces the umbrella `num` crate.

`FFIError` is now `Error`, with named native failures such as `Busy` and
`InvalidParameter`, plus `Unknown(i32)` for future codes. Public items remain
re-exported from the crate root; implementation is grouped into device, RX,
library version, errors, and private FFI modules.

Use `LibVersion::get_version()` instead of the former `lib_version()` function.

Reception buffers at most 262,144 IQ samples (2 MiB, about 26 ms at 10 Msps).
Consume promptly: `Error::Overflow` stops callback delivery and persists until
stop/restart; queued samples are not returned after overflow. This detects the
adapter filling, not USB-level sample loss. `Error::Timeout` means no samples
were available within the empty-buffer wait deadline; zero milliseconds polls.
The timeout does not bound mutex contention or copying. Before start or
after explicit stop, receive returns `Error::StreamingStopped`. Empty output
is invalid. A native stop without notification can instead appear as a timeout.

The `IQ` alias was removed; use `num_complex::Complex<f32>` directly.

Use `Airspy::open()` and `Airspy::open_serial(serial)` instead of `new()` and
`from_serial()`. The no-op `init()` and `exit()` wrappers were removed; opening
and dropping a device manage its lifetime.

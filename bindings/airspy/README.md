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

## Receiving

Call `start_rx()`, then `receive(&mut samples, timeout_ms)` with a
`num_complex::Complex<f32>` slice. The returned count is the filled prefix; the
tail is untouched. An empty slice is `Error::InvalidParameter`; before start or
after `stop_rx()`, `receive` returns `Error::StreamingStopped`.

- Reception buffers at most 262,144 IQ samples (2 MiB, about 26 ms at 10 Msps).
  Consume promptly: `Error::Overflow` stops callback delivery and persists until
  stop/restart, and queued samples are not returned after it. It detects the
  adapter filling, not USB-level sample loss.
- `Error::Timeout` means no samples arrived within the empty-buffer wait;
  zero milliseconds polls. The timeout does not bound mutex contention or
  copying. A native stop without notification can also appear as a timeout.
- Only the common C transfer prefix is read, so both 1.0.6 and newer layouts
  (with dropped-sample fields) work.

## Migrating from upstream 0.4.0

- `Airspy::new()` / `from_serial()` are `Airspy::open()` / `open_serial(serial)`.
  The no-op `init()` and `exit()` are gone; opening and dropping manage lifetime.
- `start_rx()` takes no arguments; read samples with `receive` (above).
- `FFIError` is `Error`, with named native failures (`Busy`,
  `InvalidParameter`, ...) and `Unknown(i32)` for unrecognized codes.
- `lib_version()` is `LibVersion::linked()`.
- Renamed: `get_sample_rates()` is `sample_rates()`, `get_board_id()` is
  `board_name()`, `get_version()` is `firmware_version()`, and `set_freq()` is
  `set_frequency()`. `get_partid_serial()` is `identity()`, returning
  `Identity { part_id, serial }` (was `PartID { part_id, serial_no }`).
- `Error` is `#[non_exhaustive]`.
- Removed with no aliases: `SampleType`, `Real`, `IQ` (use
  `num_complex::Complex<f32>`), integer sample formats, GPIO, register access,
  flash declarations, and the `rx` binary.

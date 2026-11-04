# Rust bindings for libairspy

A RawSocket Labs fork of [airspy-rs](https://github.com/adamgreig/airspy-rs)
0.4.0 (MIT, Adam Greig). Changes from upstream:

- An explicit stop (`Receiver::stop`, or dropping the `Receiver`) — upstream
  can only stop reception by hanging up the sample channel, which leaves
  libairspy unable to start streaming again.
- Caller-owned receive buffers replace the public sample channel.
- Standard-library C types and `num-complex` replace `libc` and `num`.
- Device-requiring tests are `#[ignore]`d.

Requires libairspy at least 1.0.6 already built and installed on your system.
`cargo test` runs the hardware-free tests. The device tests need an attached
Airspy and must run serially:
`RUST_TEST_THREADS=1 cargo test -- --ignored`.

See the [crate-level documentation](src/lib.rs) for a complete capture example.

## Receiving

`start_rx()` returns a `Receiver` that borrows the device. Call
`receive(&mut samples, timeout)` on it with a `num_complex::Complex<f32>` slice.
The returned count is the filled prefix; the tail is untouched, and an empty
slice is `Error::InvalidParameter`. The tuning setters (frequency, gains, AGC,
RF bias) also work on the `Receiver` while streaming; a sample-rate change
needs `stop()` and a restart. Dropping the `Receiver` stops reception and
discards the status; `stop()` returns it.

- Reception buffers at most 262,144 IQ samples (2 MiB, about 26 ms at 10 Msps).
  If it fills, later batches are dropped: `receive` first returns the samples
  queued before the gap, then `Error::Overflow { dropped }` once, then resumes.
  Batches arriving while that backlog drains are dropped too, so read
  promptly to keep the gap short.
  It detects the adapter filling, not USB-level sample loss.
- `Error::Timeout` means no samples arrived within the empty-buffer wait;
  `Duration::ZERO` polls. The timeout does not bound mutex contention or
  copying. A native stop without notification can also appear as a timeout.
- Only the common C transfer prefix is read, so both 1.0.6 and newer layouts
  (with dropped-sample fields) work.

## Migrating from upstream 0.4.0

- `Airspy::new()` / `from_serial()` are `Airspy::open()` / `open_serial(serial)`.
  The no-op `init()` and `exit()` are gone; opening and dropping manage lifetime.
- `start_rx()` takes no arguments and returns a `Receiver`; `receive`,
  `stop` and `is_streaming` live on it (above). `Airspy` is `Send`, and
  `receive` takes a `Duration`.
- `FFIError` is `Error`, with named native failures (`Busy`,
  `InvalidParameter`, ...) and `Unknown(i32)` for unrecognized codes.
- `lib_version()` is `LibVersion::linked()`.
- Renamed: `get_sample_rates()` is `sample_rates()`, `get_board_id()` is
  `board_name()`, `get_version()` is `firmware_version()`, and `set_freq()` is
  `set_frequency()`. `get_partid_serial()` is `identity()`, returning
  `Identity { part_id, serial }` (was `PartID { part_id, serial_no }`).
- `Error` is `#[non_exhaustive]`; `Overflow` carries `dropped` and no longer
  ends the session.
- Removed with no aliases: `SampleType`, `Real`, `IQ` (use
  `num_complex::Complex<f32>`), integer sample formats, GPIO, register access,
  flash declarations, and the `rx` binary.

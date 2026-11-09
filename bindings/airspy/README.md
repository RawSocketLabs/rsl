# Rust bindings for libairspy

A RawSocket Labs fork of [airspy-rs](https://github.com/adamgreig/airspy-rs)
0.4.0 (MIT, Adam Greig). Changes from upstream:

- An explicit stop (`Receiver::stop`, or dropping the `Receiver`) — upstream
  can only stop reception by hanging up the sample channel, which leaves
  libairspy unable to start streaming again.
- Caller-owned receive buffers, with a caller-chosen capacity, replace the
  public sample channel.
- Standard-library C types and `num-complex` replace `libc` and `num`.
- Device-requiring tests are `#[ignore]`d.

Requires libairspy at least 1.0.6 already built and installed on your system.
`cargo test` runs the hardware-free tests. The device tests need an attached
Airspy and must run serially:
`RUST_TEST_THREADS=1 cargo test -- --ignored`.

See the [crate-level documentation](src/lib.rs) for a complete capture example.

## Receiving

`start_rx(capacity)` takes the device and returns a `Receiver` that owns it;
`stop()` hands it back, and dropping the `Receiver` closes it. A failed
`start_rx` returns the device inside `StartError` for another attempt. The
rustdoc on `receive`, `discard` and `start_rx` is the contract; in short:

- The buffer holds `capacity` samples (`DEFAULT_RX_CAPACITY` is 262,144, about
  26 ms at 10 Msps); size it for the longest pause between `receive` calls.
  When it fills, `receive` delivers the queued samples, then one
  `Error::Overflow`, then resumes.
- Once libairspy ends the stream itself, `receive` returns
  `Error::StreamingStopped` after the queued samples.
- On libairspy before 1.0.11, a USB failure can leave libairspy's threads
  running: the device then refuses to start again and its handle is leaked
  rather than closed. Each session there also leaks a small allocation.
- Only the common C transfer prefix is read, so both 1.0.6 and newer layouts
  work.

## Migrating from upstream 0.4.0

- `Airspy::new()` / `from_serial()` are `Airspy::open()` / `open_serial(serial)`.
  The no-op `init()` and `exit()` are gone; opening and dropping manage lifetime.
- `start_rx(capacity)` takes the device by value and returns a `Receiver`;
  `receive`, `stop` and `is_streaming` live on it (above). `stop` returns the
  device, and `receive` takes a `Duration`. `Airspy`, `Receiver` and
  `StartError` are `Send`.
- `FFIError` is `Error`, with named native failures (`Busy`,
  `InvalidParameter`, ...) and `Unknown(i32)` for unrecognized codes.
- `lib_version()` is `LibVersion::linked()`.
- Renamed: `get_sample_rates()` is `sample_rates()`, `get_board_id()` is
  `board_name()`, `get_version()` is `firmware_version()`, and `set_freq()` is
  `set_frequency()`. `get_partid_serial()` is `identity()`, returning
  `Identity { part_id, serial }` (was `PartID { part_id, serial_no }`).
- Frequencies and sample rates are `FrequencyHz` and `SampleRateSps` (from the
  workspace's `rfus`, re-exported here), so `"434 MHz".parse()` works; both
  were bare `u32` before.
- `Error` is `#[non_exhaustive]`; `Overflow` carries `dropped` and no longer
  ends the session.
- Removed with no aliases: `SampleType`, `Real`, `IQ` (use
  `num_complex::Complex<f32>`), integer sample formats, GPIO, register access,
  flash declarations, and the `rx` binary.

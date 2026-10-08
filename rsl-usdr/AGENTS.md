# rsl-usdr

> Inherits the workspace-root `../AGENTS.md`.

A pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`: LMS6002D, Si5332, LP8758, TPS6381x,
TMP114), replacing the cxx FFI crate `../usdr`. Scope is RX at parity with that crate's API,
over both the USB and PCIe transports. Ported so far: board power-up and power-down.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-usdr` (unpublished until parity) | The driver |
| `sim/` | `rsl-usdr-sim` (unpublished, workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers, SPI, I2C, virtual clock, op trace |
| `oracle/` | `rsl-usdr-oracle` (unpublished, own workspace: compiles C) | Vendored libusdr driving `sim` through `oracle/shim/sim_plugin.c` |

## Driver layout

Each layer calls only the ones below it.

- `src/device.rs`: `Device`, the public API. It owns the bus and the board state.
- `src/board/`: `m2_lm6_1` sequences (power, then rate, tune and RX). Board wiring constants
  (I2C addresses, SPI targets) live here.
- `src/chips/`: one module per chip. Each knows its own registers (`bnb` bitfields) and maths,
  never the board.
- `src/fpga/`: gateware registers (GPO/GPI, then the stream engine).
- `src/lowlevel.rs`: the public, unstable `Bus` seam, mirroring libusdr's `ls_op`. Board code
  sleeps only through `Bus::sleep`.
- `tests/common/sim_bus.rs`: `Bus` over the sim. The oracle's parity tests include it with
  `#[path]`.

## Rules

- Every ported sequence gets a parity test in `oracle/tests/`. The test compares the Rust
  trace with libusdr's, operation for operation, on fresh boards.

- libusdr is the reference until hardware is available. Behaviour the Rust driver ports must
  first be exercised through the oracle against `sim`, then matched.
- The vendored subset under `oracle/libusdr/` is never edited by hand. Refresh it with
  `oracle/vendor.sh <usdr-lib checkout>` at the commit in `oracle/libusdr/PIN`. Bump the PIN
  in its own change.
- The sim models chips at register level and states every transport assumption it encodes,
  with its evidence, at the point it is encoded (see the I2C readback packing in `oracle/src/lib.rs`).
- Not ported: libusdr's front-end boards (`fe=`), external clock options (`extclk`,
  `extref`) and its `USDR_BARE_DEV`/`USDR_IGNORE_TMP`/`USDR_FE_TYPE` environment overrides.
  The oracle's parity tests assume those variables are unset.
- Intentional divergences from libusdr are whitelisted per test scenario, never by masking a
  register globally. The first planned one is the band-crossing fix: gate Si5332 port 3
  without cycling USYS_CTRL through READY, instead of libusdr's close/reopen.
- Register layouts in the driver use `bnb` bitfields.
- `sim` stays an unpublished crate, not a feature of the driver.

## Verify

```sh
cargo clippy -p rsl-usdr -p rsl-usdr-sim --all-targets && cargo test -p rsl-usdr -p rsl-usdr-sim
cargo fmt --manifest-path rsl-usdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path rsl-usdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rsl-usdr/oracle/Cargo.toml
```

The oracle is its own Cargo workspace: it sits inside this package's directory, where the
root workspace's `exclude` cannot reach. It needs a C compiler only; regenerating the vendored subset also needs `python3`
with PyYAML.

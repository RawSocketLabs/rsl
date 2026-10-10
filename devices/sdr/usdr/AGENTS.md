# rsl-usdr

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`.

A pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`: LMS6002D, Si5332, LP8758, TPS6381x,
TMP114), replacing the cxx FFI crate `usdr/` at the workspace root. Scope is RX at parity with that crate's API,
over both the USB and PCIe transports. Ported so far: board power-up and power-down,
temperature reading, the RX sample rate, bandwidth and frequency, and the RX stream
(create, start, receive, stop) over any streaming `Bus`, and the PCIe transport (feature
`pcie`; untested on hardware). The USB transport is next. Added beyond libusdr: the
thermal policy (`src/thermal.rs`), which gates start-up before anything is powered and has
a 110 °C hard stop no policy can lift. Parity tests strip its TMP114 temperature reads, an
intentional addition.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-usdr` (unpublished until parity) | The driver |
| `sim/` | `rsl-usdr-sim` (unpublished, workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers, SPI, I2C, virtual clock, op trace, and the RX stream engine's blocks below it |
| `oracle/` | `rsl-usdr-oracle` (unpublished, own workspace: compiles C) | Vendored libusdr driving `sim` through `oracle/shim/sim_plugin.c` |
| `pcie/` | `rsl-usdr-pcie` (unpublished, workspace member, Linux) | The `usdr_pcie_uram` driver's userspace interface: ioctls, the mapped BAR, DMA buffers. The stack's only `unsafe` |

## Driver layout

Each layer calls only the ones below it.

- `src/device.rs`: `Device`, the public API. It owns the `Board`, and the `Board` owns the bus
  (before power-up, `Identified` does). Board methods use `self.bus`; chip drivers stay
  stateless and borrow `&mut dyn Bus` per call.
- `src/thermal.rs`: the public thermal policy. `Device` applies it between
  `Board::identify` and power-up; it reads the sensor only through `Board`.
- `src/board/`: `m2_lm6_1` sequences (power, then rate, tune and RX). `board/board.rs`
  holds the `Board` struct, its chips and supply setpoints as associated consts
  (`Board::CLOCK = Si5332::usdr()`, `Board::BOOST_VOLTAGE`). Each sequence file
  (`power.rs`, ...) adds an `impl Board` block. Chip facts (IDs, revisions) stay in the chip
  module.
- `src/chips/`: one module per chip. Each knows its own registers (`bnb` bitfields) and maths.
  Its only board knowledge is where the uSDR wires it: an `at(addr)` constructor plus a
  `usdr()` constructor whose doc cites the address source (datasheet fixed address or
  libusdr's `I2C_DEV_*`). Sequences, setpoints and policy stay in `src/board/`.
- `src/fpga/`: gateware registers: GPO/GPI, the DSP chains' configuration ports (`phy`,
  with the decimator FIR tables in `fir_tables.rs`), and the RX stream engine (`stream`:
  front end, DMA, sync).
- `src/lowlevel.rs`: the public, unstable `Bus` seam, mirroring libusdr's `ls_op`. Board code
  sleeps only through `Bus::sleep`.
- `src/transport/`: `Bus` implementations for hardware, in safe code. `pcie` (feature `pcie`,
  Linux) over `rsl-usdr-pcie`, with the uSDR's driver layout (`USDR_LAYOUT`); `window` is the
  register-window and pair-write access libusdr's transports share.
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
  register globally. The first is the band-crossing fix: gate Si5332 port 3 with
  `OUT3_OE` alone, without cycling USYS_CTRL through READY as libusdr does (and without
  the FFI wrapper's close/reopen workaround); `oracle/tests/rx_parity.rs` whitelists it.
  The second leaves out stream create's per-channel baseband loop, a libusdr bug that
  reselects the band, clamps the NCOs and bypasses the filter;
  `oracle/tests/stream_parity.rs` whitelists it and the mixer exit it leaves libusdr owing.
- Registers are typed (`src/chips/register.rs`). Each chip has a `Reg` enum naming every
  address it touches, so no raw address appears.
  - A register whose contents the driver interprets gets a value type: a `bnb` bitfield,
    or a `BitEnum` with a catch-all for whole-byte values. It implements `Register`, or
    `IndexedRegister` when the chip repeats it per channel or output.
  - Opaque bytes, such as undocumented registers and tuning constants, stay `(Reg, u8)`
    writes.
  - 16-bit registers (TMP114) use `Reg` plus a value enum, without the traits.
  - Names say what a register, field or variant does (`EnableConfig::rx_enabled`, not
    `TopEncfg::srxen`). Active-low bits get names that read true when set.
  - Docs give the behaviour first, then the datasheet mnemonic, address or bits, reset
    value and units, so searching the datasheet name finds the item.
  - Sources: field meanings for the LMS6002D and Si5332 come from libusdr's register
    YAMLs (vendored beside the C). For TI parts, cite the datasheet by literature
    number and section: LP8758-E0 SNVSAC6B, TMP114 SNIS214E, TPS63810/11 SLVSEK4C. Never
    invent a field the source does not document; keep such bits in a raw byte, with the
    reason in the doc.
  - A chip whose register value types outgrow one file is a directory: `mod.rs` holds the
    chip docs, module declarations and re-exports; `<chip>.rs` the driver; the value types
    split by register group, plus any transport helper (the LMS6002D's `spi`). The
    LMS6002D splits by datasheet block (`top`, `pll`, `tx_rf`, `afe`, `lpf`, `rx_vga2`,
    `rx_fe`), each with its own `Reg`; registers several blocks repeat share a module and an
    index (`Pll`; `Lpf` for the filters' DAC bypass; `DcBlock` in `dc_cal` for the four
    DC-calibration engines).
    The I2C chips have one flat address space, so their single `Reg` sits in `reg.rs` and
    the value types split by function (Si5332: `state`, `input`, `divider`, `output`). The
    TMP114, with one value type, stays one file.
- `sim` stays an unpublished crate, not a feature of the driver.
- `unsafe` lives only in `rsl-usdr-pcie`; `rsl-usdr` keeps `#![forbid(unsafe_code)]`. That
  crate knows the driver's protocol and nothing of the board: layouts and register meanings
  stay in `rsl-usdr`. Its ABI and the uSDR layout are checked against the vendored header and
  libusdr's own layout by `oracle/tests/pcie_abi.rs`; nothing else of it can run without
  hardware (`tests/pcie_hardware.rs` is `#[ignore]`d).

## Verify

```sh
cargo clippy -p rsl-usdr -p rsl-usdr-sim -p rsl-usdr-pcie --all-targets && cargo test -p rsl-usdr -p rsl-usdr-sim -p rsl-usdr-pcie
cargo clippy -p rsl-usdr --all-targets --features pcie && cargo test -p rsl-usdr --features pcie
cargo fmt --manifest-path devices/sdr/usdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/usdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/usdr/oracle/Cargo.toml
```

The oracle is its own Cargo workspace: it sits inside this package's directory, where the
root workspace's `exclude` cannot reach. It needs a C compiler only; regenerating the vendored subset also needs `python3`
with PyYAML.

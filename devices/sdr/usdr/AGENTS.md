# rsl-usdr

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`.

A pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`: LMS6002D, Si5332, LP8758, TPS6381x,
TMP114), replacing the cxx FFI crate `bindings/usdr/`. The approved program now covers the
full functional libusdr library in phases: complete uSDR first, then all registered board
families and shared DSP/transport/service capabilities. The API is Rust-only: no production
C runtime, C ABI compatibility, Python/Soapy or legacy JSON/RPC adapters. See
[PORTING.md](PORTING.md) for the pinned capability ledger, acceptance loop and phased roadmap;
the program is not complete. Linux is first. Sans-I/O state machines and an externally owned
Mio reactor are the primary execution model; managed synchronous/asynchronous drivers are
opt-in. The first temperature/USB-I²C execution pilot is implemented, and feature `mio`
exposes it as the unstable, temperature-only `NativeUsb` (`UsbBus::into_native`); public
driver/stream handles for power, RX and TX remain unimplemented. Generic USB offers bounded
reap/filed-only completion primitives and an exclusive Mio `Readiness` (`rsl-usb` feature
`mio`), which `transport/usb/native` registers; cancellation remains an owned-worker
operation because the Linux discard ioctl can block. Native executor/worker supervision is
still open. The private `src/portable` production tree is shared unchanged with an unpublished
no_std + alloc compilation fixture; the public driver remains std-dependent. No hardware
qualification, release or publication is authorized by this roadmap.

The implemented baseline remains RX parity with the wrapper over USB and PCIe. Retained:
board power-up and power-down, temperature reading, the RX sample rate, bandwidth,
frequency, gains, path and corrections, and the RX stream
(create, start, receive, stop) over any streaming `Bus`, and the PCIe transport (feature
`pcie`; untested on hardware). USB register/SPI/I²C control-plane access is available
(feature `usb`, Linux, high-speed readback endpoint required; untested on hardware).
USB RX streaming uses eight reusable transfers on endpoint 0x83 with the pinned C
transport's 16-byte trailer. Hardware operation remains unqualified. Added beyond libusdr: the
thermal policy (`src/thermal.rs`), which gates start-up before anything is powered and has
a 110 °C hard stop no policy can lift. Parity tests strip its TMP114 temperature reads, an
intentional addition.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-usdr` (unpublished until parity) | The driver |
| `sim/` | `rsl-usdr-sim` (unpublished, workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers, SPI, I2C, virtual clock, op trace, the RX stream engine's blocks below it, and the QSPI flash controller with a fake flash (opt-in, `with_flash`) |
| `oracle/` | `rsl-usdr-oracle` (unpublished, own workspace: compiles C) | Vendored libusdr driving `sim` through `oracle/shim/sim_plugin.c` |
| `nostd-check/` | `rsl-usdr-nostd-check` (unpublished, own workspace) | Exact-source portable-core compilation witness; no separate production driver or embedded executable |
| `pcie/` | `rsl-usdr-pcie` (unpublished, workspace member, Linux) | The `usdr_pcie_uram` driver's userspace interface: ioctls, the mapped BAR, DMA buffers. The stack's only `unsafe` |

## Driver layout

Each layer calls only the ones below it.

- `src/device.rs`: `Device`, the public API. It owns the `Board`, and the `Board` owns the bus
  (before power-up, `Identified` does). Board methods use `self.bus`; chip drivers stay
  stateless and borrow `&mut dyn Bus` per call.
- `src/thermal.rs`: the public thermal policy. `Device` applies it between
  `Board::identify` and power-up; it reads the sensor only through `Board`.
- `src/board/`: `m2_lm6_1` sequences (power, then rate, tune and RX) and the executors
  that run migrated ones from `src/portable/` (power rails and Si5332 clocks so far). `board/board.rs` holds
  the `Board` struct, its chips and supply setpoints as associated consts
  (`Board::CLOCK = Si5332::usdr()`, `Board::BOOST_VOLTAGE`, `Board::RAILS`). Each sequence
  file (`power.rs`, ...) adds an `impl Board` block. Chip facts (IDs, revisions) stay with
  the chip's register values.
- `src/chips/`: one module per chip. Each knows its own registers (`bnb` bitfields) and maths.
  Its only board knowledge is where the uSDR wires it: an `at(addr)` constructor plus a
  `usdr()` constructor whose doc cites the address source (datasheet fixed address or
  libusdr's `I2C_DEV_*`). Setpoints and policy stay in `src/board/`; a sequence is in
  `src/board/` until it migrates to a portable machine. A chip whose values a portable
  machine uses keeps its address map, value types and ID facts in `src/portable/<chip>/`
  (LP8758, TPS6381x, Si5332, TMP114); `src/chips/<chip>/` then keeps the docs, address and
  driver.
- `src/cal/`: public calibration mathematics (`search`, `fit`), scalar and bit-exact with
  libusdr's `cal/opt_func.c` and `cal_filt.c` (`oracle/tests/cal_parity.rs`).
- `src/fpga/`: gateware registers: GPO/GPI, the DSP chains' configuration ports (`phy`,
  with the decimator FIR tables in `fir_tables.rs`), the RX stream engine (`stream`:
  front end, DMA, sync), and the QSPI flash controller (`flash`: libusdr's `espi_flash.c`,
  plus the public `FlashId`, `FlashError` and `WriteOptions`).
- `src/flash.rs`: the public boot-flash maintenance API. `Flash` owns a bus of its own (no
  `Device`, nothing powered) and reads the ID and contents; erase and write sit on the
  separate `Programmer` from `Flash::programmer`. Image headers and the update procedure
  are not ported (`PORTING.md`).
- `src/lowlevel.rs`: the public, unstable `Bus` seam, mirroring libusdr's `ls_op`. Board code
  sleeps only through `Bus::sleep`.
- `src/dsp/`: public `rsl_usdr::dsp`, libusdr's host sample conversions and channel
  (de)interleaving (`xdsp/conv.c` generic kernels), core-only scalar code;
  `oracle/tests/conv_parity.rs` checks every converter against the compiled C.
- `src/transport/`: `Bus` implementations for hardware, in safe code. `pcie` (feature `pcie`,
  Linux) over `rsl-usdr-pcie`, with the uSDR's driver layout (`USDR_LAYOUT`); `usb` (feature
  `usb`, Linux) over the workspace's `rsl-usb` host queues; `window` is the
  register-window and pair-write access libusdr's transports share.
- `src/portable/`: platform-independent addresses, the typed-register traits, the
  sequence seam (`sequence.rs`: register reads/writes and waits, one action at a time),
  TMP114 values/temperature operation, LP8758/TPS6381x register values and the power-rail
  machine (`power.rs`), Si5332 register values, divider maths and clock machines
  (`si5332/sequence.rs`), and USB framing/I²C state machine. `chips::register::run_sequence`
  is the blocking executor. Uses core
  types and generic executor errors; no OS/runtime ownership. Keep imports within the
  portable tree. Sequences move here as hand-written explicit state machines (decided
  2026-10-12, over async sequences); the blocking path runs the same machine through a
  small executor, so each sequence exists once and oracle parity proves it.
  Fixed-size machines do not allocate; the bnb dependency requires alloc. The native
  adapters preserve original bus errors and chip/read context.
- `src/chips/tmp114.rs` and `src/transport/usb/i2c.rs`: thin host/context adapters around
  those shared machines. The actual `Device::temperature`/`UsbBus::i2c` paths execute these
  through the existing blocking pump. `execution_tests.rs` proves the same machines
  with fake Mio readiness, managed-thread sync/future observation and callbacks, alongside
  bounded synthetic RX loans. `native/` (feature `mio`; `NativeUsb` in `handle.rs`) runs the
  I²C machine over `rsl_usb::Readiness`: the reactor only registers, schedules and
  observes; once attached, one worker owns queues, syscalls, the readiness's Drop, cleanup
  and Drop. Retirement always completes, even when deregistration reports `NotFound`, and
  the adapter keeps the shared Waker open for the worker's final wake. `into_native` checks
  the TMP114 ID over the blocking bus, then carries the notification sequence, I²C table
  and operation identities over. Notification transfers are sized as in the blocking bus
  and the pinned C (`IN_NTFY_SIZE`). Its unit tests use a socket source, not usbfs; only the
  opt-in `usb_hardware` test reaches real queues. RX/TX handles, fairness and browser
  integration remain open.
- `tests/common/sim_bus.rs`: `Bus` over the sim. The oracle's parity tests include it with
  `#[path]`.

## Rules

- Portability is qualified per migrated operation, not by declaring the entire driver no_std.
  The detached fixture must import the exact production module tree, instantiate its real
  macro/machine paths and pass `scripts/ci/portable.py`; no copied algorithms or host stubs.
  Bare-metal compilation is not linking, allocation-free operation or hardware evidence.
- Keep scalar implementations authoritative. SIMD/CPU specialization belongs beside its
  DSP/kernel owner, with scalar fallback, equivalence tests and a measured bottleneck;
  USB/PCIe and scheduling tuning stays in transports/executors. Extract sharing only when
  concrete consumers justify it.
- USB uses existing Linux usbfs, not a custom uSDR kernel module. PCIe still requires the
  external C `usdr_pcie_uram` module and ABI3; Rust userspace parity does not remove it.
  Installation/loading, matching kernel headers/DKMS and module-signing prerequisites are
  operator responsibilities. Never install/load modules or rewrite kernel/FPGA code as an
  automatic library side effect.

- Every ported sequence gets a parity test in `oracle/tests/`. The test compares the Rust
  trace with libusdr's, operation for operation, on fresh boards.
  USB's simple-I²C LUT/control sequence is checked against the actual C helpers in
  `oracle/tests/usb_i2c_parity.rs`; USB framing/router fixtures cite the same PIN.
  `oracle/tests/usb_rx_parity.rs` compares RX payload/OOB and lifecycle against compiled,
  mechanically exported C helpers (`vendor.sh --usb-rx-only` refreshes them without
  changing the PIN). It replaces only the host-buffer boundary, not C trailer parsing.
  Unlike C's discard-buffer/autorestart scheme, RX retains eight bounded transfers and
  requeues each after consumption, leaving gateware to report backpressure losses.
  USB RX fails closed on partial/malformed blocks rather than exposing stale bytes;
  ordinary receive timeouts preserve pending transfers and control-plane health.
  Receive capacity exceeds the frame length so packet-aligned frames keep their terminating
  ZLP in the same transfer. Failed cancellation during setup/stop requires dropping or
  closing the `Device` and reopening it; do not claim an in-place retry through `Device`.
  The transport fails closed on malformed replies, missing notifications, or ambiguous
  timeouts, and refuses USB routing while HWID reports `PCIe` ownership. This differs
  deliberately from C's sequence-gap resynchronization and routing-setup skip.

- libusdr is the reference until hardware is available. Behaviour the Rust driver ports must
  first be exercised through the oracle against `sim`, then matched.
- The vendored subset under `oracle/libusdr/` is never edited by hand. Refresh it with
  `oracle/vendor.sh <usdr-lib checkout>` at the commit in `oracle/libusdr/PIN`. Bump the PIN
  in its own change.
- The sim models chips at register level and states every transport assumption it encodes,
  with its evidence, at the point it is encoded (see the I2C readback packing in `oracle/src/lib.rs`).
- Currently not ported: front-end boards (`fe=`) and the remaining controls/TX/shared-library
  domains in [PORTING.md](PORTING.md). The RX gain, path and correction controls
  (`Device::set_rx_gain`, `set_rx_path`, `set_rx_dc_trim`, `set_rx_auto_dc_correction`,
  `set_rx_ip2_trim`, `set_rx_iq_correction`) are ported.
  Their functional behaviours are in the approved phased scope; legacy environment
  variable spelling is not a compatibility requirement. The present oracle assumes
  `USDR_BARE_DEV`, `USDR_IGNORE_TMP` and `USDR_FE_TYPE` are unset. AFE79xx-based DSDR is
  blocked on authorized register-sequence/profile evidence; a C plugin fallback is not
  permitted and must not be counted as a pure-Rust port.
- Intentional divergences from libusdr are whitelisted per test scenario, never by masking a
  register globally. The first is the band-crossing fix: gate Si5332 port 3 with
  `OUT3_OE` alone, without cycling USYS_CTRL through READY as libusdr does (and without
  the FFI wrapper's close/reopen workaround); `oracle/tests/rx_parity.rs` whitelists it.
  The second leaves out stream create's per-channel baseband loop, a libusdr bug that
  reselects the band, clamps the NCOs and bypasses the filter;
  `oracle/tests/stream_parity.rs` whitelists it and the mixer exit it leaves libusdr owing.
  The flash maintenance refuses, before any bus access, ranges libusdr would mishandle
  (past 16 MiB, partial erases, page-crossing chunks) and reports large capacities whole;
  `oracle/tests/flash_parity.rs` whitelists each. Two more cannot run in libusdr safely
  (its header-length underflow, its staging `assert`), so `tests/flash.rs` covers them.
- Registers are typed (`src/portable/register.rs`; `src/chips/register.rs` adds blocking I2C
  access). Each chip has a `Reg` enum naming every
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
cargo clippy -p rsl-usdr --all-targets --all-features -- -D warnings && cargo test -p rsl-usdr --all-features
cargo fmt --manifest-path devices/sdr/usdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/usdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/usdr/oracle/Cargo.toml
python3 scripts/ci/portable.py
cargo fmt --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml --locked
# Explicit local portability milestone; hosted cross-target qualification stays release-only.
cargo check --manifest-path devices/sdr/usdr/nostd-check/Cargo.toml --locked --target thumbv7em-none-eabi
# Opt-in hardware smoke: resets/claims only the explicitly selected USB port.
RSL_USDR_USB_PORT=1-2 cargo test -p rsl-usdr --features usb,mio --test usb_hardware -- --ignored
```

The oracle is its own Cargo workspace: it sits inside this package's directory, where the
root workspace's `exclude` cannot reach. It needs a C compiler only; regenerating the vendored subset also needs `python3`
with PyYAML.

The portable fixture has its own committed lockfile. Ordinary selected CI uses host-only
validation; a release of a dependency cannot opt USDR into bare-metal qualification.

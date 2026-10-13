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
opt-in. The USB I²C and register/SPI machines run in the blocking bus, and feature `mio`
exposes the unstable `NativeUsb` (`UsbBus::into_native`, or `Device::into_native` from a
powered device), whose generic executor runs any portable sequence: the board's own
(temperature, identity, reference switch, power-up behind the thermal policy and
power-down, sample rate, tuning, bandwidths, gains, paths, and a stream group's create,
start, mute, throttle and stop, the RF chains' power, tuning and DC calibration
included), so a board goes from open to streaming with no blocking `Device`; plus the
RX/TX streams at the transport, `ManagedUsb`, a synchronous runner over it, and
`AsyncUsb`, a runtime-free asynchronous one. Generic USB offers bounded
reap/filed-only completion primitives and an exclusive Mio `Readiness` (`rsl-usb` feature
`mio`), which `transport/usb/native` registers; cancellation remains an owned-worker
operation because the Linux discard ioctl can block. The worker is supervised: a panic,
a lost report or an idle hang-up ends the session with an event, and the worker still
releases the device. The private `src/portable` production tree is shared unchanged with an unpublished
no_std + alloc compilation fixture; the public driver remains std-dependent. No hardware
qualification, release or publication is authorized by this roadmap.

The implemented baseline remains RX parity with the wrapper over USB and PCIe. Retained:
board power-up and power-down, temperature reading, the RX, TX and both-chain sample rates, RX bandwidth,
frequency, gains, path and corrections, the stream group
(`create_streams`, `start_streams` with every sync type libusdr offers the uSDR but `ExternalAll`, `receive_into` from
`ci16`/`ci12` wire into `ci16`/`cf32`/`ci12` host buffers on either complex channel or both
(the second needs the RX chain), or from one real lane (`i16`/`i12`, I or Q) into
`i16`/`f32` buffers, `send`/`send_with_stat` from those host formats to `ci16` wire on
either TX channel or both, on the original TX core or the extended one (HWID bit 26, also
`ci12` wire), with timestamps and telemetry, `set_tx_mute`, `set_tx_channels`,
`stop_streams`; plus the `start_rx_stream`/`receive`/`stop_rx_stream` ci16
shim) over any streaming `Bus` (TX over the sim, PCIe and USB; `PcieBus: AsFd` is the
pollable stream descriptor), the identity's channel capabilities, raw chip debug
registers (`chip_register`), the TX
controls (frequency, path, bandwidth, gains, corrections, constant generator), and the
PCIe transport (feature `pcie`; untested on hardware). USB
register/SPI/I²C control-plane access is available
(feature `usb`, Linux, high-speed readback endpoint required; untested on hardware).
USB RX streaming uses eight reusable transfers on endpoint 0x83 with the pinned C
transport's 16-byte trailer; USB TX up to 32 on endpoint 0x03, each block behind its
16-byte header, with the TX core's status read every 64th send. Hardware operation remains unqualified. Added beyond libusdr: the
thermal policy (`src/thermal.rs`), which gates start-up before anything is powered and has
a 110 °C hard stop no policy can lift. Parity tests strip its TMP114 temperature reads, an
intentional addition.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-usdr` (unpublished until parity) | The driver |
| `sim/` | `rsl-usdr-sim` (unpublished, workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers, SPI, I2C, virtual clock, op trace, the RX stream engine's blocks below it (16- and 12-bit packing of channel 0's I and Q, channel 1's on lanes 2 and 3, both, or lane 0 or 1 alone), the original or extended TX core and its `PCIe`-driver buffer ring (`tx_*`, played packets in `tx_packets`), the sync unit shared by both, the RX DC estimator and an optional deterministic RF loopback for it (`with_loopback`: synthetic LO leakage and IQ imbalances, assumptions in `sim/src/loopback.rs`), the QSPI flash controller with a fake flash (opt-in, `with_flash`), optional front ends on I2C bus 1 (`SimBoard::with_front_end`; none by default), and a clock-measurement word tests load (`set_clock_measurement`; not modelled) |
| `oracle/` | `rsl-usdr-oracle` (unpublished, own workspace: compiles C) | Vendored libusdr (and four of its tools, `libusdr/tools/`: `usdr_flash.c`, `usdr_dm_create.c`, `usdr_dm_gpsdo.c`, `xmass_fwupd.c`) driving `sim` through `oracle/shim/sim_plugin.c`, one board per device of libusdr's multi-device aggregate (`bus=sim0:sim1:...`), and a fake XMASS device (`src/xmass.rs`) through `oracle/shim/xmass_fwupd.c`; libusdr's Verilator host bridge compiled whole by `oracle/shim/verilator.c` against a fake model |
| `nostd-check/` | `rsl-usdr-nostd-check` (unpublished, own workspace) | Exact-source portable-core compilation witness; no separate production driver or embedded executable |
| `pcie/` | `rsl-usdr-pcie` (unpublished, workspace member, Linux) | The `usdr_pcie_uram` driver's userspace interface: ioctls, the mapped BAR, DMA buffers, and its XMASS device (`/dev/xmassN`, `XmassDevice`). The stack's only `unsafe` |

## Driver layout

Each layer calls only the ones below it.

- `src/device.rs`: `Device`, the public API. It owns the `Board`, and the `Board` owns the bus
  (before power-up, `Identified` does). Board methods use `self.bus`; chip drivers stay
  stateless and borrow `&mut dyn Bus` per call. The transmit chain's `TxState` is on
  `Board` beside `RxState`.
- `src/stream.rs`: the stream group's public configuration and results (`StreamsConfig`,
  `RxConfig`, `TxConfig`, `WireFormat`, `HostFormat`, `SyncMode`, `StreamTime`,
  `RxMetadata`, `TxTelemetry`, `TxMute`, the sealed `HostSample` buffer elements). One
  device-owned group per PORTING.md's stream-handle decision; `board/stream.rs` runs the
  group and RX, `board/tx_stream.rs` TX; `portable/stream.rs` holds the stream engine's
  registers (RX DMA, front end, sync unit, TX core control) as `EngineSequence`, which
  the board and `NativeUsb` run; `fpga/stream.rs` the burst maths and burst-format word,
  `fpga/tx_stream.rs` packet announcement, status decoding and the TX create's gateware
  and block-size checks; `StreamsConfig::check_on` (`board/stream.rs`) the board's
  refusals before any I/O, and `StreamsConfig::check` those narrowed to what `NativeUsb`'s
  group runs (channel 0 on the original TX core).
- `src/multi.rs`: `MultiDevice`, libusdr's multi-device aggregate (`device/mdev.c`): a
  master and members, each a `Device`; settings broadcast through any `Device` method,
  one stream group across the devices (created stream by stream, started part by part
  with `Board::{check_streams, create_stream, start_parts, start_part, generate_sysref}`),
  and every call reporting each device's `Outcome` (`MultiError`), never claiming
  atomicity. It stays here until a second SDR driver needs an aggregate (then the
  device-independent part moves to `devices/sdr/`). `oracle/tests/multi_parity.rs`
  drives libusdr's aggregate over several sim boards (`Oracle::open_multi`).
- `src/thermal.rs`: the public thermal policy. `Device` applies it between
  `Board::identify` and power-up; it reads the sensor only through `Board`.
- `src/board/`: the `m2_lm6_1` board over a blocking bus. Its RF sequences (power-up and
  power-down, rate, tune, gains, paths, the RF chains' part of a stream's create and
  destroy) are portable machines (`src/portable/radio`), which `Board` runs with
  `Board::run_radio`/`run_script`, lending its `Radio` state and taking it back; the stream
  engine's are `portable/stream.rs`'s; the RX and TX controls, corrections, calibration
  and streaming that remain blocking code run beside them. `board/board.rs` holds
  the `Board` struct (bus, `Radio`, the DSP corrections, reference, stream group, front
  end), its chips and supply setpoints as associated consts
  (`Board::CLOCK = Si5332::usdr()`, `Board::BOOST_VOLTAGE`, `Board::RAILS`). Each sequence
  file (`power.rs`, ...) adds an `impl Board` block. Chip facts (IDs, revisions) stay with
  the chip's register values.
  `board/calibration.rs` runs libusdr's LO-leakage and IQ-imbalance calibration
  (`usdr_calibrate`, `cal/cal_lo_iqimb.c`) on `cal::search::best_2d`, measuring with the
  gateware's DC estimator (`fpga/dc_estimator.rs`, `ipblks/dc_estim.c`); blocking
  register code, no streaming (`oracle/tests/calibration_parity.rs`).
- `src/chips/`: one module per chip. Each knows its own registers (`bnb` bitfields) and maths;
  the LMS6002D's register values, cache and operations are portable (`portable/lms6002d`),
  so `chips/lms6002d` keeps its docs.
  Its only board knowledge is where the uSDR wires it: an `at(addr)` constructor plus a
  `usdr()` constructor whose doc cites the address source (datasheet fixed address or
  libusdr's `I2C_DEV_*`). Setpoints and policy stay in `src/board/`; a sequence is in
  `src/board/` until it migrates to a portable machine. A chip whose values a portable
  machine uses keeps its address map, value types and ID facts in `src/portable/<chip>/`
  (TPS6381x, Si5332, TMP114), or in a shared per-chip crate another driver needs too
  (LP8758: `rsl-lp8758`, re-exported as `src/portable/lp8758.rs`); `src/chips/<chip>/`
  then keeps the docs, address and driver.
- `src/cal/`: public calibration mathematics (`search`, `fit`), scalar and bit-exact with
  libusdr's `cal/opt_func.c` and `cal_filt.c` (`oracle/tests/cal_parity.rs`).
- `src/fpga/`: gateware registers: GPO/GPI, the DSP chains' configuration ports (`phy`,
  with the decimator FIR tables in `fir_tables.rs`), the RX stream engine (`stream`:
  front end, DMA, sync), the TX stream cores (`tx_stream`: the original core's
  announcement and status, the extended core's front end, lane map and cached
  announcement), and
  the QSPI flash controller (`flash`: libusdr's `espi_flash.c`,
  plus the public `FlashId`, `FlashError` and `WriteOptions`).
- `src/flash/`: the public boot-flash maintenance API. `Flash` owns a bus of its own (no
  `Device`, nothing powered) and reads the ID and contents; erase and write sit on the
  separate `Programmer` from `Flash::programmer` (`flash.rs`). `image.rs` ports libusdr's
  gateware image header parser and slot check (`ipblks/xlnx_bitstream.c`: `ImageHeader`,
  `Image`, `Slot`), bus-free; `update.rs` its flash tool's update procedure
  (`tools/usdr_flash.c`: `Flash::boot_images`, `Programmer::{update_master,
  update_golden, erase_master}`), step for step. The module docs hold the recovery story.
  `oracle/tests/{image,flash_update}_parity.rs` check both against the compiled C.
- `src/frontend/`: public `rsl_usdr::frontend`, the boards libusdr probes on the M.2
  connector (`device/device_fe.c`, `device/ext_*`). The probe itself, every board's
  identification, the connector's GPIO core, the `PCIe` front end and breakout whole and
  their chips (TCA6416, TCA9555, DACx0501) are `rsl-sdr-frontend`'s (`../frontend`),
  shared with the XSDR and LSDr drivers; `frontend/mod.rs` re-exports its public types
  (`FrontEndSelection`, `FrontEndKind`, the `Pciefe*` and `Exm2peOptions` options,
  `XmassCalibrationPath`) and implements its `BringUp` with the bring-ups that stay here,
  one module per board (`xmass`, `supersync`, `simplesync`; their chips LMK05318,
  LMK5C33216, TMP114 and AT24 in `src/chips/`). `xmass_firmware.rs` (public) ports libusdr's
  `tools/xmass_fwupd.c`, the XMASS switch's firmware update, over `SwitchFlash`, the
  driver's four XMASS requests as a trait (implemented for `XmassDevice` in
  `transport/pcie.rs`); `oracle/tests/xmass_firmware_parity.rs` runs both against one fake
  device. `board/frontend.rs` runs the probe at the
  end of power-up and owns the board that came up. Blocking code: the probes interleave
  GPIO-core writes with I2C transfers the `Sequence` seam cannot express. The LMK05318 (`chips/lmk05318/`, XMASS and
  `SimpleSync`) keeps libusdr's per-chip state (APLL2, the register staging map), its
  frequency plan in `plan.rs` and the plan's registers in `staging.rs`, both bus-free, so
  the oracle compiles them with the map and value types directly
  (`lmk05318_parity.rs` compares plans and staged
  registers with the C through `oracle/shim/lmk05318.c`, which needs GCC). Its
  16-bit addresses take chip-local `Lmk05318Register`/`IndexedLmk05318Register` traits in
  place of the 8-bit shared ones. Since PE_SYNC needs it too, the bus-free modules (plan,
  staging, map, `reg` and the value types) are the shared `rsl-lmk05318`
  (`devices/sdr/chips/lmk05318`), re-exported under `chips::lmk05318`; the bus code in
  `lmk05318.rs` stays here (`rsl-pe-sync` has its own, `../../sync/pe_sync/PORTING.md`).
- `src/lowlevel.rs`: the public, unstable `Bus` seam, mirroring libusdr's `ls_op`: the shared
  `rsl-lowlevel` crate's `Bus`, `BusError` and addresses, re-exported unchanged, plus
  rsl-usdr's own conversions into `BusError` (`usb_error` for `rsl_usb::Error`: `From`
  would be an orphan impl). Board code sleeps only through `Bus::sleep`. The uSDR has one
  I2C core; its transports refuse any `I2cAddr` core but 0.
- `src/dsp/`: public `rsl_usdr::dsp`, libusdr's host sample conversions and channel
  (de)interleaving (`xdsp/conv.c` generic kernels, core-only scalar code);
  `oracle/tests/conv_parity.rs` checks every converter against the compiled C.
  `window`, `fftad`, `rtsa` and `fm` port FFT windowing, log-power averaging, the RTSA
  histogram and FM quadrature (`oracle/tests/{fft,rtsa_fm}_parity.rs`).
- `src/dsp/{filter,trig,nco,sincos}.rs`: block FIR (`xdsp/filter.c`, `conv_filter.c` generic
  kernels), fixed-point sine (`trig.c`), the NCO (scalar port of `nco.c`'s SSSE3-only kernel) and
  float tone/chirp tables (`sincos_functions.c`; std `sinf`/`cosf`), checked by
  `oracle/tests/{filter,nco}_parity.rs` (the oracle builds `nco.c` with SSSE3 on x86).
  `sincos` also holds what libusdr's `usdr_dm_create` tool wires its TX test signals with
  (turns to phases, dBFS to gain, `Chirp::sweep`, complex fills, the `cf32` `ToneF32`),
  checked against the tool's own producer threads and TX loop by
  `oracle/tests/generator_parity.rs`; the tool's option policy is not ported.
- `src/discipline.rs`: public `rsl_usdr::discipline`, the oscillator discipline loop of
  libusdr's `usdr_dm_gpsdo` tool as a sans-I/O machine (`OscillatorDiscipline`): the caller
  reads the measurement, steps it with the time and writes and confirms the DAC code.
  `oracle/tests/gpsdo_parity.rs` runs the tool's own `main`; which gateware word carries the
  PPS count on the uSDR is unresolved (PORTING.md), so no `Device` method composes it.
- `src/transport/`: `Bus` implementations for hardware, in safe code. `pcie` (feature `pcie`,
  Linux) over `rsl-usdr-pcie`, with the uSDR's driver layout (`USDR_LAYOUT`); `usb` (feature
  `usb`, Linux) over the workspace's `rsl-usb` host queues; `verilator` (feature
  `verilator`, Linux) libusdr's Verilator host bridge to an external gateware model, a
  Unix socket plus a shared file read with positioned I/O (no mapping, so no `unsafe`),
  blocking on the calling thread, its divergences in its module docs;
  `oracle/tests/verilator_parity.rs` compares its bytes with libusdr's bridge against the
  fake model in `tests/common/verilator_peer.rs`. `window` is the
  register-window and pair-write access libusdr's PCIe and USB transports share.
- `src/portable/`: platform-independent addresses, the typed-register traits and the
  sequence seam (`address.rs`, `register.rs`, `sequence.rs`: thin re-exports of
  `rsl-lowlevel`; I²C register reads/writes, FPGA register reads/writes and SPI32 with
  32-bit results, and waits, one action at a time), LP8758 values (`lp8758.rs`, re-exporting
  `rsl-lp8758`), TMP114 values/temperature operation, TPS6381x register values and the
  power-rail machine (`power.rs`), Si5332 register values, divider maths and clock machines
  (`si5332/sequence.rs`), the stream engine's register sequences (`stream.rs`), the
  LMS6002D's register values, cache, straight-line operations as `Script`s, PLL tuning
  and DC calibrations (`lms6002d/`), the gateware's GPO and DSP-port words and FIR loads
  (`fpga.rs`, `fir_tables.rs`), `Script` (`script.rs`, a fixed action list), the board's
  RF state and composite machines (`radio/`: `Radio`; `run.rs`'s `Run` drives a
  composite's parts with libusdr's stop, carry-on-and-report and ignore failure
  policies; power, rate, tune, gain, path and stream parts), and USB framing plus the I²C
  and register/SPI state machines (`usb/`). A sequence that attempts steps after a
  failure says so (`rsl-lowlevel`'s `Sequence::attempts_after_failure`, added
  2026-10-13 for "attempt every step" paths, pending owner review); both executors then
  run them and report the first failure. FPGA actions name
  registers as the `Bus` does (32 bits), so a sequence may name the stream engine's
  configuration space; each executor reaches it through its index window.
  `chips::register::run_sequence` wraps `rsl-lowlevel`'s blocking executor and
  turns its `SequenceFailure` into `Error` through the accesses' shared `context()`
  decoding. Uses core
  types and generic executor errors; no OS/runtime ownership. Keep imports within the
  portable tree. Sequences move here as hand-written explicit state machines (decided
  2026-10-12, over async sequences); the blocking path runs the same machine through a
  small executor, so each sequence exists once and oracle parity proves it.
  Fixed-size machines do not allocate; the bnb dependency requires alloc. The native
  adapters preserve original bus errors and chip/read context.
- `src/chips/tmp114.rs` and `src/transport/usb/i2c.rs`: thin host/context adapters around
  those shared machines. `transport/usb/machine.rs` puts the I²C and register/SPI machines
  behind one transfer vocabulary; `UsbBus`'s register, SPI and I²C calls run them through
  one blocking pump. `execution_tests.rs` proves the same machines
  with fake Mio readiness, managed-thread sync/future observation and callbacks, alongside
  bounded synthetic RX loans. `native/` (feature `mio`; `NativeUsb` in `handle.rs`) runs the
  same machines, and any portable sequence of them with waits as caller deadlines, over
  `rsl_usb::Readiness` and endpoints 0x01/0x81/0x82: the reactor only registers, schedules and
  observes; once attached, one worker owns queues, syscalls, the readiness's Drop, cleanup
  and Drop. Retirement always completes, even when deregistration reports `NotFound`, and
  the adapter keeps the shared Waker open for the worker's final wake. `into_native` runs
  the identity sequence, holds the LMS6002D in reset and checks the TMP114 ID over the
  blocking bus, as `Board::identify` does, then carries the
  notification sequence, I²C table and operation identities over. `Device::into_native`
  (`UsbHandOver` in `bus.rs`) does the same from a powered device with no stream group,
  without I/O: the `Board` builds the session's `BoardState` (identity facts and chains,
  both chains' IQ corrections, the `Radio` RF state), the device adds its thermal policy,
  the `UsbBus` is reached through `Bus::as_any_mut` and left released
  (refused, keeping it, while it still holds a stream's endpoint), and the board stays
  powered under the handle. Notification transfers are sized as in the blocking bus
  and the pinned C (`IN_NTFY_SIZE`). Its streams (`handle/stream.rs`) run the blocking
  bus's `Rx`/`Tx` over `backend::Filed` queues, which take only completions a reap turn
  filed; the worker copies RX blocks out (at most `adapter::RX_BACKLOG` waiting) and
  cancels streams, and a send follows `TxDevice::send`'s order with its status read on the
  register machine. Its RF operations (`handle/radio.rs`) run the board's `portable::radio`
  machines on the session's `Radio` (a handed-over device's, or its own from
  `start_power_up`, which reads the TMP114 first and refuses at the thermal policy's
  start limit). Its stream group (`handle/group.rs`) creates, starts, readies, mutes,
  throttles and stops the engine with the board's own `EngineSequence`s and
  `Phy::{rx,tx}_create_writes` (the create's DSP-port writes, also `EngineSequence`s), the
  RF chains' create and stop parts, the TX gateware gate and the
  transport opens and closes, as steps of one operation; `native/window.rs` (`Windowed`)
  reaches configuration-space registers as `UsbBus` does (index, then data register).
  One operation command and one reap turn are outstanding at a time
  (`worker::MAILBOX`); `poll` advances the operation before handing over an RX block.
  `managed.rs` (`ManagedUsb`) owns a `Poll` and drives a handle on the calling thread;
  `asynchronous.rs` (`AsyncUsb`) a reactor thread of its own behind futures woken through
  `core::task::Waker`. Its unit tests use a socket source, not usbfs; only the opt-in
  `usb_hardware` test reaches real queues. A native power-up probes no front end; the
  front-end controls, corrections, calibration and the extended TX core stay with the
  blocking device; browser integration remains open.
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
  mechanically exported C helpers (`vendor.sh --usb-only` refreshes them, and the TX
  ones, without changing the PIN). It replaces only the host-buffer boundary, not C
  trailer parsing. `oracle/tests/usb_tx_parity.rs` does the same for TX: block limits,
  header bytes for full and partial blocks at every time and host format, and the
  throttled status reads, over a fake buffer and register boundary; libusdr's own
  `usb_async_start` statistics lines come along verbatim. The bus seam's `TxCommit`
  says who announces a block: USB frames it, so the driver writes no push registers.
  The parity test drives the same `TxDevice::send` sequence `UsbBus` runs. USB TX fails
  a stream on a failed transfer, at the send that would reuse its buffer (libusdr loses
  the buffer silently), refuses commits of no bytes, more than a block or part samples,
  which the driver never makes, and treats a zero-wait full ring as any timeout, where
  libusdr returns `-EAGAIN` without its status read.
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
  in its own change. Three tools come along verbatim (`oracle/libusdr/tools/`); of them,
  `usdr_dm_create.c` and `usdr_dm_gpsdo.c` reach `oracle/shim/{dm_create,gpsdo}.c`, which compile each
  whole with its `main` renamed, the gpsdo shim redirecting the tool's I/O, set-up and
  clock by macro before including it, and build them and `sincos_functions.c` with
  `-fwrapv`, so phases that wrap in signed C arithmetic compare with Rust's.
  The Verilator host bridge (`libusdr/lib/lowlevel/verilator_ll/`) is vendored verbatim
  but left out of `sources.txt`: it does not build against the rest of libusdr at the PIN,
  so `oracle/shim/verilator.c` includes it and adapts the three drifted calls by macro.
  Port only MIT (or MIT-dual) upstream sources; libusdr's `PCIe` kernel module
  (`pcie_uram/driver/usdr_pcie_uram.c`) is GPL-2.0 only, so the bridge's notification
  words rest on the MIT `pcie_uram/pcie_events.rst` instead.
- The sim models chips at register level and states every transport assumption it encodes,
  with its evidence, at the point it is encoded (see the I2C readback packing in `oracle/src/lib.rs`).
- Front ends (`fe=`): the probe (`rsl-sdr-frontend`, shared with the XSDR and LSDr) and
  the boards the uSDR admits follow libusdr's `device_fe_probe`; every board it admits
  comes up (pciefe, xmass, supersync,
  simplesync, exm2pe), and a failed probe powers the board down again where libusdr
  leaves it powered. libusdr's lock waits run on its wall clock; rsl-usdr's count their
  100 µs poll sleeps, so they last at least as long on hardware, and the sim reports lock
  at once, which keeps both traces deterministic. The default `SimBoard`
  has nothing on its connector, so both drivers end the probe at the M.2-to-PCIe breakout
  without a DAC. See the front-end row of [PORTING.md](PORTING.md).
- Currently not ported: the remaining controls/TX/shared-library domains in
  [PORTING.md](PORTING.md). The RX gain, path and correction controls
  (`Device::set_rx_gain`, `set_rx_path`, `set_rx_dc_trim`, `set_rx_auto_dc_correction`,
  `set_rx_ip2_trim`, `set_rx_iq_correction`) are ported, and so are the TX controls
  (`Device::set_tx_frequency`, `set_tx_path`, `set_tx_bandwidth`, `set_tx_gain`,
  `set_tx_dc_offset`, `set_tx_iq_correction`, `set_tx_constant`) and TX streaming on the
  original TX core (`Device::send`, `send_with_stat`, `set_tx_mute`; `oracle/tests/
  tx_stream_parity.rs`, over USB `oracle/tests/usb_tx_parity.rs`), and so is the
  LO-leakage/IQ-imbalance calibration (`Device::calibrate`;
  `oracle/tests/calibration_parity.rs`), and so is the extended TX core and both
  directions' second channels (`oracle/tests/{tx_stream,stream_formats}_parity.rs`);
  not ported natively are the front-end probe and controls, the corrections and
  calibration, and second channels and the extended TX core in a native (`NativeUsb`)
  stream group (channel 0 on the original TX core); not ported at all are the
  gateware FFT format and lane bifurcation (no gateware evidence), and
  the antenna-timing mockup gets no method.
  Their functional behaviours are in the approved phased scope; legacy environment
  variable spelling is not a compatibility requirement. The present oracle assumes
  `USDR_BARE_DEV`, `USDR_IGNORE_TMP` and `USDR_FE_TYPE` are unset. AFE79xx-based DSDR is
  blocked on authorized register-sequence/profile evidence; a C plugin fallback is not
  permitted and must not be counted as a pure-Rust port.
- Intentional divergences from libusdr are whitelisted per test scenario, never by masking a
  register globally. The first is the band-crossing fix: gate Si5332 port 3 with
  `OUT3_OE` alone, without cycling USYS_CTRL through READY as libusdr does (and without
  the FFI wrapper's close/reopen workaround); `oracle/tests/common/crossing.rs` holds the
  whitelist, which scenarios in `rx_parity.rs` and `rx_controls_parity.rs` opt into.
  The second leaves out stream create's per-channel baseband loop, a libusdr bug that
  reselects the band, clamps the NCOs and bypasses the filter;
  `oracle/tests/common/baseband.rs` holds the cut, which `stream_parity.rs` and
  `stream_formats_parity.rs` apply (the TX PLL retune before it stays compared), and
  `stream_parity.rs` whitelists the mixer exit it leaves libusdr owing. RX receive drops
  burst padding stubs that libusdr converts into its host buffer
  (`stream_formats_parity.rs::a_padded_twelve_bit_packet_matches_libusdr_up_to_its_stub`),
  and `SyncMode::ExternalAll` fails at create, before any bus access, where libusdr
  creates and then fails the sync
  (`stream_formats_parity.rs::external_sync_is_refused_by_both_before_any_sync_operation`).
  Streams also refuse, before any bus access, a host format with no converter for the
  wire (libusdr byte-copies it), a second complex channel on gateware without the
  matching DSP chain (libusdr creates it while reporting one channel), a TX remap to
  another channel count, and a real 12-bit burst ending mid-byte (`stream_formats_parity.rs::a_complex_host_on_a_real_wire_is_refused_where_libusdr_copies_bytes`,
  `tests/stream.rs`, `tests/tx_stream.rs`).
  TX streams refuse, before any bus access, a packet size of 0 (libusdr's "largest
  buffer"), a `ci12` wire on the original core, which libusdr refuses only after powering
  the transmitter
  (`tx_stream_parity.rs::a_twelve_bit_tx_wire_is_refused_before_libusdr_would_power_up`),
  a packet over one burst on the extended core (libusdr splits it, unframed over USB),
  sends of no samples, more than a packet (libusdr splits them and times the later bursts
  of a "now" send) or unequal channel buffers, and times past 47 bits on the original core
  (63 on the extended) (`tests/tx_stream.rs`).
  A group with TX starts RX, then TX, then syncs, as libusdr's clients do, where an
  RX-only group syncs first, as before.
  The flash maintenance refuses, before any bus access, ranges libusdr would mishandle
  (past 16 MiB, partial erases, page-crossing chunks) and reports large capacities whole;
  `oracle/tests/flash_parity.rs` whitelists each. Two more cannot run in libusdr safely
  (its header-length underflow, its staging `assert`), so `tests/flash.rs` covers them.
  The gateware update refuses a corrupt image file when the `Image` is made, before any
  bus access, where libusdr's flash tool reads the board first
  (`flash_update_parity.rs::a_corrupted_image_is_refused_before_rsl_usdr_reads_the_flash`);
  reports a failed read-back as an error, where the tool exits 0; and refuses files past
  16 MiB, which the tool reads past its buffer (`tests/flash_update.rs`). The oracle runs
  the tool on a device that is not brought up or powered down, as `Flash` powers nothing.
  The XMASS switch firmware update refuses an image file longer than 64 KiB, which
  libusdr's `xmass_fwupd` cuts to its first 64 KiB and flashes
  (`xmass_firmware_parity.rs::a_long_file_is_cut_by_libusdr_and_refused_by_rsl_usdr`), and
  accepts a request only at its exact success value (0, or 1 for the read), where the tool
  takes any non-negative return as success
  (`xmass_firmware_parity.rs::a_read_returning_a_failure_code_passes_libusdr_and_fails_rsl_usdr`).
  The RX controls reject trim codes libusdr masks to seven bits (`RxTrim`;
  `rx_controls_parity.rs::oversized_trims_are_masked_by_libusdr_and_rejected_by_rsl_usdr`)
  and refuse, with a fixed low band, a frequency whose LO plus the mixer LO passes
  `u32::MAX`, where libusdr's double-to-`unsigned` conversion is undefined
  (`rx_controls_parity.rs::a_low_band_lo_past_u32_is_refused_where_libusdr_wraps_it`).
  The TX controls reject DC offsets libusdr masks to 12 bits (`TxDcOffset`;
  `tx_controls_parity.rs::oversized_tx_dc_offsets_are_masked_by_libusdr_and_rejected_by_rsl_usdr`).
  The raw chip debug write names the `TPS6381x` register asked for, where libusdr leaves
  its write flag in the address byte and writes `reg | 0x80`
  (`diagnostics_parity.rs::a_tps6381x_debug_write_reaches_the_register_libusdr_misaddresses`).
  Calibration refuses, before any bus access, a board with no sample rate (libusdr
  divides by zero) or with a stream group (`tests/calibration.rs`), and reports each IQ
  search's own result where libusdr's array repeats the LO search's; channel B,
  `USDR_CAL_RXLO` (a log line on the uSDR) and the coarse flags (zeroed by libusdr's
  own setup) are not offered.
  Tuning before any sample rate, where libusdr divides by zero, skips the filter and fails
  the low-LO fallback for both chains; the oracle cannot run that, so `tests/tx_tuning.rs`
  covers it on the sim alone.
  The oscillator discipline rejects counts more than 2% below the expected frequency, which
  libusdr's tool follows to a railed DAC
  (`gpsdo_parity.rs::a_count_far_below_is_rejected_where_the_tool_rails_the_dac`), clamps
  DAC codes at 0 where the tool wraps them to full scale
  (`gpsdo_parity.rs::the_dac_holds_at_zero_where_the_tool_wraps_it_to_full_scale`), and
  steps from the last confirmed code after a failed write, where the tool steps from the
  errno (`gpsdo_parity.rs::a_failed_dac_write_steps_from_the_last_code_where_the_tool_steps_from_the_errno`).
- Registers are typed (`rsl-lowlevel`'s traits, through `src/portable/register.rs`;
  `src/chips/register.rs` adds blocking I2C access). Each chip has a `Reg` enum naming every
  address it touches, so no raw address appears.
  - A register whose contents the driver interprets gets a value type: a `bnb` bitfield,
    or a `BitEnum` with a catch-all for whole-byte values. It implements `Register`, or
    `IndexedRegister` when the chip repeats it per channel or output.
  - Opaque bytes, such as undocumented registers and tuning constants, stay `(Reg, u8)`
    writes. A vendor register load spanning a chip's whole map (the LMK5C33216's
    `lmk5c33216_rom.h`) stays a raw `(address, value)` table transcribed from the vendored
    header, the one place raw addresses appear.
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
  hardware (`tests/pcie_hardware.rs` is `#[ignore]`d). Its `unsafe` is in `device.rs`
  (ioctls, mappings); the XMASS requests pair each opcode with the buffer the driver
  copies there (`device::xmass_requests`), and `xmass.rs` forbids `unsafe`. The driver
  module itself (`usdr_pcie_uram.c`) is GPL-2.0: copy nothing from it. Cite interface facts
  from the uAPI header (`pcie_uram_driver_if.h`, taken under MIT) or MIT userspace sources;
  a behavioural claim only the driver supports is phrased as observed interface behaviour
  and listed in PORTING.md's provenance note.

## Verify

```sh
cargo clippy -p rsl-usdr -p rsl-usdr-sim -p rsl-usdr-pcie --all-targets && cargo test -p rsl-usdr -p rsl-usdr-sim -p rsl-usdr-pcie
cargo clippy -p rsl-usdr --all-targets --features pcie && cargo test -p rsl-usdr --features pcie
cargo clippy -p rsl-usdr --all-targets --all-features -- -D warnings && cargo test -p rsl-usdr --all-features
cargo fmt --manifest-path devices/sdr/usdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/usdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/usdr/oracle/Cargo.toml
cargo clippy -p rsl-lowlevel -p rsl-lp8758 --all-targets -- -D warnings && cargo clippy -p rsl-lowlevel --features std --all-targets -- -D warnings
cargo test -p rsl-lowlevel --features std && cargo test -p rsl-lp8758
cargo check -p rsl-lowlevel -p rsl-lp8758 -p rsl-sdr-frontend --target thumbv7em-none-eabi
cargo clippy -p rsl-sdr-frontend --features std --all-targets -- -D warnings && cargo test -p rsl-sdr-frontend --features std
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

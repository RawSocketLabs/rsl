# LMS7 family (`m2_lm7_1`): XSDR, XTRX, sSDR, sSDR Pro

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`. The
> uSDR guide (`../usdr/AGENTS.md`) and ledger (`../usdr/PORTING.md`, row "LMS7-family
> board/RX/TX/rate controls") govern the port programme; this file holds the family's own
> facts.

A pure-Rust port of libusdr's LMS7 family, its board simulator and the libusdr reference
oracle. The driver covers XSDR and XTRX identification and supply bring-up (the XTRX's
two PMICs and reference-trim DAC), the XSDR's front-end probe at open with every board
libusdr brings up there, with or without an `fe=` hint, and those boards' controls
(`sdr-frontend`'s, shared with the uSDR and LSDr), RFIC power-up, sample rate (on gateware
with a TX MMCM, an RX or shared MMCM too, and a DUC/DDC), RX/TX LO tuning (FDD and TDD),
gains, baseband bandwidths, explicit paths with the RF loopback, the baseband NCOs, the
DC and I/Q corrections, the TX generator and mute, the baseband and digital loopbacks, the
RX and TX streams (on channel A, B or both, in libusdr's wire and host formats; the
original or extended TX core) with their sync modes and options, the TX MMCM, the TX phase calibration and its overrides,
the TX quality check, the link's debug controls, the board's readings, and close (also on
drop); see [PORTING.md](PORTING.md) for done, divergences and remaining. Boards: LMS7002M RFIC, LP8758 PMIC(s), and per variant an XTRX
reference-trim DAC (DAC80501 or MCP4725) or, on sSDR, an LMS8001 up/down-converter and AT24
secure EEPROM. Nothing here is hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `xsdr` (unpublished, root workspace member) | The driver, on `hal`, `lp8758`, `lms7002m`, `sdr-frontend` (`../frontend`, the front-end board probe, bring-ups and controls), `sdr-stream` (`../stream`, the stream engine), `sdr-cal` (`../cal`, the quality check's tone fit), `sdr-gearbox` (`../gearbox`, the DUC/DDC's FIR microcode) and `sdr-formats` (`../formats`, the streams' host conversions) |
| `sim/` | `xsdr-sim` (unpublished, root workspace member) | Board model at libusdr's `ls_op` seam: FPGA GPO/GPI registers, LMS7002M SPI register file, I2C chips, the front-end boards on the M.2 connector (`sdr-frontend-sim`, `../frontend/sim`, shared with `usdr-sim`), the RX stream engine, TX core and sync unit (`sdr-stream-sim`, `../stream/sim`, likewise shared), the TX PHY port with its MMCM and A/B-I/Q tables and the RX PHY capture, virtual clock, op trace |
| `oracle/` | `xsdr-oracle` (unpublished, own workspace: compiles C) | libusdr's `m2_lm7_1` driving `sim` through `oracle/shim/sim_plugin.c`, its RX and TX streams' buffers included, with libusdr's division traps caught (`set_uint_trapping`) |
| `nostd-check/` | `xsdr-nostd-check` (unpublished, own workspace) | Exact-source `no_std` compilation witness for `src/portable` (`#[path]` import), as `usdr`'s; `scripts/ci/portable.py xsdr` holds its dependencies to the driver's |
| `../chips/lms7002m/` | `lms7002m` (unpublished, root workspace member, `no_std`) | The LMS7002M's register values, `lms7002m_create` (`Create`), the `hw/lms7002m` operations on its cached state (`Lms7002m`, CGEN/SXX tuning machines) and `lms7002m_ctrl.c`'s control code (`ctrl`: `Control`, `Operation`, the board's `Frontend` hooks), shared with `limesdr` (`../limesdr`) |
| `../chips/lms7002m/sim/` | `lms7002m-sim` (unpublished, root workspace member) | The LMS7002M register file `sim` uses, shared with `limesdr-sim` (extracted 2026-10-12 when LimeSDR became its second consumer; the goldens did not change). Its assumptions below hold for every consumer |

## Layout decision (2026-10-12)

One crate set per family, so this family lives in `devices/sdr/xsdr/`. There is **one
vendored libusdr tree and one PIN**: `oracle/build.rs` compiles `devices/vendor/libusdr`
by relative path. Never copy it here or edit it; a PIN bump there moves this oracle too and
must re-bless the goldens below in the same change. The shim copies the uSDR shim's
pattern instead of sharing it; a shared harness is extracted later, once both are stable.

## Driver layout

Mirrors `usdr` (`../usdr/AGENTS.md`), at the size this slice needs:

- `src/device.rs`: `Device`, the public API: `open` and `open_with` (identification, then
  the variant's supplies), `power_up`, `set_sample_rate`, `set_rx_frequency`,
  `set_tx_frequency`, `set_gain`, `set_bandwidth` and `close` (also on drop), with the
  other RF and baseband controls (paths, TDD, NCOs, corrections, generator, loopbacks) in
  `src/device/controls.rs`, the front ends' controls and oscillator trim in
  `frontend.rs`, the phase overrides and the quality check on its own in `link.rs`, the
  streams' options in `stream_options.rs` and the debug controls and readings in
  `diagnostics.rs`, each one libusdr device-manager path. It owns the bus (`Box<dyn Bus>`), the LMS7002M
  control state (`lms7002m::ctrl::Control`, its chip cache zeroed until a power-up
  creates it) and the board's rate and LO state, and runs the portable sequences and
  control operations with `hal`'s blocking `run_sequence`, keeping the state an
  operation leaves even when it fails.
- `src/device/stream.rs`: the RX stream (`create_rx_stream`, `start_rx_stream`,
  `receive`, `stop_rx_stream`): `m2_lm7_1.c`'s create and `xsdr_prepare` composed from
  the control code, `SetSampleRate::calibrate_link` and the stream engine's sequences and
  burst arithmetic, which are `sdr-stream`'s (`../stream`, shared with the uSDR, whose
  gateware has the same engine at the same registers); which channel rides which lanes
  (`RxChannels`) and the RF side stay here. Blocking code over the bus, as the uSDR's
  `board/stream.rs`. `src/device/format.rs` holds the streams' formats: `WireFormat`,
  the sealed `HostSample` buffer elements (`[i16; 2]`, `[f32; 2]`, `[u8; 3]`, `i16`,
  `f32`) and the chunked conversion through a stack scratch, with libusdr's converters
  (`sdr-formats`, `../formats`, shared with the uSDR and LimeSDR).
- `src/device/tx_stream.rs`: the TX stream (`create_tx_stream`, `start_tx_stream`,
  `send`, `stop_tx_stream`), as the RX stream's, on the original or extended TX core
  (`sdr-stream`'s words); `src/device/quality.rs`: the TX quality check a rate runs while
  TX streams, host-side (its fit is `sdr-cal`'s floating-point arithmetic).
- `src/board.rs`: the XSDR's and XTRX's wiring and setpoints (`XSDR_SUPPLIES`,
  `XTRX_SUPPLIES`, `XSDR_LMS`, `XTRX_LMS`, `REFERENCE_HZ`, `PMIC`), each citing the
  libusdr call it reproduces.
- `src/portable/`: hand-written sans-I/O `Sequence` machines (`IdentityRead`, `Supplies`,
  the XTRX's `XtrxSupplies` in `xtrx`, `PowerUp`, `SetSampleRate` in `rate`, `Close`, the
  MMCM's `ConfigureMmcm` and `OverrideTxPhase` in `mmcm`, the TX phase calibration
  `TxCalibration` in `txcal` and the DUC/DDC's `UpdateFeDsp` in `gearbox`), the XSDR's
  control-code hooks
  (`frontend`: antenna switches, `LimeLight` maps, band tables), a one-action sequence
  (`step`) and the facts they need (`fpga`: GPO banks and bits, the HWID register, the RX
  PHY port, the TX PHY's registers, the TX MMCM power-down and delay). The LMS7002M's own operations and control
  code are `lms7002m`'s; how the family composes them (`xsdr_ctrl.c`) is here.
  Core only, no allocation; imports stay within the tree and the shared `no_std` crates
  (`nostd-check` compiles it unchanged).
- `src/trim_dac.rs`: the XTRX's reference-trim DAC (DAC80501 or MCP4725), blocking, as
  its 16-bit transfers do not fit the portable I2C actions.
- `src/fe_probe.rs`: the front-end board probe at the end of open, `sdr-frontend`'s
  with compatibility `m2a+e` and the open's hint (not the RFIC's own RF front end in
  `portable/frontend`). Every board the connector admits comes up whole there, as on the
  uSDR (the `PCIe` front end, XMASS, `SuperSync`, `SimpleSync`, the breakout); `Device`
  keeps the `FrontEnd` and reports its kind (`Device::front_end`). fe4ch4007200, whose
  bring-up is not ported, cannot answer: its chips sit on I2C core 1, which the XSDR's
  transports refuse. The boards' controls are `src/device/frontend.rs`'s.
- `src/lowlevel.rs`: `hal`'s `Bus` seam, re-exported. `src/error.rs`: `Error`.
- `tests/common/sim_bus.rs`: `Bus` over `xsdr-sim`; the oracle's parity tests include
  it with `#[path]`.

Rules: every ported sequence gets an op-for-op parity test in `oracle/tests/` against
libusdr on fresh boards; divergences are whitelisted per scenario there and listed in
`PORTING.md`. LP8758 values come from `lp8758`, LMS7002M values and chip operations
from `lms7002m`; uSDR-only code is not reached into.
Something the uSDR driver also needs is extracted to a shared crate, not copied, unless it is
small, and then `PORTING.md` says so.

## How libusdr selects the family and the variant

- **Family**: the transport hands `usdr_device_create` (`device/device.c`) a 16-byte
  device ID, matched against the registered factories; `M2_LM7_1_DEVICE_ID`
  (`device/device_ids.h`) selects `m2_lm7_1`. USB derives it from VID:PID
  (`lowlevel/libusb_vidpid_map.h`: `0xdea9:0xfaef`, `0x1011:0x3727`); PCIe from the
  kernel driver's `PCIE_DRIVER_GET_UUID` (upstream `pcie_uram_main.c`, not vendored). The
  shim hardcodes `M2_LM7_1_DEVICE_ID_C`.
- **Variant**: HWID, general-purpose input `IGPI_HWID` (FPGA register 19 = 16 + 12/4),
  bits 23:16, read once by the factory (`usdr_device_m2_lm7_1_create`, which gives sSDR
  variants a second I2C bus) and again by `xsdr_init`:

| HWID 23:16 | Variant | libusdr path | Init talks to | Sim |
|---|---|---|---|---|
| `0x30` | XSDR | `new_rev`: `_xsdr_init_revx`, then front-end probe on I2C bus 1 | PMIC 0:0x60 (rev 0xe001) | Full: open, power-up, close |
| `0x2e` | XTRX | `_xsdr_init_revo`; no front-end probe; power-up skips the LDO GPO | PMIC 1:0x60 then 0:0x60 (both 0xe001), DAC80501 0:0x4b (DEVID 0x0194/0x0195/0x2195) or MCP4725 0:0x62 | Full; revision 4 as an absent DAC80501 (`without_trim_dac`), its MCP4725 the idle bus |
| `0x31` | sSDR | `new_rev` + `ssdr`: also LMS8001 detection over the SPI mux, 5 tries | PMIC 0:0x60 (0xe001), LMS8001 (reg 0x000F = 0x4040) | Identified only: refused with `-EFAULT` (no LMS8001) |
| `0x33` | sSDR Pro | as sSDR, plus AT24 serial at 0x58 | PMIC 0:0x60 (0xe302) | Identified only, as sSDR |
| other | — | `-EINVAL`, unless `XSDR_FORCE` (then treated as XSDR-like `new_rev`) | nothing | Refusal tested |

HWID bits 15:8 are the gateware revision (sSDR: `0xff` selects the LMS8001 MPW2015/B
part, `0x00` sSDR rev 2); bits 7:0 are PHY features (`0x80` separate CLKDIV, `0x40` LML2
is RX, `0x20`/`0x10` TX/RX MMCM, `0x08` extended TX front end, `0x04` DUC/DDC, `0x01`
single MMCM). `xsdr_init` only stores them; they shape later rate and stream code.

## Operation classes and environment

Open, `/dm/power/en` power-up and close use only `USDR_LSOP_HWREG` (GPO at register 0,
GPI 16–19, front-end GPIO at 3, `REG_CFG_PHY_1` = 57 at close for the TX MMCM power-down),
`USDR_LSOP_SPI` (bus 0: LMS7002M; on sSDR the LMS8001 while GPO 15 bit 0 is set) and
`USDR_LSOP_I2C_DEV`. No DRP, URAM, GPI/GPO-class or custom operation occurs. DRP starts
with the LML clocking (`xsdr_configure_lml_mmcm_tx`, `xsdr_phy_tune_rx`): `m2_lm7_1`
proxies the transport's `ls_op` and serves TX-port DRP (port 1) itself as
`REG_CFG_PHY_1` writes plus busy-bit-9 polls; RX-port DRP (port 0) reaches the
transport, which the oracle refuses (`-EOPNOTSUPP`).

The oracle panics if any of these are set, since each changes the sequence:
`USDR_BARE_DEV`, `XSDR_FORCE`, `USDR_TX_W`, `USDR_TX_H`, `LMS8_MPW2015`, `SSDR_LMS8B`,
`USDR_FE_TYPE`, `USDR_DEBUG` (`xsdr_oracle::FORBIDDEN_ENV`). `USDR_LOGLEVEL` only
changes logging.

## What the sim models, and its assumptions

Each is stated again, with its evidence, where it is encoded.

- RX PHY port (`REG_CFG_PHY_0`, 56): reads return 0, an error-free `LimeLight` link, so
  libusdr's LFSR checkers count nothing (a sim choice; the DC estimator not modelled);
  `SimBoard::with_rx_phy_reads` scripts the counts for the calibration's error paths. A
  read after a capture-bank request returns the TX PHY's A/B-I/Q table, newest push
  first, as the loopbacks would bring the generator's tone back (a sim choice that
  makes libusdr's own tone fit, so its quality check passes at once;
  `SimBoard::with_reversed_capture` reads it backwards, fault injection).
- TX PHY port (`REG_CFG_PHY_1`, 57; `sim/src/tx_phy.rs`): the TX MMCM's DRP registers as
  plain storage behind `xsdr_override_drp`'s commands, read data in bits 31:16, never
  busy (bit 9), locked (bit 8) whenever the MMCM control last released reset and power
  down; unwritten MMCM registers read 0 (a sim choice: the bitstream's values are not in
  the source). Pushes fill each channel's 64-word A/B-I/Q table. Everything else is
  accepted with no effect. `SimBoard::with_tx_mmcm_fault` injects busy DRP commands, an
  MMCM that stops locking or one without a clock (fault injection, not a model).
- `SimBoard::with_lms_comparator_failure` forces the VCO comparators to "fail" at one cap
  code (fault injection, not a model).
- TX core: the original one, or with HWID PHY bit 0x08 (`with_hwid`) the extended one
  (`sdr-stream-sim`'s `TxEngine::extended`, the uSDR sim's model, one burst per packet).
- RX stream engine, TX core and sync unit: `sdr-stream-sim`'s models, the uSDR sim's,
  with their assumptions stated there (a counting pattern, channel B on lanes 2 and 3,
  blocks ready at once, no losses; TX packets played as soon as the core runs and the
  sync unit lets it, status words 0 unless set, the `PCIe` driver's 32 TX buffers); the
  gateware is the same `ipblks/streams` cores at the same registers (`m2_lm7_1.c`'s
  `/ll/srx/0`, `/ll/stx/0`, `/ll/rfe/0`, `/ll/sync/0`). The RX DMA status (register 6) is
  plain storage, 0: alive and idle. `USR_ACCESS2` (register 16) reads 0 unless
  `SimBoard::with_gateware_build` stamps it, which libusdr's TX create refuses as too old.
  The shim reports the streams' buffers as the `PCIe` transport does (32, at most 1 MiB
  each).
- HWID PHY bits: 0 by default, which libusdr's rate path treats as gateware without a TX
  MMCM (its calibration then fails); the rate tests use `with_hwid(0x0030_0020)`.
- FPGA registers: plain storage, unwritten reads 0. GPO writes latch `data & 0xff` per
  bank (`dev_gpo_set`). HWID is `device_id << 16`: gateware revision and PHY bits 0
  (not in the source; they do not change the XSDR/XTRX init trace). `USR_ACCESS2` reads 0.
- LMS7002M (`lms7002m-sim`): 15-bit address, 16-bit registers, bit 31 = write. **No
  reset values** (the vendored YAML/header document fields only), so unwritten registers
  read 0; version `0x002F` reads 0x3840 (VER 7, REV 1 as `lms7002m_create` requires; MASK
  0 is this board's choice, `LMS7_MASK` in `sim/src/board.rs`). VCO comparators answer from
  the 26 MHz internal reference (`LMS7_REFERENCE_HZ`; model in
  `../chips/lms7002m/sim/src/vco.rs`); never-written self-test status reads 0 (done). No
  MAC channel banking, no soft-reset side effects. Reads return 0 in bits 31:16. SPI
  answers regardless of the
  board's power GPOs, which do not reset the chip model.
- LP8758: byte registers with an auto-incrementing pointer; revision bytes per variant
  (`SimBoard::with_fpga_pmic_revision` overrides the FPGA-side one), all else 0.
  Power-good status 0 is harmless: `lp8758_check_pg(.., 0xf, ..)` accepts any status (the
  derived mask is always a subset of 0xf), so the C's wait ends at once.
- XTRX bus 1 PMIC answers only while GPO 0 bit 4 hands bus 1 to the FPGA (evidence: the
  "Take control of second I2C bus" / "Switch second I2C to gpio control" writes).
- DAC80501 (XTRX revision 5): 16-bit registers, MSB first; `DEVID` 0x0195 (assumed; the
  datasheet is not vendored); `SimBoard::without_trim_dac` removes it (revision 4, whose
  MCP4725 at 0x62 is not modelled: the idle bus).
- LMS8001 (sSDR): absent; while selected, SPI reads all ones. AT24: absent.
- Absent I2C devices read `0xff` per byte, as in `usdr-sim` (no NACK is reported by
  either transport). The XSDR front-end probe sees the idle bus as GPIO expanders, fails
  the `pciefe` probe at its DAC with `-EIO` (logged, ignored without a hint), finds no
  clock board and settles on the M.2-to-`PCIe` breakout without a DAC, which has no ID
  ("Detected external FE: exm2pe"); that characterises libusdr on an empty connector, not
  hardware, and is what the goldens hold.
- Front-end boards (`SimBoard::with_front_end`): `sdr-frontend-sim`'s models on I2C bus
  1 (`I2C_BUS_FRONTEND`, as on the uSDR), the same the uSDR's sim attaches, with their
  assumptions stated there (TCA6416/TCA9555 pair access and pulled-up inputs, DAC80501
  ID 0x0195, the LMK5C33216 register file, the LMK05318 whose VCO status reports lock at
  once or never, XMASS revision 1a's TMP114s and AT24 serial). The connector's GPIO core
  (register 3) stays plain storage. A lone breakout's DAC passes for the `PCIe` front
  end's on the idle bus, in libusdr and xsdr alike.
- I2C readback packing (first byte most significant, little-endian copy) and the
  three-out/four-in limits are transport facts copied from the uSDR oracle.
- Sleeps advance virtual time only (the shim replaces `usleep`).

## Characterization goldens

`oracle/tests/golden/{xsdr,xtrx}_session.trace`: the full session `usdr_dmd_create_string("")`,
`/dm/power/en` = 1, `usdr_dmd_close`, one op per line in `xsdr_sim::Op`'s `Display`
format, `== open` / `== power-up` / `== close` phase markers, and a provenance header
(PIN, variant, date). They are **characterization**, not specification. Coverage:

- XSDR, 122 ops: open 67 (HWID twice, LMS reset pulse, PMIC revision, three set-points,
  four rail enables, one power-good poll, internal clock select, then 50 front-end probe
  ops); power-up 20 (reset, LDO enable, reset release, the 10-word LMS7002M reset
  programme and version read); close 35 (RX/TX streaming down, `lms7002m_destroy`,
  power/LDO/LED off, TX MMCM power-down).
- XTRX, 81 ops: open 28 (both PMICs, DAC trim), power-up 18, close 35.

Not covered by the goldens: sample rate, tuning, gain, streams, calibration, temperature,
sSDR; `oracle/tests/rate_parity.rs` compares the ported ones with live libusdr instead.
`oracle/tests/identify.rs` covers every variant's classification and the unknown-ID
refusal; `oracle/tests/power.rs` asserts reset-before-SPI and the closed state. The driver
is compared with live libusdr, not with these files: `oracle/tests/open_parity.rs` covers
XSDR golden lines 10–76 (open, front-end probe included), 78–97 (power-up) and 99–133
(close), plus a close without power-up and one after a refused power-up (libusdr's zeroed
LMS7002M state). `oracle/tests/lms7002m_parity.rs` calls the compiled
`hw/lms7002m/lms7002m.c` directly (`xsdr_oracle::lms7002m_run`, a zeroed chip state on
the sim's SPI) and compares `lms7002m`'s words for every bank selection, RFE/TRF path
and mode, MIMO hand-over, AFE combination and processor enable, and every chip operation
the rate, tuning, gain and bandwidth paths use (whole traces, comparator reads included),
which no session reaches. `oracle/tests/frontend_parity.rs` opens and closes an XSDR
with each board attached (the `PCIe` front end, XMASS and its revision 1a, `SuperSync`,
`SimpleSync`, the breakout, which the automatic probe takes for a `PCIe` front end, so
the breakout's own bring-up with a DAC is compared on the uSDR only, through its `fe=`
hint), XMASS without its LMK05318, and the clock boards whose
LMK05318 never locks (repeated lock polls collapsed: libusdr's count follows its wall
clock), compared whole. `oracle/tests/rate_parity.rs` drives libusdr's `/dm/rate`,
`/dm/sdr/0/{rx,tx}/frequency`, gain and bandwidth paths and compares everything between
the open and the close; `oracle/tests/controls_parity.rs` does the same for the paths,
loopbacks, TDD, NCO, correction and generator paths (`Device`'s `controls`), and
`oracle/tests/stream_parity.rs` for the RX stream, from the open to the close inclusive,
with every packet's bytes; `oracle/tests/tx_stream_parity.rs` likewise for the TX
stream (every packet the TX core played, every send's statistics), the TX MMCM and the
TX phase calibration, scripted RX PHY reads driving its sweeps;
`oracle/tests/stream_formats_parity.rs` both streams in libusdr's other wire and host
formats, with the same comparisons; and
`oracle/tests/quality_parity.rs` for a rate while TX streams and the check on its own, up
to libusdr's division trap (whole on DUC/DDC gateware); `oracle/tests/frontend_controls_parity.rs`
for every `fe=` hint and front-end control; `oracle/tests/diagnostics_parity.rs` for the
debug controls and readings, values included. The XTRX's session is compared whole in
`open_parity.rs`, its rates, streams and readings in the files above.

## Verify

```sh
cargo fmt -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim -p sdr-stream -p sdr-stream-sim -p sdr-cal --check
cargo clippy -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim -p sdr-stream -p sdr-stream-sim -p sdr-cal --all-targets -- -D warnings
cargo test -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim -p sdr-stream -p sdr-stream-sim -p sdr-cal
cargo check -p lms7002m -p sdr-frontend -p sdr-stream --target thumbv7em-none-eabi
python3 scripts/ci/portable.py xsdr
cargo fmt --manifest-path devices/sdr/xsdr/nostd-check/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/xsdr/nostd-check/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/xsdr/nostd-check/Cargo.toml --locked
cargo check --manifest-path devices/sdr/xsdr/nostd-check/Cargo.toml --locked --target thumbv7em-none-eabi
cargo fmt --manifest-path devices/sdr/xsdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/xsdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/xsdr/oracle/Cargo.toml
# Re-bless the goldens (PIN bump or deliberate sim change only), then review the diff:
RSL_XSDR_BLESS=1 cargo test --manifest-path devices/sdr/xsdr/oracle/Cargo.toml --test init_golden
```

The oracle needs a C compiler only. libusdr is process-global, so oracles are serialised
by a lock: one is open at a time per process.

## Open items and blockers

- LMS7002M reset values and channel-banked register set: not needed by the ported paths
  (they read only the version, the VCO comparators and the self-test status); a sequence
  that reads back an unwritten register would need them. Source: datasheet or a vendored
  register map with defaults; neither is in the PIN subset.
- sSDR/sSDR Pro: an LMS8001 model (SPI mux, `lms8001_create`, ID 0x4040) and AT24.
- XTRX revision 4 (MCP4725 at 0x62) is not modelled.
- RX/shared-MMCM gateware drives the TX PHY port's one MMCM with RX as its master
  (`sim/src/tx_phy.rs` models it); only `phyrxlml` reaches the RX port's DRP, which the
  oracle refuses, and is not ported. Refusing the links whose MMCM configuration libusdr
  reads past `mmcm_rom` for, TX or RX master, is pending owner review (deferred;
  `PORTING.md`, divergences).
- CI runs the detached oracle as the `xsdr-oracle` profile and the fixture as
  `xsdr-portable` (host only); a change to `xsdr` or to the shared crates it builds on
  (`sdr-frontend` included) selects both, a change to the vendored C (`devices/vendor`)
  only the oracle (`scripts/ci/plan.py`). Their thumbv7em checks (fixture,
  `lms7002m`, `sdr-frontend`) join the
  release-only `no-std` profile on an explicit XSDR release, which no product enables yet.
  `PORTING.md` here is prose to the selector, like the uSDR ledger.

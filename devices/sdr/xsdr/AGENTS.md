# LMS7 family (`m2_lm7_1`): XSDR, XTRX, sSDR, sSDR Pro

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`. The
> uSDR guide (`../usdr/AGENTS.md`) and ledger (`../usdr/PORTING.md`, row "LMS7-family
> board/RX/TX/rate controls") govern the port programme; this file holds the family's own
> facts.

A pure-Rust port of libusdr's LMS7 family, its board simulator and the libusdr reference
oracle. The driver covers XSDR identification, supply bring-up and the front-end probe at
open (`sdr-frontend`'s, shared with the uSDR and LSDr), RFIC power-up, sample rate (on gateware with a TX MMCM), RX/TX LO tuning,
gains, baseband bandwidths and close; see [PORTING.md](PORTING.md) for done,
divergences and remaining. Boards: LMS7002M RFIC, LP8758 PMIC(s), and per variant an XTRX
reference-trim DAC (DAC80501 or MCP4725) or, on sSDR, an LMS8001 up/down-converter and AT24
secure EEPROM. Nothing here is hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `xsdr` (unpublished, root workspace member) | The driver, on `rsl-lowlevel`, `lp8758`, `lms7002m` and `sdr-frontend` (`../frontend`, the front-end board probe) |
| `sim/` | `xsdr-sim` (unpublished, root workspace member) | Board model at libusdr's `ls_op` seam: FPGA GPO/GPI registers, LMS7002M SPI register file, I2C chips, virtual clock, op trace |
| `oracle/` | `xsdr-oracle` (unpublished, own workspace: compiles C) | libusdr's `m2_lm7_1` driving `sim` through `oracle/shim/sim_plugin.c` |
| `nostd-check/` | `xsdr-nostd-check` (unpublished, own workspace) | Exact-source `no_std` compilation witness for `src/portable` (`#[path]` import), as `usdr`'s; `scripts/ci/portable.py xsdr` holds its dependencies to the driver's |
| `../chips/lms7002m/` | `lms7002m` (unpublished, root workspace member, `no_std`) | The LMS7002M's register values, `lms7002m_create` (`Create`), the `hw/lms7002m` operations on its cached state (`Lms7002m`, CGEN/SXX tuning machines) and `lms7002m_ctrl.c`'s control code (`ctrl`: `Control`, `Operation`, the board's `Frontend` hooks), shared with `limesdr` (`../limesdr`) |
| `../chips/lms7002m/sim/` | `lms7002m-sim` (unpublished, root workspace member) | The LMS7002M register file `sim` uses, shared with `limesdr-sim` (extracted 2026-10-12 when LimeSDR became its second consumer; the goldens did not change). Its assumptions below hold for every consumer |

## Layout decision (2026-10-12)

One crate set per family, so this family lives in `devices/sdr/xsdr/`. There is **one
vendored libusdr tree and one PIN**: `oracle/build.rs` compiles `../../usdr/oracle/libusdr`
by relative path. Never copy it here or edit it; a PIN bump there moves this oracle too and
must re-bless the goldens below in the same change. The shim copies the uSDR shim's
pattern instead of sharing it; a shared harness is extracted later, once both are stable.

## Driver layout

Mirrors `usdr` (`../usdr/AGENTS.md`), at the size this slice needs:

- `src/device.rs`: `Device`, the public API: `open` (identification, then the XSDR's
  supplies), `power_up`, `set_sample_rate`, `set_rx_frequency`, `set_tx_frequency`,
  `set_gain`, `set_bandwidth` and `close`. It owns the bus (`Box<dyn Bus>`), the LMS7002M
  control state (`lms7002m::ctrl::Control`, its chip cache zeroed until a power-up
  creates it) and the board's rate and LO state, and runs the portable sequences and
  control operations with `rsl-lowlevel`'s blocking `run_sequence`, keeping the state an
  operation leaves even when it fails.
- `src/board.rs`: the XSDR's wiring and setpoints (`XSDR_SUPPLIES`, `XSDR_LMS`,
  `REFERENCE_HZ`), each citing the libusdr call it reproduces.
- `src/portable/`: hand-written sans-I/O `Sequence` machines (`IdentityRead`, `Supplies`,
  `PowerUp`, `SetSampleRate` in `rate`, `Close`), the XSDR's control-code hooks
  (`frontend`: antenna switches, `LimeLight` maps, band tables), a one-action sequence
  (`step`) and the facts they need (`fpga`: GPO banks and bits, the HWID register, the RX
  PHY port, the TX MMCM power-down and delay). The LMS7002M's own operations and control
  code are `lms7002m`'s; how the family composes them (`xsdr_ctrl.c`) is here.
  Core only, no allocation; imports stay within the tree and the shared `no_std` crates
  (`nostd-check` compiles it unchanged).
- `src/fe_probe.rs`: the front-end board probe at the end of open, `sdr-frontend`'s
  with compatibility `m2a+e` and no hint (not the RFIC's own RF front end in
  `portable/frontend`). The `PCIe` front end and the breakout come up whole there
  (`Device::front_end`); XMASS, `SuperSync` and `SimpleSync` need `usdr`'s LMK
  bring-ups, so one that answers fails open with `Error::FrontEndNotPorted`.
- `src/lowlevel.rs`: `rsl-lowlevel`'s `Bus` seam, re-exported. `src/error.rs`: `Error`.
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
| `0x2e` | XTRX | `_xsdr_init_revo`; no front-end probe; power-up skips the LDO GPO | PMIC 1:0x60 then 0:0x60 (both 0xe001), DAC80501 0:0x4b (DEVID 0x0194/0x0195/0x2195) or MCP4725 0:0x62 | Full, revision 5 (DAC80501) only |
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
  libusdr's LFSR checkers count nothing (a sim choice; capture and DC estimator not
  modelled); `SimBoard::with_rx_phy_reads` scripts the counts for the calibration's
  error paths. The TX PHY port (57) is plain storage: no MMCM or DRP behind it.
  `SimBoard::with_lms_comparator_failure` forces the VCO comparators to "fail" at one cap
  code (fault injection, not a model).
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
  datasheet is not vendored).
- LMS8001 (sSDR): absent; while selected, SPI reads all ones. AT24: absent.
- Absent I2C devices read `0xff` per byte, as in `usdr-sim` (no NACK is reported by
  either transport). The XSDR front-end probe sees the idle bus as GPIO expanders, fails
  the `pciefe` probe at its DAC with `-EIO` (logged, ignored without a hint), finds no
  clock board and settles on the M.2-to-`PCIe` breakout without a DAC, which has no ID
  ("Detected external FE: exm2pe"); that characterises libusdr on an empty connector, not
  hardware. The sim models no front-end board, so only that path is compared on the
  XSDR; the boards' bring-ups are compared on the uSDR's sim (`../usdr/oracle`).
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
which no session reaches. `oracle/tests/rate_parity.rs` drives libusdr's `/dm/rate`,
`/dm/sdr/0/{rx,tx}/frequency`, gain and bandwidth paths and compares everything between
the open and the close.

## Verify

```sh
cargo fmt -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim --check
cargo clippy -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim --all-targets -- -D warnings
cargo test -p xsdr -p xsdr-sim -p lms7002m -p lms7002m-sim
cargo check -p lms7002m -p sdr-frontend --target thumbv7em-none-eabi
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
- The MMCM behind DRP (TX port served by `m2_lm7_1` as `REG_CFG_PHY_1` writes with busy
  bit 9 and lock bit 8; RX port via the DRP class) must be modelled before RX/shared-MMCM
  gateware or TX streaming. libusdr's RX MMCM path indexes the 64-entry `mmcm_rom` with
  feedback dividers up to 126 at low rates.
- CI runs the detached oracle as the `xsdr-oracle` profile and the fixture as
  `xsdr-portable` (host only); a change to `xsdr` or to the shared crates it builds on
  (`sdr-frontend` included) selects both, a change to the uSDR oracle's vendored C
  only the oracle (`scripts/ci/plan.py`). Their thumbv7em checks (fixture,
  `lms7002m`, `sdr-frontend`) join the
  release-only `no-std` profile on an explicit XSDR release, which no product enables yet.
  `PORTING.md` here is prose to the selector, like the uSDR ledger.

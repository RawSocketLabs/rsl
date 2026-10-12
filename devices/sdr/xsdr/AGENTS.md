# LMS7 family (`m2_lm7_1`): XSDR, XTRX, sSDR, sSDR Pro

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`. The
> uSDR guide (`../usdr/AGENTS.md`) and ledger (`../usdr/PORTING.md`, row "LMS7-family
> board/RX/TX/rate controls") govern the port programme; this file holds the family's own
> facts.

Groundwork for a pure-Rust port of libusdr's LMS7 family: the board simulator and the
libusdr reference oracle. The driver is **not started**. Boards: LMS7002M RFIC, LP8758
PMIC(s), and per variant an XTRX reference-trim DAC (DAC80501 or MCP4725) or, on sSDR,
an LMS8001 up/down-converter and AT24 secure EEPROM. Nothing here is hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-xsdr` (planned, not in tree) | The driver |
| `sim/` | `rsl-xsdr-sim` (unpublished, root workspace member) | Board model at libusdr's `ls_op` seam: FPGA GPO/GPI registers, LMS7002M SPI register file, I2C chips, virtual clock, op trace |
| `oracle/` | `rsl-xsdr-oracle` (unpublished, own workspace: compiles C) | libusdr's `m2_lm7_1` driving `sim` through `oracle/shim/sim_plugin.c` |

## Layout decision (2026-10-12)

One crate set per family, so this family lives in `devices/sdr/xsdr/`. There is **one
vendored libusdr tree and one PIN**: `oracle/build.rs` compiles `../../usdr/oracle/libusdr`
by relative path. Never copy it here or edit it; a PIN bump there moves this oracle too and
must re-bless the goldens below in the same change. The shim copies the uSDR shim's
pattern instead of sharing it; a shared harness is extracted later, once both are stable.

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
`USDR_FE_TYPE`, `USDR_DEBUG` (`rsl_xsdr_oracle::FORBIDDEN_ENV`). `USDR_LOGLEVEL` only
changes logging.

## What the sim models, and its assumptions

Each is stated again, with its evidence, where it is encoded.

- FPGA registers: plain storage, unwritten reads 0. GPO writes latch `data & 0xff` per
  bank (`dev_gpo_set`). HWID is `device_id << 16`: gateware revision and PHY bits 0
  (not in the source; they do not change the XSDR/XTRX init trace). `USR_ACCESS2` reads 0.
- LMS7002M: 15-bit address, 16-bit registers, bit 31 = write. **No reset values** (the
  vendored YAML/header document fields only), so unwritten registers read 0; version
  `0x002F` reads 0x3840 (VER 7, REV 1 as `lms7002m_create` requires; MASK 0 is a choice).
  No MAC channel banking, no soft-reset side effects. Reads return 0 in bits 31:16.
  SPI answers regardless of the board's power GPOs.
- LP8758: byte registers with an auto-incrementing pointer; revision bytes per variant,
  all else 0. Power-good status 0 is harmless: `lp8758_check_pg(.., 0xf, ..)` accepts any
  status (the derived mask is always a subset of 0xf), so the C's wait ends at once.
- XTRX bus 1 PMIC answers only while GPO 0 bit 4 hands bus 1 to the FPGA (evidence: the
  "Take control of second I2C bus" / "Switch second I2C to gpio control" writes).
- DAC80501 (XTRX revision 5): 16-bit registers, MSB first; `DEVID` 0x0195 (assumed; the
  datasheet is not vendored).
- LMS8001 (sSDR): absent; while selected, SPI reads all ones. AT24: absent.
- Absent I2C devices read `0xff` per byte, as in `rsl-usdr-sim` (no NACK is reported by
  either transport). The XSDR front-end probe sees the idle bus as GPIO expanders, fails
  the `pciefe` probe at its DAC with `-EIO` (logged, ignored without a hint) and reports
  no front end; that characterises libusdr on an empty connector, not hardware.
- I2C readback packing (first byte most significant, little-endian copy) and the
  three-out/four-in limits are transport facts copied from the uSDR oracle.
- Sleeps advance virtual time only (the shim replaces `usleep`).

## Characterization goldens

`oracle/tests/golden/{xsdr,xtrx}_session.trace`: the full session `usdr_dmd_create_string("")`,
`/dm/power/en` = 1, `usdr_dmd_close`, one op per line in `rsl_xsdr_sim::Op`'s `Display`
format, `== open` / `== power-up` / `== close` phase markers, and a provenance header
(PIN, variant, date). They are **characterization**, not specification. Coverage:

- XSDR, 122 ops: open 67 (HWID twice, LMS reset pulse, PMIC revision, three set-points,
  four rail enables, one power-good poll, internal clock select, then 50 front-end probe
  ops); power-up 20 (reset, LDO enable, reset release, the 10-word LMS7002M reset
  programme and version read); close 35 (RX/TX streaming down, `lms7002m_destroy`,
  power/LDO/LED off, TX MMCM power-down).
- XTRX, 81 ops: open 28 (both PMICs, DAC trim), power-up 18, close 35.

Not covered: sample rate, tuning, gain, streams, calibration, temperature, sSDR.
`oracle/tests/identify.rs` covers every variant's classification and the unknown-ID
refusal; `oracle/tests/power.rs` asserts reset-before-SPI and the closed state.

## Verify

```sh
cargo fmt -p rsl-xsdr-sim --check
cargo clippy -p rsl-xsdr-sim --all-targets -- -D warnings
cargo test -p rsl-xsdr-sim
cargo fmt --manifest-path devices/sdr/xsdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/xsdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/xsdr/oracle/Cargo.toml
# Re-bless the goldens (PIN bump or deliberate sim change only), then review the diff:
RSL_XSDR_BLESS=1 cargo test --manifest-path devices/sdr/xsdr/oracle/Cargo.toml --test init_golden
```

The oracle needs a C compiler only. libusdr is process-global, so oracles are serialised
by a lock: one is open at a time per process.

## Open items and blockers

- LMS7002M reset values and channel-banked register set: needed before any sequence that
  reads back unwritten registers (VCO trims at rate/tune). Source: datasheet or a vendored
  register map with defaults; neither is in the PIN subset.
- sSDR/sSDR Pro: an LMS8001 model (SPI mux, `lms8001_create`, ID 0x4040) and AT24.
- XTRX revision 4 (MCP4725 at 0x62) is not modelled.
- The MMCM behind DRP (TX via `REG_CFG_PHY_1`, RX via the DRP class) must be modelled
  before sample-rate or LML calibration work.
- The detached oracle is not registered with CI (`scripts/ci/plan.py` `DETACHED`,
  `.github/workflows/ci.yml`), so CI does not build or run it yet.

# LSDr family (`m2_da09_4_ad45_2`)

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`. The
> uSDR guide (`../usdr/AGENTS.md`) and ledger (`../usdr/PORTING.md`, row "LSDr
> ADC/clock/power/RX driver") govern the port programme; this file holds the family's own
> facts.

A pure-Rust port of libusdr's LSDr, its board simulator and the libusdr reference oracle.
The driver is a **first slice**: open up to libusdr's front-end probe, ADC power, the
receive PGA gain and close; see [PORTING.md](PORTING.md) for done, divergences and
remaining. The board is a **receive-only** ADC card: ADS42LBx9 dual-channel ADC(s), Si549
oscillator (ADC sample clock), TPS6594 PMIC, and on dual-ADC fits an LMK04832 clock
generator. libusdr declares no TX channel and no DAC (`max_hw_tx_chans` 0,
`dac38rf_present` never set); do not invent TX support. Nothing here is hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-lsdr` (unpublished, root workspace member) | The driver, on `rsl-lowlevel` and `bnb` |
| `sim/` | `rsl-lsdr-sim` (unpublished, root workspace member) | Board model at libusdr's `ls_op` seam: FPGA registers/GPO, ADS42LBx9 and LMK04832 over SPI, TPS6594 and Si549 over I2C, virtual clock, op trace |
| `oracle/` | `rsl-lsdr-oracle` (unpublished, own workspace: compiles C) | libusdr's `m2_da09_4_ad45_2` driving `sim` through `oracle/shim/sim_plugin.c` |
| `nostd-check/` | `rsl-lsdr-nostd-check` (unpublished, own workspace) | Exact-source `no_std` compilation witness for `src/portable` (`#[path]` import), as `rsl-xsdr`'s; `scripts/ci/portable.py lsdr` holds its dependencies to the driver's |

## Layout decision (2026-10-12)

One crate set per family, as for the LMS7 family (`../xsdr/`). There is **one vendored
libusdr tree and one PIN**: `oracle/build.rs` compiles `../../usdr/oracle/libusdr` by
relative path. Never copy it here or edit it; a PIN bump there moves this oracle too and
must re-bless the goldens below in the same change. The shim and `oracle/src/lib.rs` copy
the XSDR oracle's (itself the uSDR shim's pattern); a shared harness is extracted later. CI runs the oracle as the `lsdr-oracle`
profile (`scripts/ci/plan.py` `LSDR_ORACLE`), with an edge to the uSDR oracle so a
vendored C change selects it; a change to `rsl-lsdr` selects it too (its parity tests
depend on the driver), and the fixture as `lsdr-portable` (host only; its thumbv7em check
joins the release-only `no-std` profile on an explicit LSDr release, which no product
enables yet).

## Driver layout

Mirrors `rsl-xsdr` (`../xsdr/AGENTS.md`), at the size this slice needs:

- `src/device.rs`: `Device`, the public API: `open` (with `OpenOptions`, libusdr's `lmk=1`
  as `probe_clock_generator`), `fitted`, `set_power`, `set_rx_pga_gain` and `close`. It
  owns the bus (`Box<dyn Bus>`) and runs the portable sequences with `rsl-lowlevel`'s
  blocking `run_sequence`.
- `src/board.rs`: the LSDr's wiring and setpoints (`LSDR_WIRING`, `LSDR_SUPPLIES`), each
  citing the libusdr enum or call it reproduces.
- `src/portable/`: hand-written sans-I/O `Sequence` machines (`Supplies`, `Probes`,
  `AdcPower`, `SetPga`, and `Single` for the liveness read and each close write) and the
  facts they need: `fpga` (status word, GPO banks and bits) and one module per chip
  (`tps6594`, `si549`, `ads42lbx9`, `lmk04832` with the LMK04832 presence test
  `ClockGeneratorProbe`). Only the LSDr uses these chips, so their values stay here until
  a second driver needs them (`../../AGENTS.md`). Core only, no allocation (`bnb`'s
  bitfields need `alloc`); imports stay within the tree and the shared `no_std` crates
  (`nostd-check` compiles it unchanged).
- `src/lowlevel.rs`: `rsl-lowlevel`'s `Bus` seam, re-exported. `src/error.rs`: `Error`.
- `tests/common/sim_bus.rs`: `Bus` over `rsl-lsdr-sim`; `oracle/tests/session_parity.rs`
  includes it with `#[path]`.

Rules: every ported sequence gets an op-for-op parity test in `oracle/tests/` against
libusdr on fresh boards; divergences are whitelisted per scenario there and listed in
`PORTING.md`. uSDR-only code is not reached into; code the uSDR driver also needs (the
FIR gearbox, the front-end probe, the RX stream core) is extracted to a shared crate,
not copied (`PORTING.md`).

## How libusdr selects the family, and what it probes

- **Family**: the transport hands `usdr_device_create` (`device/device.c`) a 16-byte
  device ID. The LSDr factory registers `s_uuid_000` (commented "single ADC, no DAC") in
  `device/m2_lsdr/m2_da09_4_ad45_2.c`; unlike the other families it is **not in
  `device/device_ids.h`**, and it is `static`, so no `extern` can reach it: the shim
  restates the 16 bytes (`63 51 02 06 7f 31 44 2a a7 a1 6c 05 f9 c5 ad 48`), and a changed
  ID fails every open with `-ENOENT`. No USB VID:PID maps to it
  (`lowlevel/libusb_vidpid_map.h`), so at the PIN the LSDr is reachable only through a
  transport reporting the UUID itself, i.e. PCIe (`PCIE_DRIVER_GET_UUID`, upstream
  `pcie_uram_main.c`, not vendored). The register map (`M2PCI_*`) agrees.
- **Variants**: **no HWID decoding**. Open never reads a hardware ID; it reads register 0
  once ("Device is dead!" if all ones, `-EIO`) and then **probes**: ADC on SPI 0 and on
  SPI 1 (`ads42lbx9_reset_and_check`: `(REG_HF0 & 0xFE) == 0x6C`, else `-ENODEV`,
  tolerated), and an LMK04832 on SPI 2 **only with the `lmk=1` connection parameter**
  (`lmk04832_reset_and_test`, `-ENODEV` tolerated). The results (`adc_present`,
  `lmk04832_present`) are only logged at open; `lmk04832_present` later steers
  `/dm/rate/master`. The source names "dual ADC + `clk_gen`" boards with no registered ID.
  Other parameters: `decim` (stored, unused at open), `fe=` (front-end hint).

| Fit (`rsl_lsdr_sim::Variant`) | SPI 0 | SPI 1 | SPI 2 | Sim |
|---|---|---|---|---|
| `SingleAdc` (the registered board) | ADS42LBx9 | empty | empty | Open, power-up, close; golden |
| `DualAdcClockGen` | ADS42LBx9 | ADS42LBx9 | LMK04832 | Open (`""` and `lmk=1`), power-up, close; golden with `lmk=1` |

SPI 3 (DAC) and 4 (front-end connector) are empty on both.

## Operation classes and environment

Open, `/dm/power/en` and close use only `USDR_LSOP_HWREG` (register 0 read and GPO
writes; the front-end probe's GPIO core at 3 and SPI-ext config at 58),
`USDR_LSOP_SPI` (buses 0, 1, 2, and 4 for the probe) and `USDR_LSOP_I2C_DEV` (bus 0:
TPS6594 0x48, Si549 0x67; the probe also reads 0:0x4e, 0:0x22/0x23 and 1:0x22/0x23).
No DRP, URAM, GPI/GPO-class or custom operation occurs; the oracle **panics** on any
other class (resumed after libusdr returns), because the front-end probe would swallow a
refusal. The oracle panics if any of `USDR_BARE_DEV` (open returns before touching the
board), `USDR_FE_TYPE` (front-end hint) or `USDR_DEBUG` (debug socket) is set
(`rsl_lsdr_oracle::FORBIDDEN_ENV`); `USDR_LOGLEVEL` only changes logging.

## The sessions

- **Open**: reg 0 read; TPS6594 `DEV_REV`/`NVM_CODE_1`/`_2` read (logged, not checked);
  BUCK1/2 920 mV, BUCK3 2200 mV, BUCK4 1200 mV set-points; LDO4 (ADC supply) off; Si549
  output off; LDO4 on ("FIXUP"); [LMK reset, readback routing, ID reads]; ADC presence
  reads on SPI 0 and 1; LDO4 off; front-end probe (fe1005000 via XRA1405 on SPI 4, then
  fe4ch4007200 via TMP114/TCA6424 on I2C; the other front ends are compat-filtered for
  `lsdr`). An empty connector reads all ones, so no front end is found and open succeeds.
- **Power-up** (`/dm/power/en` = 1, `_lsdr_adc_activate(3)`): LDO4 on, 10 ms, GPO bank 2
  (`IGPO_BANK_CFG_ADC`) 0x03 (ADC reset), 0xf0, then 0x00. The Si549 output stays off:
  only `/dm/rate/master` (`si549_set_freq`) enables it.
- **Close**: Si549 output off, LDO4 off, then both ADCs' `REG_PWR_CONTROL` = 0xE0
  (channels and chip powered down). Both writes come **after** the ADC supply is cut, an
  ordering in which real hardware may lose them; the sim stores them.

## What the sim models, and its assumptions

Each is stated again, with its evidence, where it is encoded.

- FPGA registers: plain storage, unwritten reads 0 (not register 0's real status word;
  open only needs "not all ones"). GPO writes latch `data & 0xff` per bank
  (`dev_gpo_set`). `SimBoard::dead` makes register 0 read all ones.
- TPS6594 and Si549: 256 byte registers behind a pointer byte; multi-byte transfers
  auto-increment (assumption; libusdr moves one register per transfer). All power on as 0
  except Si549 register 0 (`DEVICE` `TYPE`) = 0x31, read-only (vendored YAML). TPS6594
  identification bytes read 0 (datasheet not vendored; open does not check them). Si549
  `CFG` self-clearing bits are not modelled (written only by `si549_set_freq`).
- ADS42LBx9: 16-bit frame in the SPI word's low half (bit 15 read, address 13:8, data
  7:0), readback in the low byte. `REG_HF0` powers on as 0x6C (the value the C's presence
  check accepts; the datasheet is not vendored), all else 0. SPI answers regardless of
  LDO4 (the C only probes while LDO4 is on; `oracle/tests/power.rs` asserts that order).
- LMK04832: 24-bit frame (bit 23 read, address 22:8, data 7:0), per `lmk04832.c`'s own
  `MAKE_LMK04832_RDREG`; the generated header and YAML put the read bit at 31 instead.
  Readback is driven only while `RESET_CFG.RESET_MUX` = 6 (SPI readback) with
  `RESET_TYPE` = 3 (push-pull output; the YAML: the mux "only applies if `RESET_TYPE` is
  set to an output mode"), which the C sets before reading; otherwise all ones. `RESET` bit 7 restores power-on and clears
  itself (YAML). ID bytes: `PROD_HI` 0x63, `PROD_LOW` 0xD1, `VNDR_HI` 0x51, `VNDR_LOW`
  0x04, chosen so the C's `PROD_LOW << 8 | PROD_HI` = 0xD163 and `VNDR` = 0x0451 hold.
  All other registers power on as 0.
- Empty SPI buses and the LMK's unrouted readback read all ones (floating MISO, a sim
  choice). Absent I2C devices read `0xff` per byte, as in `rsl-usdr-sim`; the sim answers
  on I2C bus 1 although the factory declares one I2C core (`DNLL_I2C_COUNT` 1): the
  front-end probe and the TMP108 (1:0x4b, `/dm/sensor/temp`) address it.
- SPI cores 0–4 exist (`DNLL_SPI_COUNT` 5); a higher bus is refused (`-EINVAL`). The
  32-bit word's framing into 16/24-bit chip frames is the sim's assumption: libusdr passes
  only the bus number as the `ls_op` address, and the gateware is not in the source.
- I2C readback packing and the three-out/four-in limits are transport facts copied from
  the uSDR oracle. Sleeps advance virtual time only (the shim replaces `usleep`).

## Characterization goldens

`oracle/tests/golden/{single_adc,dual_adc_clock_gen}_session.trace`: open, `/dm/power/en`
= 1, `usdr_dmd_close`, one op per line in `rsl_lsdr_sim::Op`'s `Display` format, `== open` /
`== power-up` / `== close` markers, and a provenance header (PIN, variant, connection,
date). They are **characterization**, not specification. Coverage:

- Single ADC, `""`: 57 ops: open 46 (liveness read, PMIC ID and four set-points, LDO4
  sequence, Si549 gate, two ADC probes, 29 front-end probe ops), power-up 7, close 4.
- Dual ADC + LMK04832, `"lmk=1"`: 64 ops: as above plus the 7 LMK ops (reset, readback
  routing, reset release, four ID reads), and the second ADC answering.

Not covered: `/dm/rate/master` (Si549 frequency, FIR gearbox, LMK layout and lock check,
ADC normal mode, LVDS PHY), RX gain/PGA and NCO, `/dm/stat` PHY tap sweep, temperature,
streams, front ends. `oracle/tests/identify.rs` covers family selection (`lsdr`, one RX
and zero TX channels), the LMK opt-in, an absent LMK, no front end and the dead link;
`oracle/tests/power.rs` covers supply/reset ordering and the closed state. The driver is
compared with live libusdr, not with these files: `oracle/tests/session_parity.rs` covers
single-ADC golden lines 10–26 (open up to the front-end probe), 57–63 (power-up) and
65–68 (close), dual-ADC `lmk=1` lines 10–33, 64–70 and 72–75, plus the other
clock-generator fits, power-down, PGA gains and the dead link, none of which the goldens
hold.

## Verify

```sh
cargo fmt -p rsl-lsdr -p rsl-lsdr-sim --check
cargo clippy -p rsl-lsdr -p rsl-lsdr-sim --all-targets -- -D warnings
cargo test -p rsl-lsdr -p rsl-lsdr-sim
python3 scripts/ci/portable.py lsdr
cargo fmt --manifest-path devices/sdr/lsdr/nostd-check/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/lsdr/nostd-check/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/lsdr/nostd-check/Cargo.toml --locked
cargo check --manifest-path devices/sdr/lsdr/nostd-check/Cargo.toml --locked --target thumbv7em-none-eabi
cargo fmt --manifest-path devices/sdr/lsdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/lsdr/oracle/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/lsdr/oracle/Cargo.toml --locked
# Re-bless the goldens (PIN bump or deliberate sim change only), then review the diff:
RSL_LSDR_BLESS=1 cargo test --manifest-path devices/sdr/lsdr/oracle/Cargo.toml --test init_golden
```

The oracle needs a C compiler only. libusdr is process-global, so oracles are serialised
by a lock: one is open at a time per process.

## Open items and blockers

- `rsl-lsdr` builds on `rsl-lowlevel` (`Bus`/`ls_op`, `I2cAddr`, `Sequence`); it must not
  depend on `rsl-usdr`. What it still needs from code `rsl-usdr` owns (FIR gearbox, front-end
  probe, RX stream core) is listed in `PORTING.md`.
- LMK04832 ID byte order: libusdr's `lmk04832_read16` assembles `R(addr+1) << 8 | R(addr)`,
  so it accepts `PROD_HI` = 0x63. If TI's datasheet (not vendored) gives `PROD_HI` = 0xD1,
  as the register names suggest, libusdr would report a real LMK04832 absent. Resolve with
  the datasheet or hardware; the sim and `rsl-lsdr`'s presence test
  (`src/portable/lmk04832.rs`) follow the C, so the driver inherits the question. Open only
  logs libusdr's result (`-ENODEV` is tolerated), so whether the C accepts the sim's ID bytes
  is observed only in its log (`LMK=1`); `rsl-lsdr` reports it in `Device::fitted`, and
  `/dm/rate/master` coverage (which branches on `lmk04832_present`) will observe the C's.
- Reset values for the TPS6594, ADS42LBx9 and LMK04832 are not vendored (datasheets or
  register maps with defaults are needed before any sequence reads back unwritten state).
- `/dm/rate/master` needs: the Si549 `CFG` self-clearing bits, the FIR gearbox GPO
  programming, LMK04832 layout/dports and its lock readback (`lmk04832_check_lock`, GPI
  bank 7 `IGPI_BANK_LMK_STAT`), and GPI clock/PHY status words (`/dm/stat`); none are
  modelled.
- Front ends (fe1005000: XRA1405 expander, LOs, attenuator on SPI 4) are not modelled;
  the goldens characterise an empty connector only.
- The real SPI framing of 16/24-bit frames and the PCIe UUID path need gateware/kernel
  evidence.

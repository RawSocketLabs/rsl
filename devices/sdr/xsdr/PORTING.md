# rsl-xsdr porting ledger

The LMS7 family (`m2_lm7_1`) row of the programme ledger (`../usdr/PORTING.md`, "LMS7-family
board/RX/TX/rate controls") in detail. Same contract: **Present** means source exists, test
names are definitions, not run results; nothing is hardware-qualified (U). Reference:
`../usdr/oracle/libusdr/PIN`; `C:` paths are relative to its `lib/`. `O:` = `oracle/tests/`,
`T:` = `tests/`, `P:` = `src/portable/`, `L:` = `../chips/lms7002m/src/` (`rsl-lms7002m`).

Golden-line numbers refer to `oracle/tests/golden/xsdr_session.trace` (characterization).

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| Identification: the factory's and `xsdr_init`'s HWID reads, member by bits 23:16 | `C:device/m2_lm7_1/m2_lm7_1.c::usdr_device_m2_lm7_1_create`, `xsdr_ctrl.c::xsdr_init` | `P:hwid.rs` (`IdentityRead`, `Variant`), `Device::open` | 10–11 | `O:open_parity.rs::an_xsdr_session_matches_libusdr_except_its_front_end_probe`, `an_unknown_device_id_is_refused_like_libusdr`; `P:hwid::tests` / U |
| XSDR supply bring-up at open: LMS power-control pulse, LP8758 revision, rails 1/2/3, all four converters in forced PWM, power-good poll, internal reference | `xsdr_ctrl.c::_xsdr_init_revx` (to its `IGPO_CLK_CFG` write), `C:hw/lp8758/lp8758.c` | `P:supplies.rs` (`Supplies`), `src/board.rs` (`XSDR_SUPPLIES`), `Device::open` | 12–26 | `O:open_parity.rs` (same test); `T:open.rs::opening_an_xsdr_turns_every_converter_on_and_leaves_the_rfic_unpowered`; `P:supplies::tests` / U |
| RFIC power-up: reset hold, external then internal LDOs, reset release, `lms7002m_create` reset programme (typed values) and version check | `xsdr_ctrl.c::xsdr_pwren`, `_xsdr_pwren_revx`; `C:hw/lms7002m/lms7002m.c::lms7002m_create` | `P:power.rs` (`PowerUp`), `L:create.rs` (`Create`, `ResetPlan`), `L:regs/` (value types), `src/board.rs` (`XSDR_LMS`), `Device::power_up` | 78–97 | `O:open_parity.rs` (same test); `T:open.rs::power_up_releases_the_lms7002m_reset_before_any_spi_and_keeps_its_afe_down`; `L:create::tests`, `P:power::tests` / U |
| Close: `xsdr_dtor` without streams or LMS8001: streaming down both directions (bank selection, RX/TX processors off, RX/TX front ends off, AFE off), `lms7002m_destroy`, LMS power control, external LDOs and LED off, TX MMCM power-down; from the chip's cached state, so also after no or a refused power-up | `xsdr_ctrl.c::xsdr_dtor`, `xsdr_rfic_streaming_down`, `_xsdr_mmcm_pd`; `C:device/m2_lm7_1/lms7002m_ctrl.c::lms7002m_streaming_down`; `C:hw/lms7002m/lms7002m.c` (`mac_set`, `xxtsp_enable`, `rfe_path`, `trf_path`, `afe_enable`, `destroy`) | `P:close.rs` (`Close`), `L:chip.rs` (`Lms7002m`, `Batch`), `P:fpga.rs` (`Gpo::Led`, `TX_MMCM_POWER_DOWN`), `Device::close` | 99–133 | `O:open_parity.rs` (same test), `closing_an_xsdr_never_powered_up_matches_libusdrs_zeroed_chip_state`, `an_unaccepted_pmic_is_refused_at_power_up_before_any_bus_access`; `T:open.rs::close_powers_the_rfic_down_and_holds_it_in_reset`; `P:close::tests`, `L:chip::tests` / U |
| `no_std` compile witness for `src/portable` | — | `nostd-check/` (`rsl-xsdr-nostd-check`, exact-source `#[path]` import), `scripts/ci/portable.py xsdr` | — | `nostd-check` tests; `cargo check --target thumbv7em-none-eabi` (release-only in CI) / U |

Each sequence is a hand-written sans-I/O machine over `rsl-lowlevel`'s `Sequence`, run by
its blocking `run_sequence`. The portable tree and `rsl-lms7002m` use core only and allocate
nothing (held by review; `bnb`, under the value types, requires `alloc`); the fixture and
`rsl-lms7002m`'s own `#![no_std]` hold the compilation.

The `Lms7002m` cache is the part of libusdr's `lms7002m_state_t` the ported operations read
(`reg_mac`, `reg_en_dir`, `rfe[i]` path/loopback/enable, `trf[i]` enable/loopback/gain/
`lbloss`). Not cached until an operation reads them: the RFE gains, the RBB state, the
TSP word caches (`reg_*tsp_*`), `reg_tbb_gc*`, `temp`. Operations update the cache before
their writes and keep the update when a write fails, as libusdr does.

## Divergences (whitelisted per scenario)

| Divergence | Why | Where whitelisted |
|---|---|---|
| The XSDR front-end probe (golden lines 27–76) is not run: `Device::open` ends after the internal-reference write | Not ported yet (remaining, below) | `O:open_parity.rs`: the cut starts at libusdr's first write to FPGA register 3 and must hold only register-3 writes, bus-1 I2C and sleeps |
| XTRX, sSDR and sSDR Pro are refused (`Error::UnsupportedVariant`) after the two HWID reads | Not ported yet | `O:open_parity.rs::unported_variants_stop_after_the_identification_libusdr_shares` |
| A PMIC revision other than 0xE001: power-up refused with `Error::ChipId` before any bus access; libusdr first holds the LMS7002M in reset, waits and reads the temperature sensor's ID for its log, then returns `-EIO` | Nothing it does there serves the refusal | `O:open_parity.rs::an_unaccepted_pmic_is_refused_at_power_up_before_any_bus_access` (open and close still match) |
| Before `lms7002m_create`, libusdr's zeroed state addresses SPI target 0; `Lms7002m::new` takes the board's target | Same bus on the XSDR (`SPI_LMS7` = 0); the zeroed `lsaddr` is an accident of `memset` | Not observable on the XSDR; `L:chip.rs` documents it |

Kept as libusdr, not divergences: the HWID is read twice (the second read is used); a wrong
PMIC revision does not fail the open; the power-good wait reads both status registers once
and accepts any status (`lp8758_check_pg(.., 0xf, ..)` cannot report not-good); the rails
get libusdr's codes, and the board constants name the voltage SNVSAC6B gives each code:
channel 2's 1280 mV request is code 0x86, 1.285 V (`rsl-lp8758` would give 0x85 for 1280),
and channel 3's 1850 mV request is code 0xB3, 1.84 V (20 mV steps, rounded down). At close:
streaming-down skips a failed write's batch without reporting it and reports only its AFE
write (`lms7002m_streaming_down` ignores the other results); `trf_path` sets `EN_DIR_TRF`
from the RX front ends' enables; `LB_LOSS_24` (24) reaches the 2-bit `L_LOOPB_TXPAD_TRF`
masked to 0; a close without power-up writes to the chip in reset, from the zeroed state.

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| XSDR front-end probe and bring-up (`m2a+e`, I2C bus 1) | `m2_lm7_1.c::usdr_device_m2_lm7_1_initialize` → `C:device/device_fe.c::device_fe_probe` | Shares the uSDR's probe logic (`rsl-usdr`'s `src/frontend/`, `board/frontend.rs`); extract it into `devices/sdr/` rather than copy it. Golden lines 27–76 |
| Close on drop | `usdr_dmd_close` | `Device` dropped without `close` leaves the board as it is; `rsl-usdr` powers down on drop and logs failures |
| XTRX (`_xsdr_init_revo`, DAC80501/MCP4725 trim, `_xsdr_pwren_revo`, LDO mask 0x01B10D15) | `xsdr_ctrl.c::_xsdr_init_revo` | `ResetPlan` already takes the mask; sim models revision 5 only (golden `xtrx_session.trace`) |
| sSDR / sSDR Pro (LMS8001 over the SPI mux, AT24; the close's LMS8001 power-down and 0.9 V rail) | `_xsdr_init_revx` sSDR branch, `xsdr_dtor`, `C:hw/lms8001` | Sim has no LMS8001 or AT24 |
| Real power-good check | `lp8758_check_pg` | Would diverge from libusdr, whose check is vacuous; needs a decision and status semantics from SNVSAC6B |
| Streams, and closing with streams | `m2_lm7_1.c::usdr_device_m2_lm7_1_destroy` (stream destroy), `lms7002m_streaming_up` | `rsl-lms7002m` has the RFE/TRF/TSP/AFE operations streaming-up also uses |
| LimeSDR reuse of `rsl-lms7002m` | `C:device/u3_limesdr/limesdr_ctrl.c` | `Create` and `mac_set` serve `limesdr_init`; the 0x3841 mask check stays in that driver |
| Rate, tuning, gain, paths, calibration, temperature, TDD | `xsdr_ctrl.c`, `lms7002m_ctrl.c`, `m2_lm7_1.c` | Not in the golden session (open, power-up, close); need the LMS7002M reset values and channel banking in `rsl-lms7002m-sim`, and the MMCM behind DRP (`AGENTS.md`, open items) |
| Hardware qualification | — | Not authorized |

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
| Close: `xsdr_dtor` without streams or LMS8001: streaming down both directions (bank selection, RX/TX processors off, RX/TX front ends off, AFE off), `lms7002m_destroy`, LMS power control, external LDOs and LED off, TX MMCM power-down; from the chip's cached state, so also after no or a refused power-up | `xsdr_ctrl.c::xsdr_dtor`, `xsdr_rfic_streaming_down`, `_xsdr_mmcm_pd`; `C:device/m2_lm7_1/lms7002m_ctrl.c::lms7002m_streaming_down`; `C:hw/lms7002m/lms7002m.c` (`mac_set`, `xxtsp_enable`, `rfe_path`, `trf_path`, `afe_enable`, `destroy`) | `P:close.rs` (`Close`), `L:chip.rs` (`Lms7002m`), `L:batch.rs` (`Batch`), `P:fpga.rs` (`Gpo::Led`, `TX_MMCM_POWER_DOWN`), `Device::close` | 99–133 | `O:open_parity.rs` (same test), `closing_an_xsdr_never_powered_up_matches_libusdrs_zeroed_chip_state`, `an_unaccepted_pmic_is_refused_at_power_up_before_any_bus_access`; `T:open.rs::close_powers_the_rfic_down_and_holds_it_in_reset`; `O:lms7002m_parity.rs` (every path, mode and bank selection of the chip operations); `P:close::tests`, `L:chip::tests` / U |
| LMS7002M chip operations for rate, tuning, gain and bandwidth: `LimeLight` configuration, maps, RX mode and resets; processor decimation, DC corrector, mixer NCO, reset and self-test (with its status read); RBB/TBB paths, filter codes (`_def` fits included) and gains; RFE/TRF gains; CGEN and SXX tuning, each a hand-written machine around one capacitor search that polls the comparators; CGEN/SXX disables | `C:hw/lms7002m/lms7002m.c` (`limelight_*`, `xxtsp_*`, `rxtsp_dc_corr`, `rbb_*`, `tbb_*`, `rfe_gain`, `trf_gain`, `cgen_tune`, `_lms7002m_vco_range`, `sxx_tune`, `*_disable`) | `L:chip/` (`lml`, `dsp`, `baseband`, `gain`, `synth`: `CgenTune`, `SxxTune`), `L:vco.rs`, `L:regs/` (`cgen`, `limelight`, `dsp`, `baseband`, SXX and RFE additions) | — | `O:lms7002m_parity.rs` (every call kind against the compiled C, whole traces including comparator and status reads and waits; CGEN across rates and SXX across the LO range each lock and fail); `L:` unit tests / U |
| LMS7002M control code: sample rate (CGEN candidates and retries, dividers, `LimeLight`, running channels' bandwidth and TX gains), gains, bandwidths, RX streaming up, `LimeLight` RX mode, FDD LO tuning with automatic bands | `C:device/m2_lm7_1/lms7002m_ctrl.c` (`lms7002m_samplerate`, `_set_gain`, `_bb_set_badwidth`, `_rbb/_tbb_bandwidth`, RX half of `_streaming_up`, `_set_lmlrx_mode`, `_fe_set_freq`, `_lms7002m_signal_event`, `_set_lna_rx/_tx`) | `L:ctrl/` (`Control`, `Operation`, `Frontend`), `P:frontend.rs` (`XsdrFrontend`, `RX_BANDS`, `TX_BANDS`) | — | `O:rate_parity.rs` (all of the below) / U |
| Sample rate: power-up if needed, AFE on (first time), the control code's rate, VIO/core boost above 70 MS/s, double pump above 72 MS/s, `LimeLight` RX calibration (RX streamed up, `LimeLight` reset, deserializer reset, phase sweep against the LFSR checkers with libusdr's retries and badness), bank A, self-test polls; on gateware with a TX MMCM; without one, the fixed delay and libusdr's `-ERANGE` | `xsdr_ctrl.c::xsdr_set_samplerate_ex`, `_xsdr_calibrate_lml` (RX path), `_xsdr_rxserdes_reset`, `xsdr_phy_en_lfsr_checker_mimo`, `xsdr_phy_lfsr_mimo_state`, `xsdr_set_vio`, `xsdr_set_lms125vdd`; `m2_lm7_1.c::dev_m2_lm7_1_rate_set`, `_rate_m_set` | `P:rate.rs` (`SetSampleRate`), `P:fpga.rs` (`RxPhyBank`, `TX_DEFAULT_DELAY`, switch banks), `Device::set_sample_rate` | — | `O:rate_parity.rs::each_rate_matches_libusdr_on_a_fresh_board`, `unequal_and_one_sided_rates_match_libusdr`, `a_rate_before_power_up_powers_the_rfic_up_first_like_libusdr`, `rates_out_of_range_are_refused_before_any_access_like_libusdr`, `gateware_without_a_tx_mmcm_fails_the_link_calibration_like_libusdr`, `rx_on_lml_port_2_matches_libusdr`; `T:rate.rs` / U |
| RX/TX LO tuning (FDD), gains (every `/dm/sdr/0/{rx,tx}/gain*` path) and baseband bandwidths, both channels | `xsdr_ctrl.c::xsdr_rfic_fe_set_freq` (non-sSDR), `xsdr_rfic_set_gain`, `xsdr_rfic_bb_set_badwidth`; `m2_lm7_1.c` handlers | `Device::set_rx_frequency`, `set_tx_frequency`, `set_gain`, `set_bandwidth` | — | `O:rate_parity.rs::a_session_of_rates_tunings_gains_and_bandwidths_matches_libusdr`; `T:rate.rs` / U |
| `no_std` compile witness for `src/portable` | — | `nostd-check/` (`rsl-xsdr-nostd-check`, exact-source `#[path]` import), `scripts/ci/portable.py xsdr` | — | `nostd-check` tests; `cargo check --target thumbv7em-none-eabi` (release-only in CI) / U |

Each sequence is a hand-written sans-I/O machine over `rsl-lowlevel`'s `Sequence`, run by
its blocking `run_sequence`. The portable tree and `rsl-lms7002m` use core only and allocate
nothing (held by review; `bnb`, under the value types, requires `alloc`); the fixture and
`rsl-lms7002m`'s own `#![no_std]` hold the compilation.

The `Lms7002m` cache is the part of libusdr's `lms7002m_state_t` the ported operations read
(`reg_mac`, `reg_en_dir`, `rfe[i]` path/loopback/enable and gain codes, `trf[i]`
enable/loopback/gain/`lbloss`, the processors' `dscpcfg` and `hbdo_iq` words,
`reg_tbb_gc*`). Not cached: the RBB state, `reg_*tsp_dscmode` (written, never read) and
`temp` (the SXX search's VCO selection, local to the tuning). Operations update the cache
before their writes and keep the update when a write fails, as libusdr does; chained
control-code calls are built one at a time, so a failure leaves later calls unbuilt.
`ctrl::Control` is libusdr's `lms7002_dev_t` for the ported operations; the XSDR's
`xsdr_dev_t` fields the rate path reads (`afe_active`, `dpump`, `s_rxrate`, `s_txrate`,
`lms7_*lo_last`) live in `Device`.

## Divergences (whitelisted per scenario)

| Divergence | Why | Where whitelisted |
|---|---|---|
| The XSDR front-end probe (golden lines 27–76) is not run: `Device::open` ends after the internal-reference write | Not ported yet (remaining, below) | `O:open_parity.rs`: the cut starts at libusdr's first write to FPGA register 3 and must hold only register-3 writes, bus-1 I2C and sleeps |
| XTRX, sSDR and sSDR Pro are refused (`Error::UnsupportedVariant`) after the two HWID reads | Not ported yet | `O:open_parity.rs::unported_variants_stop_after_the_identification_libusdr_shares` |
| A PMIC revision other than 0xE001: power-up refused with `Error::ChipId` before any bus access; libusdr first holds the LMS7002M in reset, waits and reads the temperature sensor's ID for its log, then returns `-EIO` | Nothing it does there serves the refusal | `O:open_parity.rs::an_unaccepted_pmic_is_refused_at_power_up_before_any_bus_access` (open and close still match) |
| Gateware with an RX MMCM, a shared MMCM or an FPGA DUC/DDC (HWID PHY bits 0x10, 0x01, 0x04): `set_sample_rate` refuses with `Error::UnsupportedGateware` before any access; libusdr reprograms the MMCM over DRP per swept phase or loads FPGA decimation filters | Not ported (remaining); the RX MMCM path also reads `mmcm_rom[clkfbdiv]`, a 64-entry table, with `clkfbdiv` up to 126 at low rates (out of bounds in libusdr) | `O:rate_parity.rs::gateware_rsl_xsdr_does_not_port_is_refused_before_any_access`; `T:rate.rs` |
| A bus failure during CGEN tuning ends the rate; libusdr retries the tuning and reports `-ERANGE`. Other chained control code (`lms7002m_bb_set_badwidth`'s second channel) likewise stops at the first bus failure | Report what failed | Not observable on the sim (no bus failures); documented on `Control::sample_rate` |
| Streaming up before any sample rate is refused (`ControlError::NoRate`); libusdr divides by zero | Defined behaviour | Unreachable through `Device` (only the rate path streams up) |
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
On the rate path: libusdr's LFSR sweep leaves the RX FIFO on the LFSR pattern when RX was
not running before the call (its `set_lmlrx_mode(NORMAL)` runs only for an RX already
running); the badness it compares is the counters' squares summed in 32-bit arithmetic,
unscaled, and only a badness over 100 fails the rate; `lms7002m_afe_enable`'s result is
ignored. In the chip code: the TX mixer bypass is set in the RX register layout (bit 7,
`ISINC_BYP`, not `CMIX_BYP`); the TBB amplifier code clamps an unsigned sum, so a negative
correction larger than the filter code gives 63, and a negative TBB resistor fit 255; the
`LimeLight` processor dividers write `div / 2 - 1` wrapped (0xFF for 1); `rfe_gain` picks
the first code *below* the asked attenuation; `set_gain` passes the TX pad `-10 * gain`,
so a TX gain of −20 dB stores loss code −20 masked to 5 bits; `sxx_tune` leaves the bank
selection on the synthesizer's bank when it fails; the CGEN disable's result is ignored.
The `/dm/sdr/0/rx/frequency` skip compares with the last LO *tuned*, so setting 0 first does
nothing.

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| XSDR front-end probe and bring-up (`m2a+e`, I2C bus 1) | `m2_lm7_1.c::usdr_device_m2_lm7_1_initialize` → `C:device/device_fe.c::device_fe_probe` | Shares the uSDR's probe logic (`rsl-usdr`'s `src/frontend/`, `board/frontend.rs`); extract it into `devices/sdr/` rather than copy it. Golden lines 27–76 |
| Close on drop | `usdr_dmd_close` | `Device` dropped without `close` leaves the board as it is; `rsl-usdr` powers down on drop and logs failures |
| XTRX (`_xsdr_init_revo`, DAC80501/MCP4725 trim, `_xsdr_pwren_revo`, LDO mask 0x01B10D15) | `xsdr_ctrl.c::_xsdr_init_revo` | `ResetPlan` already takes the mask; sim models revision 5 only (golden `xtrx_session.trace`) |
| sSDR / sSDR Pro (LMS8001 over the SPI mux, AT24; the close's LMS8001 power-down and 0.9 V rail) | `_xsdr_init_revx` sSDR branch, `xsdr_dtor`, `C:hw/lms8001` | Sim has no LMS8001 or AT24 |
| Real power-good check | `lp8758_check_pg` | Would diverge from libusdr, whose check is vacuous; needs a decision and status semantics from SNVSAC6B |
| Streams, and closing with streams | `m2_lm7_1.c::usdr_device_m2_lm7_1_destroy` (stream destroy), `xsdr_prepare`, TX half of `lms7002m_streaming_up` | The TX-running rate path follows from them: `_xsdr_calibrate_lml`'s TX phase calibration (DRP MMCM, IQ alignment), the 53–60 MS/s core-supply fix-up, `xsdr_check_rxtx_quality` (capture and float scoring) |
| RX/shared MMCM and DUC/DDC gateware | `xsdr_configure_lml_mmcm_tx`, `C:ipblks/xlnx_mmcm.c`, `_xsdr_update_fe_dsp`, `C:ipblks/fgearbox.c` | Needs a DRP/MMCM model in `rsl-xsdr-sim` (TX port over `REG_CFG_PHY_1` with busy and lock bits); decide the `mmcm_rom` out-of-bounds read at low rates (refuse or bound) |
| Explicit RX/TX paths, loopbacks, TDD tuning, baseband NCO frequencies, DC/IQ corrections, calibration, temperature | `lms7002m_ctrl.c` (`_rfe_set_path`, `_tfe_set_path`, `_bb_set_freq`, `_set_corr_param`), `xsdr_ctrl.c` (`xsdr_calibrate`, `xsdr_gettemp`), `m2_lm7_1.c` | Paths stay automatic; `Control` keeps the band tables they would index |
| LMS7002M reset values and channel banking in the sim | — | Not needed by the ported paths (they read only the version, the comparators and the self-test status); no vendored source gives them |
| Hardware qualification | — | Not authorized |

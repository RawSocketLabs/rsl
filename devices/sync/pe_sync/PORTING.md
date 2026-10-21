# pe-sync porting ledger

The PE_SYNC row of the programme ledger (`../../sdr/usdr/PORTING.md`, "PE_SYNC clock tree /
distribution / synchronization") in detail. Same contract: **Present** means source
exists, test names are definitions, not run results; nothing is hardware-qualified (U).
Reference: `devices/vendor/libusdr/PIN`; `C:` paths are relative to its `lib/`.
`O:` = `oracle/tests/`, `T:` = `tests/`, `P:` = `src/portable/`.

Golden-line numbers refer to `oracle/tests/golden/session.trace` (characterization).

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| Power-on: five supply GPO banks, three LED fades (libusdr's `float` brightness steps kept) | `C:device/pe_sync/pe_sync.c::pe_sync_power_on`, `leds_fader` | `P:power.rs` (`Power::on`), `P:fpga.rs` | 9–1216 | `O:open_parity.rs::a_pe_sync_session_matches_libusdr_operation_for_operation`; `P:power::tests` / U |
| Status check: status word, LP87524 OTP byte, LMX readbacks with MUXOUT readback, LMK1D1208I identities | `pe_sync.c::pe_sync_check_status` | `P:status.rs` | 1217–1226 | same; `O:open_parity.rs::a_silent_pmic_changes_nothing_but_the_byte_both_log`; `P:status::tests` / U |
| LMK05318B: identity, XO/APLL1/output plan, staged registers, soft reset, APLL1/APLL2 calibration and lock waits, lock-status read, output sync | `C:hw/lmk05318/lmk05318.c::lmk05318_create`, `_wait_apll1_lock`, `_wait_apll2_lock`, `_check_lock`, `_sync` | `src/lmk05318.rs` on `lmk05318` (`plan`, `staging`, `map`, value types); setpoints `P:board.rs` | 1227–1339 | same; `O:open_parity.rs::a_board_without_its_lmk05318_is_refused_and_powered_off_like_libusdr`; usdr's `O:lmk05318_parity.rs` for the plan / U |
| LMX2820 ×2: reset, temperature sensor, status; solver (reference doubler/multiplier/dividers, VCO core, fraction, channel dividers and muxes); tune registers; lock wait | `C:hw/lmx2820/lmx2820.c::lmx2820_create`, `lmx2820_solver`, `lmx2820_tune`, `lmx2820_wait_pll_lock`, `lmx2820_read_status` | `P:lmx2820/` (`Create`, `Plan`, `Tune`, `status`) | 1340–1417 | same; `O:chip_parity.rs::lmx2820_tunes_match_libusdr_across_references_orders_and_outputs`; `P:lmx2820::{plan,sequence}::tests` / U |
| LMX1214: setup and temperature; solver (buffer/divider, auxiliary pre-divider/divider/bypass); divider reset around the registers | `C:hw/lmx1214/lmx1214.c::lmx1214_create`, `lmx1214_solver`, `lmx1214_get_temperature` | `P:lmx1214/` (`create`, `Plan`, `configure`, `temperature`) | 1418–1440 | same; `O:chip_parity.rs::lmx1214_solutions_match_libusdr_across_modes_and_auxiliary_paths`; `P:lmx1214::*::tests` / U |
| LMX1204: setup, temperature and status; solver (buffer/divider/multiplier, state machine clock, logic clock, SYSREF delay/pre-divider/divider in all three modes); soft reset and registers; multiplier lock wait and state machine clock switch-off | `C:hw/lmx1204/lmx1204.c::lmx1204_create`, `lmx1204_solver`, `lmx1204_read_status`, `lmx1204_wait_pll_lock` | `P:lmx1204/` (`create`, `Plan`, `configure`, `status`, `WaitLock`) | 1441–1487 | same; `O:chip_parity.rs::lmx1204_solutions_and_lock_waits_match_libusdr_across_modes`; `P:lmx1204::*::tests` / U |
| LMK1D1208I ×2: identity check, output enables, swings, inputs and banks | `C:hw/lmk1d1208i/lmk1d1208i.c::lmk1d1208i_create` | `P:lmk1d1208i.rs` (`Create`) | 1488–1495 | same; `O:open_parity.rs::a_board_without_a_fan_out_buffer_is_refused_and_powered_off_like_libusdr`; `P:lmk1d1208i::tests` / U |
| Lock waits that poll again (slow lock) or time out (never lock), for each of APLL1, APLL2, both LMX2820s and the LMX1204 multiplier; the open's status reads and power-off after a timeout; a reopen after one | `lmk05318_wait_apll1_lock`, `_wait_apll2_lock`, `lmx2820_wait_pll_lock`, `lmx1204_wait_pll_lock`, `pe_sync_configure` | `src/lmk05318.rs` (`poll_lock`), `P:poll.rs` (`LockPoll`), `P:lmx1204/sequence.rs` (`WaitLock`), `Device::open` | — (the golden is the healthy board) | `O:open_parity.rs::a_pll_that_never_locks_times_out_and_powers_off_like_libusdr`, `a_slow_lock_repeats_its_polls_and_opens_like_libusdr`, `reopening_after_a_lock_timeout_matches_libusdr` (sim `Lock::{Never, AfterReads}`; libusdr's `clock_gettime` is the board's virtual clock, `oracle/build.rs`) / U |
| LMX2820 SYSREF: master or repeater (each SRREQ format), continuous or pulsed, delay steps through the JESD DAC table, the pre-divider and divider search, the registers; phase sync | `lmx2820_calculate_systef_chain`, `lmx2820_fill_jesd`, `lmx2820_encode_sysref_prediv`, `lmx2820_tune_internal`, `lmx2820_sync` | `P:lmx2820/plan.rs` (`SysrefRequest`, `Plan::solve`), `P:lmx2820/controls.rs` (`sync`) | — | `O:chip_parity.rs::lmx2820_sysref_chains_match_libusdr` / U |
| LMX2820 instant calibration: set-up at the VCO's lowest frequency with its wait, numerator and doubler, the calibration switched off after lock; instant retunes over the set-up's phase detector | `lmx2820_instant_calibration_init`, `lmx2820_solver_instcal`, `lmx2820_tune_instcal` | `P:lmx2820/plan/instant.rs` (`Plan::for_instant_calibration`, `Plan::retune`), `P:lmx2820/sequence.rs` (`Tune`), `P:lmx2820/controls.rs` (`InstantTune`) | — | `O:chip_parity.rs::lmx2820_instant_calibration_and_retunes_match_libusdr`; `P:lmx2820::controls::tests` / U |
| LMX1214 precision mode; SYNC windowing (arm, capture, disarm) | `lmx1214_solver(.., prec_mode, ..)`, `lmx1214_sysref_windowing_beforesync`, `_capture`, `_aftersync`, `C:hw/common/common.c::common_ti_calc_sync_delay` | `P:lmx1214/plan.rs`, `P:lmx1214/windowing.rs`, `P:ti_sync.rs` (`Capture`, `sync_delay`), `P:ti_spi.rs` (`Modify::pulse`) | — | `O:chip_parity.rs::lmx1214_solutions_match_libusdr_across_modes_and_auxiliary_paths`, `lmx1214_sync_windowing_matches_libusdr`; `P:ti_sync::tests` / U |
| LMX1204 precision mode; per-output SYSREF delays before the solver and reloaded after it; SYSREF windowing (arm, capture, disarm) | `lmx1204_solver(.., prec_mode, ..)`, `lmx1204_init_sysrefout_ch_delay`, `lmx1204_reload_sysrefout_ch_delay`, `lmx1204_sysref_windowing_*` | `P:lmx1204/plan.rs` (`Delay`, `Plan::with_delays`), `P:lmx1204/windowing.rs` | — | `O:chip_parity.rs::lmx1204_solutions_and_lock_waits_match_libusdr_across_modes`, `lmx1204_sysref_delays_and_windowing_match_libusdr`; `P:ti_sync::tests` / U |
| Power-off on a failed open; close | `pe_sync.c::usdr_device_pe_sync_initialize`, `pe_sync_power_off`, `usdr_device_pe_sync_destroy` | `Device::open`, `Device::close`, `P:power.rs` (`Power::off`) | 1496–1903 | `O:open_parity.rs` (all four tests); `T:open.rs` / U |

The four rows above are chip operations no libusdr device calls: `pe_sync.c` tunes with
SYSREF off, without instant calibration or precision mode, and has no control for
windowing, delays or phase sync. They are ported with chip-level parity (the C driver
functions called directly, `oracle/shim/sim_plugin.c`) and are not reachable from `Device`
(*pending owner review, 2026-10-20*: which of them `Device` should offer, and how; each needs
the chip plans the open makes, which `Device` does not keep yet).

Each register sequence is a hand-written sans-I/O machine over `hal`'s `Sequence`,
run by its blocking `run_sequence`; `nostd-check/` compiles the exact `src/portable` tree for
`thumbv7em-none-eabi` and runs its machines. The LMK05318's bus traffic is blocking code
(see below).

## Divergences (whitelisted per scenario)

None in the tested scenarios: every compared trace matches libusdr exactly. Behaviour that
differs where the oracle cannot run it:

| Divergence | Why | Where tested |
|---|---|---|
| Lock waits count their poll sleeps (LMK05318 100 µs × 1000, LMX2820 100 µs × 10 000, LMX1204 1 ms × 1000) where libusdr reads the wall clock | Deterministic; waits at least as long on hardware, where libusdr's budget also spends the bus time. On the sim the two agree poll for poll (the oracle's wall clock is virtual) | `O:open_parity.rs` (lock tests above), `P:poll::tests`, `P:lmx2820::sequence::tests::an_unlocked_pll_times_out_after_a_second_of_polls`, `P:lmx1204::sequence::tests::an_unlocked_multiplier_times_out_after_a_second_and_keeps_its_clock` |
| An LMX2820 output pair whose ratio is not an exact power of two is refused at once; libusdr refuses it later in the same solver (its 1e-8 `log2` tolerance admits near ratios its exactness checks then reject) | Same refusal, no bus access in either | `O:chip_parity.rs` (outcomes compared) |
| An LMX1204 SYSREF output of 0 Hz in generator modes is refused; libusdr divides by zero | Undefined in C | Not reachable from `PE_SYNC`'s setpoints; `Plan::solve` |
| An LMX2820 SYSREF output (SROUT) of 0 Hz is refused; libusdr divides by zero | Undefined in C | `Plan::solve`; not swept against libusdr |
| The windowing capture's delay treats the clock-position flags libusdr never writes as clear and ignores its writes past the 32-entry array; libusdr reads them uninitialised whenever it finds a clock edge | Undefined in C, so its success path is not compared | `P:ti_sync::tests`; `O:chip_parity.rs` compares the refused capture (the sim's LMX1214 positions) |
| A capture's refused positions end the sequence with `Capture::delay` empty, for the caller to fail; libusdr returns `-EINVAL` | The `Sequence` seam's failures name bus accesses | `O:chip_parity.rs::lmx1214_sync_windowing_matches_libusdr` |
| An invalid SRREQ format code, and LMX1204 delays wider than their fields, are unrepresentable (`RequestFormat`, `u2`/`u7`); libusdr refuses the one (`-EINVAL`) and masks the others | Typed requests | — |

Kept as libusdr, not divergences: the LMK1D1208I identity check refuses only a chip wrong in
both revision and device (`rev_id != 2 && dev_id != 0`); an LED fade's per-period writes are
not checked, only its last; status readbacks are read and discarded (libusdr logs them); the
LMX1204 solver's search starts with `found = true` and carries on past its last pre-divider;
LMX1204's lock-time state machine write ignores its result; the LMX1204's SYSREF request
input is LMX2820 #0's planned 25 MHz though that synthesizer's SYSREF stays off; LMX1204
precision mode refuses every multiplier plan but the 1:1 filter (libusdr compares CLKIN with
CLKOUT times the multiplier); LMX2820 instant calibration's set-up ignores the requested outputs (libusdr
tunes both to 5.65 GHz).

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| A transport (USB VID 0x3727 PID 0xE001, PCIe) | `C:lowlevel/usb_uram/`, `pcie_uram/` | PE_SYNC has two I2C cores; `usdr`'s transports refuse core ≠ 0 and know the uSDR's register layout. Shared transports belong outside `usdr` first |
| LMK05318 bus code shared with `sdr-frontend` | `C:hw/lmk05318/lmk05318.c` | Duplicated today (`src/lmk05318.rs` here, `src/chips/lmk05318.rs` in `sdr-frontend`, which runs it for the front ends of the uSDR and XSDR; the plan and registers are shared). Merge into `lmk05318` once `hal`'s seam carries 16-bit-pointer and multi-byte I2C transfers, or with an error mapping both drivers accept |
| LMK05318 DPLL enabled (reference validation, `REFVALSTAT`, the DPLL's ~90 registers, APLL1's fixed 40-bit denominator, the DPLL lock condition) | `lmk05318_dpll_config`, `lmk05318_set_common_registers`, `lmk05318_tune_apll1`, `lmk05318_wait_apll1_lock`, `lmk05318_wait_dpll_ref_stat` | `pe_sync.c` configures the DPLL disabled (`dpll.enabled = false; //true;`), and that path is ported and compared every session (no reference wait, the no-DPLL registers, the BAW-lock APLL1 wait). Enabling it changes `lmk05318`'s shared plan and staging (bit-exact against the uSDR and XSDR oracles too), needs the sim's reset values and reference validation, and no ported board enables it (libusdr's only user is `m2_dsdr`); not started (2026-10-20) |
| AD5662 DACs, external SPI | `pe_sync.c` (SPI buses 0, 1, 6, 7) | Nothing to port: the pinned `hw/ad5662/ad5662.c` holds no function (only includes) and nothing in libusdr touches SPI buses 0, 1, 6 or 7 of `PE_SYNC`. Porting waits on a libusdr driver or the parts' datasheets |
| One home for C's numeric helpers | — | `gcd`, `abs` and the C conversions (`c_f64`, `c_u64`, `c_unsigned`) exist three times: `P:float.rs`, `lmk05318`'s `plan.rs` and `usdr`'s `cal::search::gcd`. Consolidate once a shared `no_std` home is agreed |
| Hardware qualification | — | Not authorized |

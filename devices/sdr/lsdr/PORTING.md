# rsl-lsdr porting ledger

The LSDr (`m2_da09_4_ad45_2`) row of the programme ledger (`../usdr/PORTING.md`, "LSDr
ADC/clock/power/RX driver") in detail. Same contract: **Present** means source exists, test
names are definitions, not run results; nothing is hardware-qualified (U). Reference:
`../usdr/oracle/libusdr/PIN`; `C:` paths are relative to its `lib/`, and `C:lsdr` is
`C:device/m2_lsdr/m2_da09_4_ad45_2.c`. `O:` = `oracle/tests/`, `T:` = `tests/`, `P:` =
`src/portable/`.

Golden-line numbers refer to `oracle/tests/golden/single_adc_session.trace` (S) and
`dual_adc_clock_gen_session.trace` (D, `lmk=1`), both characterization.

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| Liveness: register 0 read once, all ones refused before anything is driven | `C:lsdr::usdr_device_m2_d09_4_ad45_2_initialize` (to "Device is dead!") | `P:single.rs` (`Single`), `P:fpga.rs` (`LIVENESS`, `DEAD`), `Device::open`, `Error::DeadLink` | S10, D10 | `O:session_parity.rs::the_golden_sessions_match_libusdr_except_its_front_end_probe`, `a_dead_link_is_refused_like_libusdr`; `T:open.rs::a_dead_link_is_refused_having_read_only_the_status_word` / U |
| Supplies at open: TPS6594 identification (read, not checked), bucks 1–4 at 920/920/2200/1200 mV, LDO4 (ADC supply) off, Si549 output gated, LDO4 on for the probes | same function, to its first "FIXUP" block; `C:hw/tps6594/tps6594.c` (`tps6594_check`, `_vout_set`, `_vout_ctrl`), `C:hw/si549/si549.c::si549_enable` | `P:supplies.rs` (`Supplies`), `P:tps6594.rs`, `P:si549.rs`, `src/board.rs` (`LSDR_SUPPLIES`) | S11–23, D11–23 | `O:session_parity.rs` (same test); `T:open.rs::open_sets_the_rails_finds_the_adc_and_leaves_it_unsupplied_and_unclocked`; `P:supplies::tests`, `P:tps6594::tests` / U |
| Probes: LMK04832 test on request (`lmk=1`), both ADC sockets' `REG_HF0`, LDO4 off; absences reported, not refused | same function, to its second "FIXUP" block; `C:hw/lmk04832/lmk04832.c::lmk04832_reset_and_test`, `C:hw/ads42lbx9/ads42lbx9.c::ads42lbx9_reset_and_check` | `P:probes.rs` (`Probes`, `Fitted`), `P:lmk04832.rs` (`ClockGeneratorProbe`), `P:ads42lbx9.rs`, `OpenOptions::probe_clock_generator`, `Device::fitted` | S24–26, D24–33 | `O:session_parity.rs` (same test), `the_clock_generator_test_follows_the_option_on_every_fit`; `T:open.rs::the_clock_generator_is_tested_only_when_asked_for`; `P:probes::tests`, `P:lmk04832::tests`, `P:ads42lbx9::tests` / U |
| Front-end probe at open (compat `lsdr`, no hint): the five M.2 boards filtered without a bus access; fe1005000's pins, then its XRA1405 check over the SPI extension (configuration register 58 before each exchange on SPI 4); fe4ch4007200's pins, TMP114 ID and four TCA6424A direction reads on I2C core 1; nothing found, open succeeds | `C:device/device_fe.c::device_fe_probe`; `C:device/ext_fe_100_5000/ext_fe_100_5000.c::ext_fe_100_5000_init` (to its GCR check), `C:device/ext_fe_ch4_400_7200/ext_fe_ch4_400_7200.c::ext_fe_ch4_400_7200_init` (to its ID check) | `../frontend` (`rsl-sdr-frontend`: `Connector::probe`, `fe1005000::Fe1005000Probe`, fe4ch4007200's identification), `src/frontend.rs` (`LsdrBringUp`) | S27–55, D34–62 | `O:session_parity.rs::the_golden_sessions_match_libusdr_front_end_probe_included` (and every other session, open compared whole, I2C cores included); `T:open.rs::open_ends_with_the_front_end_probe_on_an_empty_connector`; `rsl-sdr-frontend`'s `fe1005000::tests`, `probe::tests` / U |
| ADC power (`/dm/power/en`): LDO4 switched, both ADCs held in reset, FIFO reset, ADCs released (on) or held (off) | `C:lsdr::_lsdr_adc_activate`, `dev_m2_d09_4_ad45_2_pwren_set` | `P:adc_power.rs` (`AdcPower`), `Device::set_power` | S57–63, D64–70 (power-down: none) | `O:session_parity.rs` (same test), `power_down_and_pga_gains_match_libusdr`; `T:open.rs::power_supplies_and_releases_the_adcs_and_power_down_holds_them_unsupplied`; `P:adc_power::tests` / U |
| Receive PGA gain (`/dm/sdr/0/rx/gain/pga`; `/dm/sdr/0/rx/gain` with no front end): code `63 - 2 × gain`, latch pulsed | `C:lsdr::dev_m2_d09_4_ad45_2_pga` | `P:pga.rs` (`PgaGain`, `SetPga`), `Device::set_rx_pga_gain` | none | `O:session_parity.rs::power_down_and_pga_gains_match_libusdr`, `a_pga_setting_past_31_is_clamped_by_libusdr_and_unrepresentable_in_rsl_lsdr`; `T:open.rs::a_pga_gain_is_written_as_libusdrs_code_and_latched`; `P:pga::tests` / U |
| Close: Si549 output gated, LDO4 off, both ADC sockets' `REG_PWR_CONTROL` = 0xE0, each write attempted whatever the others did | `C:lsdr::usdr_device_m2_d09_4_ad45_2_destroy`; `ads42lbx9_ctrl` | `P:close.rs` (`Wiring::close`), `P:ads42lbx9.rs` (`PowerControl::OFF`), `Device::close` | S65–68, D72–75 | `O:session_parity.rs` (every test closes); `T:open.rs::close_gates_the_clock_cuts_the_supply_and_powers_both_sockets_down`, `close_attempts_every_write_and_reports_the_first_failure` / U |
| `no_std` compile witness for `src/portable` | — | `nostd-check/` (`rsl-lsdr-nostd-check`, exact-source `#[path]` import), `scripts/ci/portable.py lsdr` | — | `nostd-check` tests; `cargo check --target thumbv7em-none-eabi` (release-only in CI) / U |

Each sequence is a hand-written sans-I/O machine over `rsl-lowlevel`'s `Sequence`, run by
its blocking `run_sequence`. The portable tree uses core only and allocates nothing (held by
review; `bnb`, under the register value types, requires `alloc`); the fixture holds the
compilation. The four chips' register values live in `P:` (one module each) because only
the LSDr uses them; they move to `devices/sdr/chips/<chip>` crates when a second driver
needs them (`../../AGENTS.md`).

## Divergences (whitelisted per scenario)

| Divergence | Why | Where whitelisted |
|---|---|---|
| A front end that answers the probe (fe1005000's XRA1405 keeps its configuration, or fe4ch4007200's TMP114 identifies itself) fails open (`Error::FrontEndNotPorted`) where libusdr brings it up | Neither bring-up is ported | Not reachable in the oracle (the sim models no front end); `rsl-sdr-frontend`'s `fe1005000::tests::an_expander_that_keeps_its_configuration_is_found` and `probe::tests::a_stopping_driver_ends_the_automatic_probe_at_the_board_it_cannot_bring_up` |
| A PGA setting above 31: libusdr clamps it to 31; `PgaGain::new` refuses it, so `Device` cannot be asked for it | A typed setting over a silent clamp | `O:session_parity.rs::a_pga_setting_past_31_is_clamped_by_libusdr_and_unrepresentable_in_rsl_lsdr` |
| A failed liveness read fails open (`Error::Bus`); libusdr ignores the read's result and carries on with its `0xdeaddead` placeholder | A failed first access is not a live board | Not reachable in the oracle (the sim injects no bus faults): `T:open.rs::a_failed_liveness_read_fails_open_before_anything_else` (a fault-injecting bus over the sim) |
| A failed close write is reported (`Device::close` returns the first failure after attempting all four); libusdr's destroy returns nothing | Same writes, same order; only the report differs | Not reachable in the oracle: `T:open.rs::close_attempts_every_write_and_reports_the_first_failure` (both writes after the failures reach the board; the first failure is returned) |
| A failed exchange in the LMK04832 test or an ADC probe fails open (`Error::Bus`); libusdr tolerates any `-ENODEV` there, so a transport that reported one would read as an absent chip | `BusError` carries no errno, and a transport failure is not an absent chip | Not reachable in the oracle (the sim reports no errno); `P:probes::tests::a_failed_adc_read_ends_open_before_the_supply_is_cut`, `P:lmk04832::tests::a_failed_exchange_ends_the_test_absent` |

Kept as libusdr, not divergences: the TPS6594 identification is read and not checked; the
LDO4 enable writes 0x33, including bit 1, which libusdr names only for the bucks
(`BUCKX_FPWM`); both ADC power-down writes at close come after their supply is cut; open
leaves the Si549 gated and the ADCs unsupplied; the LMK04832 ID bytes are assembled as
libusdr does (`PROD_LOW << 8 | PROD_HI`, an open question in `AGENTS.md`).

## Remaining

A board the automatic probe skips (libusdr logs `Unable to initialize ...` at ERROR) is
skipped silently here: the driver has no logging dependency yet (`BringUp::skipped` keeps
its default).

| Capability | Pinned C | Blocker or note |
|---|---|---|
| Front-end bring-ups and controls: fe1005000 (`tune_rf_path`: band switches, MAX2871 LOs, attenuator; `/dm/sdr/0/fe/*`, gain/VGA/LNA with a front end, frequency with fe1005000), fe4ch4007200 (expander test vectors and set-up); `fe=` hints | `C:device/ext_fe_100_5000`, `C:device/ext_fe_ch4_400_7200`; `C:device/device_fe.c` | Their identification is `rsl-sdr-frontend`'s; the bring-ups go there too (fe1005000 is LSDr-only, fe4ch4007200 shared with sSDR/dSDR). A hint that finds nothing must fail the open (libusdr's `-ENODEV`). Needs front-end models in `rsl-lsdr-sim`, and an answer to the I2C core 1 question (`AGENTS.md`) |
| Sample rate (`/dm/rate/master`, `/dm/rate/rxtxadcdac`) | `C:lsdr::dev_m2_d09_4_ad45_2_rate_set`; `C:hw/si549/si549.c::si549_set_freq`; `C:ipblks/fgearbox.c::fgearbox_load_fir`; `C:hw/lmk04832/lmk04832.c` (`configure_layout`, `configure_dports`, `check_lock`); `ads42lbx9_set_normal` | Shared need: the FIR gearbox's microcode tables (`ipblks/fgearbox.c`, 1280 GPO writes per load) are already transcribed, privately, in `rsl-usdr` (`../usdr/src/fpga/fir_tables.rs`, `fpga/phy.rs`); extract them to a shared crate, do not copy. The rest is LSDr's: the Si549 frequency plan, ADC normal mode, DSP mix and LVDS PHY step, and on dual fits the LMK04832 layout and lock check (GPI word 17, `IGPI_BANK_LMK_STAT`). A scratch oracle run (2026-10-12, not kept) completed it on both fits without sim changes, but the Si549 `CFG` self-clearing bits and the lock status are unmodelled (`AGENTS.md`) |
| NCO frequency (`/dm/sdr/0/rx/frequency` without a front end) | `C:lsdr::set_nco` | Needs the ADC rate the sample-rate setting stores; libusdr writes the channel mask and then fails with `-EINVAL` before one is set |
| RX streaming | `C:lsdr::usdr_device_m2_d09_4_ad45_2_create_stream`; `C:ipblks/streams/{sfe_rx_4,stream_sfetrx4_dma32}.c` (`CORE_SFERX_DMA32_R0`) | Shared need: the same FPGA RX stream core (`sfetrx4`) `rsl-usdr` ports in `src/fpga/stream.rs` and `board/stream.rs`, uSDR-specific today. Extract it into `devices/sdr/` (with the stream-handle API) rather than copy; not restructured here. The LSDr's register map (`M2PCI_*`, `CSR_RFE4_BASE`, `VIRT_CFG_SFX_BASE`) and channel map (`lsdr_map_channels`) are its own |
| Transport | — | libusdr reaches the LSDr only through a transport reporting its UUID (PCIe, `PCIE_DRIVER_GET_UUID`, not vendored); `rsl-usdr-pcie` carries the uSDR's layout. A PCIe `Bus` for the LSDr needs its layout and is a shared-transport question |
| Temperature (`/dm/sensor/temp`), `/dm/stat` PHY sweep, `/debug/clk_info`, debug paths | `C:lsdr::dev_m2_d09_4_ad45_2_senstemp_get` (TMP108 at 1:0x4b), `_stat`, `_get_clk` | TMP108 not modelled; `/dm/stat` and the clock readout need GPI status words the sim does not model |
| Chip crates | `C:hw/{tps6594,si549,ads42lbx9,lmk04832}` | Move `P:{tps6594,si549,ads42lbx9,lmk04832}.rs` to `devices/sdr/chips/` when a second driver needs one; the LMK04832's 15-bit addresses then need chip-local register traits |
| Hardware qualification | — | Not authorized |

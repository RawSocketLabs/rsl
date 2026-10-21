# limesdr porting ledger

The LimeSDR family (`u3_limesdr`) rows of the programme ledger (`../usdr/PORTING.md`,
"LimeSDR board / framing / streams" and "FT601 USB transport") in detail. Same contract:
**Present** means source exists, test names are definitions, not run results; nothing is
hardware-qualified (U). Reference: `devices/vendor/libusdr/PIN`; `C:` paths are relative to
its `lib/`. `O:` = `oracle/tests/`, `T:` = `tests/`, `P:` = `src/portable/`, `L:` =
`../chips/lms7002m/src/` (`lms7002m`).

Golden-line numbers refer to `oracle/tests/golden/limesdr_mini_session.trace`
(characterization): lines 10–71 are the open, 72 and 73 the empty power-up and close.

## Done

| Capability | Pinned C | Rust owner | Golden lines | Evidence |
|---|---|---|---|---|
| LMS64C framing: single-frame register writes and reads for the gateware and the LMS7002M, the LMS7002M reset pulse, reply values | `C:ipblks/lms64c_proto.c::lms64c_fill_packet`, `lms64c_parse_packet`; `lms64c_cmds.h`, `lms64c_proto.h` | `P:lms64c.rs` (public `limesdr::lms64c`) | every `req`/`rep` line | `O:lms64c_parity.rs` (0 to 15 records, both register files, reset, 14 reply values, against the compiled C); `P:lms64c::tests` / U |
| `ls_op` → LMS64C mapping (gateware 16-bit write/read, the 32-bit count read over two registers, LMS7002M SPI by write flag, custom command 0 as the reset pulse; a status other than completed, or a reply to another command, fails) | Oracle shim `oracle/shim/sim_plugin.c` (assumption: the FT601 transport is not vendored) | `P:lsop.rs` (`LsOp`), `src/device.rs` (one exchange per operation) | every `req`/`rep` line | `O:open_parity.rs` (all scenarios); `P:lsop::tests`; `T:open.rs::a_failed_exchange_names_the_register_it_struck` / U |
| Open: reference-clock measurement (start, polls 50 ms apart, count), LMS7002M reset pulse, the exact 0x3841 version gate, `lms7002m_create` (typed reset programme and version check), stream-control stop | `C:device/u3_limesdr/limesdr_ctrl.c::limesdr_init`, `limesdr_detect_refclk`, `limesdr_stop_streaming`; `C:hw/lms7002m/lms7002m.c::lms7002m_create` | `P:open.rs` (`Open`), `P:reference.rs`, `P:fpga.rs`, `L:create.rs` (`Create`), `L:spi.rs` (`spi_read`), `Device::open` | 10–71 | `O:open_parity.rs::a_fresh_session_matches_libusdr`, `a_detector_taking_its_gate_time_is_polled_and_waited_for_like_libusdr`, `an_lms7002m_other_than_mask_1_is_refused_like_libusdr`, `references_libusdr_decides_reliably_are_decided_alike`; `T:open.rs`; `P:open::tests`, `P:reference::tests` / U |
| Power-up (`/dm/power/en`) and close: no bus operation | `C:device/u3_limesdr/u3_limesdr.c::dev_limesdr_pwren_set`, `limesdr_device_destroy` → `limesdr_ctrl.c::limesdr_dtor` ("TODO") | `Device::power_up`, `Device::close` | 72–73 | `O:open_parity.rs::a_fresh_session_matches_libusdr`; `T:open.rs::power_up_and_close_touch_nothing` / U |
| Identity: the emulated `/dm/revision` | `u3_limesdr.c::dev_limesdr_sdr_revision_get` | `Device::revision` | — | `O:open_parity.rs::the_revision_is_libusdrs_emulated_constant` / U |
| Control state kept from the open: the created chip, the measured reference as the chip's clock (`lms7002m_init`), the board's band tables (`cfg_auto_rx`, `cfg_auto_tx`) | `limesdr_ctrl.c::limesdr_init`, `limesdr_ctor`; `C:device/m2_lm7_1/lms7002m_ctrl.c::lms7002m_init` | `P:open.rs` (`Open::control`), `P:frontend.rs` (`RX_BANDS`, `TX_BANDS`), `Device` | — | `O:control_parity.rs` (every scenario starts from it) / U |
| Sample rate (`/dm/rate/master`, `/dm/rate/rxtxadcdac` without converter floors): range checks, `lms7002m_samplerate` with `XSDR_SR_MAXCONVRATE` and SISO DDR both ways, RX on `LimeLight` port 2, then direct clocking 0 and 1 (read-modify-writes of 0x0005) | `u3_limesdr.c::dev_limesdr_rate_set`, `_rate_m_set`; `limesdr_ctrl.c::limesdr_set_samplerate`, `limesdr_set_direct_clocking`; `lms7002m_ctrl.c::lms7002m_samplerate` | `P:rate.rs` (`SetSampleRate`), `P:modify.rs` (`Modify`), `P:fpga.rs` (`DIRECT_CLOCKING`), `L:ctrl/` (`Control::sample_rate`), `Device::set_sample_rate` | — | `O:control_parity.rs::each_rate_matches_libusdr_on_a_fresh_board` (90 kS/s to 122.88 MS/s, all lock), `unequal_and_one_sided_rates_match_libusdr`, `rates_out_of_range_are_refused_before_any_access_like_libusdr`, `a_failing_vco_comparator_fails_tuning_and_the_rate_like_libusdr`; `T:control.rs::rates_out_of_range_are_refused_before_any_exchange`; `P:modify::tests` / U |
| RX/TX LO tuning (FDD, every call, 0 stops the synthesizer), gains (every `/dm/sdr/0/{rx,tx}/gain*` path) and baseband bandwidths, both channels; once running, the band and antenna switch follow each LO (0x0017 bits 9:8 RX, 13:12 TX, read-modify-write) | `u3_limesdr.c` handlers; `limesdr_ctrl.c::_limesdr_antenna_port_switch`, `_limesdr_lml_portcfg`; `lms7002m_ctrl.c::lms7002m_fe_set_freq`, `_set_gain`, `_bb_set_badwidth`, `_lms7002m_signal_event` | `P:frontend.rs` (`LimeFrontend`), `P:fpga.rs` (`RF_SWITCHES`), `Device::set_rx_frequency`, `set_tx_frequency`, `set_gain`, `set_bandwidth` | — | `O:control_parity.rs::a_session_of_rates_tunings_gains_and_bandwidths_matches_libusdr`, `bands_and_antenna_switches_follow_the_los_once_streaming_like_libusdr` (every band edge both ways, other bits of 0x0017 kept); `T:control.rs::the_antenna_switches_move_only_once_streaming_is_prepared`. Gains are libusdr's six paths (`Gain`, no TX PGA) / U |
| Stream preparation: the 1 MS/s default rate when none is set (its failure ignored), both directions' run flags, channel A streamed up RX and TX (`lms7002m_streaming_up`, TX half added to `lms7002m`) | `u3_limesdr.c::dev_limesdr_initialize`; `limesdr_ctrl.c::limesdr_prepare_streaming`; `lms7002m_ctrl.c::lms7002m_streaming_up` | `P:stream.rs` (`PrepareStreaming`), `L:ctrl/op.rs` (`Control::streaming_up`, `set_running`), `Device::prepare_streaming` | — | `O:control_parity.rs::preparing_streams_on_a_fresh_board_sets_the_default_rate_like_libusdr`, `streams_prepared_after_a_rate_tunings_and_bandwidths_match_libusdr`, `bands_and_antenna_switches_follow_the_los_once_streaming_like_libusdr` (the rate, gains and bandwidths of running paths after it); `L:ctrl/op.rs::tests::the_transmit_half_selects_its_own_banks_before_its_processor_and_its_band` (TX on both channels and SISO-swapped to B, which LimeSDR's channel A does not reach), `a_transmit_direction_without_a_channel_or_rate_refuses_the_whole_streaming_up` / U |
| Streams created (`usdr_dms_create` of `/ll/srx/0` and `/ll/stx/0` in `ci16`): the packet arithmetic (RX bursts per block, a whole packet's or two fifths of it; TX 0 → 32 640, below 4096 → 4096, multiples of 4), the stream preparation, then the transport's stream for blocks of whole 4 KiB bursts; destroy forgets the stream with no bus operation | `u3_limesdr.c::limesdr_device_create_stream`, `dev_limesdr_initialize`, `limesdr_device_unregister_stream`; `C:ipblks/streams/stream_limesdr.c::create_limesdr_stream` | `P:framing.rs` (`rx_bursts`, `tx_plan`), `src/device/stream.rs` (`Device::create_rx_stream`, `create_tx_stream`, `destroy_rx_stream`, `destroy_tx_stream`), `control::ControlPort::{rx,tx}_stream_open` | — | `O:stream_parity.rs` (every scenario; `tx_packet_sizes_are_checked_like_libusdr`); `O:control_parity.rs` (a create as its stream preparation, a second create as destroy then create); `P:framing::tests::packet_sizes_follow_libusdr`; `T:stream.rs` / U |
| Stream sync (`usdr_dms_sync`): any type but `"off"` runs `limesdr_setup_stream(d, false, true, false)` once until stopped (chip select, stop, the debug build's running check, the timestamp reset, the channel-A TX DC DACs at 127 and the DC corrector enables, SISO SDR mode 0x0040, channel 0, start, then bits 1 and 3 of 0x0009 pulsed: `TXPCT_LOSS_CLR` and one the source does not name); `"off"` runs `limesdr_disable_stream` | `u3_limesdr.c::limesdr_device_stream_sync`; `limesdr_ctrl.c::limesdr_setup_stream`, `limesdr_disable_stream`, `limesdr_reset_timestamp`, `limesdr_start_streaming`, `limesdr_stop_streaming`; `C:hw/lms7002m/lms7002m.c::lms7002m_dc_corr`, `lms7002m_dc_corr_en` | `P:sync.rs` (`StreamSync`), `P:fpga.rs`, `L:chip/dc.rs` (`dc_dac`, `dc_corrector_enable`, already `xsdr`'s), `Device::start_streams`, `stop_streams` | — | `O:stream_parity.rs::an_rx_session_matches_libusdr`, `both_streams_with_repeated_syncs_match_libusdr` (a second start does nothing, a stop then start sets up again, a stop before any start); `P:sync::tests` / U |
| RX framing (`usdr_dms_recv` into `ci16`): bursts' headers and times, packets spanning blocks (the rest held for the next receive), lost samples between bursts zero-filled with their whole packets dropped and counted, the first sample's time | `stream_limesdr.c::_limestr_stream_recv` | `P:framing.rs` (`RxDeframer`), `Device::receive`, `RxPacket` | — | `O:stream_parity.rs::rx_packets_across_and_within_blocks_match_libusdr` (1500 to 10 000 samples), `lost_rx_samples_are_zero_filled_like_libusdr` (gaps of 5 to 9000 samples, several packets lost), `receiving_before_the_streams_start_times_out_like_libusdr`: every sample and time. The lost-packet count is not in libusdr's API (`totlost` is 0), so `P:framing::tests` alone covers it: a gap reached mid-packet, one of whole packets, bursts timed backwards / U |
| TX framing (`usdr_dms_send` from `ci16`): the size checks, a burst per 1020 samples with the "no time" flag, length and time in each header, the commit's time | `stream_limesdr.c::_limestr_stream_send` | `P:framing.rs` (`frame_tx`), `Device::send` | — | `O:stream_parity.rs::a_tx_session_matches_libusdr_byte_for_byte` (timed and untimed, 0 to 4100 samples, a refused 4098, four packet sizes): every block byte for byte; `P:framing::tests::tx_frames_carry_the_flag_length_and_each_bursts_time` / U |
| `no_std` compile witness for `src/portable` | — | `nostd-check/` (`limesdr-nostd-check`, exact-source `#[path]` import), `scripts/ci/portable.py limesdr` | the open's 20 exchanges | `nostd-check` tests (the open, a rate, the stream preparation and the stream sync over the real mapping and codec, on a fake board with stored LMS7002M registers and comparators; a TX frame received back through the RX deframer); `cargo check --target thumbv7em-none-eabi` (release-only in CI) / U |

The open is a hand-written sans-I/O machine (`Open`: `next` names a `LsOp` or a wait,
`complete` takes its result) run by `Device::open`; `lms7002m`'s `Create` runs inside it
on the same seam. The portable tree uses core only and allocates nothing (held by review;
`bnb`, under `lms7002m`'s value types, requires `alloc`). The created chip's cached state
is kept in `Device`'s control state, which each operation hands back also after a failure,
as libusdr keeps its updates. Later sequences (`SetSampleRate`, `PrepareStreaming`, the
control code's operations) run on `hal`'s `Sequence` seam; `Device` maps each access
to one LMS64C exchange (`LsOp::from_access`).

## Divergences (whitelisted per scenario)

| Divergence | Why | Where whitelisted |
|---|---|---|
| The reference is the nominal one (10, 30.72, 38.4, 40, 52 MHz) nearest the measurement within 1 MHz, the higher of two equally near. libusdr's walk skips entries and wraps: it opens only [30.72, 31.72] MHz (as 30.72) and [40, 41] MHz (as 40); it refuses exactly 10 MHz, (31.72, 38.4] and (41, 52) MHz with `-EFAULT`; it reads past its table at `clkTbl[5]` for (10, 30.72), (38.4, 40) and [52, 110] MHz, and at `clkTbl[i - 1]` with unsigned `i` = 0 below 10 or above 110 MHz (`P:reference.rs` has the table) | A real 40 MHz oscillator a few ppm slow lands in libusdr's undefined case; the fix is the walk with `ABS` evaluated once and signed | `O:open_parity.rs::references_libusdrs_walk_misses_are_recognised_and_opened_as_from_40_mhz` (10, 38.4, 52 MHz): libusdr's operations up to its refusal match, then limesdr's match a 40 MHz open from the reset on, with the reference recognised as itself; `references_libusdr_decides_reliably_are_decided_alike` covers 31, 35, 40, 40.9 and 41.1 MHz, where both decide alike. The undefined cases cannot run in libusdr: `T:open.rs::a_40_mhz_reference_a_few_ppm_slow_opens` |
| A detector that never finishes: `Error::DetectorStalled` after libusdr's ten polls and waits; libusdr tests `j == 100` after at most ten polls, so it reads whatever count the registers hold and may open on a stale one | A stale count is not a measurement | `O:open_parity.rs::a_stalled_detector_is_refused_where_libusdr_opens_on_a_stale_count` (the same polls and waits, then libusdr's count read) |
| The measurement is kept 64 bits wide; libusdr's 32-bit estimate wraps for counts above about 720.6 million (4.29 GHz), which could alias a nominal reference | No board produces such counts | Not observable on the sim's references; `P:reference::tests::counts_no_board_produces_do_not_wrap_onto_a_reference` |
| A reply naming another command than its request fails the exchange (`ExchangeError::Mismatched`); libusdr's shim checks only the status | A reply out of step with its request is not its result | Not reachable on the sim, which echoes the command: `P:lsop::tests::a_reply_to_another_command_is_not_this_operations_result` |
| A failed open reports its own error; libusdr's API reports `-ENODEV` for any device init failure (`lowlevel_create` returns the last transport plugin's result) | The device's errno is what failed | Outcomes, not operations: `O:open_parity.rs` compares the device's errno (`OpenError::device_errno`) where it checks one |
| Stream preparation whose default rate did not lock stops with `Error::NoSampleRate` after the rate's attempt, the run flags as they were; libusdr ignores the rate's failure, sets the run flags, streams up with a bandwidth of 0 and divides by it (`lms7002m_rbb_lpf_def`) | Defined behaviour; paths that never came up do not run | Cannot run in libusdr: `T:control.rs::streaming_is_not_prepared_when_the_default_rate_does_not_lock` (the trace is the rate's attempt alone; later tunings move no antenna switch) |
| A bus failure ends a rate, tuning, stream preparation or stream sync; libusdr retries a CGEN tuning that failed on the bus, carries on to channel B after a failed channel-A bandwidth write, streams up after a default rate that failed on the bus, and goes on with the setup after a failed read in its timestamp reset (`limesdr_reset_timestamp` returns 0) | Report what failed (as `xsdr`) | Not observable on the sim (no bus failures) |
| The stream sync reports its failures, and a start that failed counts as not started, so a later start sets up again; libusdr's `limesdr_device_stream_sync` returns 0 whatever the setup returned, having counted the streams started first | Report what failed | Not observable on the sim (no bus failures; the running check never trips after a stop): `P:sync::tests::a_setup_whose_stop_did_not_take_refuses_before_the_timestamp_reset` (`Error::StillStreaming`) |
| An RX packet of 0 samples is refused (`Error::UnsupportedPacketSize`) before any exchange; libusdr creates the stream, whose every receive waits for a block and delivers nothing | No samples is not a stream | `O:stream_parity.rs::an_rx_packet_of_zero_is_refused_where_libusdr_streams_empty_packets` (libusdr's preparation and stream setup, against nothing) |
| A second create of a stream already created is refused (`Error::AlreadyStreaming`); libusdr creates another, keeping the first (never freed), and receives on the newest | One stream per direction, like `xsdr`; destroy forgets it as libusdr's unregister does, so destroy then create is libusdr's second create op for op | `O:control_parity.rs` (its repeated stream creation); `T:stream.rs::stream_calls_out_of_turn_are_refused_before_any_exchange` |
| Refused where libusdr writes past a buffer or misreads. Before any exchange: a receive buffer shorter than a packet (`Error::BufferTooSmall`), a TX time at or after 2^63 (`Error::UnsupportedTimestamp`; libusdr takes it for none). After the transport's wait: an RX block shorter than the stream's (`Error::ShortRxBlock`; libusdr reads past it), and a TX buffer shorter than the block (`Error::Stream`), which is then not committed | Defined behaviour at the trust boundary | `T:stream.rs::stream_calls_out_of_turn_are_refused_before_any_exchange`, `a_short_rx_block_is_refused`, `a_tx_buffer_shorter_than_the_block_is_not_committed` |
| An RX block is copied out and released at once; libusdr holds the transport's buffer until its samples are consumed | The driver owns its copy; the transport gets its buffer back sooner | Invisible at the shim's one-buffer boundary and on the sim (`O:stream_parity.rs`) |
| A packet's bursts per block are computed without libusdr's 32-bit wrap of `2 * pktsyms`, and a block over 4 GiB is refused | No packet below 2^31 samples is affected | `P:framing::tests::streams_without_samples_or_bursts_or_over_4_gib_are_not_framed` |

Kept as libusdr, not divergences: the reset pulse precedes any LMS7002M access; the version
read happens twice (the board's exact gate, then `lms7002m_create`'s VER/REV check); stream
control is read and written back with only `RX_EN` and `TX_EN` cleared; close leaves the
LMS7002M, its reset pin and the gateware as the open and later operations left them. On the
control paths: tuning has no "same LO" skip (the XSDR's is in `xsdr_ctrl.c`); the bands and
antenna switches move only once streaming is prepared (the run flags gate them); the
direct-clocking bits are set again by every rate; the last RX band's 4.1 GHz edge is never
read (`get_antenna_cfg_by_freq`); a 4.1 GHz LO is refused (`-ERANGE`) as by libusdr's SXX
tuning; the `LimeLight` map is always A-then-B, I-then-Q, whatever the stream flags; and the
chip quirks `../xsdr/PORTING.md` lists (the TX pad's `-10 * gain`, `rfe_gain`'s first code
below, and the rest) apply here as well. On the streams: the setup's running check after
the stop is the pinned C as the oracle compiles it (no `NDEBUG`), so it is ported; a release
build of libusdr skips that read. The stream mode is always SISO SDR with 16-bit samples, the
only setup libusdr's stream sync asks for; streams are `ci16` on the host, the only format
`stream_limesdr.c` streams without a conversion (`tf_data` is a copy). Stream ops
(`_limestr_stream_op`) do nothing in libusdr and are not offered; libusdr reports no lost
samples (`totlost` 0), and `RxPacket::lost_packets` is its own `overruns` count, wrapping
as libusdr's `uint64_t` does. A stop then start resets the gateware's time but not the RX
stream's last burst time, so the first burst after a restart is a gap of nearly 2^64
samples: that receive delivers the samples held from before, then zeroes, its time near
2^64, and the lost-packet count jumps by about
2^64 over the packet size (`O:stream_parity.rs::both_streams_with_repeated_syncs_match_libusdr`
runs it; `P:framing::tests::bursts_timed_backwards_wrap_the_overrun_count_as_libusdr_does`).
Close issues nothing with streams open, as libusdr's.

## Pending owner review (public API)

- `Device::{create_rx_stream, create_tx_stream, start_streams, stop_streams, receive, send,
  destroy_rx_stream, destroy_tx_stream}` and `RxPacket`: shaped after `xsdr`'s streams
  (packet sizes as `u32`, `[[i16; 2]]` buffers, `Option<u64>` times), with libusdr's
  device-wide sync as `start_streams`/`stop_streams` rather than `xsdr`'s per-stream
  start; only `ci16`, so no format parameter yet.
- `ControlPort`'s stream methods with `hal::Bus`-like shapes and unsupported defaults, and
  no close (libusdr never tears a LimeSDR stream down).
- The new `Error` variants (`AlreadyStreaming`, `NotStreaming`, `UnsupportedPacketSize`,
  `Stream`, `Timeout`, `ShortRxBlock`, `BufferTooSmall`, `SendSize`, which here also carries
  the multiple, `UnsupportedTimestamp`, `StillStreaming`), named as `xsdr`'s where they
  mean the same.

## Remaining

| Capability | Pinned C | Blocker or note |
|---|---|---|
| FT601 USB transport (a `ControlPort` over `usb`): endpoint setup, framing of LMS64C frames on the bulk pipes, transport-level open traffic (board info, FIFO reset), USB identity by VID:PID (`0x0403:0x601f`, `0x0403:0x601e`) | `C:lowlevel/usb_ft601/usb_ft601_{generic,libusb}.c` (upstream, **not vendored**); `lowlevel/libusb_vidpid_map.h` | Not provable without hardware or the C: vendor it (a PIN-subset change in `devices/vendor/vendor.sh`), then frame-test against it. It would also turn the `ls_op` → LMS64C mapping from an assumption into evidence |
| Firmware reply semantics (echoed header, record layout, write-flag handling, other reset levels) | Not in libusdr; LimeSuite or the firmware | The sim and the driver share the shim's reading |
| Close on drop | `usdr_dmd_close` | Nothing to do today: libusdr's close issues nothing |
| Converter floors (`/dm/rate/rxtxadcdac`'s ADC and DAC rates) | `u3_limesdr.c::dev_limesdr_rate_m_set` | Not offered: `Device::set_sample_rate` takes RX and TX only, as `xsdr`'s |
| Debug register access (`/debug/hw/lms7002m/0/reg`) | `u3_limesdr.c::dev_mp_lm7_1_gps_debug_lms7002m_reg_set` | Not a driver operation |
| Stream host formats other than `ci16` (`cf32` and the rest through `xdsp/conv`), 12-bit wire samples, the "&" flat formats, the `fd` option | `stream_limesdr.c::create_limesdr_stream`; `xdsp/conv*.c` | Not offered: no LimeSDR path asks for them yet. The stream sync only ever sets 16-bit samples |
| The FT601 stream pipes behind `ControlPort::{rx,tx}_stream_*` (block transfers, buffer count 16, timeouts) | `C:lowlevel/usb_ft601/` (not vendored) | With the FT601 transport row; the oracle's shim stands in with one buffer per direction |
| Multi-frame LMS64C requests (more than 14 records) | `lms64c_fill_packet` with `out_cnt` > 1 | Nothing in the open needs them; libusdr's parser misreads reads over 14 registers (`AGENTS.md`) |
| Hardware qualification | — | Not authorized |

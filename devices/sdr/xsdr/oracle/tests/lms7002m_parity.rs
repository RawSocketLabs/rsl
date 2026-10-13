//! `rsl-lms7002m`'s chip operations against libusdr's compiled `hw/lms7002m/lms7002m.c`,
//! operation for operation (writes, reads and waits), on fresh simulated boards: every
//! bank selection, every RFE/TRF path and mode, MIMO hand-over between channels, every
//! AFE combination and both processors; the `LimeLight` configuration, maps and resets;
//! decimation, the DC corrector, the mixer and the self-test; the basebands' paths,
//! filters and gains; the front-end gains; and the CGEN and SXX tunings, whose capacitor
//! searches read the sim's comparator model. The XSDR session reaches only some of these.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use bnb::{u2, u3};
use rsl_lms7002m::{
    Batch, Create, LmlConfig, LmlMap, LmlSample, Lms7002m, LpfParams, Mac, NoChannel, RbbMode,
    RbbPath, ResetPlan, RxGainStage, RxMode, RxPath, Synthesizer, TbbMode, TbbPath, Tsp, TuneError,
    TxGainStage, TxMode, TxPath,
};
use rsl_lowlevel::run_sequence;
use rsl_xsdr::lowlevel::SpiAddr;
use rsl_xsdr_oracle::{Lms7002mCall as Call, lms7002m_run};
use rsl_xsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EINVAL`: libusdr's refusal of a banked call with no bank selected.
const EINVAL: i32 = 22;
/// `ERANGE`: a synthesizer that does not tune.
const ERANGE: i32 = 34;
/// `EIO`: a comparator that reads "fail".
const EIO: i32 = 5;
/// The XSDR's reference clock, which the sim's comparators assume.
const FREF: u32 = 26_000_000;

/// One call's result and reported values, and the board's trace.
type Run = (Vec<(i32, [i32; 3])>, Vec<Op>);

/// libusdr's run of `calls`.
fn libusdr(calls: &[Call]) -> Run {
    let (results, board) = lms7002m_run(SimBoard::new(Variant::Xsdr), calls);
    (
        results.iter().map(|r| (r.status, r.out)).collect(),
        board.trace().to_vec(),
    )
}

/// rsl-lms7002m's run of `calls`, from [`Lms7002m::new`], on its own board.
fn rsl(calls: &[Call]) -> Run {
    let bus = SimBus::new(SimBoard::new(Variant::Xsdr));
    let mut executor = bus.clone();
    let mut chip = Lms7002m::new(SpiAddr(0));
    let mut results = Vec::new();
    for &call in calls {
        results.push(apply(&mut chip, &mut executor, call));
    }
    let trace = bus.board().trace().to_vec();
    (results, trace)
}

/// Runs one call on rsl-lms7002m's `chip` over `executor`: its result and reported values.
fn apply(chip: &mut Lms7002m, executor: &mut SimBus, call: Call) -> (i32, [i32; 3]) {
    match call {
        Call::MacSet(mac) => written(executor, chip.mac_set(mac_of(mac))),
        Call::RfePath(path, mode) => banked(executor, chip.rfe_path(rx_path(path), rx_mode(mode))),
        Call::TrfPath(path, mode) => banked(executor, chip.trf_path(tx_path(path), tx_mode(mode))),
        Call::AfeEnable([rxa, rxb, txa, txb]) => {
            written(executor, chip.afe_enable([rxa, rxb], [txa, txb]))
        }
        Call::TspEnable(tsp, enable) => written(executor, chip.tsp_enable(tsp_of(tsp), enable)),
        Call::Destroy => written(executor, chip.destroy()),
        Call::LimelightConfigure(flags, bytes) => {
            written(executor, chip.limelight_configure(config(flags, bytes)))
        }
        Call::LimelightReset => written(executor, chip.limelight_reset()),
        Call::LimelightFifoReset(rx, tx) => written(executor, chip.limelight_fifo_reset(rx, tx)),
        Call::LimelightMap(siso, map1, map2) => written(
            executor,
            chip.limelight_map([siso & 1 == 1, siso & 2 == 2], [map(map1), map(map2)]),
        ),
        Call::LimelightSwitchRxMode(flags, bytes) => written(
            executor,
            chip.limelight_switch_rx_mode(config(flags, bytes)),
        ),
        Call::TspIntDec(tsp, order) => banked(executor, chip.tsp_int_dec(tsp_of(tsp), order)),
        Call::RxtspDcCorr(bypass, window) => banked(
            executor,
            chip.rxtsp_dc_corr(bypass, u3::from_raw(u8::try_from(window).unwrap())),
        ),
        Call::TspCmix(tsp, freq) => banked(executor, chip.tsp_cmix(tsp_of(tsp), freq)),
        Call::TspBist(tsp) => written(executor, chip.tsp_bist_start(tsp_of(tsp))),
        Call::TspReset(tsp) => written(executor, chip.tsp_reset(tsp_of(tsp))),
        Call::TspBistDone(tsp) => {
            let mut check = chip.bist_check(tsp_of(tsp));
            run_sequence(executor, &mut check).expect("the sim answers");
            (0, [i32::from(check.done().unwrap()), 0, 0])
        }
        Call::RbbPath(path, mode) => {
            written(executor, chip.rbb_path(rbb_path(path), rbb_mode(mode)))
        }
        Call::RbbPga(gain) => written(executor, chip.rbb_pga(gain)),
        Call::RbbLpfRaw(rcc, r, c) => written(executor, chip.rbb_lpf(LpfParams { rcc, r, c })),
        Call::TbbPath(path, mode) => {
            written(executor, chip.tbb_path(tbb_path(path), tbb_mode(mode)))
        }
        Call::TbbLpfRaw(rcc, r, c) => written(executor, chip.tbb_lpf(LpfParams { rcc, r, c })),
        Call::TbbGain(gain) => written(executor, chip.tbb_gain(gain)),
        Call::RfeGain(3, _) => written(executor, chip.rfe_gains()),
        Call::RfeGain(stage, gain) => {
            let (writes, actual) = chip.rfe_gain(rx_gain(stage), gain);
            let (status, _) = written(executor, writes);
            (status, [actual, 0, 0])
        }
        Call::TrfGain(stage, gain) => match chip.trf_gain(tx_gain(stage), gain) {
            Ok((writes, actual)) => {
                let (status, _) = written(executor, writes);
                (status, [actual.unwrap_or(0), 0, 0])
            }
            Err(NoChannel) => (-EINVAL, [0; 3]),
        },
        Call::CgenTune(fref, out, order) => {
            let mut tune = chip.cgen_tune(fref, out, u2::from_raw(u8::try_from(order).unwrap()));
            run_sequence(executor, &mut tune).expect("the sim answers");
            (status(tune.outcome().expect("finished")), [0; 3])
        }
        Call::CgenDisable => written(executor, chip.cgen_disable()),
        Call::SxxTune(path, fref, lo, lochen) => {
            let mut tune = chip.sxx_tune(synthesizer(path), fref, lo, lochen);
            run_sequence(executor, &mut tune).expect("the sim answers");
            (status(tune.outcome().expect("finished")), [0; 3])
        }
        Call::SxxDisable(path) => written(executor, chip.sxx_disable(synthesizer(path))),
        Call::RbbLpfDef(bw, low) => (0, params(LpfParams::rbb_default(bw, low))),
        Call::TbbLpfDef(bw, low) => (0, params(LpfParams::tbb_default(bw, low))),
        Call::Create(ldo_mask, shared_reference) => {
            let mut create = Create::new(ResetPlan {
                target: SpiAddr(0),
                ldo_mask,
                shared_reference,
            });
            run_sequence(executor, &mut create).expect("the sim's LMS7002M is accepted");
            *chip = create.chip().expect("created");
            (0, [0; 3])
        }
    }
}

/// Runs `batch` on `bus`: a call that returns 0.
fn written(bus: &mut SimBus, mut batch: Batch) -> (i32, [i32; 3]) {
    run_sequence(bus, &mut batch).expect("the sim accepts every write");
    (0, [0; 3])
}

/// Runs a banked call's batch, or reports libusdr's `-EINVAL` for no bank.
fn banked(bus: &mut SimBus, batch: Result<Batch, NoChannel>) -> (i32, [i32; 3]) {
    batch.map_or((-EINVAL, [0; 3]), |batch| written(bus, batch))
}

/// A tuning's outcome as libusdr's result.
fn status(outcome: Result<(), TuneError>) -> i32 {
    match outcome {
        Ok(()) => 0,
        Err(TuneError::OutOfRange) => -ERANGE,
        Err(TuneError::ComparatorFailed) => -EIO,
    }
}

/// Filter codes as the shim reports them.
fn params(params: LpfParams) -> [i32; 3] {
    [params.rcc, params.r, params.c].map(|code| i32::try_from(code).unwrap())
}

/// The shim's packed `lms7002m_limelight_conf_t`.
fn config(flags: u32, bytes: u32) -> LmlConfig {
    let bit = |n: u32| flags >> n & 1 == 1;
    let [rx_div, tx_div, tx_tsp_delay, tx_lml_delay] = bytes.to_le_bytes();
    LmlConfig {
        rx_siso_ddr: bit(0),
        tx_siso_ddr: bit(1),
        high_drive: bit(2),
        rx_fclk_read: bit(3),
        rx_lfsr: bit(4),
        rx_tx_loopback: bit(5),
        rx_port1: bit(6),
        rx_div,
        tx_div,
        tx_tsp_delay,
        tx_lml_delay,
    }
}

/// The shim's packed map.
fn map(packed: u32) -> LmlMap {
    core::array::from_fn(|slot| {
        [LmlSample::AI, LmlSample::AQ, LmlSample::BI, LmlSample::BQ]
            [usize::try_from(packed >> (2 * slot) & 3).unwrap()]
    })
}

/// libusdr's `LMS7_CH_*` code as a bank selection.
fn mac_of(code: u32) -> Mac {
    [Mac::None, Mac::A, Mac::B, Mac::Both][code as usize]
}

/// libusdr's `lms7002m_xxtsp_t`.
fn tsp_of(code: u32) -> Tsp {
    if code == 1 { Tsp::Rx } else { Tsp::Tx }
}

/// libusdr's `lms7002m_rfe_path_t`.
fn rx_path(code: u32) -> RxPath {
    [RxPath::None, RxPath::LnaH, RxPath::LnaL, RxPath::LnaW][code as usize]
}

/// libusdr's `lms7002m_rfe_mode_t`.
fn rx_mode(code: u32) -> RxMode {
    [RxMode::Disable, RxMode::Normal, RxMode::LoopbackRf][code as usize]
}

/// libusdr's `lms7002m_trf_path_t`.
fn tx_path(code: u32) -> TxPath {
    [TxPath::Mute, TxPath::Band1, TxPath::Band2][code as usize]
}

/// libusdr's `lms7002m_trf_mode_t`.
fn tx_mode(code: u32) -> TxMode {
    [TxMode::Disable, TxMode::Normal, TxMode::Loopback][code as usize]
}

/// libusdr's `lms7002m_rbb_path_t`.
fn rbb_path(code: u32) -> RbbPath {
    [
        RbbPath::LowBand,
        RbbPath::HighBand,
        RbbPath::Bypass,
        RbbPath::PowerDetector,
    ][code as usize]
}

/// libusdr's `lms7002m_rbb_mode_t`.
fn rbb_mode(code: u32) -> RbbMode {
    [
        RbbMode::Disable,
        RbbMode::Normal,
        RbbMode::Loopback,
        RbbMode::LoopbackAdc,
    ][code as usize]
}

/// libusdr's `lms7002m_tbb_path_t`.
fn tbb_path(code: u32) -> TbbPath {
    [TbbPath::Bypass, TbbPath::Ladder, TbbPath::HighBand][code as usize]
}

/// libusdr's `lms7002m_tbb_mode_t`.
fn tbb_mode(code: u32) -> TbbMode {
    [
        TbbMode::Disable,
        TbbMode::Normal,
        TbbMode::Loopback,
        TbbMode::LoopbackSwapIq,
        TbbMode::LoopbackDac,
    ][code as usize]
}

/// libusdr's `lms7002m_rfe_gain_t`, short of `RFE_GAIN_NONE`.
fn rx_gain(code: u32) -> RxGainStage {
    [RxGainStage::Lna, RxGainStage::Tia, RxGainStage::Loopback][code as usize]
}

/// libusdr's `lms7002m_trf_gain_t`.
fn tx_gain(code: u32) -> TxGainStage {
    [TxGainStage::Pad, TxGainStage::Loopback][code as usize]
}

/// libusdr's `lms7002m_sxx_path_t`.
fn synthesizer(code: u32) -> Synthesizer {
    if code == 0 {
        Synthesizer::Rx
    } else {
        Synthesizer::Tx
    }
}

/// Panics, naming `scenario`, where the two disagree.
fn assert_same(calls: &[Call], scenario: &str) {
    let (expected, actual) = (libusdr(calls), rsl(calls));
    assert_eq!(actual.0, expected.0, "{scenario}: call results");
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        panic!(
            "{scenario}: rsl-lms7002m diverges at op {at} of {}: libusdr {}, rsl-lms7002m {}",
            expected.1.len(),
            expected.1.get(at).map_or("(end)".into(), Op::to_string),
            actual.1.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

#[test]
fn every_bank_selection_path_and_mode_matches_libusdr() {
    for mac in 0..4 {
        for path in 0..4 {
            for mode in 0..3 {
                let calls = [
                    Call::MacSet(mac),
                    Call::RfePath(path, mode),
                    Call::TrfPath(path % 3, mode),
                    Call::Destroy,
                ];
                assert_same(&calls, &format!("MAC {mac}, path {path}, mode {mode}"));
            }
        }
    }
}

#[test]
fn a_channel_serving_the_other_matches_libusdr() {
    assert_same(
        &[
            Call::MacSet(3),
            Call::RfePath(1, 1),
            Call::TrfPath(2, 2),
            Call::MacSet(1),
            Call::RfePath(0, 0),
            Call::TrfPath(0, 0),
            Call::MacSet(2),
            Call::RfePath(3, 2),
            Call::TrfPath(1, 1),
        ],
        "channel B running while A is turned off",
    );
}

#[test]
fn every_afe_combination_and_both_processors_match_libusdr() {
    let afe: Vec<Call> = (0..16u8)
        .map(|bits| Call::AfeEnable(core::array::from_fn(|i| bits >> i & 1 == 1)))
        .collect();
    assert_same(&afe, "AFE");
    let tsp = [
        Call::TspEnable(0, true),
        Call::TspEnable(0, false),
        Call::TspEnable(1, true),
        Call::TspEnable(1, false),
    ];
    assert_same(&tsp, "processors");
}

#[test]
fn every_limelight_configuration_flag_and_divider_matches_libusdr() {
    let mut calls = vec![Call::Create(0, true)];
    for flags in 0..128 {
        for bytes in [0x0000_0101, 0x0102_0402, 0x0302_2001, 0xff00_0000] {
            calls.push(Call::LimelightConfigure(flags, bytes));
            calls.push(Call::LimelightSwitchRxMode(flags, bytes));
        }
    }
    calls.extend([
        Call::LimelightReset,
        Call::LimelightFifoReset(true, false),
        Call::LimelightFifoReset(false, true),
        Call::MacSet(1),
        Call::LimelightReset,
        Call::LimelightFifoReset(true, true),
    ]);
    assert_same(&calls, "LimeLight configuration");
}

#[test]
fn every_limelight_map_matches_libusdr() {
    // MIMO maps must name each stream once; SISO maps read only slots 0 and 1.
    let mimo = [0b11_10_01_00, 0b10_11_00_01, 0b01_00_11_10, 0b00_01_10_11];
    let mut calls = Vec::new();
    for siso in 0..4 {
        for (first, second) in (0..256).map(|map| (map, mimo[map as usize % 4])) {
            let pick = |is_siso: bool, map: u32| if is_siso { map } else { mimo[map as usize % 4] };
            calls.push(Call::LimelightMap(
                siso,
                pick(siso & 1 == 1, first),
                pick(siso & 2 == 2, second),
            ));
        }
    }
    assert_same(&calls, "LimeLight maps");
}

#[test]
fn the_processor_operations_match_libusdr_on_every_bank_selection() {
    for mac in 0..4 {
        let calls = [
            Call::Create(0, true),
            Call::MacSet(mac),
            Call::TspEnable(1, true),
            Call::TspEnable(0, true),
            Call::TspIntDec(1, 0),
            Call::TspIntDec(0, 3),
            Call::TspIntDec(1, 5),
            Call::RxtspDcCorr(true, 7),
            Call::RxtspDcCorr(false, 0),
            Call::TspCmix(1, 0x1234_5678),
            Call::TspCmix(0, -0x0765_4321),
            Call::TspCmix(1, 0),
            Call::TspCmix(0, i32::MIN),
            Call::TspBist(0),
            Call::TspBist(1),
            Call::TspReset(0),
            Call::TspReset(1),
            Call::TspBistDone(1),
            Call::TspBistDone(0),
        ];
        assert_same(&calls, &format!("processors, MAC {mac}"));
    }
}

#[test]
fn the_baseband_paths_filters_and_gains_match_libusdr() {
    let mut calls = vec![Call::Create(0, true)];
    for mac in [1, 2, 3, 0] {
        calls.push(Call::MacSet(mac));
        for path in 0..4 {
            for mode in 0..4 {
                calls.push(Call::RbbPath(path, mode));
            }
        }
        for path in 0..3 {
            for mode in 0..5 {
                calls.push(Call::TbbPath(path, mode));
            }
        }
    }
    for gain in [-10, 0, 5, 79, 80, 125, 130, 205, 210, 310, 400] {
        calls.push(Call::RbbPga(gain));
    }
    for (rcc, r, c) in [(0, 16, 0), (5, 16, 2047), (7, 31, 255), (9, 40, 4000)] {
        calls.push(Call::RbbLpfRaw(rcc, r, c));
    }
    for gain in [-70, -12, 0, 13, 63, 70] {
        calls.push(Call::TbbGain(gain));
        calls.push(Call::TbbLpfRaw(40, 200, 0));
        calls.push(Call::TbbLpfRaw(1, 0, 0));
    }
    assert_same(&calls, "basebands");
}

#[test]
fn the_filter_defaults_match_libusdr_across_their_ranges() {
    let mut calls = Vec::new();
    for bandwidth in (0..200).map(|step| 100_000 + step * 997_331) {
        for low in [false, true] {
            if bandwidth > 0 {
                calls.push(Call::RbbLpfDef(bandwidth, low));
            }
            calls.push(Call::TbbLpfDef(bandwidth, low));
        }
    }
    calls.push(Call::RbbLpfDef(u32::MAX, true));
    calls.push(Call::TbbLpfDef(u32::MAX, false));
    // No register is touched, so the call results carry the whole comparison.
    let (expected, actual) = (libusdr(&calls), rsl(&calls));
    assert_eq!(actual.0, expected.0, "filter codes");
}

#[test]
fn the_front_end_gains_match_libusdr() {
    let mut calls = vec![Call::Create(0, true)];
    for mac in [0, 1, 2, 3] {
        calls.push(Call::MacSet(mac));
        for stage in 0..4 {
            for gain in [-500, -400, -300, -299, -150, -35, -30, 0, 50] {
                calls.push(Call::RfeGain(stage, gain));
            }
        }
        for stage in 0..2 {
            for gain in [-600, -530, -520, -250, -210, -200, -140, -100, -9, 0, 30] {
                calls.push(Call::TrfGain(stage, gain));
            }
        }
        calls.push(Call::RfePath(3, 2));
        calls.push(Call::TrfPath(1, 2));
        calls.push(Call::RfeGain(3, 0));
        calls.push(Call::TrfGain(1, -230));
    }
    assert_same(&calls, "front-end gains");
}

#[test]
fn the_clock_generator_tunes_like_libusdr_across_rates() {
    let mut calls = vec![Call::Create(0, true)];
    for out in [
        1_000_000,
        8_000_000,
        30_720_000,
        61_440_000,
        122_880_000,
        128_000_000,
        245_760_000,
        320_000_000,
        370_000_000,
        700_000_000,
        2_000_000_000,
    ] {
        for order in 0..4 {
            calls.push(Call::CgenTune(FREF, out, order));
        }
    }
    calls.push(Call::CgenDisable);
    assert_same(&calls, "CGEN tuning");
    let (results, _) = libusdr(&calls);
    let tuned = |status| results.iter().any(|&(result, _)| result == status);
    assert!(
        tuned(0) && tuned(-ERANGE),
        "the scenario both locks and runs out of range"
    );
}

#[test]
fn the_lo_synthesizers_tune_like_libusdr_across_their_range() {
    let mut calls = vec![Call::Create(0, true)];
    for lo in (0..60).map(|step| 20_000_000 + step * 69_333_333) {
        calls.push(Call::SxxTune(lo % 2, FREF, lo, lo % 3 == 0));
    }
    calls.extend([
        Call::SxxTune(0, FREF, 3_999_000_001, false),
        Call::SxxTune(1, FREF, 29_000_000, false),
        Call::SxxTune(0, FREF, 0, false),
        Call::SxxDisable(0),
        Call::SxxTune(0, FREF, 1_000_000_000, false),
        Call::SxxDisable(1),
        Call::SxxTune(1, 40_000_000, 2_400_000_000, true),
    ]);
    assert_same(&calls, "SXX tuning");
    let (results, _) = libusdr(&calls);
    let tuned = |status| results.iter().any(|&(result, _)| result == status);
    assert!(
        tuned(0) && tuned(-ERANGE),
        "the scenario both locks and runs out of range"
    );
}

//! How libusdr identifies `PE_SYNC`: the transport's 16-byte device ID selects the `pe_sync`
//! factory (no hardware ID or variant follows), and the clock chips are then checked by
//! their own identity registers during configuration.

use pe_sync_oracle::Oracle;
use pe_sync_sim::{I2cAddress, Op, SimBoard};

/// `ENODEV`: `lmk05318_create`'s refusal of a wrong device identity.
const ENODEV: i32 = 19;
/// `EINVAL`: `dev_pe_sync_rate_set`'s refusal of any sample rate.
const EINVAL: i32 = 22;
/// The LMK05318B (`I2C_BUS_LMK05318B`: core 0, bus 0, 0x64).
const LMK05318: I2cAddress = I2cAddress {
    core: 0,
    bus: 0,
    addr: 0x64,
};

#[test]
fn pe_sync_opens_as_a_device_without_sample_channels() {
    let mut oracle = Oracle::open(SimBoard::new()).expect("libusdr opens PE_SYNC");
    for path in [c"/ll/sdr/max_hw_rx_chans", c"/ll/sdr/max_hw_tx_chans"] {
        assert_eq!(
            oracle.get_uint(path),
            Ok(0),
            "PE_SYNC declares no hardware channels"
        );
    }
    assert_eq!(
        oracle.set_uint(c"/dm/rate/master", 1_000_000),
        Err(-EINVAL),
        "PE_SYNC refuses a sample rate: it is not a sample source"
    );
    let _ = oracle.close();
}

#[test]
fn a_board_whose_lmk05318_does_not_answer_is_refused_and_powered_off() {
    let board = SimBoard::new().without(LMK05318);
    let err = Oracle::open(board)
        .err()
        .expect("libusdr refuses a board without its network synchronizer");
    assert_eq!(err.errno, -ENODEV);

    let lmk05318_ops: Vec<&Op> = err
        .board
        .trace()
        .iter()
        .filter(|op| matches!(op, Op::I2c { addr, .. } if *addr == LMK05318))
        .collect();
    assert_eq!(
        lmk05318_ops.len(),
        1,
        "only the identity read reaches the LMK05318 before the refusal"
    );
    for bank in [0, 1, 2, 3, 4, 6] {
        assert_eq!(
            err.board.gpo(bank),
            Some(0),
            "a failed open powers the board off (GPO bank {bank})"
        );
    }
}

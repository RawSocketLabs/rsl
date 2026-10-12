//! Power sequencing around the clock tree: `pe_sync_power_on` enables every supply before
//! any chip is addressed, and close leaves every output it drove off.

use rsl_pe_sync_oracle::Oracle;
use rsl_pe_sync_sim::{Op, SimBoard};

/// The supply banks `pe_sync_power_on` enables: `IGPO_IN_CTRL`, `IGPO_SY0_CTRL`,
/// `IGPO_SY1_CTRL`, `IGPO_GEN_CTRL`, `IGPO_DISTRIB_CTRL`.
const SUPPLY_BANKS: [u8; 5] = [0, 1, 2, 3, 4];
/// `IGPO_LED_CTRL`.
const LED_BANK: u8 = 6;
/// The status read (`dev_gpi_get32(IGPI_STAT)`, register 16 + 16/4) that opens
/// `pe_sync_check_status`, the first step after power-on.
const STATUS_READ: u32 = 20;

#[test]
fn every_supply_is_enabled_before_any_chip_is_addressed() {
    let board = Oracle::open(SimBoard::new())
        .expect("libusdr opens PE_SYNC")
        .close();
    let trace = board.trace();
    let first_chip_op = trace
        .iter()
        .position(|op| matches!(op, Op::Spi { .. } | Op::I2c { .. }))
        .expect("open addresses the clock chips");
    let mut powered = SimBoard::new();
    for op in &trace[..first_chip_op] {
        if let Op::RegWrite { addr, value } = op {
            powered.write_reg(*addr, *value);
        }
    }
    for bank in SUPPLY_BANKS {
        assert!(
            powered.gpo(bank).is_some_and(|data| data != 0),
            "GPO bank {bank} is enabled before the first SPI or I2C transfer"
        );
    }
    assert!(
        trace[..first_chip_op].contains(&Op::RegRead {
            addr: STATUS_READ,
            value: 0
        }),
        "the status word is read after power-on, before the chips"
    );
}

#[test]
fn close_leaves_every_supply_and_led_off() {
    let board = Oracle::open(SimBoard::new())
        .expect("libusdr opens PE_SYNC")
        .close();
    for bank in SUPPLY_BANKS.into_iter().chain([LED_BANK]) {
        assert_eq!(
            board.gpo(bank),
            Some(0),
            "GPO bank {bank} is off after close"
        );
    }
}

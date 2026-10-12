//! Deterministic transaction tests using pinned USB frames, without an executor or hardware.

use std::io;
use std::time::Duration;

use super::i2c::{Action, Event, I2cTransaction};
use super::protocol::{I2cCache, Notification};
use crate::lowlevel::{BusError, I2cAddr};

/// A TMP114 read at the uSDR's sensor address, using a fresh session cache.
fn transaction() -> I2cTransaction {
    I2cTransaction::new(
        7,
        I2cAddr::new(0, 0x4e),
        &[0],
        2,
        &mut I2cCache::default(),
        Duration::ZERO,
    )
    .unwrap()
}

#[test]
fn tmp114_request_and_scalar_reply_match_pinned_usb_golden() {
    // Pinned C MAKE_I2C_CMD: slot0, one write byte, two read bytes, fresh address LUT.
    let mut transaction = transaction();
    match transaction.start(Duration::ZERO).unwrap() {
        Action::Write {
            id,
            packet,
            timeout,
        } => {
            assert_eq!(id, 7);
            assert_eq!(packet, [0, 0, 1, 0, 0x4e, 0, 0, 0x80, 0, 0, 0, 0x94]);
            assert_eq!(timeout, Duration::from_secs(20));
        }
        other => panic!("expected framed command, got {other:?}"),
    }
    let mut sequence = None;
    assert!(
        matches!(transaction.complete(Event::Written { id: 7 }, Duration::ZERO, &mut sequence).unwrap(),
        Action::Notification { id: 7, timeout } if timeout == Duration::from_secs(1))
    );
    // libusb_transfer_ntfy: global sequence0xabcd, scalar IRQ6, wire bytes 0x0c80 (25°C).
    let packet = [6, 0, 0xcd, 0xab, 0x0c, 0x80, 0, 0];
    assert!(matches!(
        transaction
            .complete(
                Event::Notification {
                    id: 7,
                    packet: &packet
                },
                Duration::ZERO,
                &mut sequence
            )
            .unwrap(),
        Action::Complete {
            bytes: [0x0c, 0x80, 0, 0],
            len: 2
        }
    ));
    assert_eq!(sequence, Some(0xabce));
    assert!(
        transaction
            .complete(
                Event::Notification {
                    id: 7,
                    packet: &packet
                },
                Duration::ZERO,
                &mut sequence
            )
            .is_err(),
        "a terminal result cannot be delivered twice"
    );
}

#[test]
fn unrelated_irqs_do_not_restart_completion_deadline() {
    let mut transaction = transaction();
    transaction.start(Duration::ZERO).unwrap();
    let mut sequence = None;
    transaction
        .complete(
            Event::Written { id: 7 },
            Duration::from_secs(19),
            &mut sequence,
        )
        .unwrap();
    let packet = [2, 0, 0, 0, 42, 0, 0, 0];
    assert!(
        matches!(transaction.complete(Event::Notification { id: 7, packet: &packet }, Duration::from_millis(19_750), &mut sequence).unwrap(),
        Action::Notification { timeout, .. } if timeout == Duration::from_millis(250))
    );
    let packet = [6, 0, 1, 0, 0, 0, 0, 0];
    assert!(matches!(
        transaction.complete(
            Event::Notification {
                id: 7,
                packet: &packet
            },
            Duration::from_secs(20),
            &mut sequence
        ),
        Err(BusError::Timeout)
    ));
    assert_eq!(
        sequence,
        Some(1),
        "late data must not advance session sequence"
    );
}

#[test]
fn timer_expiration_needs_no_transfer_callback_and_is_terminal() {
    let mut operation = transaction();
    operation.start(Duration::ZERO).unwrap();
    operation
        .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
        .unwrap();
    assert_eq!(
        operation.advance(Duration::from_millis(999)).unwrap(),
        Some(Duration::from_millis(1))
    );
    assert!(matches!(
        operation.advance(Duration::from_secs(1)),
        Err(BusError::Timeout)
    ));
    assert_eq!(operation.advance(Duration::from_secs(2)).unwrap(), None);
    assert!(
        operation
            .complete(Event::Written { id: 7 }, Duration::from_secs(2), &mut None)
            .is_err()
    );
}

#[test]
fn delayed_dispatch_clamps_each_io_timeout_without_losing_terminal_results() {
    let mut operation = transaction();
    let action = operation.start(Duration::ZERO).unwrap();
    assert!(matches!(
        operation.dispatch(action, Duration::from_secs(19)).unwrap(),
        Action::Write { timeout, .. } if timeout == Duration::from_secs(1)
    ));
    let action = operation
        .complete(Event::Written { id: 7 }, Duration::from_secs(19), &mut None)
        .unwrap();
    assert!(matches!(
        operation.dispatch(action, Duration::from_millis(19_900)).unwrap(),
        Action::Notification { timeout, .. } if timeout == Duration::from_millis(100)
    ));
    let packet = [6, 0, 0, 0, 0x0c, 0x80, 0, 0];
    let complete = operation
        .complete(
            Event::Notification {
                id: 7,
                packet: &packet,
            },
            Duration::from_millis(19_999),
            &mut None,
        )
        .unwrap();
    assert!(
        matches!(
            operation
                .dispatch(complete, Duration::from_secs(21))
                .unwrap(),
            Action::Complete {
                bytes: [0x0c, 0x80, 0, 0],
                len: 2
            }
        ),
        "an acknowledged result survives delayed observation"
    );
}

#[test]
fn an_expired_cached_action_cannot_submit_more_io() {
    let mut operation = transaction();
    let action = operation.start(Duration::ZERO).unwrap();
    assert!(matches!(
        operation.dispatch(action, Duration::from_secs(20)),
        Err(BusError::Timeout)
    ));
    assert!(
        operation
            .complete(Event::Written { id: 7 }, Duration::from_secs(20), &mut None)
            .is_err()
    );
}

#[test]
fn cancelled_commands_reject_late_completions() {
    let mut queued = transaction();
    queued.cancel(Duration::ZERO).unwrap();
    assert!(queued.start(Duration::ZERO).is_err());
    let mut issued = transaction();
    issued.start(Duration::ZERO).unwrap();
    assert!(
        issued.cancel(Duration::ZERO).is_err(),
        "issued cancellation requires cleanup outside the machine"
    );
    assert!(
        issued
            .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
            .is_err()
    );
}

#[test]
fn write_only_completes_without_waiting_for_an_irq() {
    let mut operation = I2cTransaction::new(
        3,
        I2cAddr::new(0, 0x4e),
        &[0x0b],
        0,
        &mut I2cCache::default(),
        Duration::ZERO,
    )
    .unwrap();
    operation.start(Duration::ZERO).unwrap();
    let mut sequence = Some(4);
    assert!(matches!(
        operation
            .complete(Event::Written { id: 3 }, Duration::ZERO, &mut sequence)
            .unwrap(),
        Action::Complete {
            bytes: [0, 0, 0, 0],
            len: 0
        }
    ));
    assert_eq!(
        sequence,
        Some(4),
        "write-only operations consume no notifications"
    );
}

#[test]
fn transfer_failure_is_terminal_and_keeps_its_error() {
    let mut operation = transaction();
    operation.start(Duration::ZERO).unwrap();
    assert!(matches!(
        operation.complete(
            Event::Failed {
                id: 7,
                error: BusError::Disconnected
            },
            Duration::ZERO,
            &mut None
        ),
        Err(BusError::Disconnected)
    ));
    assert!(
        operation
            .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
            .is_err()
    );
}

#[test]
fn native_io_sources_survive_the_portable_error_boundary() {
    let mut operation = transaction();
    operation.start(Duration::ZERO).unwrap();
    assert!(matches!(
        operation.complete(
            Event::Failed { id: 7, error: BusError::Io(io::Error::from_raw_os_error(5)) },
            Duration::ZERO,
            &mut None,
        ),
        Err(BusError::Io(error)) if error.raw_os_error() == Some(5)
    ));
}

#[test]
fn mismatched_identity_and_duplicate_write_fail_closed() {
    let mut wrong = transaction();
    wrong.start(Duration::ZERO).unwrap();
    assert!(
        wrong
            .complete(Event::Written { id: 8 }, Duration::ZERO, &mut None)
            .is_err()
    );
    assert!(
        wrong
            .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
            .is_err()
    );
    let mut duplicate = transaction();
    duplicate.start(Duration::ZERO).unwrap();
    duplicate
        .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
        .unwrap();
    assert!(
        duplicate
            .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
            .is_err()
    );
}

#[test]
fn invalid_clocks_fail_before_issuing_more_work() {
    let mut backward = transaction();
    backward.start(Duration::from_secs(1)).unwrap();
    assert!(
        backward
            .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
            .is_err()
    );
    assert!(
        I2cTransaction::new(
            7,
            I2cAddr::new(0, 0x4e),
            &[0],
            2,
            &mut I2cCache::default(),
            Duration::MAX
        )
        .is_err()
    );
}

#[test]
fn malformed_tail_and_duplicate_scalar_do_not_commit_sequence() {
    let mut sequence = Some(0xabcd);
    // A valid scalar followed by a truncated record must not hide the malformed tail.
    let packet = [6, 0, 0xcd, 0xab, 12, 0, 0, 0, 6, 0, 0xce, 0xab];
    assert!(Notification::completion(&packet, 6, &mut sequence).is_err());
    assert_eq!(sequence, Some(0xabcd));
    let packet = [6, 0, 0xcd, 0xab, 12, 0, 0, 0, 6, 0, 0xce, 0xab, 13, 0, 0, 0];
    assert!(Notification::completion(&packet, 6, &mut sequence).is_err());
    assert_eq!(sequence, Some(0xabcd));
}

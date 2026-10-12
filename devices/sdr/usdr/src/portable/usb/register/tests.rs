//! The register/SPI machine against the pinned C framing, without host I/O or allocation
//! (the `no_std` fixture compiles these too).

use super::*;

/// The machine's error type in these tests: a caller's own transport error.
type Tx = RegisterTransaction<u8>;

/// A scalar IRQ record as `libusb_transfer_ntfy` delivers it: header (sequence in bits
/// 31:16, event in 5:0), then the value.
fn record(sequence: u16, event: u8, value: u32) -> [u8; 8] {
    let header = (u32::from(sequence) << 16) | u32::from(event);
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&header.to_le_bytes());
    bytes[4..].copy_from_slice(&value.to_le_bytes());
    bytes
}

/// Starts `transaction` and returns its command frame's bytes.
fn command(transaction: &mut Tx) -> Frame {
    match transaction.start(Duration::ZERO) {
        Ok(Action::Write { frame, .. }) => frame,
        other => panic!("a fresh transaction sends its command first: {other:?}"),
    }
}

/// Starts `transaction` and completes its command, returning what the machine asks next.
fn written(transaction: &mut Tx, id: u64) -> Result<Action, Failure<u8>> {
    command(transaction);
    transaction.complete(Event::Written { id }, Duration::ZERO, &mut None)
}

#[test]
fn a_read_sends_only_the_pinned_c_command_word() {
    // usb_async_regread32: 0xc0000000 | ((count - 1) << 16) | addr.
    let mut transaction = Tx::read(1, 0x1234, 64, Duration::ZERO).unwrap();
    assert_eq!(
        command(&mut transaction).as_bytes(),
        [0x34, 0x12, 0x3f, 0xc0]
    );
    let mut single = Tx::read(2, 0x13, 1, Duration::ZERO).unwrap();
    assert_eq!(
        command(&mut single).as_bytes(),
        Command::encode(0x13, 1, true),
        "the frame is the shared command encoder's word"
    );
}

#[test]
fn a_write_sends_the_header_then_every_value_in_one_256_byte_frame() {
    let values = [0x1122_3344; MAX_WRITE_WORDS];
    let mut transaction = Tx::write(1, 0x1234, &values, Duration::ZERO).unwrap();
    let frame = command(&mut transaction);
    // OUT_REGOUT_SIZE=256 includes the header, leaving 63 payload words.
    assert_eq!(frame.as_bytes().len(), 256);
    assert_eq!(
        frame.as_bytes()[..8],
        [0x34, 0x12, 0x3e, 0, 0x44, 0x33, 0x22, 0x11]
    );
    assert!(
        frame.as_bytes()[4..]
            .chunks_exact(4)
            .all(|word| word == [0x44, 0x33, 0x22, 0x11]),
        "every value follows the header in order"
    );
}

#[test]
fn spi_writes_its_word_to_register_two_and_waits_for_irq_two() {
    let mut transaction = Tx::spi32(1, SpiAddr(0), 0x8000_0001, Duration::ZERO).unwrap();
    assert_eq!(
        command(&mut transaction).as_bytes(),
        [2, 0, 0, 0, 0x01, 0, 0, 0x80]
    );
    let mut sequence = None;
    assert!(matches!(
        transaction.complete(Event::Written { id: 1 }, Duration::ZERO, &mut sequence),
        Ok(Action::Notification { id: 1, timeout }) if timeout == IRQ_TIMEOUT
    ));
    // An unrelated I²C completion (IRQ 6) does not answer the SPI exchange.
    let elapsed = Duration::from_millis(400);
    assert!(matches!(
        transaction.complete(
            Event::Notification { id: 1, packet: &record(9, 6, 7) },
            elapsed,
            &mut sequence,
        ),
        Ok(Action::Notification { timeout, .. }) if Some(timeout) == IRQ_TIMEOUT.checked_sub(elapsed)
    ));
    let Ok(Action::Complete { words, len }) = transaction.complete(
        Event::Notification {
            id: 1,
            packet: &record(10, 2, 0xabcd),
        },
        elapsed,
        &mut sequence,
    ) else {
        panic!("the SPI IRQ completes the exchange");
    };
    assert_eq!(words[..len], [0xabcd]);
    assert_eq!(sequence, Some(11));
}

#[test]
fn only_a_readback_of_exactly_the_requested_words_completes() {
    let mut exact = Tx::read(1, 4, 2, Duration::ZERO).unwrap();
    assert!(matches!(
        written(&mut exact, 1),
        Ok(Action::Readback { id: 1, timeout }) if timeout == IO_TIMEOUT
    ));
    let packet = [1, 0, 0, 0, 2, 0, 0, 0];
    let Ok(Action::Complete { words, len }) = exact.complete(
        Event::Readback {
            id: 1,
            packet: &packet,
        },
        Duration::ZERO,
        &mut None,
    ) else {
        panic!("a whole readback completes the read");
    };
    assert_eq!(words[..len], [1, 2]);
    for packet in [&[1, 0, 0, 0][..], &[0; 12], &[0; 7], &[]] {
        let mut transaction = Tx::read(1, 4, 2, Duration::ZERO).unwrap();
        written(&mut transaction, 1).unwrap();
        assert!(
            matches!(
                transaction.complete(Event::Readback { id: 1, packet }, Duration::ZERO, &mut None),
                Err(Failure::InvalidData(_))
            ),
            "a {}-byte readback is rejected",
            packet.len()
        );
        assert!(
            transaction.advance(Duration::ZERO).unwrap().is_none(),
            "a rejected readback ends the transaction"
        );
    }
}

#[test]
fn a_notification_sequence_gap_fails_without_moving_the_session_sequence() {
    let mut transaction = Tx::spi32(1, SpiAddr(0), 1, Duration::ZERO).unwrap();
    written(&mut transaction, 1).unwrap();
    let mut sequence = Some(5);
    assert!(matches!(
        transaction.complete(
            Event::Notification {
                id: 1,
                packet: &record(7, 2, 42)
            },
            Duration::ZERO,
            &mut sequence,
        ),
        Err(Failure::InvalidData(_))
    ));
    assert_eq!(sequence, Some(5));
}

#[test]
fn each_reply_has_its_own_deadline_after_the_command() {
    // The readback gets USB_IO_TIMEOUT from the command's completion, not from its start.
    let mut read = Tx::read(1, 4, 1, Duration::ZERO).unwrap();
    command(&mut read);
    let late = Duration::from_secs(19);
    read.complete(Event::Written { id: 1 }, late, &mut None)
        .unwrap();
    assert!(
        read.advance(late + IO_TIMEOUT.saturating_sub(Duration::from_millis(1)))
            .is_ok()
    );
    assert!(matches!(
        read.advance(late + IO_TIMEOUT),
        Err(Failure::Timeout)
    ));
    // The IRQ wait is one second, however many unrelated records arrive.
    let mut spi = Tx::spi32(2, SpiAddr(0), 1, Duration::ZERO).unwrap();
    written(&mut spi, 2).unwrap();
    assert!(matches!(
        spi.complete(
            Event::Notification {
                id: 2,
                packet: &record(0, 6, 0)
            },
            IRQ_TIMEOUT,
            &mut None,
        ),
        Err(Failure::Timeout)
    ));
    // A command that never completes times out too.
    let mut stuck = Tx::write(3, 4, &[1], Duration::ZERO).unwrap();
    command(&mut stuck);
    assert!(matches!(stuck.advance(IO_TIMEOUT), Err(Failure::Timeout)));
}

#[test]
fn a_completion_for_another_operation_poisons_the_transaction() {
    let mut transaction = Tx::read(5, 4, 1, Duration::ZERO).unwrap();
    command(&mut transaction);
    assert!(matches!(
        transaction.complete(Event::Written { id: 6 }, Duration::ZERO, &mut None),
        Err(Failure::InvalidData(_))
    ));
    assert!(
        transaction
            .complete(Event::Written { id: 5 }, Duration::ZERO, &mut None)
            .is_err(),
        "the matching completion can no longer be attributed"
    );
}

#[test]
fn transfer_kinds_and_actions_must_match_the_active_phase() {
    let mut transaction = Tx::read(1, 4, 1, Duration::ZERO).unwrap();
    command(&mut transaction);
    assert!(
        transaction
            .complete(
                Event::Readback {
                    id: 1,
                    packet: &[0; 4]
                },
                Duration::ZERO,
                &mut None
            )
            .is_err(),
        "a readback cannot precede its command's completion"
    );
    let mut stale = Tx::read(2, 4, 1, Duration::ZERO).unwrap();
    let write = stale.start(Duration::ZERO).unwrap();
    stale
        .complete(Event::Written { id: 2 }, Duration::ZERO, &mut None)
        .unwrap();
    assert!(
        stale.dispatch(write, Duration::ZERO).is_err(),
        "a completed command cannot be dispatched again"
    );
}

#[test]
fn a_transport_error_is_returned_unchanged() {
    let mut transaction = Tx::write(1, 4, &[9], Duration::ZERO).unwrap();
    command(&mut transaction);
    assert!(matches!(
        transaction.complete(
            Event::Failed { id: 1, error: 42 },
            Duration::ZERO,
            &mut None
        ),
        Err(Failure::Transport(42))
    ));
}

#[test]
fn requests_outside_the_core_limits_are_refused_before_any_transfer() {
    assert!(matches!(
        Tx::read(1, 0, 0, Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(matches!(
        Tx::read(1, 0, 65, Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(matches!(
        Tx::write(1, 0, &[], Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(matches!(
        Tx::write(1, 0, &[0; 64], Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(matches!(
        Tx::spi32(1, SpiAddr(1), 0, Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(matches!(
        Tx::read(1, 0xffff, 2, Duration::ZERO),
        Err(Failure::Unsupported(_))
    ));
    assert!(Tx::read(1, 0xffc0, 64, Duration::ZERO).is_ok());
}

#[test]
fn a_sequence_action_becomes_the_matching_single_word_transaction() {
    let mut read = Tx::access(1, FpgaAccess::Read { reg: 19 }, Duration::ZERO).unwrap();
    assert_eq!(command(&mut read).as_bytes(), Command::encode(19, 1, true));
    let mut write = Tx::access(
        2,
        FpgaAccess::Write {
            reg: 0,
            value: 0x1100_0001,
        },
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(
        command(&mut write).as_bytes(),
        [0, 0, 0, 0, 0x01, 0, 0, 0x11]
    );
    let spi = FpgaAccess::Spi {
        target: SpiAddr(0),
        word: 7,
        reg: 0,
        write: false,
    };
    let mut exchange = Tx::access(3, spi, Duration::ZERO).unwrap();
    assert_eq!(command(&mut exchange).as_bytes(), [2, 0, 0, 0, 7, 0, 0, 0]);
}

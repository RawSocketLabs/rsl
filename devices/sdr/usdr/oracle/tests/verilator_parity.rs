//! usdr's Verilator host bridge against libusdr's, compiled whole: each connects to its
//! own fake model (`tests/common/verilator_peer.rs`), runs the same operations, and the
//! models' records of what each sent must match byte for byte. Results match too, but for
//! the whitelisted divergences each test names.

#[path = "../../tests/common/verilator_peer.rs"]
mod verilator_peer;

use std::time::Duration;

use usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use usdr::transport::verilator::VerilatorBus;
use usdr_oracle::verilator::Bridge;
use verilator_peer::{NOTIFY_BASE, NOTIFY_BYTES, Op, Peer, Trace, i2c_reply, spi_reply};

/// What libusdr's bridge returns for an invalid argument.
const EINVAL: i32 = -22;

/// Runs `c` on libusdr's bridge and `rust` on usdr's, each against a fresh model, and
/// returns what each model received with what each side returned. Each model's
/// acknowledgements are awaited before it hangs up, as libusdr's monitor sends them from
/// its own thread.
fn run<C, R>(
    name: &str,
    c: impl FnOnce(&mut Bridge, &Peer) -> C,
    rust: impl FnOnce(&mut VerilatorBus, &Peer) -> R,
) -> ((Trace, C), (Trace, R)) {
    let peer = Peer::start(&format!("{name}-c"));
    // SAFETY: the model announces the uSDR's UUID and made its 4 GiB shared file.
    let mut bridge = unsafe { Bridge::open(peer.dir()) }.expect("libusdr's bridge connects");
    let from_c = c(&mut bridge, &peer);
    peer.settle();
    drop(bridge);
    let c_trace = peer.finish();

    let peer = Peer::start(&format!("{name}-rust"));
    let mut bus = VerilatorBus::connect(peer.socket()).expect("usdr's bridge connects");
    let from_rust = rust(&mut bus, &peer);
    peer.settle();
    drop(bus);
    ((c_trace, from_c), (peer.finish(), from_rust))
}

/// Both models received the same operations and the same bytes.
fn assert_same_wire(c: &Trace, rust: &Trace) {
    assert_eq!(rust.ops, c.ops, "usdr sends libusdr's operations");
    assert_eq!(rust.bytes, c.bytes, "usdr sends libusdr's bytes");
}

#[test]
fn connecting_matches_libusdrs_bridge() {
    let ring = |peer: &Peer| peer.memory(NOTIFY_BASE, NOTIFY_BYTES);
    let ((c, c_ring), (rust, rust_ring)) =
        run("connect", |_, peer| ring(peer), |_, peer| ring(peer));
    assert_same_wire(&c, &rust);
    // 32 router writes, the ring's address, four acknowledgements, the ring's mapping.
    assert_eq!(rust.ops.len(), 38);
    assert_eq!(
        rust.ops.last(),
        Some(&Op::Map {
            addr: 0xfee0_0000,
            len: 4096,
            flags: 2
        })
    );
    assert!(
        c_ring.iter().all(|&byte| byte == 0xff),
        "libusdr clears the ring"
    );
    assert_eq!(rust_ring, c_ring, "usdr clears the ring as libusdr does");
}

#[test]
fn register_access_matches_libusdrs_bridge() {
    // Direct single and burst writes, then reads; then the configuration window.
    let writes: [(u32, &[u32]); 4] = [
        (20, &[0x11]),
        (21, &[0x22, 0x33, 0x44]),
        (0x1_0010, &[0x55, 0x66]),
        (0x1_0020, &[0x77]),
    ];
    let reads: [(u32, usize); 4] = [(20, 1), (20, 5), (0x1_0010, 3), (0x1_0020, 1)];
    let ((c, c_values), (rust, rust_values)) = run(
        "registers",
        |bridge, _| {
            for (addr, values) in writes {
                assert_eq!(bridge.write_regs(addr, values), 0);
            }
            reads.map(|(addr, len)| {
                let mut out = vec![0; len];
                assert_eq!(bridge.read_regs(addr, &mut out), 0);
                out
            })
        },
        |bus, _| {
            for (addr, values) in writes {
                bus.write_regs(addr, values).expect("write");
            }
            reads.map(|(addr, len)| {
                let mut out = vec![0; len];
                bus.read_regs(addr, &mut out).expect("read");
                out
            })
        },
    );
    assert_same_wire(&c, &rust);
    assert_eq!(
        rust_values, c_values,
        "both read the model's registers alike"
    );
}

/// SPI words, more than 32 so the ring's acknowledgement is crossed.
fn spi_words() -> Vec<u32> {
    (0..40).map(|n| 0x0100_0000 * n + 0x10 + n).collect()
}

/// I2C transfers: (`ls_op` address on bus 0 of core 0, bytes out, bytes back).
const I2C_TRANSFERS: [(u8, &[u8], usize); 8] = [
    (0x48, &[], 0),
    (0x49, &[0x01], 0),
    (0x6a, &[0x02, 0x03], 0),
    (0x23, &[0x04, 0x05, 0x06], 0),
    (0x48, &[0x0f], 1),
    (0x61, &[0x10], 2),
    (0x6a, &[0x11], 3),
    (0x23, &[0x12], 4),
];

/// Whitelisted: libusdr's bridge returns its readback array, which nothing writes (zeroed
/// by the harness), for every SPI and I2C read; usdr returns the reply word the
/// notification carries.
#[test]
fn spi_and_i2c_match_libusdrs_bridge_on_the_wire_and_return_the_models_replies() {
    let ((c, (c_spi, c_i2c)), (rust, (rust_spi, rust_i2c))) = run(
        "spi-i2c",
        |bridge, peer| {
            let spi: Vec<u32> = spi_words()
                .into_iter()
                .map(|word| {
                    let read = bridge.spi32(0, word).expect("SPI");
                    peer.settle();
                    read
                })
                .collect();
            let i2c: Vec<Vec<u8>> = I2C_TRANSFERS
                .iter()
                .map(|&(addr, write, len)| {
                    let mut read = vec![0; len];
                    assert_eq!(bridge.i2c(addr.into(), write, &mut read), 0);
                    peer.settle();
                    read
                })
                .collect();
            (spi, i2c)
        },
        |bus, _| {
            let spi: Vec<u32> = spi_words()
                .into_iter()
                .map(|word| bus.spi32(SpiAddr(0), word).expect("SPI"))
                .collect();
            let i2c: Vec<Vec<u8>> = I2C_TRANSFERS
                .iter()
                .map(|&(addr, write, len)| {
                    let mut read = vec![0; len];
                    bus.i2c(I2cAddr::new(0, addr), write, &mut read)
                        .expect("I2C");
                    read
                })
                .collect();
            (spi, i2c)
        },
    );
    assert_same_wire(&c, &rust);
    assert!(
        rust.ops.contains(&Op::Write {
            reg: 9,
            words: vec![62]
        }),
        "the 32nd notification is acknowledged"
    );
    assert!(
        c_spi.iter().all(|&read| read == 0),
        "libusdr reads nothing back"
    );
    assert!(
        c_i2c.iter().flatten().all(|&byte| byte == 0),
        "libusdr reads nothing back"
    );
    let replies: Vec<u32> = spi_words().into_iter().map(spi_reply).collect();
    assert_eq!(rust_spi, replies, "usdr returns the model's SPI replies");
    // Each I2C read's reply is to the command the model received.
    let commands = rust.ops.iter().filter_map(|op| match op {
        Op::Write { reg: 1, words } if words[0] >> 31 == 1 => Some(words[0]),
        _ => None,
    });
    let reads = rust_i2c.iter().filter(|read| !read.is_empty());
    for (read, command) in reads.zip(commands) {
        assert_eq!(read[..], i2c_reply(command).to_le_bytes()[..read.len()]);
    }
}

/// 600 notifications take the ring through its phase flip at entry 256 and its wrap at 512,
/// acknowledging every 32nd entry on the way.
#[test]
fn the_notification_ring_wraps_and_is_acknowledged_like_libusdrs_bridge() {
    let words: Vec<u32> = (0..600).map(|n| 0x00c0_0000 | n).collect();
    let ((c, ()), (rust, replies)) = run(
        "ring",
        |bridge, peer| {
            for &word in &words {
                bridge.spi32(0, word).expect("SPI");
                peer.settle();
            }
        },
        |bus, _| {
            words
                .iter()
                .map(|&word| bus.spi32(SpiAddr(0), word).expect("SPI"))
                .collect::<Vec<_>>()
        },
    );
    assert_same_wire(&c, &rust);
    let acks = rust
        .ops
        .iter()
        .filter(
            |op| matches!(op, Op::Write { reg: 9, words } if words[0] >> 16 == 0 && words[0] != 0),
        )
        .count();
    assert_eq!(
        acks,
        600 / 32,
        "every 32nd entry acknowledged, across the wrap"
    );
    let expected: Vec<u32> = words.iter().map(|&word| spi_reply(word)).collect();
    assert_eq!(replies, expected, "every entry read in its pass's phase");
}

/// Whitelisted: libusdr's bridge writes a block's first two status words through its size
/// argument (the harness reads them there); usdr lends the lost-packet word and burst
/// mask, then the DMA status, as libusdr's stream code reads its out-of-band words.
#[test]
fn rx_streams_match_libusdrs_bridge() {
    // Not a page multiple, so each buffer's mapping is rounded up.
    const BLOCK: usize = 4196;
    let blocks: Vec<Vec<u8>> = (0..3u8).map(|n| vec![n + 1; BLOCK]).collect();
    let block = u32::try_from(BLOCK).expect("a small block");
    let ((c, (c_blocks, statuses)), (rust, rust_blocks)) = run(
        "rx",
        |bridge, peer| {
            assert_eq!(bridge.rx_open(block, 32), 0);
            let mut statuses = peer.deliver_rx(&[&blocks[0], &blocks[1]]);
            peer.settle();
            let mut received = vec![
                bridge.rx_recv(BLOCK).expect("block 0"),
                bridge.rx_recv(BLOCK).expect("block 1"),
            ];
            statuses.extend(peer.deliver_rx(&[&blocks[2]]));
            peer.settle();
            received.push(bridge.rx_recv(BLOCK).expect("block 2"));
            (received, statuses)
        },
        |bus, peer| {
            bus.rx_stream_open(block).expect("stream opens");
            let mut received = Vec::new();
            let mut recv = |bus: &mut VerilatorBus| {
                bus.rx_stream_recv(Duration::from_secs(1), &mut |data, oob| {
                    received.push((data.to_vec(), oob));
                })
                .expect("a block");
            };
            let _ = peer.deliver_rx(&[&blocks[0], &blocks[1]]);
            recv(bus);
            recv(bus);
            let _ = peer.deliver_rx(&[&blocks[2]]);
            recv(bus);
            received
        },
    );
    assert_same_wire(&c, &rust);
    assert_eq!(
        rust.ops
            .iter()
            .filter(|op| matches!(op, Op::Map { len: 8192, .. }))
            .count(),
        32,
        "every buffer mapped, rounded to pages"
    );
    for (((c_data, c_words), (rust_data, oob)), status) in
        c_blocks.iter().zip(&rust_blocks).zip(&statuses)
    {
        assert_eq!(rust_data, c_data, "both lend the block libusdr lends");
        assert_eq!(
            *c_words,
            [status[0], status[1]],
            "libusdr's misplaced words"
        );
        assert_eq!(
            *oob,
            [
                u64::from(status[1]) << 32 | u64::from(status[0]),
                u64::from(status[2])
            ]
        );
    }
}

#[test]
fn an_rx_block_over_1_mib_is_refused_by_both_before_the_wire() {
    let ((c, c_res), (rust, rust_res)) = run(
        "rx-oversized",
        |bridge, _| bridge.rx_open((1 << 20) + 1, 32),
        |bus, _| bus.rx_stream_open((1 << 20) + 1),
    );
    assert_same_wire(&c, &rust);
    assert_eq!(c_res, EINVAL);
    assert!(matches!(rust_res, Err(BusError::Unsupported(_))));
}

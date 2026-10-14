//! A fake Verilator model of the uSDR's gateware, speaking libusdr's host bridge protocol
//! (`lowlevel/verilator_ll`) on a Unix socket and a shared file, as the external harness
//! would. It records every byte the host sends and the operations they decode to.
//!
//! Shared by `usdr`'s tests and the oracle's parity tests (`#[path]`-included), so the
//! Rust transport and libusdr's compiled bridge talk to the same model.
//!
//! The model is a register file, not gateware: registers hold what was written (those never
//! written read [`unwritten`]), the configuration window (index register 6, data register
//! 7) reaches registers from 0x10000, an SPI word written to register 2 is answered at once
//! with a notification for event 2 carrying [`spi_reply`], an I2C command with its read bit
//! written to register 1 with one for event 6 carrying [`i2c_reply`], and RX blocks arrive
//! when the test delivers them. Each answer is one interrupt packet after its ring entries.
//!
//! Assumption, as the transport makes it: a notification's second word is the data the
//! gateware read for the event (SPI and I2C replies), and an RX block's three later words
//! are the DMA engine's status, the third holding the block's number in bits 29:24. The
//! evidence is libusdr's `lowlevel/pcie_uram/pcie_events.rst` (MIT, vendored), whose
//! `ADDR_0` data follows the metadata word, and the bridge's own sequence check
//! (`verilatorll_wrap.c`, `recv_dma_wait`).

#![allow(dead_code, reason = "each test binary uses part of the model")]

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::FileExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// `M2_LM6_1_DEVICE_ID`, the UUID a uSDR model announces.
pub(crate) const USDR_UUID: [u8; 16] = [
    0x1f, 0x09, 0xe2, 0x53, 0xc8, 0xad, 0x42, 0xa1, 0x81, 0xab, 0x96, 0x0f, 0x73, 0xeb, 0x3c, 0x62,
];

/// The first RX buffer's place in the shared file.
pub(crate) const RX_DMA_BASE: u64 = 0x1000_0000;
/// The distance between RX buffers.
pub(crate) const RX_DMA_STRIDE: u64 = 0x0100_0000;
/// The notification ring's place in the shared file.
pub(crate) const NOTIFY_BASE: u64 = 0xfee0_0000;
/// The notification ring's length.
pub(crate) const NOTIFY_BYTES: usize = 4096;

/// The shared file's length: the bridge maps 4 GiB.
const MEMORY_BYTES: u64 = 1 << 32;
/// The I2C core's register.
const I2C: u32 = 1;
/// The SPI core's register.
const SPI: u32 = 2;
/// The window's index register.
const WINDOW_INDEX: u32 = 6;
/// The window's data register.
const WINDOW_DATA: u32 = 7;
/// The first register the window reaches.
const WINDOW_FIRST: u32 = 0x1_0000;
/// The notification acknowledgement register.
const NOTIFY_ACK: u32 = 9;
/// The SPI core's event.
const SPI_EVENT: u32 = 2;
/// The I2C core's event.
const I2C_EVENT: u32 = 6;
/// The bytes a Unix socket's path may take, its terminating NUL included (`sun_path`).
const SOCKET_PATH_LIMIT: usize = 108;
/// How long [`Peer::settle`] waits before failing the test.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

/// What a register never written reads.
#[must_use]
pub(crate) fn unwritten(reg: u32) -> u32 {
    0xa500_0000 | reg
}

/// The model's SPI reply to `word`.
#[must_use]
pub(crate) fn spi_reply(word: u32) -> u32 {
    word.rotate_left(8) ^ 0x5a00_00a5
}

/// The model's I2C reply to `command`.
#[must_use]
pub(crate) fn i2c_reply(command: u32) -> u32 {
    (command & 0x00ff_ffff) ^ 0xc3d2_e1f0
}

/// The three status words the model gives the RX block numbered `number`: a lost-packet
/// word, a burst mask, and a DMA status carrying the block's number modulo 64 in bits
/// 29:24, which the host checks.
#[must_use]
pub(crate) fn rx_status(number: u32) -> [u32; 3] {
    [
        0x00a0_0000 | number,
        0xb000_0000 | number,
        (number & 0x3f) << 24 | 0x5500 | (number & 0xff),
    ]
}

/// One host packet, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// Registers written from `reg`.
    Write {
        /// The first register.
        reg: u32,
        /// The words.
        words: Vec<u32>,
    },
    /// `count` registers asked for from `reg`.
    Read {
        /// The first register.
        reg: u32,
        /// The words asked for.
        count: u32,
        /// The request's tag.
        tag: u8,
    },
    /// A region of the shared file to map for DMA.
    Map {
        /// Its address.
        addr: u32,
        /// Its length.
        len: u32,
        /// Its flags.
        flags: u32,
    },
    /// Anything else.
    Other {
        /// The packet type.
        kind: u8,
        /// Its tag.
        tag: u8,
        /// Its words.
        words: Vec<u32>,
    },
}

/// What the host sent, whole.
#[derive(Debug)]
pub(crate) struct Trace {
    /// Every byte, in order.
    pub(crate) bytes: Vec<u8>,
    /// The packets they decode to.
    pub(crate) ops: Vec<Op>,
}

/// The model's state, shared between its thread and the test.
#[derive(Default)]
struct State {
    /// The connection, for answers; `None` until the host connects.
    writer: Option<UnixStream>,
    /// Every byte received.
    bytes: Vec<u8>,
    /// The packets received.
    ops: Vec<Op>,
    /// Registers written.
    registers: BTreeMap<u32, u32>,
    /// The window's index.
    window_index: u32,
    /// The next ring entry to write, modulo 512.
    next_entry: u32,
    /// Acknowledgements the entries written so far call for.
    acks_due: usize,
    /// Acknowledgements received.
    acks_seen: usize,
    /// The number of the next RX block.
    next_block: u32,
}

/// The model's state and the signal that it changed.
type Shared = Arc<(Mutex<State>, Condvar)>;

/// A fake model, serving one connection.
pub(crate) struct Peer {
    /// The directory of its socket and shared file.
    dir: PathBuf,
    /// The shared file.
    memory: File,
    /// Its state.
    shared: Shared,
    /// Its thread, until it is stopped.
    thread: Option<JoinHandle<()>>,
}

impl Peer {
    /// A uSDR model in a fresh directory named for `name`.
    ///
    /// # Panics
    ///
    /// If the directory, socket or file cannot be made.
    #[must_use]
    pub(crate) fn start(name: &str) -> Self {
        let mut hello = vec![255, 0];
        hello.extend(20u16.to_ne_bytes());
        hello.extend(USDR_UUID);
        Self::start_with_hello(name, hello)
    }

    /// A model that greets the host with `hello` instead of a uSDR's UUID.
    ///
    /// # Panics
    ///
    /// If the directory, socket or file cannot be made.
    #[must_use]
    pub(crate) fn start_with_hello(name: &str, hello: Vec<u8>) -> Self {
        // Under the system's temporary directory, not the target directory: a Unix socket's
        // path must fit `sun_path`, which libusdr's bridge would silently truncate.
        let dir = env::temp_dir().join(format!("rsl-vll-{}-{name}", process::id()));
        let socket = dir.join("verilator.sock");
        assert!(
            socket.as_os_str().len() < SOCKET_PATH_LIMIT,
            "the model's socket path fits sun_path"
        );
        // A previous run's leftovers, if any.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("model directory");
        let memory = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.join("verilator.sock.mmap"))
            .expect("shared file");
        // Sparse: only what is written takes space.
        memory.set_len(MEMORY_BYTES).expect("4 GiB sparse file");
        let listener = UnixListener::bind(socket).expect("model socket");
        let shared: Shared = Arc::default();
        let thread = {
            let shared = Arc::clone(&shared);
            let memory = memory.try_clone().expect("shared file handle");
            thread::spawn(move || serve(&listener, &hello, &memory, &shared))
        };
        Self {
            dir,
            memory,
            shared,
            thread: Some(thread),
        }
    }

    /// The model's directory.
    #[must_use]
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// The model's socket.
    #[must_use]
    pub(crate) fn socket(&self) -> PathBuf {
        self.dir.join("verilator.sock")
    }

    /// `len` bytes of the shared file from `addr`.
    ///
    /// # Panics
    ///
    /// If the file cannot be read.
    #[must_use]
    pub(crate) fn memory(&self, addr: u64, len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        self.memory
            .read_exact_at(&mut bytes, addr)
            .expect("shared file read");
        bytes
    }

    /// Delivers `blocks` as the RX DMA engine would, each into the next buffer with its
    /// notification, then raises one interrupt. Returns each block's status words.
    ///
    /// # Panics
    ///
    /// If the host is not connected or the file cannot be written.
    pub(crate) fn deliver_rx(&self, blocks: &[&[u8]]) -> Vec<[u32; 3]> {
        let mut state = lock(&self.shared);
        let mut statuses = Vec::new();
        let mut events = Vec::new();
        for block in blocks {
            let number = state.next_block;
            state.next_block += 1;
            let place = RX_DMA_BASE + u64::from(number & 0x1f) * RX_DMA_STRIDE;
            self.memory.write_all_at(block, place).expect("RX block");
            let status = rx_status(number);
            statuses.push(status);
            events.push((0, status));
        }
        notify(&mut state, &self.memory, &events);
        statuses
    }

    /// Sends `bytes` to the host as they are.
    ///
    /// # Panics
    ///
    /// If the host is not connected.
    pub(crate) fn send_raw(&self, bytes: &[u8]) {
        let mut state = lock(&self.shared);
        let writer = state.writer.as_mut().expect("a connected host");
        writer.write_all(bytes).expect("raw bytes to the host");
    }

    /// Waits until the host has acknowledged every 32nd notification the model wrote, so
    /// a host that acknowledges from another thread (libusdr's monitor) has sent it before
    /// the test goes on.
    ///
    /// # Panics
    ///
    /// If the acknowledgements do not arrive within 10 s.
    pub(crate) fn settle(&self) {
        let (lock_state, changed) = &*self.shared;
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        let mut state = lock_state.lock().unwrap_or_else(PoisonError::into_inner);
        while state.acks_seen < state.acks_due {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "the host never acknowledged its notifications"
            );
            state = changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Hangs up and returns what the host sent.
    #[must_use]
    pub(crate) fn finish(mut self) -> Trace {
        self.stop();
        let mut state = lock(&self.shared);
        Trace {
            bytes: std::mem::take(&mut state.bytes),
            ops: std::mem::take(&mut state.ops),
        }
    }

    /// Ends the connection (or the wait for one) and the model's thread.
    fn stop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        let writer = lock(&self.shared).writer.take();
        match writer {
            Some(writer) => {
                let _ = writer.shutdown(Shutdown::Both);
            }
            // Wakes the accept; the model greets it and finds it gone.
            None => drop(UnixStream::connect(self.socket())),
        }
        thread.join().expect("model thread");
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The model's state, whatever a panicking holder left.
fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.0.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Serves one host until it hangs up or the test stops the model.
fn serve(listener: &UnixListener, hello: &[u8], memory: &File, shared: &Shared) {
    let Ok((mut stream, _)) = listener.accept() else {
        return;
    };
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    {
        // The writer is in place before the host can see the greeting, so nothing the test
        // sends after the host connects finds it missing.
        let mut state = lock(shared);
        if writer.write_all(hello).is_err() {
            return;
        }
        state.writer = Some(writer);
    }
    loop {
        let mut header = [0; 4];
        if stream.read_exact(&mut header).is_err() {
            break;
        }
        let size = usize::from(u16::from_ne_bytes([header[2], header[3]]));
        let mut payload = vec![0; size.saturating_sub(4)];
        if stream.read_exact(&mut payload).is_err() {
            break;
        }
        let words: Vec<u32> = payload
            .chunks_exact(4)
            .map(|word| u32::from_ne_bytes(word.try_into().expect("four bytes")))
            .collect();
        let mut state = lock(shared);
        state.bytes.extend(header);
        state.bytes.extend(&payload);
        handle(&mut state, memory, header[0], header[1], &words);
        shared.1.notify_all();
    }
}

/// Records one host packet and answers it.
fn handle(state: &mut State, memory: &File, kind: u8, tag: u8, words: &[u32]) {
    match (kind, words) {
        (0, [addr, values @ ..]) => {
            let reg = addr / 4;
            state.ops.push(Op::Write {
                reg,
                words: values.to_vec(),
            });
            for (offset, &value) in (0..).zip(values) {
                store(state, reg + offset, value);
            }
            match (reg, values) {
                (SPI, &[word]) => notify(state, memory, &[(SPI_EVENT, [spi_reply(word), 0, 0])]),
                (I2C, &[command]) if command >> 31 == 1 => {
                    notify(state, memory, &[(I2C_EVENT, [i2c_reply(command), 0, 0])]);
                }
                // A position in bucket 0, never 0; the connection's bucket resets (`n << 16`)
                // acknowledge nothing.
                (NOTIFY_ACK, &[position]) if position != 0 && position >> 16 == 0 => {
                    state.acks_seen += 1;
                }
                _ => {}
            }
        }
        (1, &[addr, count]) => {
            let reg = addr / 4;
            state.ops.push(Op::Read { reg, count, tag });
            let values: Vec<u32> = (reg..reg + count).map(|reg| load(state, reg)).collect();
            let mut packet = vec![3, tag];
            let size = u16::try_from(4 + 4 * values.len()).expect("a short reply");
            packet.extend(size.to_ne_bytes());
            for value in values {
                packet.extend(value.to_ne_bytes());
            }
            send(state, &packet);
        }
        (254, &[addr, len, flags]) => state.ops.push(Op::Map { addr, len, flags }),
        _ => state.ops.push(Op::Other {
            kind,
            tag,
            words: words.to_vec(),
        }),
    }
}

/// A register write, through the window where it leads.
fn store(state: &mut State, reg: u32, value: u32) {
    match reg {
        WINDOW_INDEX => state.window_index = value,
        WINDOW_DATA => {
            let windowed = WINDOW_FIRST + state.window_index;
            state.registers.insert(windowed, value);
        }
        _ => {}
    }
    state.registers.insert(reg, value);
}

/// A register read, through the window where it leads.
fn load(state: &State, reg: u32) -> u32 {
    let reg = if reg == WINDOW_DATA {
        WINDOW_FIRST + state.window_index
    } else {
        reg
    };
    state
        .registers
        .get(&reg)
        .copied()
        .unwrap_or_else(|| unwritten(reg))
}

/// Writes one ring entry per event (its number, then three words), then raises one
/// interrupt, tagged with the last event.
fn notify(state: &mut State, memory: &File, events: &[(u32, [u32; 3])]) {
    let mut last = 0;
    for &(event, words) in events {
        let entry = state.next_entry;
        let phase = (entry >> 8) & 1;
        let first = phase << 31 | (entry & 0xffff) << 16 | event;
        let mut bytes = Vec::with_capacity(16);
        for word in [first, words[0], words[1], words[2]] {
            bytes.extend(word.to_be_bytes());
        }
        let place = NOTIFY_BASE + u64::from(entry & 0xff) * 16;
        memory.write_all_at(&bytes, place).expect("ring entry");
        if entry % 32 == 31 {
            state.acks_due += 1;
        }
        state.next_entry = (entry + 1) & 0x1ff;
        last = event;
    }
    let mut packet = vec![7, u8::try_from(last).expect("an event below 32")];
    packet.extend(4u16.to_ne_bytes());
    send(state, &packet);
}

/// Sends `packet` to the host, if it is still there.
fn send(state: &mut State, packet: &[u8]) {
    if let Some(writer) = state.writer.as_mut() {
        let _ = writer.write_all(packet);
    }
}

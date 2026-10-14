//! The uSDR's gateware simulated in Verilator, through libusdr's host bridge
//! (`lowlevel/verilator_ll`: `verilatorll_wrap.c`, `vpu.c`, `unix_vll.c`).
//!
//! The model is an external Verilator harness, not part of libusdr's sources. It listens on
//! a Unix stream socket and backs its DMA with a file it shares with the host,
//! `<socket>.mmap`, which it creates 4 GiB long. [`VerilatorBus::connect`] connects as
//! libusdr's bridge does and returns a [`Bus`] for [`Device::builder`](crate::Device::builder).
//! It speaks the bridge's protocol packet for packet; `oracle/tests/verilator_parity.rs`
//! compares the bytes both send a fake model.
//!
//! # Protocol
//!
//! Every packet is a 4-byte header (type, tag, then the whole packet's length in bytes as
//! 16 bits) followed by 32-bit words, all in host byte order: the C sends its structs as
//! they lie in memory (`vpu.h`).
//!
//! - The model first sends its device UUID (type 255, 16 bytes).
//! - The host writes registers (type 0: the byte address, then the words), asks for reads
//!   (type 1: the byte address and word count, tag 0) and maps regions of the shared file
//!   into the model's DMA space (type 254: address, length, flags).
//! - The model answers a read with its words (type 3, the request's tag) and raises
//!   interrupts (type 7). The events behind an interrupt are in the notification ring, the
//!   4 KiB at `0xFEE0_0000` of the shared file: 256 entries of four big-endian words, the
//!   first holding the event number in bits 5:0 and, in bit 31, a phase that flips on each
//!   pass, so the host knows which entries are new. Every 32 entries the host acknowledges
//!   its position. SPI and I2C replies, and the RX DMA engine's status words, are the
//!   entries' later words.
//! - RX blocks land in 32 buffers of the shared file, 16 MiB apart from `0x1000_0000`, and
//!   are handed back by writing the buffer's number to the RX confirm register.
//!
//! No `unsafe`: the host reads and writes the shared file with positioned I/O, which sees
//! the same page cache as the model's mapping, rather than mapping it.
//!
//! # Divergences from libusdr's bridge
//!
//! On the wire it sends what libusdr's does for every operation both perform. Otherwise:
//!
//! - The socket path is the caller's; libusdr opens `verilator.sock` in the working
//!   directory ([`DEFAULT_SOCKET`]).
//! - Packets are read by the calling thread while it waits for one, not by a monitor
//!   thread, so an interrupt is handled (and acknowledged) at the next wait rather than at
//!   once.
//! - SPI and I2C reads return the reply word their notification carries: its second word,
//!   the data the gateware read from the core's register for the event (libusdr's
//!   `lowlevel/pcie_uram/pcie_events.rst`, MIT, vendored beside the oracle's C: `ADDR_0`
//!   data, SPI0 register 2, I2C register 1).
//!   libusdr's bridge returns a readback array it never writes.
//! - RX blocks' out-of-band words are laid out as `pcie_events.rst` gives them and
//!   libusdr's stream code reads them (the lost-packet word and burst mask, then the DMA
//!   status, with no timestamp); the bridge writes the status words through its size
//!   argument instead. RX waits honour their timeout (libusdr waits forever), and an RX
//!   interrupt is never held back (libusdr delays those whose batch starts at entry 132).
//!   Closing the RX stream succeeds, sending nothing; libusdr's is unsupported.
//! - Malformed or unexpected packets, interrupt tags past 32 or notifications for events
//!   past 31, a status word out
//!   of sequence, a read or an SPI or I2C reply not answered in time, and any partial
//!   packet leave the bus unusable ([`BusError::Disconnected`] from then on): the stream
//!   cannot be trusted after them. libusdr's monitor thread stops on most of them, which
//!   stalls the next wait, aborts on the status check, and writes past its arrays for
//!   large events. Register reads wait 10 s, where libusdr waits forever.
//! - Accesses libusdr's bridge mishandles are refused before anything is sent: SPI targets
//!   but 0 (libusdr writes to an unconfigured core entry) and I2C buses or cores but 0
//!   (libusdr indexes past its tables), reads of more than 64
//!   words (whose reply its monitor cannot receive) and writes longer than a packet's
//!   16-bit length.
//! - Only the uSDR is accepted: libusdr creates whichever registered board the model's
//!   UUID names.
//! - Writes use `write(2)`, where libusdr sends with `MSG_NOSIGNAL`: a model that goes away
//!   raises `SIGPIPE`, which Rust programs ignore by default; a host that restores its
//!   default action must ignore it to see the error instead.
//!
//! Kept as libusdr: I2C commands name the device by the low two bits of its address in the
//! lookup-table slot field, with no lookup table written (libusdr's USB transport writes
//! one, `usb_uram_generic.c`), so I2C reaches devices by address only in a model that
//! ignores the table; RX buffer numbering continues across streams; only RX
//! streams. Whether a given Verilator harness accepts all of this is untested: none is
//! vendored.

use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::mem;
use std::os::unix::fs::FileExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use deps::tracing::{debug, trace};

use crate::error::Error;
use crate::lowlevel::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};

/// The socket libusdr's bridge connects to, in the working directory.
pub const DEFAULT_SOCKET: &str = "verilator.sock";

/// What the shared file's name adds to the socket's.
const MEMORY_SUFFIX: &str = ".mmap";

/// `M2_LM6_1_DEVICE_ID` (`device/device_ids.h`), the UUID a uSDR model announces.
const USDR_UUID: [u8; 16] = [
    0x1f, 0x09, 0xe2, 0x53, 0xc8, 0xad, 0x42, 0xa1, 0x81, 0xab, 0x96, 0x0f, 0x73, 0xeb, 0x3c, 0x62,
];

/// Packet types (`enum ptype`, `vpu.h`): a register write.
const MEMORY_WRITE: u8 = 0;
/// A register read request.
const READ_REQUEST: u8 = 1;
/// The model's answer to a read.
const READ_COMPLETION: u8 = 3;
/// An interrupt: new entries in the notification ring.
const INTERRUPT: u8 = 7;
/// A region of the shared file the model is to map for DMA.
const MAP_REGION: u8 = 254;
/// The model's device UUID, its first packet.
const DEVICE_UUID: u8 = 255;

/// The tag of every read: the bridge allows one outstanding read and always takes the
/// first free tag.
const READ_TAG: u8 = 0;
/// The packet header's length in bytes.
const HEADER_BYTES: usize = 4;
/// The most a packet from the model may carry: the bridge receives into 64 words
/// (`verilator_process_recv`).
const MAX_PAYLOAD: usize = 256;
/// The words a read may ask for.
const MAX_READ_WORDS: usize = MAX_PAYLOAD / 4;

/// The flags of every region the bridge maps: the model writes it (`PROT_WRITE`).
const MAP_WRITE: u32 = 2;

/// The uSDR's register layout, as libusdr's device description gives it to the bridge
/// (`device_bus_init` over `m2_lm6_1.c`'s `s_params_m2_lm6_1_rev000`), and the
/// notification registers the bridge names itself: the interrupt router.
const INTERRUPT_CONFIG: u32 = 15;
/// Where the notification ring is (`REG_WR_PNTFY_CFG`).
const NOTIFY_CONFIG: u32 = 8;
/// The notification acknowledgement (`REG_WR_PNTFY_ACK`).
const NOTIFY_ACK: u32 = 9;
/// The simple I2C core.
const I2C: u32 = 1;
/// The simple SPI core.
const SPI: u32 = 2;
/// The RX DMA engine's buffer confirmation.
const RX_CONFIRM: u32 = 4;
/// The index register of the window into the stream engine's configuration space; data
/// is the register after.
const WINDOW_INDEX: u32 = 6;
/// The window's data register.
const WINDOW_DATA: u32 = 7;
/// The first register reached through the window.
const WINDOW_FIRST: u32 = 0x1_0000;
/// The RX DMA engine's buffer addresses, one register per buffer, in the window.
const RX_CONFIG: u32 = 0x1_0000;

/// The interrupts the bridge tracks.
const INTERRUPTS: usize = 32;
/// The RX DMA engine's interrupt.
const RX_IRQ: usize = 0;
/// The SPI core's interrupt.
const SPI_IRQ: usize = 2;
/// The I2C core's interrupt.
const I2C_IRQ: usize = 6;

/// The notification ring's place in the shared file (`PHYS_NTFY_BASE`).
const NOTIFY_BASE: u32 = 0xfee0_0000;
/// Its length in bytes.
const NOTIFY_BYTES: usize = 4096;
/// Its entries, four words each.
const NOTIFY_ENTRIES: u32 = 256;
/// The DMA status words kept for RX blocks not yet received.
const DMA_STATUS_SLOTS: usize = 64;
/// The first RX buffer's place in the shared file (`PHYS_RX_DMA_ST_0`).
const RX_DMA_BASE: u32 = 0x1000_0000;
/// The distance between RX buffers (`PHYS_RX_DMA_OFF`).
const RX_DMA_STRIDE: u32 = 0x0100_0000;
/// The RX buffers: the uSDR's DMA capability (0x855) allows exactly 32, as libusdr's
/// stream code asks for.
const RX_BUFFERS: u32 = 32;
/// The largest RX block the capability allows.
const RX_MAX_BLOCK: u32 = 1 << 20;
/// The granule each RX buffer's mapping is rounded up to.
const PAGE: u32 = 4096;

/// How long a register read may take; libusdr waits forever.
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// How long an SPI transaction may take, as libusdr allows.
const SPI_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the rest of a packet may take once it has begun, whatever the caller's
/// deadline: the model sends each packet whole.
const PACKET_GRACE: Duration = Duration::from_secs(1);
/// The shortest read timeout: a poll, for a deadline already past (the socket refuses
/// zero).
const POLL: Duration = Duration::from_micros(1);
/// How long an I2C read may take, as libusdr allows.
const I2C_TIMEOUT: Duration = Duration::from_secs(1);

/// A uSDR gateware model in Verilator.
#[derive(Debug)]
pub struct VerilatorBus {
    /// The model's socket.
    socket: UnixStream,
    /// The file the host shares with the model.
    memory: File,
    /// Interrupts noted but not yet waited for, per event (the bridge's semaphores).
    pending: [u32; INTERRUPTS],
    /// The reply word of each event's last notification.
    replies: [u32; INTERRUPTS],
    /// The notification ring entry to look at next.
    next_entry: u32,
    /// The RX DMA engine's three status words per block, in arrival order.
    dma_status: [[u32; 3]; DMA_STATUS_SLOTS],
    /// Where the next block's status words go.
    next_status: usize,
    /// The open RX stream's block length in bytes.
    rx_block: Option<u32>,
    /// The number of the next RX block, counted across streams as libusdr does.
    rx_next: u32,
    /// The buffer RX blocks are copied into, kept between receives.
    rx_buffer: Vec<u8>,
    /// Whether the stream lost its framing, so nothing more can be trusted.
    broken: bool,
}

impl VerilatorBus {
    /// Connects to a uSDR model listening on `socket`, sharing `<socket>.mmap` with it, as
    /// libusdr's bridge connects: it takes the model's UUID, routes the interrupts to the
    /// notification ring, maps the ring and clears it.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] if the socket or file cannot be opened, the model's first packet is
    /// not its UUID within 10 s, or the model is not a uSDR.
    pub fn connect(socket: impl AsRef<Path>) -> Result<Self, Error> {
        let socket = socket.as_ref();
        let mut memory = socket.as_os_str().to_owned();
        memory.push(MEMORY_SUFFIX);
        let stream = UnixStream::connect(socket).map_err(|error| Error::Open(error.into()))?;
        let memory = OpenOptions::new()
            .read(true)
            .write(true)
            .open(memory)
            .map_err(|error| Error::Open(error.into()))?;
        let mut bus = Self {
            socket: stream,
            memory,
            pending: [0; INTERRUPTS],
            replies: [0; INTERRUPTS],
            next_entry: 0,
            dma_status: [[0; 3]; DMA_STATUS_SLOTS],
            next_status: 0,
            rx_block: None,
            rx_next: 0,
            rx_buffer: Vec::new(),
            broken: false,
        };
        bus.handshake().map_err(Error::Open)?;
        debug!(socket = %socket.display(), "Verilator model connected");
        Ok(bus)
    }

    /// The bridge's plugin create after the connection: the UUID, then the interrupt
    /// set-up (`verilator_wrap_plugin_create`).
    fn handshake(&mut self) -> Result<(), BusError> {
        let hello = self.receive(Instant::now() + READ_TIMEOUT)?;
        if hello.kind != DEVICE_UUID || hello.len != USDR_UUID.len() {
            return Err(self.protocol("the model's first packet is not its UUID"));
        }
        if hello.payload[..USDR_UUID.len()] != USDR_UUID {
            return Err(BusError::Unsupported("the Verilator model is not a uSDR"));
        }
        // Every interrupt line to the notification ring.
        for line in 0..u32::try_from(INTERRUPTS).expect("32 interrupts") {
            self.write_word(INTERRUPT_CONFIG, line | 1 << 16)?;
        }
        self.write_word(NOTIFY_CONFIG, NOTIFY_BASE)?;
        for bucket in 0..4 {
            self.write_word(NOTIFY_ACK, bucket << 16)?;
        }
        let ring = u32::try_from(NOTIFY_BYTES).expect("4 KiB");
        self.send(MAP_REGION, 0, &[NOTIFY_BASE, ring, MAP_WRITE])?;
        // All ones, so no entry reads as written in the first pass's phase.
        self.memory
            .write_all_at(&[0xff; NOTIFY_BYTES], u64::from(NOTIFY_BASE))
            .map_err(|error| self.fail(error.into()))
    }

    /// Sends one packet.
    fn send(&mut self, kind: u8, tag: u8, words: &[u32]) -> Result<(), BusError> {
        let length = HEADER_BYTES + 4 * words.len();
        let Ok(size) = u16::try_from(length) else {
            return Err(BusError::Unsupported("a packet longer than 64 KiB"));
        };
        let mut packet = Vec::with_capacity(length);
        packet.extend([kind, tag]);
        packet.extend(size.to_ne_bytes());
        for word in words {
            packet.extend(word.to_ne_bytes());
        }
        self.socket
            .write_all(&packet)
            .map_err(|error| self.fail(error.into()))
    }

    /// Writes one register.
    fn write_word(&mut self, reg: u32, value: u32) -> Result<(), BusError> {
        self.send(MEMORY_WRITE, 0, &[reg * 4, value])
    }

    /// Reads `out.len()` (at most 64) consecutive registers from `reg` in one request.
    fn read_words(&mut self, reg: u32, out: &mut [u32]) -> Result<(), BusError> {
        let count = u32::try_from(out.len()).expect("at most 64 words");
        self.send(READ_REQUEST, READ_TAG, &[reg * 4, count])?;
        let deadline = Instant::now() + READ_TIMEOUT;
        loop {
            let received = self.receive(deadline);
            let packet = self.fatal(received)?;
            match packet.kind {
                INTERRUPT => self.notified(&packet)?,
                READ_COMPLETION if packet.tag == READ_TAG && packet.len == 4 * out.len() => {
                    for (value, word) in out.iter_mut().zip(packet.words()) {
                        *value = word;
                    }
                    return Ok(());
                }
                _ => return Err(self.protocol("an unexpected packet while reading registers")),
            }
        }
    }

    /// Waits up to `timeout` for interrupt `irq`, taking it. A timeout with nothing of a
    /// packet received keeps the bus usable.
    fn wait_interrupt(&mut self, irq: usize, timeout: Duration) -> Result<(), BusError> {
        let deadline = Instant::now() + timeout;
        while self.pending[irq] == 0 {
            let packet = self.receive(deadline)?;
            if packet.kind != INTERRUPT {
                return Err(self.protocol("an unexpected packet while waiting for an interrupt"));
            }
            self.notified(&packet)?;
        }
        self.pending[irq] -= 1;
        Ok(())
    }

    /// Takes the ring's new entries after an interrupt, as the bridge's monitor thread
    /// does (`verilator_process_recv`), and acknowledges every 32nd.
    fn notified(&mut self, packet: &Packet) -> Result<(), BusError> {
        // libusdr's check, `hdr.tag > MAX_INTERRUPTS`; the tag is otherwise unused.
        if usize::from(packet.tag) > INTERRUPTS {
            return Err(self.protocol("an interrupt tag past 32"));
        }
        let mut ring = [0; NOTIFY_BYTES];
        let read = self.memory.read_exact_at(&mut ring, u64::from(NOTIFY_BASE));
        self.fatal(read.map_err(BusError::from))?;
        let word = |index: u32| {
            let at = usize::try_from(index & 0x3ff).expect("ten bits") * 4;
            u32::from_be_bytes(ring[at..at + 4].try_into().expect("four bytes"))
        };
        let first = self.next_entry;
        let mut entry = first;
        let mut ack = None;
        while entry < first + NOTIFY_ENTRIES {
            let data = [0, 1, 2, 3].map(|offset| word(4 * entry + offset));
            if data[0] >> 31 != (entry >> 8) & 1 {
                break;
            }
            if entry % 32 == 31 {
                ack = Some((entry << 1) & 0x3ff);
            }
            let event = usize::try_from(data[0] & 0x3f).expect("six bits");
            if event >= INTERRUPTS {
                return Err(self.protocol("a notification for an event past 31"));
            }
            if event == RX_IRQ {
                self.dma_status[self.next_status] = [data[1], data[2], data[3]];
                self.next_status = (self.next_status + 1) % DMA_STATUS_SLOTS;
            } else {
                self.replies[event] = data[1];
            }
            self.pending[event] = self.pending[event].saturating_add(1);
            entry += 1;
        }
        self.next_entry = entry & 0x1ff;
        match ack {
            Some(position) => self.write_word(NOTIFY_ACK, position),
            None => Ok(()),
        }
    }

    /// Receives one packet by `deadline`.
    fn receive(&mut self, deadline: Instant) -> Result<Packet, BusError> {
        let mut header = [0; HEADER_BYTES];
        self.fill(&mut header, deadline, true)?;
        let length = usize::from(u16::from_ne_bytes([header[2], header[3]]));
        let Some(len) = length.checked_sub(HEADER_BYTES) else {
            return Err(self.protocol("a packet shorter than its header"));
        };
        if length % 4 != 0 || len > MAX_PAYLOAD {
            return Err(self.protocol("a packet of a length the bridge refuses"));
        }
        let mut packet = Packet {
            kind: header[0],
            tag: header[1],
            len,
            payload: [0; MAX_PAYLOAD],
        };
        self.fill(&mut packet.payload[..len], deadline, false)?;
        Ok(packet)
    }

    /// Reads exactly `buf` by `deadline`. A timeout before the first byte of a packet
    /// (`starts_packet`) leaves the stream framed; anything else breaks the bus. Once a
    /// packet has begun, its rest gets at least [`PACKET_GRACE`].
    fn fill(
        &mut self,
        buf: &mut [u8],
        mut deadline: Instant,
        starts_packet: bool,
    ) -> Result<(), BusError> {
        if !starts_packet {
            deadline = deadline.max(Instant::now() + PACKET_GRACE);
        }
        let mut filled = 0;
        let mut tried = false;
        while filled < buf.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // A deadline already past still gets one short read, so a caller polling with
            // no wait sees what has arrived.
            if remaining.is_zero() && tried {
                break;
            }
            tried = true;
            let read = self
                .socket
                .set_read_timeout(Some(remaining.max(POLL)))
                .and_then(|()| self.socket.read(&mut buf[filled..]));
            match read {
                Ok(0) => return Err(self.fail(BusError::Disconnected)),
                Ok(count) => {
                    if filled == 0 {
                        deadline = deadline.max(Instant::now() + PACKET_GRACE);
                    }
                    filled += count;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    break;
                }
                Err(error) => return Err(self.fail(error.into())),
            }
        }
        if filled == buf.len() {
            Ok(())
        } else if starts_packet && filled == 0 {
            Err(BusError::Timeout)
        } else {
            Err(self.fail(BusError::Timeout))
        }
    }

    /// Fails with an error for a stream that can no longer be trusted.
    fn protocol(&mut self, reason: &'static str) -> BusError {
        self.fail(io::Error::new(ErrorKind::InvalidData, reason).into())
    }

    /// Marks the bus unusable and returns `error`.
    fn fail(&mut self, error: BusError) -> BusError {
        self.broken = true;
        error
    }

    /// `result`, marking the bus unusable if it failed: for waits whose timeout leaves an
    /// answer that may still arrive.
    fn fatal<T>(&mut self, result: Result<T, BusError>) -> Result<T, BusError> {
        result.map_err(|error| self.fail(error))
    }

    /// Refuses every operation once the stream lost its framing.
    fn usable(&self) -> Result<(), BusError> {
        if self.broken {
            Err(BusError::Disconnected)
        } else {
            Ok(())
        }
    }
}

impl Bus for VerilatorBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.usable()?;
        if addr >= WINDOW_FIRST {
            for (offset, value) in (addr - WINDOW_FIRST..).zip(out.iter_mut()) {
                self.write_word(WINDOW_INDEX, offset)?;
                self.read_words(WINDOW_DATA, slice_of(value))?;
            }
        } else if out.len() > MAX_READ_WORDS {
            return Err(BusError::Unsupported(
                "a register read of more than 64 words",
            ));
        } else if !out.is_empty() {
            self.read_words(addr, out)?;
        }
        trace!(target: BUS_TARGET, addr, values = ?out, "read_regs");
        Ok(())
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.usable()?;
        trace!(target: BUS_TARGET, addr, ?values, "write_regs");
        if addr >= WINDOW_FIRST {
            for (offset, &value) in (addr - WINDOW_FIRST..).zip(values) {
                self.write_word(WINDOW_INDEX, offset)?;
                self.write_word(WINDOW_DATA, value)?;
            }
            return Ok(());
        }
        if values.is_empty() {
            return Ok(());
        }
        let mut words = Vec::with_capacity(values.len() + 1);
        words.push(addr * 4);
        words.extend_from_slice(values);
        self.send(MEMORY_WRITE, 0, &words)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.usable()?;
        if target.0 != 0 {
            return Err(BusError::Unsupported("the uSDR has one SPI bus"));
        }
        self.write_word(SPI, word)?;
        let waited = self.wait_interrupt(SPI_IRQ, SPI_TIMEOUT);
        self.fatal(waited)?;
        let read = self.replies[SPI_IRQ];
        trace!(target: BUS_TARGET, word, read, "spi32");
        Ok(read)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.usable()?;
        if dev.core() != 0 || dev.bus() != 0 {
            return Err(BusError::Unsupported(
                "the Verilator bridge reaches I2C bus 0 of core 0 only",
            ));
        }
        if write.len() > I2C_MAX_WRITE || read.len() > I2C_MAX_READ {
            return Err(BusError::Unsupported(
                "an I2C transfer over the core's limits",
            ));
        }
        let length = u8::try_from(read.len()).expect("at most four bytes");
        self.write_word(I2C, i2c_command(dev.addr(), write, length))?;
        if !read.is_empty() {
            let waited = self.wait_interrupt(I2C_IRQ, I2C_TIMEOUT);
            self.fatal(waited)?;
            let reply = self.replies[I2C_IRQ].to_le_bytes();
            read.copy_from_slice(&reply[..read.len()]);
        }
        trace!(target: BUS_TARGET, addr = dev.addr(), ?write, ?read, "i2c");
        Ok(())
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }

    /// Points the RX DMA engine's 32 buffers at the shared file and has the model map
    /// each (`verilator_wrap_stream_initialize`).
    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.usable()?;
        if block_bytes > RX_MAX_BLOCK {
            return Err(BusError::Unsupported("an RX block over 1 MiB"));
        }
        let mapped = block_bytes.div_ceil(PAGE) * PAGE;
        for buffer in 0..RX_BUFFERS {
            let address = RX_DMA_BASE + buffer * RX_DMA_STRIDE;
            self.write_regs(RX_CONFIG + buffer, &[address])?;
            self.send(MAP_REGION, 0, &[address, mapped, MAP_WRITE])?;
        }
        self.rx_block = Some(block_bytes);
        Ok(())
    }

    /// Waits for the RX interrupt, lends the block from its buffer in the shared file, then
    /// confirms the buffer (`verilator_wrap_recv_dma_wait`, `_recv_dma_release`).
    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.usable()?;
        let Some(block) = self.rx_block else {
            return Err(BusError::Unsupported("no RX stream is open"));
        };
        self.wait_interrupt(RX_IRQ, timeout)?;
        let number = self.rx_next;
        let status = self.dma_status[usize::try_from(number & 0x3f).expect("six bits")];
        if (status[2] >> 24) & 0x3f != number & 0x3f {
            return Err(self.protocol("an RX block's status out of sequence"));
        }
        self.rx_next = number.wrapping_add(1);
        let buffer = number & 0x1f;
        let mut data = mem::take(&mut self.rx_buffer);
        data.resize(usize::try_from(block).expect("at most 1 MiB"), 0);
        let place = u64::from(RX_DMA_BASE + buffer * RX_DMA_STRIDE);
        let read = self.memory.read_exact_at(&mut data, place);
        self.fatal(read.map_err(BusError::from))?;
        consume(
            &data,
            [
                u64::from(status[1]) << 32 | u64::from(status[0]),
                u64::from(status[2]),
            ],
        );
        self.rx_buffer = data;
        self.write_regs(RX_CONFIRM, &[buffer])
    }

    /// Forgets the stream: the bridge has nothing to tell the model.
    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        self.rx_block = None;
        Ok(())
    }
}

/// The `tracing` target of every bus operation, at TRACE level.
const BUS_TARGET: &str = "usdr::bus";

/// The I2C core's command word, as the bridge makes it: `si2c_make_ctrl_reg` with the
/// device's address as the lookup-table slot, of which `MAKE_I2C_CMD` (`ipblks/si2c.h`)
/// keeps two bits. `write` holds at most three bytes and `read` is at most four.
fn i2c_command(addr: u8, write: &[u8], read: u8) -> u32 {
    let mut data = [0; 4];
    data[..write.len()].copy_from_slice(write);
    let written = u32::try_from(write.len()).expect("at most three bytes");
    u32::from(read > 0) << 31
        | u32::from(read.wrapping_sub(1) & 7) << 28
        | (written & 3) << 26
        | u32::from(addr & 3) << 24
        | u32::from_le_bytes(data) & 0x00ff_ffff
}

/// One word as a one-word slice.
fn slice_of(value: &mut u32) -> &mut [u32] {
    std::slice::from_mut(value)
}

/// A packet from the model.
struct Packet {
    /// Its type.
    kind: u8,
    /// Its tag.
    tag: u8,
    /// The bytes after the header.
    len: usize,
    /// Those bytes, from the start.
    payload: [u8; MAX_PAYLOAD],
}

impl Packet {
    /// The payload's words.
    fn words(&self) -> impl Iterator<Item = u32> + '_ {
        self.payload[..self.len]
            .chunks_exact(4)
            .map(|word| u32::from_ne_bytes(word.try_into().expect("four bytes")))
    }
}

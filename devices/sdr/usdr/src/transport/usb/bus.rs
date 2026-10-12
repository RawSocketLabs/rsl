//! Serialized USB register transactions and the uSDR's SPI/I²C completions.

use std::fmt;
use std::io;
#[cfg(feature = "mio")]
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "mio")]
use mio::{Registry, Token, Waker};

use rsl_deps::tracing::{debug, trace};
use rsl_usb::{DeviceInfo, InQueue, Interface, OutQueue};

use super::i2c::{Event, I2cTransaction};
use super::machine::{Machine, Output, Step, Submission, Transfer};
#[cfg(feature = "mio")]
use super::native::{self, NativeUsb, Session};
use super::protocol::I2cCache;
use super::rx::Rx;
#[cfg(feature = "mio")]
use crate::chips::tmp114::Tmp114;
use crate::error::Error;
#[cfg(feature = "mio")]
use crate::fpga::{Gpi, Hwid};
use crate::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use crate::portable::usb::register::{MAX_READ_WORDS, MAX_WRITE_WORDS, RegisterTransaction};
use crate::transport::window::{self, RegisterIo, Window};
/// `IN_NTFY_SIZE`: one notification transfer contains at most 64 bytes.
pub(super) const MAX_NOTIFICATION_BYTES: usize = 64;

/// One reply transfer's length: `limit` bytes rounded up to whole `packet_size` packets, as
/// the pinned C transport sizes its notification reads (`IN_NTFY_SIZE`).
pub(super) fn transfer_length(limit: usize, packet_size: usize) -> Result<usize, BusError> {
    if packet_size == 0 {
        return Err(BusError::Unsupported("USB endpoint has zero packet size"));
    }
    Ok(limit.div_ceil(packet_size) * packet_size)
}
/// The uSDR's index/data window, `s_params_m2_lm6_1_rev000`.
const WINDOW: Window = Window {
    index: 6,
    first: 0x1_0000,
};
/// Opaque notification-router configuration, `usb_uram_generic_create_and_init`.
const NOTIFICATION_CONFIG: [u32; 8] = [
    0xdead_b000,
    0xeead_b001,
    0xfead_b002,
    0xaead_b003,
    0xbead_b004,
    0xcead_b005,
    0x9ead_b006,
    0x8ead_b007,
];

/// Which IN endpoint a transaction reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reply {
    /// Raw register values, bulk endpoint 0x81.
    Registers,
    /// IRQ completion records, interrupt endpoint 0x82.
    Notification,
}

/// USB transfer boundary; the simulator substitutes whole endpoint transfers here.
pub(super) trait UsbIo: Send {
    /// Sends one register packet on bulk endpoint 0x01.
    fn write(&mut self, packet: &[u8], timeout: Duration) -> Result<(), BusError>;
    /// Receives one complete transfer, never truncating it to the expected reply length.
    fn read(&mut self, reply: Reply, timeout: Duration) -> Result<Vec<u8>, BusError>;
    /// Allocates and primes the optional RX endpoint.
    fn rx_open(&mut self, _: u32) -> Result<(), BusError> {
        Err(BusError::Unsupported("USB RX streaming"))
    }
    /// Lends a completed sample block and its two trailer words.
    fn rx_recv(&mut self, _: Duration, _: &mut dyn FnMut(&[u8], [u64; 2])) -> Result<(), BusError> {
        Err(BusError::Unsupported("USB RX streaming"))
    }
    /// Cancels and drains RX even after control-plane failure.
    fn rx_close(&mut self) -> Result<(), BusError> {
        Err(BusError::Unsupported("USB RX streaming"))
    }
    /// Waits between chip operations; the test board advances virtual time.
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
    /// Gives up every queue and returns the claimed interface for another control plane.
    #[cfg(feature = "mio")]
    fn into_interface(self: Box<Self>) -> Result<Interface, BusError> {
        Err(BusError::Unsupported(
            "this USB transport cannot hand over its device",
        ))
    }
}

/// The three claimed control-plane queues; their handles keep the interface/device alive.
struct Queues {
    /// Retains the claim so RX can be opened lazily.
    interface: Interface,
    /// Bulk sample endpoint 0x83, including failed streams awaiting cleanup.
    rx: Option<Rx<InQueue>>,
    /// Register command endpoint 0x01.
    commands: OutQueue,
    /// Raw register response endpoint 0x81.
    registers: InQueue,
    /// Notification endpoint 0x82 (an interrupt endpoint, not bulk).
    notifications: InQueue,
}

impl UsbIo for Queues {
    #[cfg(feature = "mio")]
    fn into_interface(self: Box<Self>) -> Result<Interface, BusError> {
        if self.rx.is_some() {
            return Err(BusError::Unsupported(
                "stop the RX stream before handing over the USB device",
            ));
        }
        // The command, register and notification queues close here, before another
        // control plane opens the same endpoints.
        let Self { interface, .. } = *self;
        Ok(interface)
    }

    fn rx_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        Rx::open_endpoint(&mut self.rx, block_bytes, || {
            self.interface.in_queue(0x83).map_err(BusError::from)
        })
    }

    fn rx_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        let Some(rx) = self.rx.as_mut() else {
            return Err(BusError::Unsupported("USB RX is closed"));
        };
        rx.receive(timeout, consume)
    }

    fn rx_close(&mut self) -> Result<(), BusError> {
        Rx::close_endpoint(&mut self.rx)
    }
    fn write(&mut self, packet: &[u8], timeout: Duration) -> Result<(), BusError> {
        self.commands.write(packet, timeout).map_err(BusError::from)
    }

    fn read(&mut self, reply: Reply, timeout: Duration) -> Result<Vec<u8>, BusError> {
        let (queue, limit) = match reply {
            Reply::Registers => (&mut self.registers, MAX_READ_WORDS * 4),
            Reply::Notification => (&mut self.notifications, MAX_NOTIFICATION_BYTES),
        };
        let length = transfer_length(limit, queue.max_packet_size())?;
        let buffer = queue.alloc(length).map_err(BusError::from)?;
        queue
            .submit(buffer)
            .map_err(|rejected| BusError::from(rejected.error))?;
        let Some(done) = queue.wait(timeout).map_err(BusError::from)? else {
            // The bus is poisoned by the caller regardless of whether cancellation succeeds.
            // Queue Drop still owns any transfer not returned by the kernel.
            let _ = queue.cancel_all();
            return Err(BusError::Timeout);
        };
        done.status
            .map_err(rsl_usb::Error::from)
            .map_err(BusError::from)?;
        Ok(done.buffer.to_vec())
    }
}

/// A uSDR reached over USB. Owns interface 0 and its serialized control-plane queues.
///
/// Supports register/SPI/I²C operations and RX streaming. An ambiguous control transaction
/// failure permanently disables this handle; reopen it instead of reusing late replies.
pub struct UsbBus {
    /// Real queues, or the hardware-free transfer boundary.
    io: Box<dyn UsbIo>,
    /// False once the command/response association may have been lost.
    healthy: bool,
    /// Expected next global notification number; the first record establishes the baseline.
    next_sequence: Option<u16>,
    /// The simple I²C core's address table.
    i2c: I2cCache,
    /// Never-reused identity for each prepared I²C operation in this session; carried over
    /// to a native handle.
    next_i2c_id: u64,
    /// Never-reused identity for each register/SPI operation in this session.
    next_register_id: u64,
}

impl fmt::Debug for UsbBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UsbBus")
            .field("healthy", &self.healthy)
            .finish_non_exhaustive()
    }
}

impl UsbBus {
    /// Enumerates uSDRs with the old (`faee:dea9`) or new (`3727:1001`) ID pair.
    ///
    /// # Errors
    /// [`Error::Open`] if USB enumeration fails. Does not open/reset any device.
    pub fn discover() -> Result<Vec<DeviceInfo>, Error> {
        rsl_usb::devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .filter(|info| Self::supported(info.vendor_id(), info.product_id()))
                    .collect()
            })
            .map_err(|error| Error::Open(error.into()))
    }

    /// Opens and resets this uSDR, claims interface 0, and configures USB notification routing.
    /// Pass the result to [`crate::Device::with_bus`] to identify and power up its chips.
    ///
    /// # Errors
    /// [`Error::Open`] if the ID is not a uSDR, another process owns its interface,
    /// the USB endpoints cannot be opened, `PCIe` owns interrupt routing, or setup fails.
    pub fn open(info: &DeviceInfo) -> Result<Self, Error> {
        if !Self::supported(info.vendor_id(), info.product_id()) {
            return Err(Error::Open(BusError::Unsupported(
                "USB device is not a uSDR",
            )));
        }
        let device = info
            .open()
            .and_then(rsl_usb::Device::reset)
            .map_err(|error| Error::Open(error.into()))?;
        let interface = device
            .claim_interface(0)
            .map_err(|error| Error::Open(error.into()))?;
        let queues = Queues {
            interface: interface.clone(),
            rx: None,
            commands: interface
                .out_queue(0x01)
                .map_err(|error| Error::Open(error.into()))?,
            registers: interface
                .in_queue(0x81)
                .map_err(|error| Error::Open(error.into()))?,
            notifications: interface
                .in_queue(0x82)
                .map_err(|error| Error::Open(error.into()))?,
        };
        // A 256-byte readback must end in a short bulk packet. With full-speed MPS=64,
        // valid word counts divisible by 16 could otherwise wait forever for a ZLP.
        if queues.registers.max_packet_size() <= MAX_READ_WORDS * 4 {
            return Err(Error::Open(BusError::Unsupported(
                "USB readback requires a high-speed endpoint (packet size over 256 bytes)",
            )));
        }
        let mut bus = Self::from_io(Box::new(queues));
        bus.initialize().map_err(Error::Open)?;
        debug!(port = %info.port_path(), "USB control plane opened");
        Ok(bus)
    }

    /// USB identity mapping, `lowlevel/libusb_vidpid_map.h` at the pinned reference.
    fn supported(vendor: u16, product: u16) -> bool {
        matches!((vendor, product), (0xfaee, 0xdea9) | (0x3727, 0x1001))
    }

    /// Constructs an unexposed bus before its notification router is initialized.
    pub(super) fn from_io(io: Box<dyn UsbIo>) -> Self {
        Self {
            io,
            healthy: true,
            next_sequence: None,
            i2c: I2cCache::default(),
            next_i2c_id: 0,
            next_register_id: 0,
        }
    }

    /// Hands this bus's device to a [`NativeUsb`] driven by the caller's Mio reactor, whose
    /// `registry` registers it under `token`. `waker` is the reactor's shared [`Waker`], which
    /// the handle's worker uses to report.
    ///
    /// Reads the board revision and checks the TMP114's ID first, over this blocking bus, as
    /// [`Device`](crate::Device) identifies a board: a missing or wrong sensor fails here
    /// rather than as a plausible reading, and the revision decides what
    /// [`ReferenceClock::Board`](crate::ReferenceClock::Board) selects. The notification
    /// sequence, I²C table and operation identities carry over. The device must be exclusive to the handle: its
    /// readiness makes the reactor the device's only completion driver. This blocks (sensor
    /// I/O, worker start), so call it before driving the reactor, not on its thread. The bus
    /// is consumed even on error; the device is then released, possibly by a worker shortly
    /// afterwards, so a reopen can briefly find it busy.
    ///
    /// # Errors
    /// [`Error::UnsupportedRevision`] outside revisions 1 to 3; [`Error::ChipId`] or
    /// [`Error::Bus`] from the revision read or sensor check; [`Error::Open`] if the bus
    /// lost synchronization, an RX stream is open or awaiting cleanup, the endpoints cannot be
    /// reopened, the device already has a reactor, or registration fails.
    #[cfg(feature = "mio")]
    pub fn into_native(
        self,
        registry: &Registry,
        token: Token,
        waker: Arc<Waker>,
    ) -> Result<NativeUsb, Error> {
        let (session, io) = self.hand_over()?;
        let interface = io.into_interface().map_err(Error::Open)?;
        let queues = native::Queues::new(interface).map_err(Error::Open)?;
        NativeUsb::attach(queues, session, registry, token, waker).map_err(Error::Open)
    }

    /// Reads the revision and checks the sensor over this bus, then gives up its
    /// control-plane session and transport.
    #[cfg(feature = "mio")]
    pub(super) fn hand_over(mut self) -> Result<(Session, Box<dyn UsbIo>), Error> {
        self.ready().map_err(Error::Open)?;
        let revision = Hwid::from_raw(Gpi::Hwid.read(&mut self)?).revision();
        if !(1..=3).contains(&revision) {
            return Err(Error::UnsupportedRevision(revision));
        }
        Tmp114::usdr().check_id(&mut self)?;
        let session = Session {
            i2c: self.i2c,
            next_sequence: self.next_sequence,
            next_id: self.next_i2c_id,
            rev3: revision == 3,
        };
        Ok((session, self.io))
    }

    /// Matches `usb_uram_generic_create_and_init` before the board's power-up sequence.
    pub(super) fn initialize(&mut self) -> Result<(), BusError> {
        let hwid = self.read(19)?;
        if hwid & 0x4000_0000 == 0 {
            // C skips USB routing setup in this mode. No USB SPI/I²C completions are assured:
            // do not steal routing from a live PCIe driver or silently wait on the wrong path.
            return Err(BusError::Unsupported(
                "PCIe owns the uSDR interrupt routing",
            ));
        }
        self.write(15, (1 << 2) | (1 << 6))?;
        for value in NOTIFICATION_CONFIG {
            self.write(8, value)?;
        }
        for event in 0..8 {
            self.write(15, event | event << 8 | 1 << 16 | 7 << 20)?;
            self.write(9, event << 16)?;
        }
        self.write(0, (15 << 24) | 0x80)?;
        self.write(6, 0xe0)?;
        self.write(7, u32::MAX)?;
        Ok(())
    }

    /// Rejects use after loss of reply association, before writing any new command.
    fn ready(&self) -> Result<(), BusError> {
        if self.healthy {
            Ok(())
        } else {
            Err(BusError::Unsupported(
                "USB control plane lost synchronization; reopen it",
            ))
        }
    }

    /// Records a malformed reply as a permanent transport failure.
    fn malformed(&mut self, reason: &'static str) -> BusError {
        self.healthy = false;
        BusError::Io(io::Error::new(io::ErrorKind::InvalidData, reason))
    }

    /// Runs one prepared control machine to its result: the blocking executor. Any failure
    /// once it has started poisons the bus, since a late reply could still arrive.
    fn run(&mut self, mut machine: Machine) -> Result<Output, BusError> {
        self.ready()?;
        let result = self.pump(&mut machine);
        if result.is_err() {
            self.healthy = false;
        }
        result
    }

    /// Performs each transfer the machine asks for and reports its completion.
    fn pump(&mut self, machine: &mut Machine) -> Result<Output, BusError> {
        // The machine was prepared at time zero; this pump supplies its clock.
        let started = Instant::now();
        let mut step = machine.start(started.elapsed())?;
        loop {
            let Submission {
                id,
                transfer,
                timeout,
            } = match step {
                Step::Submit(submission) => submission,
                Step::Done(output) => return Ok(output),
            };
            step = match transfer {
                Transfer::Write(frame) => {
                    trace!(target: "rsl_usdr::bus", bytes = ?frame.as_bytes(), "USB command");
                    let event = match self.io.write(frame.as_bytes(), timeout) {
                        Ok(()) => Event::Written { id },
                        Err(error) => Event::Failed { id, error },
                    };
                    machine.complete(event, started.elapsed(), &mut self.next_sequence)
                }
                Transfer::Readback => self.deliver(machine, id, Reply::Registers, timeout, started),
                Transfer::Notification => {
                    self.deliver(machine, id, Reply::Notification, timeout, started)
                }
            }?;
        }
    }

    /// Receives one whole reply transfer and reports it, or its failure, to the machine.
    fn deliver(
        &mut self,
        machine: &mut Machine,
        id: u64,
        reply: Reply,
        timeout: Duration,
        started: Instant,
    ) -> Result<Step, BusError> {
        let received = self.io.read(reply, timeout);
        let now = started.elapsed();
        let sequence = &mut self.next_sequence;
        match received {
            Ok(packet) => {
                let packet = packet.as_slice();
                let event = match reply {
                    Reply::Registers => Event::Readback { id, packet },
                    Reply::Notification => Event::Notification { id, packet },
                };
                machine.complete(event, now, sequence)
            }
            Err(error) => machine.complete(Event::Failed { id, error }, now, sequence),
        }
    }

    /// The next register/SPI operation identity; exhaustion poisons the bus.
    fn register_id(&mut self) -> Result<u64, BusError> {
        let id = self.next_register_id;
        let Some(next) = id.checked_add(1) else {
            self.healthy = false;
            return Err(BusError::Unsupported(
                "USB register operation identity exhausted; reopen it",
            ));
        };
        self.next_register_id = next;
        Ok(id)
    }

    /// Reads a checked block of 1–64 physical registers; no length header is present.
    fn raw_read(&mut self, address: u16, out: &mut [u32]) -> Result<(), BusError> {
        self.ready()?;
        let id = self.register_id()?;
        let read = RegisterTransaction::read(id, address, out.len(), Duration::ZERO)?;
        let Output::Words { words, len } = self.run(Machine::Register(read))? else {
            return Err(self.malformed("USB register read completed as another operation"));
        };
        out.copy_from_slice(&words[..len]);
        Ok(())
    }

    /// Writes a checked block of 1–63 words, including a four-byte command header.
    fn raw_write(&mut self, address: u16, values: &[u32]) -> Result<(), BusError> {
        self.ready()?;
        let id = self.register_id()?;
        let write = RegisterTransaction::write(id, address, values, Duration::ZERO)?;
        self.run(Machine::Register(write)).map(|_| ())
    }

    /// Validates the whole caller-supplied span before making any bus access.
    fn span(address: u32, count: usize) -> Result<(), BusError> {
        if count > 0
            && u32::try_from(count - 1)
                .ok()
                .and_then(|last| address.checked_add(last))
                .is_none()
        {
            return Err(BusError::Unsupported(
                "register span wraps the address space",
            ));
        }
        Ok(())
    }
}

impl RegisterIo for UsbBus {
    fn read(&mut self, reg: u32) -> Result<u32, BusError> {
        let address = u16::try_from(reg)
            .map_err(|_| BusError::Unsupported("physical register over 16 bits"))?;
        let mut values = [0];
        self.raw_read(address, &mut values)?;
        Ok(values[0])
    }

    fn write(&mut self, reg: u32, value: u32) -> Result<(), BusError> {
        let address = u16::try_from(reg)
            .map_err(|_| BusError::Unsupported("physical register over 16 bits"))?;
        self.raw_write(address, &[value])
    }

    fn write_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), BusError> {
        let address = u16::try_from(reg)
            .map_err(|_| BusError::Unsupported("physical register over 16 bits"))?;
        self.raw_write(address, &[first, second])
    }
}

impl Bus for UsbBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.ready()?;
        Self::span(addr, out.len())?;
        let mut address = addr;
        let mut rest = out;
        while !rest.is_empty() {
            if address >= WINDOW.first {
                return window::read_regs(self, WINDOW, address, rest);
            }
            let count = rest
                .len()
                .min(MAX_READ_WORDS)
                .min(usize::try_from(WINDOW.first - address).expect("16-bit space"));
            let (chunk, tail) = rest.split_at_mut(count);
            self.raw_read(u16::try_from(address).expect("physical register"), chunk)?;
            address += u32::try_from(count).expect("64-word block");
            rest = tail;
        }
        Ok(())
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.ready()?;
        Self::span(addr, values.len())?;
        let mut address = addr;
        let mut rest = values;
        while !rest.is_empty() {
            if address >= WINDOW.first {
                return window::write_regs(self, WINDOW, address, rest);
            }
            let count = rest
                .len()
                .min(MAX_WRITE_WORDS)
                .min(usize::try_from(WINDOW.first - address).expect("16-bit space"));
            let (chunk, tail) = rest.split_at(count);
            self.raw_write(u16::try_from(address).expect("physical register"), chunk)?;
            address += u32::try_from(count).expect("63-word block");
            rest = tail;
        }
        Ok(())
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.ready()?;
        let id = self.register_id()?;
        let exchange = RegisterTransaction::spi32(id, target, word, Duration::ZERO)?;
        match self.run(Machine::Register(exchange))? {
            Output::Words { words, len: 1 } => Ok(words[0]),
            _ => Err(self.malformed("USB SPI exchange completed without one reply word")),
        }
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.ready()?;
        // This compatibility pump blocks; event/callback adapters use the same pure machine.
        let id = self.next_i2c_id;
        let Some(next_id) = id.checked_add(1) else {
            self.healthy = false;
            return Err(BusError::Unsupported(
                "USB I2C operation identity exhausted; reopen it",
            ));
        };
        let transaction =
            I2cTransaction::new(id, dev, write, read.len(), &mut self.i2c, Duration::ZERO)?;
        self.next_i2c_id = next_id;
        let Output::I2c { bytes, len } = self.run(Machine::I2c(transaction))? else {
            return Err(self.malformed("USB I2C transfer completed as another operation"));
        };
        read.copy_from_slice(&bytes[..len]);
        Ok(())
    }

    fn sleep(&mut self, duration: Duration) {
        self.io.sleep(duration);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.ready()?;
        self.io.rx_open(block_bytes)
    }

    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        self.ready()?;
        self.io.rx_recv(timeout, consume)
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        self.io.rx_close()
    }
}

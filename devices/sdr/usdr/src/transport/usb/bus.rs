//! Serialized USB register transactions and the uSDR's SPI/I²C completions.

use std::fmt;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use rsl_deps::tracing::{debug, trace};
use rsl_usb::{DeviceInfo, InQueue, Interface, OutQueue};

use super::protocol::{Command, I2cCache, Notification};
use super::rx::Rx;
use crate::error::Error;
use crate::lowlevel::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
use crate::transport::window::{self, RegisterIo, Window};

/// The reference's `USB_IO_TIMEOUT`, in `usb_uram_generic.h` (milliseconds).
const REGISTER_TIMEOUT: Duration = Duration::from_secs(20);
/// The reference's `usb_read_bus` event wait.
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(1);
/// `OUT_REGOUT_SIZE` includes the command word: 63 payload words fit in 256 bytes.
const MAX_WRITE_WORDS: usize = 63;
/// `MAX_REQUEST_RB_SIZE`: 256 bytes of raw readback.
const MAX_READ_WORDS: usize = 64;
/// `IN_NTFY_SIZE`: one notification transfer contains at most 64 bytes.
const MAX_NOTIFICATION_BYTES: usize = 64;
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
    fn rx_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        if self.rx.is_some() {
            return Err(BusError::Unsupported(
                "USB RX already open or awaiting cleanup",
            ));
        }
        let queue = self.interface.in_queue(0x83).map_err(BusError::from)?;
        let rx = self.rx.insert(Rx::new(queue));
        if let Err(error) = rx.open(block_bytes) {
            // Preserve ownership on a failed drain, so reopen cannot steal late transfers.
            if rx.close().is_ok() {
                self.rx = None;
            }
            return Err(error);
        }
        Ok(())
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
        if let Some(rx) = self.rx.as_mut() {
            rx.close()?;
            self.rx = None;
        }
        Ok(())
    }
    fn write(&mut self, packet: &[u8], timeout: Duration) -> Result<(), BusError> {
        self.commands.write(packet, timeout).map_err(BusError::from)
    }

    fn read(&mut self, reply: Reply, timeout: Duration) -> Result<Vec<u8>, BusError> {
        let (queue, limit) = match reply {
            Reply::Registers => (&mut self.registers, MAX_READ_WORDS * 4),
            Reply::Notification => (&mut self.notifications, MAX_NOTIFICATION_BYTES),
        };
        let packet_size = queue.max_packet_size();
        if packet_size == 0 {
            return Err(BusError::Unsupported("USB endpoint has zero packet size"));
        }
        let length = limit.div_ceil(packet_size) * packet_size;
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
        }
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

    /// Sends one packet; any ambiguous write failure poisons the bus.
    fn send(&mut self, packet: &[u8]) -> Result<(), BusError> {
        self.ready()?;
        trace!(target: "rsl_usdr::bus", bytes = ?packet, "USB command");
        let result = self.io.write(packet, REGISTER_TIMEOUT);
        if result.is_err() {
            self.healthy = false;
        }
        result
    }

    /// Receives a full transfer; a failed read cannot be reused by another request.
    fn receive(&mut self, reply: Reply, timeout: Duration) -> Result<Vec<u8>, BusError> {
        let result = self.io.read(reply, timeout);
        if result.is_err() {
            self.healthy = false;
        }
        result
    }

    /// Reads a checked block of 1–64 physical registers; no length header is present.
    fn raw_read(&mut self, address: u16, out: &mut [u32]) -> Result<(), BusError> {
        let count = u8::try_from(out.len()).expect("register reads split into 64-word blocks");
        self.send(&Command::encode(address, count, true))?;
        let bytes = self.receive(Reply::Registers, REGISTER_TIMEOUT)?;
        if bytes.len() != out.len() * 4 {
            return Err(self.malformed("USB register readback length differs from request"));
        }
        for (value, bytes) in out.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_le_bytes(bytes.try_into().expect("four-byte chunks"));
        }
        Ok(())
    }

    /// Writes a checked block of 1–63 words, including a four-byte command header.
    fn raw_write(&mut self, address: u16, values: &[u32]) -> Result<(), BusError> {
        let count = u8::try_from(values.len()).expect("register writes split into 63-word blocks");
        let mut bytes = Vec::with_capacity((values.len() + 1) * 4);
        bytes.extend_from_slice(&Command::encode(address, count, false));
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        self.send(&bytes)
    }

    /// Waits for one scalar IRQ reply, checking the entire packet including its tail.
    fn completion(&mut self, event: u8) -> Result<u32, BusError> {
        let started = Instant::now();
        loop {
            let Some(timeout) = COMPLETION_TIMEOUT.checked_sub(started.elapsed()) else {
                self.healthy = false;
                return Err(BusError::Timeout);
            };
            let packet = self.receive(Reply::Notification, timeout)?;
            if packet.is_empty() || packet.len() > MAX_NOTIFICATION_BYTES || packet.len() % 4 != 0 {
                return Err(self.malformed("invalid USB notification transfer length"));
            }
            let mut rest = packet.as_slice();
            let mut value = None;
            while !rest.is_empty() {
                if rest.len() < 8 {
                    return Err(self.malformed("truncated USB notification"));
                }
                let header = Notification::from_raw(u32::from_le_bytes(
                    rest[..4].try_into().expect("notification header checked"),
                ));
                let sequence = header.sequence();
                if self
                    .next_sequence
                    .is_some_and(|expected| sequence != expected)
                {
                    return Err(self.malformed("USB notification sequence gap"));
                }
                self.next_sequence = Some(sequence.wrapping_add(1));
                let length = (usize::from(header.extra_words().value()) + 2) * 4;
                if length > rest.len() || header.event().value() >= 32 {
                    return Err(self.malformed("invalid USB notification record"));
                }
                if header.event().value() == event {
                    if header.extra_words().value() != 0 || value.is_some() {
                        return Err(self.malformed("ambiguous USB control completion"));
                    }
                    value = Some(u32::from_le_bytes(
                        rest[4..8].try_into().expect("scalar payload checked"),
                    ));
                }
                rest = &rest[length..];
            }
            if let Some(value) = value {
                return Ok(value);
            }
        }
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
        if target.0 != 0 {
            return Err(BusError::Unsupported("uSDR has only simple SPI bus 0"));
        }
        self.write(2, word)?;
        self.completion(2)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.ready()?;
        if dev.bus() > 1 || write.len() > I2C_MAX_WRITE || read.len() > I2C_MAX_READ {
            return Err(BusError::Unsupported(
                "I2C transfer over the uSDR core limits",
            ));
        }
        let command = self.i2c.command(
            dev.addr(),
            dev.bus(),
            write,
            u8::try_from(read.len()).expect("four I2C input bytes"),
        );
        self.raw_write(0, &command)?;
        if !read.is_empty() {
            let bytes = self.completion(6)?.to_le_bytes();
            read.copy_from_slice(&bytes[..read.len()]);
        }
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

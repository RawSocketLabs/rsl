//! An open uSDR on the `usdr_pcie_uram` driver: register access through the mapped BAR,
//! SPI and I2C through the driver, and RX DMA through mapped buffers.
//!
//! Source for the protocol: libusdr's `pcie_uram_main.c` (MIT); interface facts from the
//! driver, `usdr_pcie_uram.c`.

use std::ffi::c_void;
use std::fs;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};
use std::slice;
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags};
use rustix::io::Errno;
use rustix::ioctl::{Ioctl, IoctlOutput, Opcode};
use rustix::mm::{MapFlags, ProtFlags};

use crate::layout::{BoardLayout, LayoutTooLarge};
use crate::ring::{BUFFER_COUNT, BUFFERS, MAX_RECORDS, Ring};
use crate::uapi::{self, SdmaConf, Si2c, Spi32, Uuid, WaitOob};

/// Bytes of BAR0 the driver maps: one page, 1024 registers.
const BAR_BYTES: usize = 4096;
/// Registers reachable through the mapped page.
const BAR_REGS: u32 = 1024;
/// Most bytes one I2C transfer writes, and reads (the FPGA core's limits).
const I2C_MAX: (usize, usize) = (3, 4);
/// The pause libusdr makes before every I2C transfer. The driver waits for completion only
/// when reading, so this is all that paces back-to-back writes.
const I2C_PAUSE: Duration = Duration::from_millis(1);
/// The longest DMA wait asked of the driver at once, in milliseconds: its field holds 24 bits,
/// but it converts to ticks in 32-bit arithmetic (`ms * HZ / 1000`), which wraps above this
/// at `HZ` 1000. Longer waits are split.
const MAX_WAIT_MS: u128 = 4_294_967;
/// The shortest nonzero DMA wait asked of the driver, in milliseconds: it truncates to whole
/// ticks, so anything under one tick (4 ms at `HZ` 250, 10 ms at 100) would not wait at all.
const MIN_WAIT_MS: u128 = 10;
/// The RX stream's number.
const RX_STREAM: u32 = 0;

/// The device could not be opened.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenError {
    /// Another process has the device open; the driver allows one.
    #[error("the device is open elsewhere")]
    Busy,

    /// The driver reports the device in a PCI error state.
    #[error("the device is in a PCI error state")]
    PciError,

    /// The device is a different board.
    #[error("not the expected board: UUID {found:02x?}")]
    WrongBoard {
        /// The UUID the driver reported.
        found: [u8; 16],
    },

    /// The driver does not speak interface version 3.
    #[error("unsupported usdr_pcie_uram driver version")]
    DriverVersion,

    /// The board layout does not fit the driver's structure.
    #[error(transparent)]
    Layout(#[from] LayoutTooLarge),

    /// Any other failure, from opening the node to mapping the registers.
    #[error("cannot open the device")]
    Io(#[source] io::Error),
}

/// An operation on an open device failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PcieError {
    /// No DMA block arrived in time.
    #[error("timed out")]
    Timeout,

    /// The device went away.
    #[error("device disconnected")]
    Disconnected,

    /// An SPI or I2C transfer timed out or was interrupted. The driver then hands the next
    /// transfer this one's late completion, so the device refuses further transfers.
    #[error("an earlier SPI or I2C transfer did not complete; reopen the device")]
    Poisoned,

    /// A register outside the mapped page.
    #[error("register {0:#x} is outside the mapped registers")]
    OutOfRange(u32),

    /// A request the device cannot make.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    /// Any other driver failure.
    #[error("driver request failed")]
    Io(#[source] io::Error),
}

impl From<Errno> for PcieError {
    fn from(errno: Errno) -> Self {
        match errno {
            Errno::TIMEDOUT => Self::Timeout,
            // After a hot unplug the driver answers every request with EIO.
            Errno::IO => Self::Disconnected,
            other => Self::Io(other.into()),
        }
    }
}

/// The uSDR device nodes the driver has created (`/dev/usdrN` for each
/// `/sys/class/usdr/usdrN`), in name order.
#[must_use]
pub fn discover() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir("/sys/class/usdr") else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| Path::new("/dev").join(entry.file_name()))
        .collect();
    nodes.sort();
    nodes
}

/// An open uSDR.
///
/// Dropping it unmaps any DMA buffers without releasing them to the driver, which frees
/// them only from [`PcieDevice::dma_close`]: freeing them while the gateware may still
/// write would hand it freed pages. The next stream setup reuses them.
#[derive(Debug)]
pub struct PcieDevice {
    /// The RX DMA buffers, while a stream is set up.
    dma: Option<Dma>,

    /// The mapped register page.
    bar: Bar,

    /// The device node, closed last.
    fd: OwnedFd,

    /// An SPI or I2C transfer failed in a way that desynchronises the driver.
    poisoned: bool,
}

// SAFETY: the mappings are owned by this value alone and only accessed through `&mut self`;
// nothing in them is tied to the thread that created them.
unsafe impl Send for PcieDevice {}

impl PcieDevice {
    /// Opens the device at `path` as `board`: checks its UUID, claims driver interface
    /// version 3, resets the driver's I2C address cache, sends the layout and maps the
    /// registers.
    ///
    /// Unlike libusdr, this resets the I2C cache (`PCIE_DRIVER_CLAIM`): the driver keeps it
    /// across opens, and after an FPGA reset a stale entry could address the wrong chip.
    ///
    /// # Errors
    ///
    /// [`OpenError`] for each refusal.
    pub fn open(path: &Path, board: BoardLayout) -> Result<Self, OpenError> {
        let layout = board.to_uapi()?;
        // Never non-blocking: the driver then turns DMA waits into EAGAIN.
        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()).map_err(
            |errno| match errno {
                Errno::BUSY => OpenError::Busy,
                Errno::IO => OpenError::PciError,
                other => OpenError::Io(other.into()),
            },
        )?;
        let io = |errno: Errno| OpenError::Io(errno.into());

        let mut uuid = Uuid::default();
        request_with(fd.as_fd(), uapi::GET_UUID, &mut uuid).map_err(io)?;
        if uuid.id != board.uuid {
            return Err(OpenError::WrongBoard { found: uuid.id });
        }
        request_value(fd.as_fd(), uapi::CLAIM_VERSION, uapi::INTERFACE_VERSION).map_err(
            |errno| match errno {
                Errno::OPNOTSUPP => OpenError::DriverVersion,
                other => io(other),
            },
        )?;
        request_value(fd.as_fd(), uapi::CLAIM, 0).map_err(io)?;
        let mut layout = layout;
        request_with(fd.as_fd(), uapi::SET_DEVLAYOUT, &mut layout).map_err(io)?;
        let bar = Bar::map(fd.as_fd()).map_err(io)?;
        Ok(Self {
            dma: None,
            bar,
            fd,
            poisoned: false,
        })
    }

    /// Reads a register in the mapped page.
    ///
    /// # Errors
    ///
    /// [`PcieError::OutOfRange`] past the page.
    pub fn read_reg(&mut self, reg: u32) -> Result<u32, PcieError> {
        self.bar.read(reg)
    }

    /// Writes a register in the mapped page.
    ///
    /// # Errors
    ///
    /// [`PcieError::OutOfRange`] past the page.
    pub fn write_reg(&mut self, reg: u32, value: u32) -> Result<(), PcieError> {
        self.bar.write(reg, value)
    }

    /// Writes an even register and the next in one 64-bit store, as libusdr does for
    /// consecutive pairs.
    ///
    /// # Errors
    ///
    /// [`PcieError::OutOfRange`] past the page; [`PcieError::Unsupported`] for an odd
    /// register.
    pub fn write_reg_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), PcieError> {
        if reg % 2 != 0 {
            return Err(PcieError::Unsupported(
                "a register pair starts at an even register",
            ));
        }
        self.bar.write_pair(reg, first, second)
    }

    /// Shifts `word` through SPI core `bus` and returns the word read back
    /// (`PCIE_DRIVER_SPI32_TRANSACT`).
    ///
    /// # Errors
    ///
    /// [`PcieError::Poisoned`] after a transfer that timed out or was interrupted, this one
    /// included; any driver failure.
    pub fn spi32(&mut self, bus: u32, word: u32) -> Result<u32, PcieError> {
        self.check_poison()?;
        let mut transfer = Spi32 {
            buscfg: bus,
            dw_io: word,
        };
        self.transact(uapi::SPI32_TRANSACT, &mut transfer)?;
        Ok(transfer.dw_io)
    }

    /// Writes `write` to the I2C device at 7-bit `addr` on bus `bus`, then reads `read`
    /// (`PCIE_DRIVER_SI2C_TRANSACT`), after libusdr's 1 ms pause.
    ///
    /// # Errors
    ///
    /// [`PcieError::Unsupported`] over 3 bytes written or 4 read; [`PcieError::Poisoned`]
    /// as [`Self::spi32`]; any driver failure.
    pub fn i2c(
        &mut self,
        bus: u8,
        addr: u8,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), PcieError> {
        self.check_poison()?;
        if write.len() > I2C_MAX.0 || read.len() > I2C_MAX.1 {
            return Err(PcieError::Unsupported(
                "I2C transfer over the FPGA core's limits",
            ));
        }
        let mut transfer = Si2c {
            // libusdr's I2C address: instance 0 in bits 31:24, bus in 23:16, device below.
            addr: u32::from(bus) << 16 | u32::from(addr),
            wcnt: count(write.len()),
            rcnt: count(read.len()),
            wrb: [0; 8],
            rdb: [0; 8],
            wrb_p: ptr::null_mut(),
            rdb_p: ptr::null_mut(),
        };
        transfer.wrb[..write.len()].copy_from_slice(write);
        thread::sleep(I2C_PAUSE);
        self.transact(uapi::SI2C_TRANSACT, &mut transfer)?;
        read.copy_from_slice(&transfer.rdb[..read.len()]);
        Ok(())
    }

    /// Sets up the RX stream's 32 DMA buffers of `block_bytes` each and maps them
    /// (`PCIE_DRIVER_DMA_CONF`).
    ///
    /// # Errors
    ///
    /// [`PcieError::Unsupported`] if a stream is set up; any driver failure, after which no
    /// buffer stays mapped.
    pub fn dma_open(&mut self, block_bytes: u32) -> Result<(), PcieError> {
        if self.dma.is_some() {
            return Err(PcieError::Unsupported("the RX stream is already set up"));
        }
        let mut conf = SdmaConf {
            kind: uapi::STREAM_MMAPED,
            sno: RX_STREAM,
            dma_bufs: BUFFER_COUNT,
            dma_buf_sz: block_bytes,
            ..SdmaConf::default()
        };
        request_with(self.fd.as_fd(), uapi::DMA_CONF, &mut conf)?;
        let too_large = || PcieError::Unsupported("DMA buffers larger than the address space");
        let buffer_len = usize::try_from(conf.out_vma_length).map_err(|_| too_large())? / BUFFERS;
        let block_bytes = usize::try_from(conf.dma_buf_sz).map_err(|_| too_large())?;
        let mut buffers = Vec::with_capacity(BUFFERS);
        for i in 0..BUFFERS {
            let mapped = i64::try_from(i * buffer_len)
                .ok()
                .and_then(|start| conf.out_vma_off.checked_add(start))
                .ok_or(Errno::INVAL)
                .and_then(|offset| Mapping::map(self.fd.as_fd(), buffer_len, offset));
            match mapped {
                Ok(mapping) => buffers.push(mapping),
                Err(errno) => {
                    drop(buffers);
                    let _ = request_value(self.fd.as_fd(), uapi::DMA_UNCONF, RX_STREAM);
                    return Err(errno.into());
                }
            }
        }
        self.dma = Some(Dma {
            buffers,
            // The driver rounds buffers up to whole pages; only the block is data.
            block_bytes,
            ring: Ring::default(),
        });
        Ok(())
    }

    /// Waits up to `timeout` for the next filled RX block, lends it to `consume` with its
    /// out-of-band record, then hands the buffer back (`PCIE_DRIVER_DMA_WAIT_OOB`,
    /// `PCIE_DRIVER_DMA_RELEASE`). The buffer is handed back even if `consume` panics.
    ///
    /// # Errors
    ///
    /// [`PcieError::Timeout`] if no block arrives in time; [`PcieError::Unsupported`] with
    /// no stream set up; any driver failure.
    pub fn dma_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), PcieError> {
        let fd = self.fd.as_fd();
        let Some(dma) = self.dma.as_mut() else {
            return Err(PcieError::Unsupported("no RX stream is set up"));
        };
        if dma.ring.is_empty() {
            wait_for_blocks(fd, &mut dma.ring, timeout)?;
        }
        let Some((index, record)) = dma.ring.take() else {
            return Err(PcieError::Timeout);
        };
        let release = Release {
            fd,
            released: false,
        };
        consume(&dma.buffers[index].bytes()[..dma.block_bytes], record);
        release.now()
    }

    /// Unmaps the RX buffers and releases them to the driver (`PCIE_DRIVER_DMA_UNCONF`).
    /// Call only once the stream is stopped.
    ///
    /// # Errors
    ///
    /// Any driver failure.
    pub fn dma_close(&mut self) -> Result<(), PcieError> {
        let Some(dma) = self.dma.take() else {
            return Ok(());
        };
        // Unmap first: the driver frees the pages.
        drop(dma);
        request_value(self.fd.as_fd(), uapi::DMA_UNCONF, RX_STREAM)?;
        Ok(())
    }

    /// Fails once a transfer has desynchronised the driver.
    fn check_poison(&self) -> Result<(), PcieError> {
        if self.poisoned {
            return Err(PcieError::Poisoned);
        }
        Ok(())
    }

    /// Runs an SPI or I2C transfer, poisoning the device if it times out or is interrupted.
    fn transact<T>(&mut self, opcode: Opcode, transfer: &mut T) -> Result<(), PcieError> {
        match request_with(self.fd.as_fd(), opcode, transfer) {
            Ok(_) => Ok(()),
            Err(Errno::TIMEDOUT | Errno::INTR) => {
                self.poisoned = true;
                Err(PcieError::Poisoned)
            }
            Err(errno) => Err(errno.into()),
        }
    }
}

/// The RX stream's mapped buffers.
#[derive(Debug)]
struct Dma {
    /// The 32 buffers, in the gateware's fill order.
    buffers: Vec<Mapping>,

    /// Bytes of each buffer that hold the block.
    block_bytes: usize,

    /// Which buffer comes next.
    ring: Ring,
}

/// Waits for at least one filled buffer and records the result in `ring`. A signal restarts
/// the wait with the time left: nothing was sent to the device.
fn wait_for_blocks(
    fd: BorrowedFd<'_>,
    ring: &mut Ring,
    timeout: Duration,
) -> Result<(), PcieError> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let ms = match left.as_micros().div_ceil(1000) {
            0 => 0,
            ms => ms.clamp(MIN_WAIT_MS, MAX_WAIT_MS),
        };
        let ms = u32::try_from(ms).expect("invariant: at most 24 bits");
        let mut records = [[0_u64; 2]; MAX_RECORDS];
        let mut wait = WaitOob {
            streamnoto: ms << 8 | RX_STREAM,
            ooblength: u32::try_from(size_of_val(&records)).expect("512"),
            oobdata: records.as_mut_ptr().cast(),
        };
        match request_with(fd, uapi::DMA_WAIT_OOB, &mut wait) {
            Ok(count) => {
                let count = usize::try_from(count).unwrap_or(0);
                let returned = (wait.ooblength as usize / 16).min(MAX_RECORDS);
                ring.filled(count, &records[..returned]);
                if ring.is_empty() {
                    return Err(PcieError::Timeout);
                }
                return Ok(());
            }
            Err(Errno::INTR) if !left.is_zero() => {}
            Err(errno) => return Err(errno.into()),
        }
    }
}

/// Hands a DMA buffer back to the driver when dropped, if not already.
struct Release<'fd> {
    /// The device.
    fd: BorrowedFd<'fd>,

    /// Already handed back.
    released: bool,
}

impl Release<'_> {
    /// Hands the buffer back now, reporting the result.
    fn now(mut self) -> Result<(), PcieError> {
        self.released = true;
        request_value(self.fd, uapi::DMA_RELEASE, RX_STREAM)?;
        Ok(())
    }
}

impl Drop for Release<'_> {
    fn drop(&mut self) {
        if !self.released {
            // Unwinding from `consume`: the buffer must still go back to the gateware.
            let _ = request_value(self.fd, uapi::DMA_RELEASE, RX_STREAM);
        }
    }
}

/// The mapped register page.
#[derive(Debug)]
struct Bar(Mapping);

impl Bar {
    /// Maps BAR0's first page.
    fn map(fd: BorrowedFd<'_>) -> Result<Self, Errno> {
        Mapping::map(fd, BAR_BYTES, 0).map(Self)
    }

    /// The register's address in the mapping, if it is in the page.
    fn word(&self, reg: u32) -> Result<*mut u32, PcieError> {
        if reg >= BAR_REGS {
            return Err(PcieError::OutOfRange(reg));
        }
        // SAFETY: `reg` is below 1024 words, inside the 4096-byte page.
        Ok(unsafe { self.0.base.as_ptr().cast::<u32>().add(reg as usize) })
    }

    /// Reads a register: big-endian on the bus.
    fn read(&self, reg: u32) -> Result<u32, PcieError> {
        let word = self.word(reg)?;
        // SAFETY: an aligned address inside the device page, which stays mapped while
        // `self` lives; device memory needs a volatile access.
        Ok(u32::from_be(unsafe { ptr::read_volatile(word) }))
    }

    /// Writes a register.
    fn write(&mut self, reg: u32, value: u32) -> Result<(), PcieError> {
        let word = self.word(reg)?;
        // SAFETY: as `read`.
        unsafe { ptr::write_volatile(word, value.to_be()) };
        Ok(())
    }

    /// Writes an even register and the next in one 64-bit store.
    fn write_pair(&mut self, reg: u32, first: u32, second: u32) -> Result<(), PcieError> {
        if reg % 2 != 0 {
            return Err(PcieError::Unsupported(
                "a register pair starts at an even register",
            ));
        }
        self.word(reg + 1)?;
        let word = self.word(reg)?;
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&first.to_be_bytes());
        bytes[4..].copy_from_slice(&second.to_be_bytes());
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "an even register in a page-aligned mapping is 8-byte aligned"
        )]
        let pair = word.cast::<u64>();
        // SAFETY: `reg` is even, so `pair` is 8-byte aligned, and both registers are in the
        // page (checked above); volatile as `read`.
        unsafe { ptr::write_volatile(pair, u64::from_ne_bytes(bytes)) };
        Ok(())
    }
}

/// A shared mapping of the device, unmapped when dropped.
#[derive(Debug)]
struct Mapping {
    /// The mapping's start.
    base: NonNull<c_void>,

    /// Its length in bytes.
    len: usize,
}

impl Mapping {
    /// Maps `len` bytes of the device at `offset`, readable and writable, shared.
    fn map(fd: BorrowedFd<'_>, len: usize, offset: i64) -> Result<Self, Errno> {
        let offset = u64::try_from(offset).map_err(|_| Errno::INVAL)?;
        // SAFETY: a fresh mapping at an address the kernel picks, so nothing existing is
        // replaced; the driver validates `offset` and `len`.
        let base = unsafe {
            rustix::mm::mmap(
                ptr::null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                fd,
                offset,
            )
        }?;
        let base = NonNull::new(base).ok_or(Errno::NOMEM)?;
        Ok(Self { base, len })
    }

    /// The mapping as bytes.
    fn bytes(&self) -> &[u8] {
        // SAFETY: `len` mapped, readable bytes that live as long as `self`. The gateware does
        // not overwrite a buffer it has reported filled until the host hands it back: with no
        // free buffer it drops the packet instead and counts it in the next record's lost
        // count (out-of-band word 0, bits 23:0). Nothing in the driver enforces this; it is
        // the gateware's contract, as libusdr relies on it too.
        unsafe { slice::from_raw_parts(self.base.as_ptr().cast(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the mapping came from `mmap` with this length and is not used after this.
        let _ = unsafe { rustix::mm::munmap(self.base.as_ptr(), self.len) };
    }
}

/// A transfer's byte count, already bounded by the I2C limits.
fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// One driver request whose argument points at `data`, returning the driver's return value.
fn request_with<T>(fd: BorrowedFd<'_>, opcode: Opcode, data: &mut T) -> Result<i32, Errno> {
    let request = Request {
        opcode,
        arg: ptr::from_mut(data).cast(),
    };
    // SAFETY: every opcode passed here takes a pointer to exactly `T` (see `uapi`), which
    // stays valid and exclusively borrowed for the call.
    unsafe { rustix::ioctl::ioctl(fd, request) }
}

/// One driver request whose argument is `value` itself, not a pointer.
fn request_value(fd: BorrowedFd<'_>, opcode: Opcode, value: u32) -> Result<i32, Errno> {
    let request = Request {
        opcode,
        arg: ptr::without_provenance_mut(value as usize),
    };
    // SAFETY: every opcode passed here takes its argument as an integer (see `uapi`); the
    // driver never dereferences it.
    unsafe { rustix::ioctl::ioctl(fd, request) }
}

/// A driver request: an opcode and its argument.
struct Request {
    /// The request code.
    opcode: Opcode,

    /// A pointer to the request's structure, or an integer in pointer form.
    arg: *mut c_void,
}

// SAFETY: `opcode` and `arg` are paired by `request_with` and `request_value`, whose callers
// uphold the argument's type; the driver may write through pointer arguments, so the request
// is mutating, and the output is the raw return value, which carries no borrowed data.
unsafe impl Ioctl for Request {
    type Output = IoctlOutput;

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.arg
    }

    unsafe fn output_from_ptr(
        out: IoctlOutput,
        _extract_output: *mut c_void,
    ) -> rustix::io::Result<Self::Output> {
        Ok(out)
    }
}

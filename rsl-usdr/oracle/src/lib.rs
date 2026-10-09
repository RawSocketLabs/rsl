//! Runs wavelet-lab libusdr's C driver against a [`SimBoard`].
//!
//! libusdr (vendored in `libusdr/`, pinned in `libusdr/PIN`) is compiled with its hardware
//! transports replaced by `shim/sim_plugin.c`. That plugin forwards every low-level
//! operation to the board installed by [`Oracle::open`], and libusdr's `usleep` advances the
//! board's virtual clock. The board's trace is the reference `rsl-usdr` is checked against.
//!
//! libusdr keeps process-global state, so one [`Oracle`] exists at a time: [`Oracle::open`]
//! blocks until any other instance is closed.

use std::any::Any;
use std::ffi::{CStr, c_char, c_int, c_uint, c_ulonglong, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use rsl_usdr_sim::{I2cAddress, SimBoard};

/// `EINVAL`: an operation shape the board's transports reject.
const EINVAL: c_int = 22;
/// `EIO`: the Rust side panicked; the panic is resumed once libusdr returns.
const EIO: c_int = 5;
/// `EOPNOTSUPP`: an operation class the uSDR transports do not provide.
const EOPNOTSUPP: c_int = 95;

/// libusdr `ls_op` class: FPGA registers (write, then read).
const LSOP_HWREG: c_uint = 0;
/// libusdr `ls_op` class: one 32-bit SPI transaction.
const LSOP_SPI: c_uint = 1;
/// libusdr `ls_op` class: I2C write-then-read.
const LSOP_I2C_DEV: c_uint = 2;

/// Serialises oracles: libusdr's device registry and logging are process-global.
static SESSION: Mutex<()> = Mutex::new(());
/// The board the C plugin forwards to while an oracle is open.
static BOARD: Mutex<Option<SimBoard>> = Mutex::new(None);
/// A panic raised inside a callback, held until control is back in Rust.
static PANIC: Mutex<Option<Box<dyn Any + Send>>> = Mutex::new(None);

/// Opaque libusdr device-manager handle (`pdm_dev_t`).
type DmDev = *mut c_void;

unsafe extern "C" {
    fn usdr_dmd_create_string(connection_string: *const c_char, odev: *mut DmDev) -> c_int;
    fn usdr_dmd_close(dev: DmDev) -> c_int;
    fn usdr_dme_get_uint(dev: DmDev, path: *const c_char, oval: *mut u64) -> c_int;
    fn usdr_dme_set_uint(dev: DmDev, path: *const c_char, val: u64) -> c_int;
}

/// libusdr opened on a simulated board.
pub struct Oracle {
    /// The open libusdr device; `None` once closed.
    dev: Option<NonNull<c_void>>,
    /// Held for the oracle's lifetime; see [`SESSION`].
    _session: MutexGuard<'static, ()>,
}

/// libusdr refused to open the board.
#[derive(Debug, thiserror::Error)]
#[error("libusdr failed to open the board: errno {errno}")]
pub struct OpenError {
    /// The negative errno libusdr returned.
    pub errno: i32,
    /// The board, with the trace of everything libusdr did before failing.
    pub board: Box<SimBoard>,
}

impl Oracle {
    /// Opens `board` through libusdr's `usdr_dmd_create_string`, which runs the full uSDR
    /// bring-up (power, clocks, RF chip reset).
    ///
    /// # Errors
    ///
    /// [`OpenError`] when libusdr's open fails.
    pub fn open(board: SimBoard) -> Result<Self, OpenError> {
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));

        let mut dev: DmDev = ptr::null_mut();
        // SAFETY: the connection string is NUL-terminated and `dev` is a valid out-pointer.
        let errno = unsafe { usdr_dmd_create_string(c"".as_ptr(), &raw mut dev) };

        let Some(dev) = NonNull::new(dev) else {
            resume_callback_panic();
            // A null handle with errno 0 would be a libusdr bug; still report a failure.
            let errno = if errno == 0 { -EIO } else { errno };
            return Err(OpenError {
                errno,
                board: Box::new(take_board()),
            });
        };
        // Owned before any callback panic resumes, so unwinding closes the device.
        let mut oracle = Self {
            dev: Some(dev),
            _session: session,
        };
        resume_callback_panic();
        if errno == 0 {
            return Ok(oracle);
        }
        oracle.shutdown();
        Err(OpenError {
            errno,
            board: Box::new(take_board()),
        })
    }

    /// Reads a libusdr device-manager value, such as `c"/dm/sensor/temp"`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn get_uint(&mut self, path: &CStr) -> Result<u64, i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut value = 0;
        // SAFETY: `dev` is open, `path` is NUL-terminated, `value` is a valid out-pointer.
        let errno = unsafe { usdr_dme_get_uint(dev.as_ptr(), path.as_ptr(), &raw mut value) };
        resume_callback_panic();
        if errno == 0 { Ok(value) } else { Err(errno) }
    }

    /// Sets the RX sample rate in samples per second, leaving TX unset, as the FFI `usdr`
    /// crate does: `/dm/rate/rxtxadcdac` with `{rate, 0, 0, 0}`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned (`-ERANGE` outside its supported rates).
    pub fn set_rx_rate(&mut self, rate: u32) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // RX, TX, ADC and DAC rates; libusdr ignores the last two.
        let rates: [u32; 4] = [rate, 0, 0, 0];
        // libusdr's `dev_m2_lm6_1_rate_m_set` reads the value as a pointer to four rates.
        let value = rates.as_ptr() as u64;
        // SAFETY: `dev` is open, the path is NUL-terminated, and `rates` outlives the call,
        // which reads four `u32`s through `value` and keeps no reference.
        let errno =
            unsafe { usdr_dme_set_uint(dev.as_ptr(), c"/dm/rate/rxtxadcdac".as_ptr(), value) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
    }

    /// How many operations the board has traced so far; slice the trace with it, after
    /// [`Oracle::close`], to isolate one call's operations.
    ///
    /// # Panics
    ///
    /// Only if the oracle's board went missing, which its session lock rules out.
    #[must_use]
    pub fn trace_len(&self) -> usize {
        BOARD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .expect("invariant: a board is installed while an oracle is open")
            .trace()
            .len()
    }

    /// Closes libusdr's device, which powers the board down, and returns the board.
    #[must_use]
    pub fn close(mut self) -> SimBoard {
        self.shutdown();
        take_board()
    }

    /// Closes the libusdr device if it is still open, re-raising any callback panic.
    fn shutdown(&mut self) {
        self.close_device();
        resume_callback_panic();
    }

    /// Closes the libusdr device if it is still open.
    fn close_device(&mut self) {
        if let Some(dev) = self.dev.take() {
            // SAFETY: `dev` came from a successful `usdr_dmd_create_string` and is closed once.
            unsafe { usdr_dmd_close(dev.as_ptr()) };
        }
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        self.close_device();
        if thread::panicking() {
            // Already unwinding: a second panic would abort and hide the first.
            PANIC.lock().unwrap_or_else(PoisonError::into_inner).take();
        } else {
            resume_callback_panic();
        }
        install(None);
    }
}

/// Replaces the board the C plugin forwards to.
fn install(board: Option<SimBoard>) {
    *BOARD.lock().unwrap_or_else(PoisonError::into_inner) = board;
}

/// Removes the installed board.
fn take_board() -> SimBoard {
    BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .expect("invariant: a board is installed while an oracle is open")
}

/// Re-raises a panic caught in a callback, now that no C frames are on the stack.
fn resume_callback_panic() {
    if let Some(payload) = PANIC.lock().unwrap_or_else(PoisonError::into_inner).take() {
        panic::resume_unwind(payload);
    }
}

/// Runs `f` on the installed board, converting a panic into `-EIO` so it never unwinds
/// through C.
fn with_board(f: impl FnOnce(&mut SimBoard) -> c_int) -> c_int {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut board = BOARD.lock().unwrap_or_else(PoisonError::into_inner);
        f(board
            .as_mut()
            .expect("invariant: libusdr calls back only while an oracle is open"))
    }));
    result.unwrap_or_else(|payload| {
        // Keep the first panic: later ones are usually libusdr's error path calling back.
        PANIC
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(payload);
        -EIO
    })
}

/// libusdr's `usleep`, redirected by the shim. The redirect is process-wide, so a call
/// while no oracle is open returns at once.
#[unsafe(no_mangle)]
extern "C" fn rsl_oracle_sleep_us(us: c_ulonglong) {
    if BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_none()
    {
        return;
    }
    with_board(|board| {
        board.sleep_us(us);
        0
    });
}

/// The sim plugin's `ls_op`: dispatches one libusdr low-level operation to the board.
///
/// # Safety
///
/// `pin` must be valid for `insz` writable bytes and `pout` for `outsz` readable bytes
/// (either may be null when its size is zero), as libusdr's `lowlevel_ops.ls_op` requires.
#[unsafe(no_mangle)]
unsafe extern "C" fn rsl_oracle_ls_op(
    op: c_uint,
    addr: c_uint,
    insz: usize,
    pin: *mut c_void,
    outsz: usize,
    pout: *const c_void,
) -> c_int {
    let input: &mut [u8] = if insz == 0 {
        &mut []
    } else {
        // SAFETY: the caller guarantees `pin` is valid for `insz` writable bytes.
        unsafe { slice::from_raw_parts_mut(pin.cast(), insz) }
    };
    let output: &[u8] = if outsz == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `pout` is valid for `outsz` readable bytes.
        unsafe { slice::from_raw_parts(pout.cast(), outsz) }
    };
    with_board(|board| match op {
        LSOP_HWREG => hwreg(board, addr, input, output),
        // SPI addresses carry config in [31:16]; the bus is [7:0] (`SPIEXT_LSOP_GET_BUS`).
        LSOP_SPI => spi(board, addr & 0xff, input, output),
        LSOP_I2C_DEV => i2c(board, I2cAddress::from_lsop(addr), input, output),
        _ => -EOPNOTSUPP,
    })
}

/// Register operation, as the `PCIe` transport performs it: each output word is written to
/// consecutive addresses from `addr`, then each input word is read the same way.
fn hwreg(board: &mut SimBoard, addr: u32, input: &mut [u8], output: &[u8]) -> c_int {
    if input.len() % 4 != 0 || output.len() % 4 != 0 {
        return -EINVAL;
    }
    for (reg, word) in (addr..).zip(output.chunks_exact(4)) {
        board.write_reg(
            reg,
            u32::from_le_bytes(word.try_into().expect("invariant: 4-byte chunk")),
        );
    }
    for (reg, word) in (addr..).zip(input.chunks_exact_mut(4)) {
        word.copy_from_slice(&board.read_reg(reg).to_le_bytes());
    }
    0
}

/// One 32-bit SPI word on bus `bus`; the readback is optional.
fn spi(board: &mut SimBoard, bus: u32, input: &mut [u8], output: &[u8]) -> c_int {
    let Ok(out) = <[u8; 4]>::try_from(output) else {
        return -EINVAL;
    };
    if !matches!(input.len(), 0 | 4) {
        return -EINVAL;
    }
    match board.spi(bus, u32::from_le_bytes(out)) {
        Ok(read) => {
            input.copy_from_slice(&read.to_le_bytes()[..input.len()]);
            0
        }
        // Both transports reject a bus beyond the device's SPI core count.
        Err(_) => -EINVAL,
    }
}

/// I2C write-then-read: at most three bytes written and four read, the limits both
/// transports enforce through `si2c_make_ctrl_reg` (`ipblks/si2c.c`).
///
/// The FPGA I2C core shifts received bytes into a 32-bit word, first byte most significant,
/// and both transports copy that word's little-endian bytes into `input`. Evidence: libusdr
/// reads the TMP114 ID (sent `0x11, 0x14`) as a native `u16` and compares it with `0x1114`.
fn i2c(board: &mut SimBoard, addr: I2cAddress, input: &mut [u8], output: &[u8]) -> c_int {
    if output.len() > 3 || input.len() > 4 {
        return -EINVAL;
    }
    let read = board.i2c(addr, output, input.len());
    let word = read
        .iter()
        .fold(0u32, |word, &byte| word << 8 | u32::from(byte));
    let len = input.len();
    input.copy_from_slice(&word.to_le_bytes()[..len]);
    0
}

#[cfg(test)]
mod tests {
    use rsl_usdr_sim::BoardRevision;

    use super::*;

    /// The TMP114 temperature sensor.
    const TEMP: I2cAddress = I2cAddress { bus: 0, addr: 0x4e };

    #[test]
    fn i2c_rejects_what_the_fpga_core_cannot_encode() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        assert_eq!(
            i2c(&mut board, TEMP, &mut [0; 2], &[0x03, 0, 0, 0]),
            -EINVAL
        );
        assert_eq!(i2c(&mut board, TEMP, &mut [0; 5], &[0x0b]), -EINVAL);
        assert!(board.trace().is_empty(), "rejected before reaching the bus");
    }

    #[test]
    fn i2c_readback_is_the_core_word_low_byte_first() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        let mut id = [0; 2];
        assert_eq!(i2c(&mut board, TEMP, &mut id, &[0x0b]), 0);
        assert_eq!(u16::from_ne_bytes(id), 0x1114);
    }
}

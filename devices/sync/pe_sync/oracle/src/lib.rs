//! Runs wavelet-lab libusdr's `PE_SYNC` driver (`device/pe_sync`) against a [`SimBoard`].
//!
//! libusdr (vendored once, in `../../../vendor/libusdr/`, pinned by its `PIN`) is
//! compiled with its hardware transports replaced by `shim/sim_plugin.c`. That plugin
//! reports the `PE_SYNC` device ID and forwards every low-level operation to the board
//! installed by [`Oracle::open`]; libusdr's `usleep` advances the board's virtual clock, and
//! the chip drivers' lock waits read it as their wall clock (`build.rs`). The board's trace
//! is the reference the `PE_SYNC` port is checked against.
//!
//! libusdr keeps process-global state, so one [`Oracle`] exists at a time: [`Oracle::open`]
//! blocks until any other instance is closed.

use std::any::Any;
use std::env;
use std::ffi::{CStr, c_char, c_int, c_uint, c_ulonglong, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use pe_sync_sim::{I2cAddress, SimBoard};

/// `EINVAL`: an operation shape the board's transports reject.
const EINVAL: c_int = 22;
/// `EIO`: the Rust side panicked; the panic is resumed once libusdr returns.
const EIO: c_int = 5;

/// libusdr `ls_op` class `USDR_LSOP_HWREG`: FPGA registers (write, then read).
const LSOP_HWREG: c_uint = 0;
/// `USDR_LSOP_SPI`: one SPI word.
const LSOP_SPI: c_uint = 1;
/// `USDR_LSOP_I2C_DEV`: I2C write-then-read.
const LSOP_I2C_DEV: c_uint = 2;

/// Environment variables the pinned libusdr reads on the paths the oracle drives. Each
/// changes the operation sequence, so none may be set while an oracle runs:
/// `USDR_BARE_DEV` (`usdr_device_pe_sync_initialize` skips power-on and configuration) and
/// `USDR_DEBUG` (`_usdr_dmd_create` starts a debug socket service). `USDR_LOGLEVEL` only
/// changes logging and is allowed.
pub const FORBIDDEN_ENV: [&str; 2] = ["USDR_BARE_DEV", "USDR_DEBUG"];

/// Serialises oracles: libusdr's device registry and logging are process-global.
static SESSION: Mutex<()> = Mutex::new(());
/// The board the C plugin forwards to while an oracle is open.
static BOARD: Mutex<Option<SimBoard>> = Mutex::new(None);
/// A panic raised inside a callback, held until control is back in Rust.
static PANIC: Mutex<Option<Box<dyn Any + Send>>> = Mutex::new(None);

/// Opaque libusdr device-manager handle (`pdm_dev_t`).
type DmDev = *mut c_void;

/// `lmx2820_tune`'s arguments (`hw/lmx2820/lmx2820.c`); enum arguments take libusdr's codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lmx2820Tune {
    /// The SPI bus the chip answers on (`lsaddr`): 2 or 3 on `PE_SYNC`.
    pub bus: u32,
    /// `osc_in`, in Hz.
    pub osc_in: u64,
    /// `mash_order`, 0 to 3 (`MASH_ORDER_*`).
    pub mash_order: u32,
    /// `force_mult`; 0 lets the solver choose.
    pub force_mult: u32,
    /// `rfouta`, in Hz.
    pub rfouta: u64,
    /// `rfoutb`, in Hz.
    pub rfoutb: u64,
}

/// `lmx1214_solver`'s arguments, the shim's `struct rsl_lmx1214_request`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lmx1214Request {
    /// `in`, in Hz.
    pub input: u64,
    /// `out`, in Hz.
    pub output: u64,
    /// `out_en[n]` in bit n.
    pub outputs: c_uint,
    /// `aux->enable`.
    pub aux_enable: c_uint,
    /// `aux->fmt`: `LMX1214_FMT_LVDS` 0, `LMX1214_FMT_CML` 2.
    pub aux_format: c_uint,
    /// `aux->freq`, in Hz.
    pub aux_freq: f64,
}

/// The `lmx1204_state_t` fields a board sets, the shim's `struct rsl_lmx1204_request`; the
/// masks hold channel n (4: the logic channel) in bit n; enums take libusdr's codes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lmx1204Request {
    /// `clkin`, in Hz.
    pub clkin: u64,
    /// `sysrefreq`, in Hz.
    pub sysrefreq: u64,
    /// `filter_mode`.
    pub filter_mode: c_uint,
    /// `sysref_mode`: `LMX1204_CONTINUOUS` 0, `LMX1204_PULSER` 1, `LMX1204_REPEATER` 2.
    pub sysref_mode: c_uint,
    /// `sysref_en`.
    pub sysref_en: c_uint,
    /// `ch_en`.
    pub ch_en: c_uint,
    /// `clkout_en`.
    pub clkout_en: c_uint,
    /// `sysrefout_en`.
    pub sysrefout_en: c_uint,
    /// `logiclkout_fmt`: `LMX1204_FMT_LVDS` 0, `_LVPECL` 1, `_CML` 2.
    pub logiclkout_fmt: c_uint,
    /// `logisysrefout_fmt`, as `logiclkout_fmt`.
    pub logisysrefout_fmt: c_uint,
    /// `clkout`, in Hz.
    pub clkout: f64,
    /// `sysrefout`, in Hz.
    pub sysrefout: f64,
    /// `logiclkout`, in Hz.
    pub logiclkout: f64,
}

/// Runs libusdr's `lmx2820_tune` (solver, registers, lock wait) on a zeroed chip state whose
/// SPI reaches `board`: the result (0 or a negative errno) and the board with its trace.
/// Serialised with [`Oracle`]s, which share the board slot.
///
/// # Panics
///
/// If a callback panicked (resumed here).
#[must_use]
pub fn lmx2820_tune(board: SimBoard, tune: Lmx2820Tune) -> (i32, SimBoard) {
    let Lmx2820Tune {
        bus,
        osc_in,
        mash_order,
        force_mult,
        rfouta,
        rfoutb,
    } = tune;
    // SAFETY: the shim takes plain values and keeps nothing.
    run_on(board, || unsafe {
        pe_sync_oracle_lmx2820_tune(bus, osc_in, mash_order, force_mult, rfouta, rfoutb)
    })
}

/// Runs libusdr's `lmx1214_solver` (registers written, no precision mode) on a zeroed chip
/// state on SPI bus 5 of `board`; result and board, as [`lmx2820_tune`].
///
/// # Panics
///
/// If a callback panicked (resumed here).
#[must_use]
pub fn lmx1214_solver(board: SimBoard, request: &Lmx1214Request) -> (i32, SimBoard) {
    // SAFETY: `request` is valid for the call; the shim only reads it.
    run_on(board, || unsafe { pe_sync_oracle_lmx1214_solver(request) })
}

/// Runs libusdr's `lmx1204_solver` (registers written, no precision mode) on SPI bus 4 of
/// `board`, from the state `lmx1204_create` leaves, then, if it succeeded, its
/// `lmx1204_wait_pll_lock`; result and board, as [`lmx2820_tune`].
///
/// # Panics
///
/// If a callback panicked (resumed here).
#[must_use]
pub fn lmx1204_solver(board: SimBoard, request: &Lmx1204Request) -> (i32, SimBoard) {
    // SAFETY: `request` is valid for the call; the shim only reads it.
    run_on(board, || unsafe { pe_sync_oracle_lmx1204_solver(request) })
}

/// Installs `board` for the duration of `call` (one shim entry), then returns its result
/// and the board.
fn run_on(board: SimBoard, call: impl FnOnce() -> c_int) -> (i32, SimBoard) {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    install(Some(board));
    let result = call();
    let board = take_board();
    resume_callback_panic();
    (result, board)
}

unsafe extern "C" {
    fn pe_sync_oracle_lmx2820_tune(
        bus: c_uint,
        osc_in: u64,
        mash_order: c_uint,
        force_mult: c_uint,
        rfouta: u64,
        rfoutb: u64,
    ) -> c_int;
    fn pe_sync_oracle_lmx1214_solver(request: *const Lmx1214Request) -> c_int;
    fn pe_sync_oracle_lmx1204_solver(request: *const Lmx1204Request) -> c_int;
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
    /// Opens `board` through libusdr's `usdr_dmd_create_string`: the `pe_sync` factory, then
    /// `usdr_device_pe_sync_initialize`: power-on, a status read of every chip and the whole
    /// clock tree configuration (LMK05318B, both LMX2820s, LMX1214, LMX1204, both
    /// LMK1D1208Is).
    ///
    /// # Errors
    ///
    /// [`OpenError`] when libusdr's open fails; the C has powered the board off by then.
    ///
    /// # Panics
    ///
    /// When a variable in [`FORBIDDEN_ENV`] is set: libusdr would take a different path
    /// than the one the trace stands for.
    pub fn open(board: SimBoard) -> Result<Self, OpenError> {
        for name in FORBIDDEN_ENV {
            assert!(
                env::var_os(name).is_none(),
                "{name} must be unset: it changes libusdr's PE_SYNC operation sequence"
            );
        }
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));

        let mut dev: DmDev = ptr::null_mut();
        // SAFETY: the connection string is NUL-terminated and `dev` is a valid out-pointer.
        let errno = unsafe { usdr_dmd_create_string(c"".as_ptr(), &raw mut dev) };

        let Some(dev) = NonNull::new(dev) else {
            // Taken first, so a resumed callback panic leaves no board installed.
            let board = Box::new(take_board());
            resume_callback_panic();
            // A null handle with errno 0 would be a libusdr bug; still report a failure.
            let errno = if errno == 0 { -EIO } else { errno };
            return Err(OpenError { errno, board });
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

    /// Reads a libusdr device-manager value, such as `c"/ll/sdr/max_hw_rx_chans"`.
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

    /// Sets a libusdr device-manager value, such as `c"/dm/rate/master"`.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn set_uint(&mut self, path: &CStr, value: u64) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and `path` is NUL-terminated.
        let errno = unsafe { usdr_dme_set_uint(dev.as_ptr(), path.as_ptr(), value) };
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

    /// Closes libusdr's device (`usdr_device_pe_sync_destroy`: every power GPO off, then the
    /// LED fade-out) and returns the board.
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
extern "C" fn pe_sync_oracle_sleep_us(us: c_ulonglong) {
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

/// The chip drivers' wall clock, redirected by the shim: the board's virtual time, or 0
/// while no oracle is open.
#[unsafe(no_mangle)]
extern "C" fn pe_sync_oracle_now_us() -> c_ulonglong {
    BOARD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map_or(0, SimBoard::now_us)
}

/// The sim plugin's `ls_op`: dispatches one libusdr low-level operation to the board.
///
/// An operation the sim does not model (a class other than register, SPI and I2C, such as
/// `USDR_LSOP_DRP`, or an SPI bus without a modelled chip) panics: the C sees `-EIO`, and
/// the panic is resumed once control is back in Rust, since libusdr ignores the result of
/// several status reads and a quiet error would leave no sign in the trace. Shapes the
/// real transports reject are refused with `-EINVAL`, as they would be.
///
/// # Safety
///
/// `pin` must be valid for `insz` writable bytes and `pout` for `outsz` readable bytes
/// (either may be null when its size is zero), as libusdr's `lowlevel_ops.ls_op` requires,
/// and the two buffers must not overlap.
#[unsafe(no_mangle)]
unsafe extern "C" fn pe_sync_oracle_ls_op(
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
        // SAFETY: the caller guarantees `pin` is valid for `insz` writable bytes and does
        // not overlap `pout`.
        unsafe { slice::from_raw_parts_mut(pin.cast(), insz) }
    };
    let output: &[u8] = if outsz == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `pout` is valid for `outsz` readable bytes and does
        // not overlap `pin`.
        unsafe { slice::from_raw_parts(pout.cast(), outsz) }
    };
    with_board(|board| match op {
        LSOP_HWREG => hwreg(board, addr, input, output),
        // SPI addresses carry config in [31:16]; the bus is [7:0] (`SPIEXT_LSOP_GET_BUS`).
        // `pe_sync.c` passes bare bus numbers.
        LSOP_SPI => spi(board, addr & 0xff, input, output),
        LSOP_I2C_DEV => i2c(board, I2cAddress::from_lsop(addr), input, output),
        _ => panic!("libusdr issued ls_op class {op}, which the PE_SYNC sim does not model"),
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

/// One SPI word on bus `bus`, passed as a native `u32` (`lowlevel_spi_tr32`); the readback
/// is optional.
fn spi(board: &mut SimBoard, bus: u32, input: &mut [u8], output: &[u8]) -> c_int {
    let Ok(out) = <[u8; 4]>::try_from(output) else {
        return -EINVAL;
    };
    if !matches!(input.len(), 0 | 4) {
        return -EINVAL;
    }
    // A real transport would shift the word out on any bus; the sim models only some.
    let read = board
        .spi(bus, u32::from_ne_bytes(out))
        .unwrap_or_else(|err| panic!("libusdr used SPI the sim does not model: {err}"));
    input.copy_from_slice(&read.to_ne_bytes()[..input.len()]);
    0
}

/// I2C write-then-read: at most three bytes written and four read, the limits both
/// transports enforce through `si2c_make_ctrl_reg` (`ipblks/si2c.c`).
///
/// The FPGA I2C core shifts received bytes into a 32-bit word, first byte most significant,
/// and both transports copy that word's little-endian bytes into `input` (as in the uSDR
/// oracle). Evidence here: `lmk05318_create` reads the four identity bytes 0x0000-0x0003 in
/// one burst and checks `dummy[3]`, `dummy[2]`, `dummy[1]`, `dummy[0]` against
/// 10 0b 35 42, so the byte from register 0 lands in `dummy[3]`.
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
    use super::*;

    /// The LMK05318B (`I2C_BUS_LMK05318B`).
    const LMK05318: I2cAddress = I2cAddress {
        core: 0,
        bus: 0,
        addr: 0x64,
    };

    #[test]
    fn i2c_rejects_what_the_fpga_core_cannot_encode() {
        let mut board = SimBoard::new();
        assert_eq!(
            i2c(&mut board, LMK05318, &mut [0; 1], &[0, 0, 0, 0]),
            -EINVAL
        );
        assert_eq!(i2c(&mut board, LMK05318, &mut [0; 5], &[0, 0]), -EINVAL);
        assert!(board.trace().is_empty(), "rejected before reaching the bus");
    }

    #[test]
    fn a_four_byte_i2c_burst_lands_last_byte_first() {
        let mut board = SimBoard::new();
        let mut id = [0; 4];
        assert_eq!(i2c(&mut board, LMK05318, &mut id, &[0, 0]), 0);
        assert_eq!(
            id,
            [0x42, 0x35, 0x0b, 0x10],
            "lmk05318_create's dummy[0..3]"
        );
    }
}

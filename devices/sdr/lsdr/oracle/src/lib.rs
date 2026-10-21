//! Runs wavelet-lab libusdr's `LSDr` (`m2_da09_4_ad45_2`) driver against a [`SimBoard`].
//!
//! libusdr (vendored once, in `../../../vendor/libusdr/`, pinned by its `PIN`) is compiled
//! with its hardware transports replaced by `shim/sim_plugin.c`. That plugin reports the
//! `LSDr` factory's device ID and forwards every low-level operation to the board installed
//! by [`Oracle::open`]; libusdr's `usleep` advances the board's virtual clock. The board's
//! trace is the reference the `LSDr` port is checked against.
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

use lsdr_sim::{I2cAddress, SimBoard};

/// `EINVAL`: an operation shape the board's transports reject.
const EINVAL: c_int = 22;
/// `EIO`: the Rust side panicked; the panic is resumed once libusdr returns.
const EIO: c_int = 5;

/// libusdr `ls_op` class `USDR_LSOP_HWREG`: FPGA registers (write, then read).
const LSOP_HWREG: c_uint = 0;
/// `USDR_LSOP_SPI`: one 32-bit SPI transaction.
const LSOP_SPI: c_uint = 1;
/// `USDR_LSOP_I2C_DEV`: I2C write-then-read.
const LSOP_I2C_DEV: c_uint = 2;

/// Environment variables the pinned libusdr reads on the paths the oracle drives. Each
/// changes the operation sequence, so none may be set while an oracle runs:
/// `USDR_BARE_DEV` (`usdr_device_m2_d09_4_ad45_2_initialize` returns before touching the
/// board), `USDR_FE_TYPE` (`device_fe_probe`'s front-end hint) and `USDR_DEBUG`
/// (`_usdr_dmd_create` starts a debug socket service). `USDR_LOGLEVEL` only changes
/// logging and is allowed.
pub const FORBIDDEN_ENV: [&str; 3] = ["USDR_BARE_DEV", "USDR_FE_TYPE", "USDR_DEBUG"];

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
    fn usdr_dme_get_string(dev: DmDev, path: *const c_char, oval: *mut *const c_char) -> c_int;
    fn usdr_dme_set_uint(dev: DmDev, path: *const c_char, val: u64) -> c_int;
    fn usdr_dms_create_ex(
        dev: DmDev,
        sobj: *const c_char,
        dformat: *const c_char,
        channels: u64,
        pktsyms: c_uint,
        flags: c_uint,
        outu: *mut *mut c_void,
    ) -> c_int;
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
    /// Opens `board` through libusdr's `usdr_dmd_create_string(connection)`: the factory
    /// for the shim's device ID builds the device, then its initialize checks the link,
    /// sets the PMIC's supplies, gates the Si549, probes the ADCs (and, with `lmk=1`, the
    /// LMK04832), powers the ADCs back down and probes for a front end. `c""` is the
    /// default connection.
    ///
    /// # Errors
    ///
    /// [`OpenError`] when libusdr's open fails.
    ///
    /// # Panics
    ///
    /// When a variable in [`FORBIDDEN_ENV`] is set: libusdr would take a different path
    /// than the one the trace stands for. Also when libusdr uses an `ls_op` class the sim
    /// does not model (anything but registers, SPI and I2C), once libusdr has returned.
    pub fn open(board: SimBoard, connection: &CStr) -> Result<Self, OpenError> {
        for name in FORBIDDEN_ENV {
            assert!(
                env::var_os(name).is_none(),
                "{name} must be unset: it changes libusdr's LSDr operation sequence"
            );
        }
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        install(Some(board));

        let mut dev: DmDev = ptr::null_mut();
        // SAFETY: the connection string is NUL-terminated and `dev` is a valid out-pointer.
        let errno = unsafe { usdr_dmd_create_string(connection.as_ptr(), &raw mut dev) };

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

    /// Sets `/dm/power/en` (`dev_m2_d09_4_ad45_2_pwren_set`): nonzero powers both ADC
    /// channels up (`_lsdr_adc_activate(3)`: LDO4 on, ADC reset pulse), zero powers them
    /// down.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn set_power(&mut self, on: bool) -> Result<(), i32> {
        self.set_uint(c"/dm/power/en", u64::from(on))
    }

    /// Sets `/dm/sdr/0/rx/gain/pga` (`dev_m2_d09_4_ad45_2_gainpga_set`): libusdr clamps
    /// `value` to 31, writes `63 - 2 × value` to the PGA gain bank and pulses its latch.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn set_rx_pga_gain(&mut self, value: u64) -> Result<(), i32> {
        self.set_uint(c"/dm/sdr/0/rx/gain/pga", value)
    }

    /// Sets `/dm/rate/master` (`dev_m2_d09_4_ad45_2_rate_set`): picks the decimation,
    /// tunes the Si549 to the ADC rate, loads the FIR gearbox, lays the LMK04832 out when
    /// open found one, waits, resets the bitslip logic, puts the ADCs in normal mode and
    /// steps the LVDS PHY.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<(), i32> {
        self.set_uint(c"/dm/rate/master", rate.into())
    }

    /// Sets `/dm/sdr/0/rx/frequency` (`dev_m2_d09_4_ad45_2_sdr_rx_freq_set`): without a
    /// front end, the NCO offset `hz` (`set_nco`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned: `-EINVAL` before a sample rate is set.
    pub fn set_rx_frequency(&mut self, hz: u64) -> Result<(), i32> {
        self.set_uint(c"/dm/sdr/0/rx/frequency", hz)
    }

    /// Reads `/dm/sensor/temp` (`dev_m2_d09_4_ad45_2_senstemp_get`): the TMP108's
    /// temperature word, sign-extended, as libusdr returns it.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn temperature(&mut self) -> Result<i64, i32> {
        self.get_uint(c"/dm/sensor/temp")
            .map(|raw| i64::from_ne_bytes(raw.to_ne_bytes()))
    }

    /// Creates an RX stream on `/ll/srx/0` in `ci16` (`usdr_dms_create_ex`, no flags) on
    /// libusdr's channel mask `channels`, with `samples_per_packet` samples per packet.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    ///
    /// # Panics
    ///
    /// If libusdr creates the stream: at the pin its sync step fails first (the `LSDr`
    /// declares no `/ll/sync/0/base`), and the shim has no stream buffers to give it.
    pub fn create_rx_stream(&mut self, channels: u64, samples_per_packet: u32) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut stream: *mut c_void = ptr::null_mut();
        // SAFETY: `dev` is open, the strings are NUL-terminated, `stream` is a valid
        // out-pointer.
        let errno = unsafe {
            usdr_dms_create_ex(
                dev.as_ptr(),
                c"/ll/srx/0".as_ptr(),
                c"ci16".as_ptr(),
                channels,
                samples_per_packet,
                0,
                &raw mut stream,
            )
        };
        resume_callback_panic();
        assert!(
            errno != 0,
            "libusdr cannot create an LSDr stream at the pin"
        );
        Err(errno)
    }

    /// Writes one fe1005000 command, `/debug/hw/fe_100_5000_cmd/0/reg`
    /// (`ext_fe_100_5000_cmd_wr`): `command` is the `FE_*` address
    /// (`ext_fe_100_5000.yaml`: 0 frequency in kHz, 1 attenuation, 2 preselector), `value`
    /// its 24-bit data; bit 31 marks the write.
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned; the path exists only while fe1005000 is up.
    pub fn fe1005000_command(&mut self, command: u8, value: u32) -> Result<(), i32> {
        let word = 0x8000_0000 | u64::from(command & 0x7f) << 24 | u64::from(value & 0x00ff_ffff);
        self.set_uint(c"/debug/hw/fe_100_5000_cmd/0/reg", word)
    }

    /// Reads a libusdr device-manager value, such as `c"/ll/sdr/max_hw_tx_chans"`.
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

    /// The device name libusdr's factory declares (`/ll/device/name`).
    ///
    /// # Errors
    ///
    /// The negative errno libusdr returned.
    pub fn device_name(&mut self) -> Result<String, i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        let mut name: *const c_char = ptr::null();
        // SAFETY: `dev` is open, the path is NUL-terminated, `name` is a valid out-pointer.
        let errno = unsafe {
            usdr_dme_get_string(dev.as_ptr(), c"/ll/device/name".as_ptr(), &raw mut name)
        };
        resume_callback_panic();
        if errno != 0 {
            return Err(errno);
        }
        if name.is_null() {
            return Err(-EINVAL);
        }
        // SAFETY: `/ll/device/name` is a constant parameter holding a pointer to a string
        // literal in the factory's table (`(uintptr_t)"lsdr"`), NUL-terminated and static.
        let name = unsafe { CStr::from_ptr(name) };
        Ok(name.to_string_lossy().into_owned())
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

    /// Closes libusdr's device (`usdr_device_m2_d09_4_ad45_2_destroy`: Si549 output off,
    /// LDO4 off, both ADCs powered down) and returns the board.
    #[must_use]
    pub fn close(mut self) -> SimBoard {
        self.shutdown();
        take_board()
    }

    /// Sets a libusdr device-manager value.
    fn set_uint(&mut self, path: &CStr, value: u64) -> Result<(), i32> {
        let Some(dev) = self.dev else {
            return Err(-EINVAL);
        };
        // SAFETY: `dev` is open and `path` is NUL-terminated; the oracle sets only numeric
        // values, which its handlers read as numbers.
        let errno = unsafe { usdr_dme_set_uint(dev.as_ptr(), path.as_ptr(), value) };
        resume_callback_panic();
        if errno == 0 { Ok(()) } else { Err(errno) }
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
extern "C" fn lsdr_oracle_sleep_us(us: c_ulonglong) {
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
/// Any class but registers, SPI and I2C (`USDR_LSOP_DRP`, `URAM`, ...) panics, and the
/// panic resumes once libusdr returns: callers such as the front-end probe would otherwise
/// swallow a refusal and quietly take a path the sim does not stand for.
///
/// # Safety
///
/// `pin` must be valid for `insz` writable bytes and `pout` for `outsz` readable bytes
/// (either may be null when its size is zero), as libusdr's `lowlevel_ops.ls_op` requires.
#[unsafe(no_mangle)]
unsafe extern "C" fn lsdr_oracle_ls_op(
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
        // The instance (bits 31:24) reaches the board and its trace. The `LSDr` declares one
        // I2C core (`DNLL_I2C_COUNT` 1), and the uSDR oracle reads the upstream PCIe driver's
        // `instance >= i2c_count` check as refusing any other; this shim passes core 1 on, so
        // the goldens characterise a transport that accepts fe4ch4007200's transfers (an open
        // item in `AGENTS.md`).
        LSOP_I2C_DEV => i2c(board, I2cAddress::from_lsop(addr), input, output),
        _ => panic!(
            "libusdr used ls_op class {op} (address {addr:#x}), which the sim does not model"
        ),
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
/// and both transports copy that word's little-endian bytes into `input`. The fact is
/// copied from the uSDR oracle; the `LSDr`'s chips only ever read one byte, which packing
/// cannot reorder.
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
    use lsdr_sim::Variant;

    use super::*;

    /// The TPS6594.
    const PMIC: I2cAddress = I2cAddress::new(0, 0x48);

    #[test]
    fn i2c_rejects_what_the_fpga_core_cannot_encode() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        assert_eq!(i2c(&mut board, PMIC, &mut [0; 1], &[1, 0, 0, 0]), -EINVAL);
        assert_eq!(i2c(&mut board, PMIC, &mut [0; 5], &[1]), -EINVAL);
        assert!(board.trace().is_empty(), "rejected before reaching the bus");
    }

    #[test]
    fn spi_rejects_a_bus_the_fpga_lacks() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        assert_eq!(spi(&mut board, 5, &mut [], &[0; 4]), -EINVAL);
    }
}

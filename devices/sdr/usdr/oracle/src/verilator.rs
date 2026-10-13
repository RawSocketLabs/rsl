//! libusdr's Verilator host bridge (`lowlevel/verilator_ll`), compiled whole by
//! `shim/verilator.c` (which says what it adapts), connected to a model the caller serves.

use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::sync::{MutexGuard, PoisonError};

use crate::{LSOP_HWREG, LSOP_I2C_DEV, LSOP_SPI, SESSION};

unsafe extern "C" {
    fn rsl_oracle_vll_open(dir: *const c_char, out: *mut *mut c_void) -> c_int;
    fn rsl_oracle_vll_ls_op(
        dev: *mut c_void,
        op: c_uint,
        addr: c_uint,
        meminsz: usize,
        pin: *mut c_void,
        memoutsz: usize,
        pout: *const c_void,
    ) -> c_int;
    fn rsl_oracle_vll_rx_open(dev: *mut c_void, block: c_uint, buffers: c_uint) -> c_int;
    fn rsl_oracle_vll_rx_recv(
        dev: *mut c_void,
        out: *mut c_void,
        len: usize,
        oob: *mut c_uint,
    ) -> c_int;
    fn rsl_oracle_vll_close(dev: *mut c_void);
}

/// libusdr's bridge, connected. Dropping it ends the connection and its monitor thread.
pub struct Bridge {
    /// The bridge's device.
    dev: NonNull<c_void>,
    /// libusdr's device registry and logging are process-global.
    _session: MutexGuard<'static, ()>,
}

impl Bridge {
    /// Connects to the model serving `dir/verilator.sock` (and `dir/verilator.sock.mmap`) as
    /// libusdr's plugin create does, board bring-up excepted: the create's negative errno
    /// on failure.
    ///
    /// # Errors
    ///
    /// The create's negative errno.
    ///
    /// # Panics
    ///
    /// If `dir` holds a NUL byte.
    pub fn open(dir: &Path) -> Result<Self, i32> {
        let dir = CString::new(dir.as_os_str().as_bytes()).expect("a path without NUL");
        let session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
        let mut dev = ptr::null_mut();
        // SAFETY: `dir` is a C string that outlives the call, and `dev` takes one pointer.
        let res = unsafe { rsl_oracle_vll_open(dir.as_ptr(), &raw mut dev) };
        match NonNull::new(dev) {
            Some(dev) if res == 0 => Ok(Self {
                dev,
                _session: session,
            }),
            _ => Err(res),
        }
    }

    /// `ls_op` on the bridge with `pout`/`memoutsz` and `pin`/`meminsz` from `write` and
    /// `read`: 0 or a negative errno.
    fn ls_op(&mut self, op: c_uint, addr: u32, write: &[u8], read: &mut [u8]) -> i32 {
        // SAFETY: the device is open, `read` is valid for `read.len()` writes and `write`
        // for `write.len()` reads; the bridge accesses no more.
        unsafe {
            rsl_oracle_vll_ls_op(
                self.dev.as_ptr(),
                op,
                addr,
                read.len(),
                read.as_mut_ptr().cast(),
                write.len(),
                write.as_ptr().cast(),
            )
        }
    }

    /// Writes `values` from register `addr`: 0 or a negative errno.
    pub fn write_regs(&mut self, addr: u32, values: &[u32]) -> i32 {
        let bytes: Vec<u8> = values.iter().flat_map(|word| word.to_ne_bytes()).collect();
        self.ls_op(LSOP_HWREG, addr, &bytes, &mut [])
    }

    /// Reads `out.len()` registers from `addr`: 0 or a negative errno.
    pub fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> i32 {
        let mut bytes = vec![0; 4 * out.len()];
        let res = self.ls_op(LSOP_HWREG, addr, &[], &mut bytes);
        for (value, word) in out.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes([word[0], word[1], word[2], word[3]]);
        }
        res
    }

    /// One SPI word on `bus`: what the bridge reads back.
    ///
    /// # Errors
    ///
    /// The bridge's negative errno.
    pub fn spi32(&mut self, bus: u32, word: u32) -> Result<u32, i32> {
        let mut read = [0; 4];
        match self.ls_op(LSOP_SPI, bus, &word.to_ne_bytes(), &mut read) {
            0 => Ok(u32::from_ne_bytes(read)),
            errno => Err(errno),
        }
    }

    /// An I2C transfer to `addr` (libusdr's `ls_op` address): 0 or a negative errno.
    pub fn i2c(&mut self, addr: u32, write: &[u8], read: &mut [u8]) -> i32 {
        self.ls_op(LSOP_I2C_DEV, addr, write, read)
    }

    /// Sets RX stream 0 up for `buffers` blocks of `block` bytes: 0 or a negative errno.
    pub fn rx_open(&mut self, block: u32, buffers: u32) -> i32 {
        // SAFETY: the device is open.
        unsafe { rsl_oracle_vll_rx_open(self.dev.as_ptr(), block, buffers) }
    }

    /// Waits for the next RX block (forever, as libusdr's bridge does), returning its
    /// first `len` bytes and the two status words the bridge wrote through its size
    /// argument, then releases it.
    ///
    /// # Errors
    ///
    /// The bridge's negative errno.
    pub fn rx_recv(&mut self, len: usize) -> Result<(Vec<u8>, [u32; 2]), i32> {
        let mut data = vec![0; len];
        let mut oob = [0; 2];
        // SAFETY: the device is open, `data` takes `len` bytes and `oob` two words.
        let res = unsafe {
            rsl_oracle_vll_rx_recv(
                self.dev.as_ptr(),
                data.as_mut_ptr().cast(),
                len,
                oob.as_mut_ptr(),
            )
        };
        if res == 0 { Ok((data, oob)) } else { Err(res) }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // SAFETY: the device is open and is not used again.
        unsafe { rsl_oracle_vll_close(self.dev.as_ptr()) }
    }
}

//! libusdr's LMK05318 planning (`hw/lmk05318/lmk05318.c`) called directly through
//! `shim/lmk05318.c`: XO and APLL1 setup, the output solver with APLL2's settling, and output
//! dividers, each on caller-supplied state, with the registers it staged.
//!
//! The C keeps its register map in a static, so every call holds the oracle's session lock.

use std::ffi::c_int;
use std::sync::PoisonError;

use crate::SESSION;

/// How many addresses libusdr's staging map holds (0x0000 to 0x019B).
pub const MAP_LEN: usize = 0x19c;

/// The registers a call staged, by address; `None` where nothing was staged.
pub type Staged = Vec<Option<u8>>;

/// One output request (`lmk05318_port_request`, `lmk05318_set_port_affinity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortRequest {
    /// The output, 0 to 7.
    pub port: u32,
    /// Its frequency in Hz; 0 turns it off.
    pub freq: u32,
    /// Inverted (`revert_phase`).
    pub revert: bool,
    /// `enum lmk05318_type`'s value.
    pub format: i32,
    /// `enum lmk05318_port_affinity`'s value.
    pub affinity: i32,
}

/// One output's result, in libusdr's sorted order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortResult {
    /// The output.
    pub port: u32,
    /// Its requested frequency, in Hz.
    pub freq: u32,
    /// `enum lmk05318_out_pll_sel_t`'s value.
    pub mux: i32,
    /// The output divider.
    pub out_div: u64,
    /// The frequency reached, in Hz.
    pub freq_out: f64,
}

/// APLL2's state (the APLL2 fields of `lmk05318_state_t`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Apll2State {
    /// `fref_pll2_div_rp`.
    pub rp: u32,
    /// `fref_pll2_div_rs`.
    pub rs: u32,
    /// `vco2_freq`.
    pub vco2_freq: u64,
    /// `vco2_n`.
    pub n: u32,
    /// `vco2_num`.
    pub num: u32,
    /// `vco2_den`.
    pub den: u32,
    /// `pd1`.
    pub pd1: u32,
    /// `pd2`.
    pub pd2: u32,
}

/// `struct rsl_lmk_port` in the shim.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawPort {
    port: u32,
    freq: u32,
    revert: i32,
    format: i32,
    affinity: i32,
    mux: i32,
    out_div: u64,
    freq_out: f64,
}

unsafe extern "C" {
    fn rsl_oracle_lmk05318_xo_apll1(
        fref: u32,
        xo_type: i32,
        fdet_bypass: i32,
        doubler: *mut u32,
        rdiv: *mut u32,
        apll1_res: *mut i32,
        regs: *mut u16,
    ) -> c_int;
    fn rsl_oracle_lmk05318_solve(
        apll2: *mut Apll2State,
        ports: *mut RawPort,
        count: u32,
        regs: *mut u16,
    ) -> c_int;
    fn rsl_oracle_lmk05318_out_div(port: u32, div: u64, regs: *mut u16) -> c_int;
}

/// What libusdr's XO and APLL1 setup gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XoApll1 {
    /// `lmk05318_set_xo_fref`'s result.
    pub xo_res: c_int,
    /// `lmk05318_tune_apll1`'s result, run whenever the XO setup chose a path (`-EINVAL`
    /// otherwise).
    pub apll1_res: c_int,
    /// The doubler is on.
    pub doubler: bool,
    /// The reference divider; 0 when no path was chosen.
    pub rdiv: u32,
    /// The registers both staged.
    pub regs: Staged,
}

/// The XO path and APLL1 for an XO (`lmk05318_set_xo_fref`, then `lmk05318_tune_apll1`
/// without a DPLL).
#[must_use]
pub fn xo_apll1(fref: u32, xo_type: i32, fdet_bypass: bool) -> XoApll1 {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    let (mut doubler, mut rdiv, mut apll1_res) = (0, 0, 0);
    let mut regs = [0_u16; MAP_LEN];
    // SAFETY: the out-pointers are valid; `regs` holds the shim's MAP_LEN entries.
    let xo_res = unsafe {
        rsl_oracle_lmk05318_xo_apll1(
            fref,
            xo_type,
            fdet_bypass.into(),
            &raw mut doubler,
            &raw mut rdiv,
            &raw mut apll1_res,
            regs.as_mut_ptr(),
        )
    };
    XoApll1 {
        xo_res,
        apll1_res,
        doubler: doubler != 0,
        rdiv,
        regs: staged(&regs),
    }
}

/// The output solver on `apll2` (`lmk05318_solver`): libusdr's result, the outputs in its
/// order, and the registers staged.
///
/// # Panics
///
/// For more than eight requests.
#[must_use]
pub fn solve(apll2: &mut Apll2State, requests: &[PortRequest]) -> (c_int, Vec<PortResult>, Staged) {
    assert!(requests.len() <= 8, "libusdr takes at most eight outputs");
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ports: Vec<RawPort> = requests
        .iter()
        .map(|request| RawPort {
            port: request.port,
            freq: request.freq,
            revert: request.revert.into(),
            format: request.format,
            affinity: request.affinity,
            ..RawPort::default()
        })
        .collect();
    let mut regs = [0_u16; MAP_LEN];
    let count = u32::try_from(ports.len()).expect("invariant: at most eight");
    // SAFETY: `apll2` is valid, `ports` holds `count` entries the shim reads and writes,
    // and `regs` holds MAP_LEN entries.
    let status =
        unsafe { rsl_oracle_lmk05318_solve(apll2, ports.as_mut_ptr(), count, regs.as_mut_ptr()) };
    let results = ports
        .iter()
        .map(|port| PortResult {
            port: port.port,
            freq: port.freq,
            mux: port.mux,
            out_div: port.out_div,
            freq_out: port.freq_out,
        })
        .collect();
    (status, results, staged(&regs))
}

/// One output's divider (`lmk05318_set_out_div`): libusdr's result and the registers
/// staged.
#[must_use]
pub fn out_div(port: u32, div: u64) -> (c_int, Staged) {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    let mut regs = [0_u16; MAP_LEN];
    // SAFETY: `regs` holds MAP_LEN entries.
    let status = unsafe { rsl_oracle_lmk05318_out_div(port, div, regs.as_mut_ptr()) };
    (status, staged(&regs))
}

/// The capture slots as staged bytes.
fn staged(regs: &[u16; MAP_LEN]) -> Staged {
    regs.iter()
        .map(|&slot| (slot & 0x100 != 0).then(|| slot.to_le_bytes()[0]))
        .collect()
}

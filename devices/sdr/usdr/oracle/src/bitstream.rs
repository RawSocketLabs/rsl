//! libusdr's gateware image header parser and update check (`ipblks/xlnx_bitstream.c`).

use std::ffi::{c_int, c_uint};
use std::sync::PoisonError;

use crate::SESSION;

/// `XLNX_BSTRM_PARSE_F_CRC_CHECK`: check every CRC word, and require one.
pub const CRC_CHECK: u32 = 1;
/// `XLNX_BSTRM_ALLOW_CROP`: accept a packet cut off by the end.
pub const ALLOW_CROP: u32 = 2;

/// libusdr's `xlnx_image_params_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageParams {
    /// `devid`: the last `IDCODE` write.
    pub devid: u32,
    /// `wbstar`: the last `WBSTAR` write.
    pub wbstar: u32,
    /// `usr_access2`: the last `AXSS` write.
    pub usr_access2: u32,
    /// `iprog`: an IPROG command was seen.
    pub iprog: bool,
}

unsafe extern "C" {
    fn rsl_oracle_btstrm_parse(
        mem: *const u32,
        len: c_uint,
        stat: *mut ImageParams,
        flags: c_uint,
    ) -> c_int;
    fn rsl_oracle_btstrm_iprgcheck(
        golden: *const ImageParams,
        image: *const ImageParams,
        wbstar: c_uint,
        golden_image: bool,
    ) -> c_int;
}

/// `xlnx_btstrm_parse_header_ex` over `bytes`' whole words, as libusdr's callers pass an
/// image (`(const uint32_t*)` over its bytes, length in words): its return value (0 or a
/// negative errno) and the fields it left, set or not.
///
/// # Panics
///
/// If `bytes` holds 2^32 words or more.
#[must_use]
pub fn parse_header(bytes: &[u8], flags: u32) -> (i32, ImageParams) {
    // The words in memory order, as the C reads them through its cast.
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|word| u32::from_ne_bytes(word.try_into().expect("four bytes")))
        .collect();
    let len = c_uint::try_from(words.len()).expect("test images fit 32 bits");
    let mut params = ImageParams::default();
    // The parser and its log redirection are process-global.
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: `words` is valid for `len` reads and `params` for one write.
    let errno = unsafe { rsl_oracle_btstrm_parse(words.as_ptr(), len, &raw mut params, flags) };
    (errno, params)
}

/// `xlnx_btstrm_iprgcheck(golden, image, wbstar, golden_image)`: 0 or `-EINVAL`.
#[must_use]
pub fn iprgcheck(
    golden: &ImageParams,
    image: &ImageParams,
    wbstar: u32,
    golden_image: bool,
) -> i32 {
    let _session = SESSION.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: both pointers are to live, initialized structs the C only reads.
    unsafe { rsl_oracle_btstrm_iprgcheck(golden, image, wbstar, golden_image) }
}

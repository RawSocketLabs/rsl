//! Classify failed SOCKS5 command decoding without consuming retained input.
use crate::error::Error;
// Classify a failed command decode, not its length. Unknown ATYP has no payload
// width and cannot form an Endpoint, but RFC 1928 §6 still assigns it reply 0x08.
// A failed bnb attempt retains the input; probe its header and restore the cursor.
pub(crate) fn command_error(error: Error, input: &mut bnb::BitBuf) -> Error {
    use bnb::Source;
    if !matches!(error, Error::Codec(_)) {
        return error;
    }
    let start = input.bit_pos();
    let mut header = [0; 4];
    let result = input.read_into(&mut header);
    input
        .seek_to_bit(start)
        .expect("invariant: a header probe cannot invalidate the saved cursor");
    if result.is_ok() {
        if let crate::v5::AddressType::Other(code) = header[3].into() {
            return Error::UnsupportedAddressType(code);
        }
    }
    error
}

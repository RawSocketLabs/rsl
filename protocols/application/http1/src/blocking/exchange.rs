//! The exchange.

// --- Standard library ---
use std::io::{self, Read, Write};

// --- Internal modules ---
use super::Error;
use crate::{Limits, Request, Response, ResponseDecoder};

/// How many bytes one transport read asks for.
const READ_SIZE: usize = 8 * 1024;

/// Sends `request` over `transport` and reads its whole response into memory.
///
/// This is one exchange on a connection the caller owns: it opens nothing, follows no
/// redirect and adds no header. All protocol decisions are the [`ResponseDecoder`]'s; this
/// function only moves bytes.
///
/// - The transport's end of input is taken as a clean close, and an `UnexpectedEof` error
///   from it as an unconfirmed one. That is how a TLS stream reports a connection that ended
///   without `close_notify`, and it matters only to a response whose body runs to the end
///   of the connection ([`Limits::accept_unclean_close`]). The other side of that contract
///   is the transport's: one that reports a cut connection as plain end of input gets such
///   a body accepted as complete.
/// - [`Response::keep_alive`] is cleared when the request was not HTTP/1.1, when it asked
///   for `Connection: close`, and when bytes beyond the response arrived in the same read as
///   its end. Surplus that arrives later is not seen, because a `Read` cannot be asked
///   whether more is waiting. So `true` means the server left the connection open, not that
///   it is idle: a caller that reuses connections must be ready for a stale or unsolicited
///   response on the next exchange.
/// - There is no clock here. A transport whose reads can block forever makes this block
///   forever; bound the whole exchange with a transport that enforces a deadline.
///
/// # Errors
///
/// [`Error::Io`] when the transport fails, and [`Error::Http`] when the response cannot be
/// decoded or the connection ends before it is complete. After either, the connection must
/// not be reused. A failed write returns at once: a response the server sent early, to
/// refuse a request it had not finished reading, is not read.
pub fn exchange<T: Read + Write>(
    transport: &mut T,
    request: &Request,
    limits: Limits,
) -> Result<Response, Error> {
    transport.write_all(&request.encode())?;
    transport.flush()?;

    let mut decoder = ResponseDecoder::new(&request.method, limits);
    let mut buffer = [0; READ_SIZE];
    let mut surplus = false;
    while !decoder.is_complete() {
        match transport.read(&mut buffer) {
            Ok(0) => decoder.receive_eof(true)?,
            Ok(count) => surplus = decoder.receive(&buffer[..count])? < count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                decoder.receive_eof(false)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut response = decoder.finish()?;
    let asked_to_close = request.headers.has_token("Connection", "close");
    if surplus || asked_to_close || request.version != b"HTTP/1.1" {
        response.keep_alive = false;
    }
    Ok(response)
}

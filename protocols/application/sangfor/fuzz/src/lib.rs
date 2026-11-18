//! What the Sangfor fuzz targets share.

use http1::{Limits, Response, ResponseDecoder};

/// A source of bytes that counts upward, so every run of an input is the same run. It is
/// not random and must never leave the fuzz targets.
#[derive(Default)]
pub struct Counting(u8);

impl crypto::RandomSource for Counting {
    fn fill_bytes(&mut self, output: &mut [u8]) -> crypto::Result<()> {
        for byte in output {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// An HTTP response with `status`, the `headers` lines and `body`, decoded from its wire form
/// the way a real one is. `None` when HTTP cannot carry what was asked for.
#[must_use]
pub fn response(status: u16, headers: &[&[u8]], body: &[u8]) -> Option<Response> {
    let head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", body.len());
    let mut wire = head.into_bytes();
    for header in headers {
        wire.extend_from_slice(header);
        wire.extend_from_slice(b"\r\n");
    }
    wire.extend_from_slice(b"\r\n");
    wire.extend_from_slice(body);

    let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
    decoder.receive(&wire).ok()?;
    decoder.finish().ok()
}

//! Helpers shared by the test binaries.

use http1::{Limits, Response, ResponseDecoder};

/// An HTTP response with `status`, the given extra header lines and `body`, decoded from its
/// wire form the way a real one is.
pub(crate) fn response(status: u16, headers: &[&str], body: &[u8]) -> Response {
    let mut wire = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", body.len());
    for header in headers {
        wire.push_str(header);
        wire.push_str("\r\n");
    }
    wire.push_str("\r\n");
    let mut wire = wire.into_bytes();
    wire.extend_from_slice(body);

    let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
    let consumed = decoder.receive(&wire).unwrap();
    assert_eq!(
        consumed,
        wire.len(),
        "the test response must be one whole response"
    );
    decoder.finish().unwrap()
}

/// A 200 response with a JSON body.
pub(crate) fn ok(body: &str) -> Response {
    response(200, &[], body.as_bytes())
}

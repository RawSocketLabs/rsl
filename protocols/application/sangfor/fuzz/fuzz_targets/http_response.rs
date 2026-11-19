//! A response must end at the same byte with the same result at any transport read size.
#![no_main]

use http1::{Error, Limits, Response, ResponseDecoder};
use libfuzzer_sys::fuzz_target;

fn decode(wire: &[u8], stride: usize, clean: bool) -> Result<(usize, Response), Error> {
    let mut decoder = ResponseDecoder::new(b"GET", Limits::default().with_body(2048));
    let mut taken = 0;
    for part in wire.chunks(stride) {
        let count = decoder.receive(part)?;
        assert!(count <= part.len());
        taken += count;
        if decoder.is_complete() {
            break;
        }
        assert_eq!(count, part.len());
    }
    decoder.receive_eof(clean)?;
    let response = decoder.finish()?;
    assert!(response.body.len() <= 2048);
    Ok((taken, response))
}

fuzz_target!(|input: &[u8]| {
    let Some((&mode, data)) = input.split_first() else {
        return;
    };
    let data = &data[..data.len().min(4096)];
    let mut wire = Vec::new();
    match mode % 5 {
        0 => {}
        1 => wire.extend_from_slice(b"HTTP/1.1 200 OK\r\n"),
        2 => wire.extend_from_slice(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"),
        3 => wire.extend_from_slice(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\n\r\n"),
        _ => wire.extend_from_slice(
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", data.len()).as_bytes(),
        ),
    }
    wire.extend_from_slice(data);
    let clean = mode & 0x80 == 0;
    let whole = decode(&wire, wire.len().max(1), clean);
    assert_eq!(decode(&wire, 1 + usize::from(mode >> 3), clean), whole);
    if mode % 5 == 4 && data.len() <= 2048 {
        let (taken, response) = whole.unwrap();
        assert_eq!(taken, wire.len());
        assert_eq!(response.body, data);
    }
});

//! Full-tier, in-memory handshake benchmark: fragmentation versus coalescing.
//! No sockets, sleeps, allocator instrumentation, or timing assertions.

use socks::server::policy::ServerAuth;
use std::hint::black_box;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

struct Input<'a> {
    bytes: &'a [u8],
    chunk: usize,
}

impl Read for Input<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes.len().min(self.chunk);
        self.bytes.read(&mut bytes[..count])
    }
}

impl Write for Input<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for Input<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let count = Read::read(&mut *self, bytes.initialize_unfilled())?;
        bytes.advance(count);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Input<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Write::write(&mut *self, bytes))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

// Benchmark output is intentional and is not library logging.
#[allow(clippy::print_stdout)]
fn measure(label: &str, mut handshake: impl FnMut()) {
    const ITERATIONS: u32 = 10_000;
    for _ in 0..100 {
        handshake();
    }
    let mut samples = [Duration::ZERO; 5];
    for sample in &mut samples {
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            handshake();
        }
        *sample = start.elapsed() / ITERATIONS;
    }
    samples.sort_unstable();
    println!(
        "{label}: median {:?}/handshake (5 x {ITERATIONS})",
        samples[2]
    );
}

fn main() {
    // RFC 1929 §2 maximum credentials followed by RFC 1928 §4 IPv4 CONNECT.
    let mut credentials = vec![5, 1, 2, 1, 255];
    credentials.extend_from_slice(&[b'u'; 255]);
    credentials.push(255);
    credentials.extend_from_slice(&[b'p'; 255]);
    credentials.extend_from_slice(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 80]);
    // RFC 1928 §4 maximum domain; its length excludes the two-byte port.
    let mut domain = vec![5, 1, 0, 5, 1, 0, 3, 255];
    domain.extend_from_slice(&[b'x'; 255]);
    domain.extend_from_slice(&[0, 80]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for (case, bytes, auth) in [
        (
            "credentials",
            credentials,
            ServerAuth::username_password(|_, _| true),
        ),
        ("domain", domain, ServerAuth::no_authentication()),
    ] {
        for chunk in [1, 16, usize::MAX] {
            let label = format!("{case}/chunk={chunk}");
            measure(&format!("blocking/{label}"), || {
                let stream = Input {
                    bytes: black_box(&bytes),
                    chunk,
                };
                black_box(socks::v5::server::blocking::exchange(stream, &auth).unwrap());
            });
            measure(&format!("tokio/{label}"), || {
                let stream = Input {
                    bytes: black_box(&bytes),
                    chunk,
                };
                black_box(
                    runtime
                        .block_on(socks::v5::server::tokio::exchange(stream, &auth))
                        .unwrap(),
                );
            });
        }
    }
}

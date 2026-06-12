//! Performance benchmarks for the SOCKS crate.
//!
//! The groups are feature-gated to match the crate.
//!
//! Under `v5` (the default):
//!
//!   * `parse`     — pure-CPU wire decoding of the hot message types.
//!   * `handshake` — a full negotiate + CONNECT round trip over loopback.
//!   * `relay`     — steady-state byte throughput on an established tunnel.
//!
//! Under `v4`:
//!
//!   * `v4/parse`  — pure-CPU wire decoding of a SOCKS4 request.
//!
//! Run measurements:      cargo bench -p socks
//! SOCKS4 arm only:       cargo bench -p socks --no-default-features --features v4
//! Emit flamegraph SVGs:  cargo bench -p socks -- --profile-time 5
//! (criterion writes reports under target/criterion/.)

use criterion::Criterion;
use pprof::criterion::{Output, PProfProfiler};

#[cfg(feature = "v5")]
use std::io::{Read, Write};
#[cfg(feature = "v5")]
use std::net::{SocketAddr, TcpListener};
#[cfg(feature = "v5")]
use std::thread;

#[cfg(feature = "v5")]
use criterion::{BenchmarkId, Throughput};

#[cfg(feature = "v5")]
use socks::client::Client;
#[cfg(feature = "v5")]
use socks::server::Server;
#[cfg(feature = "v5")]
use socks::v5::{Identifier, Reply, Request};

/// Decodes a SOCKS4 CONNECT request — the SOCKS4 hot-path codec.
#[cfg(feature = "v4")]
fn bench_v4_parse(c: &mut Criterion) {
    use socks::v4::Request as V4Request;
    // VN=4, CD=CONNECT, port 443, 127.0.0.1, userid "bench", NUL.
    let request = [
        4u8, 1, 0x01, 0xBB, 127, 0, 0, 1, b'b', b'e', b'n', b'c', b'h', 0,
    ];
    c.bench_function("v4/parse/request", |b| {
        b.iter(|| V4Request::read_from(&mut request.as_slice()).unwrap())
    });
}

/// Spawns a long-lived TCP echo server that mirrors bytes for every client.
#[cfg(feature = "v5")]
///
/// `nodelay` controls Nagle on the echo's side of each connection: it stands
/// in for the relay's peer, so toggling it lets the relay benchmark contrast a
/// cooperating (nodelay) peer against a Nagle one.
fn spawn_echo(nodelay: bool) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut stream = stream;
                let _ = stream.set_nodelay(nodelay);
                let mut buf = [0u8; 64 * 1024];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 || stream.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

/// Spawns a long-lived proxy serving an unbounded stream of clients.
#[cfg(feature = "v5")]
fn spawn_proxy() -> SocketAddr {
    let server = Server::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    thread::spawn(move || {
        let _ = server.serve();
    });
    addr
}

#[cfg(feature = "v5")]
fn bench_parse(c: &mut Criterion) {
    // VER, CMD=CONNECT, RSV, ATYP=IPv4, 127.0.0.1, port 443.
    let request = [5u8, 1, 0, 1, 127, 0, 0, 1, 0x01, 0xBB];
    // VER, REP=succeeded, RSV, ATYP=IPv4, 127.0.0.1, port 1080.
    let reply = [5u8, 0, 0, 1, 127, 0, 0, 1, 0x04, 0x38];
    // VER, NMETHODS=2, [NoAuth, Username/Password].
    let identifier = [5u8, 2, 0x00, 0x02];

    let mut group = c.benchmark_group("parse");
    group.bench_function("request", |b| {
        b.iter(|| Request::read_from(&mut request.as_slice()).unwrap())
    });
    group.bench_function("reply", |b| {
        b.iter(|| Reply::read_from(&mut reply.as_slice()).unwrap())
    });
    group.bench_function("identifier", |b| {
        b.iter(|| Identifier::read_from(&mut identifier.as_slice()).unwrap())
    });
    group.finish();
}

#[cfg(feature = "v5")]
fn bench_handshake(c: &mut Criterion) {
    let echo = spawn_echo(true);
    let proxy = spawn_proxy();

    c.bench_function("handshake/connect", |b| {
        b.iter(|| {
            let stream = Client::new(proxy).connect(echo).expect("connect");
            drop(stream);
        })
    });
}

#[cfg(feature = "v5")]
fn bench_relay(c: &mut Criterion) {
    let proxy = spawn_proxy();
    let echo_nodelay = spawn_echo(true);

    // Relay arms: (label, client-side nodelay, stand-in peer). The Nagle
    // baseline contrasts the full-path nodelay state against pre-change
    // behavior, but it is slow by design (~37ms/iter at 64 KiB), so it is
    // gated behind the `bench-nagle` feature to keep a default run fast.
    #[cfg_attr(not(feature = "bench-nagle"), allow(unused_mut))]
    let mut arms: Vec<(&str, bool, SocketAddr)> = vec![("nodelay", true, echo_nodelay)];
    #[cfg(feature = "bench-nagle")]
    arms.push(("nagle", false, spawn_echo(false)));

    let mut group = c.benchmark_group("relay");
    for size in [256usize, 4096, 65536] {
        let payload = vec![0xABu8; size];
        for &(label, nodelay, echo) in &arms {
            let mut stream = Client::new(proxy).connect(echo).expect("connect");
            // The crate sets nodelay on the client stream; override it to match
            // the scenario so the "nagle" arm reflects pre-change behavior.
            // (The server's relay sockets always run nodelay — unreachable from
            // here — so this isolates the client+peer contribution.)
            stream.set_nodelay(nodelay).expect("set_nodelay");
            let mut buf = vec![0u8; size];

            group.throughput(Throughput::Bytes(size as u64));
            group.bench_with_input(BenchmarkId::new(label, size), &size, |b, _| {
                b.iter(|| {
                    stream.write_all(&payload).unwrap();
                    stream.read_exact(&mut buf).unwrap();
                })
            });
        }
    }
    group.finish();
}

// A manual `main` (rather than `criterion_main!`) so the registered groups can
// be selected by feature at compile time — the macro form cannot be
// conditionally fed its target list.
fn main() {
    let mut criterion = Criterion::default()
        .with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)))
        .configure_from_args();

    #[cfg(feature = "v4")]
    bench_v4_parse(&mut criterion);

    #[cfg(feature = "v5")]
    {
        bench_parse(&mut criterion);
        bench_handshake(&mut criterion);
        bench_relay(&mut criterion);
    }

    criterion.final_summary();
}

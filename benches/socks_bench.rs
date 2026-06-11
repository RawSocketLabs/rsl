//! Performance benchmarks for the SOCKS5 crate.
//!
//! Three groups, cheapest first:
//!   * `parse`     — pure-CPU wire decoding of the hot message types.
//!   * `handshake` — a full negotiate + CONNECT round trip over loopback.
//!   * `relay`     — steady-state byte throughput on an established tunnel.
//!
//! Run measurements:      cargo bench -p socks
//! Emit flamegraph SVGs:  cargo bench -p socks -- --profile-time 5
//! (criterion writes reports under target/criterion/.)

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pprof::criterion::{Output, PProfProfiler};

use socks::client::Client;
use socks::server::Server;
use socks::v5::{Identifier, Reply, Request};

/// Spawns a long-lived TCP echo server that mirrors bytes for every client.
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
fn spawn_proxy() -> SocketAddr {
    let server = Server::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    thread::spawn(move || {
        let _ = server.serve();
    });
    addr
}

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

criterion_group! {
    name = benches;
    config = Criterion::default()
        .with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench_parse, bench_handshake, bench_relay
}
criterion_main!(benches);

//! Performance benchmarks for the TFTP crate.
//!
//! Two groups:
//!
//!   * `parse`    — pure-CPU decoding of the hot packet types.
//!   * `transfer` — a full download over UDP loopback (handshake + DATA/ACK
//!     round trips), measured as throughput in bytes.
//!
//! Run measurements:      cargo bench -p tftp
//! Emit flamegraph SVGs:  cargo bench -p tftp -- --profile-time 5
//! Add a 1 MiB arm:       cargo bench -p tftp --features bench-slow
//! (criterion writes reports under target/criterion/.)

use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput};
use pprof::criterion::{Output, PProfProfiler};

use tftp::client::Client;
use tftp::server::{MemoryStore, Server};
use tftp::wire::{Ack, Data, Packet, Request, TransferMode};

/// Decodes the three hot packet types.
fn bench_parse(c: &mut Criterion) {
    let rrq = Request::read("boot.img", TransferMode::Octet).encode();
    let data = Data::new(1, vec![0xAB; 512]).encode();
    let ack = Ack::new(1).encode();

    let mut group = c.benchmark_group("parse");
    group.bench_function("rrq", |b| b.iter(|| Packet::decode(&rrq).unwrap()));
    group.bench_function("data512", |b| b.iter(|| Packet::decode(&data).unwrap()));
    group.bench_function("ack", |b| b.iter(|| Packet::decode(&ack).unwrap()));
    group.finish();
}

/// Spawns a long-lived server preloaded with each benchmark payload.
fn spawn_server(files: &[(&str, Vec<u8>)]) -> SocketAddr {
    let mut store = MemoryStore::new();
    for (name, bytes) in files {
        store = store.with_file(*name, bytes.clone());
    }
    let server = Server::bind("127.0.0.1:0", Arc::new(store))
        .unwrap()
        .with_timeout(Duration::from_secs(2));
    let addr = server.local_addr().unwrap();
    thread::spawn(move || {
        let _ = server.serve();
    });
    addr
}

/// Full download throughput across a range of file sizes.
fn bench_transfer(c: &mut Criterion) {
    let mut sizes: Vec<usize> = vec![4 * 1024, 64 * 1024];
    if cfg!(feature = "bench-slow") {
        sizes.push(1024 * 1024);
    }

    let files: Vec<(&str, Vec<u8>)> = sizes
        .iter()
        .map(|&n| {
            let name: &'static str = Box::leak(format!("f{n}").into_boxed_str());
            (name, vec![0x5A; n])
        })
        .collect();
    let addr = spawn_server(&files);

    let mut group = c.benchmark_group("transfer");
    for (name, bytes) in &files {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("download", bytes.len()),
            name,
            |b, name| {
                b.iter(|| {
                    let got = Client::new(addr).get(name).unwrap();
                    assert_eq!(got.len(), bytes.len());
                })
            },
        );
    }
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default()
        .with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)))
        .configure_from_args();
    bench_parse(&mut criterion);
    bench_transfer(&mut criterion);
    criterion.final_summary();
}

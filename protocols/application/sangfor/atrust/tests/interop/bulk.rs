//! Release-only throughput qualification against an authenticated, independent TLS peer.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use os_random::OsRandom;
use sangfor_atrust::{
    net::dial,
    policy::Policy,
    tcp::{Destination, Identity, Stream},
};
use tls::ServerVerification;
use x509::Certificate;

use super::{
    interop::{Server, certificates},
    support::fixtures::{DEVICE_ID, V21},
};

const MIB: usize = 1024 * 1024;
const CHUNK_MIB: u16 = 4;

/// Acknowledge each fully checked upload, or request and verify one whole download.
/// The sequence number catches duplication or reordering of otherwise identical chunks.
fn transfer(
    stream: &mut (impl Read + Write),
    upload: bool,
    payload: &mut [u8],
    received: &mut [u8],
) -> f64 {
    let start = Instant::now();
    let mut chunks = 0_u32;
    while start.elapsed() < Duration::from_secs(30) {
        payload[..4].copy_from_slice(&chunks.to_be_bytes());
        if upload {
            stream.write_all(b"U").unwrap();
            stream.write_all(payload).unwrap();
            stream.flush().unwrap();
            let mut ack = [0];
            stream.read_exact(&mut ack).unwrap();
            assert_eq!(ack, *b"K");
        } else {
            stream.write_all(b"D").unwrap();
            stream.flush().unwrap();
            stream.read_exact(received).unwrap();
            assert!(received == payload, "download differs at chunk {chunks}");
        }
        chunks += 1;
    }
    f64::from(CHUNK_MIB) * f64::from(chunks) / start.elapsed().as_secs_f64()
}

#[test]
#[ignore = "six-minute release qualification, needs python3 and openssl"]
#[expect(
    clippy::print_stderr,
    reason = "report each measured rate with --nocapture"
)]
fn authenticated_bulk_throughput() {
    #[cfg(all(feature = "aes-gcm-accel", target_arch = "x86_64"))]
    let accelerated =
        std::is_x86_feature_detected!("aes") && std::is_x86_feature_detected!("pclmulqdq");
    #[cfg(not(all(feature = "aes-gcm-accel", target_arch = "x86_64")))]
    let accelerated = false;
    let minimum = if accelerated { 75.0 } else { 10.0 };
    let directory = certificates("bulk");
    let peer = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/interop/bulk_peer.py");
    let mut server = Server(
        Command::new("python3")
            .arg("-I")
            .arg(peer)
            .current_dir(&directory)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let ca = fs::read(directory.join("ca.der")).unwrap();
    let leaf = fs::read(directory.join("cert.der")).unwrap();
    let anchors = [Certificate::from_der(&ca).unwrap()];
    let verification = ServerVerification::Path {
        trust_anchors: &anchors,
        time: Certificate::from_der(&leaf)
            .unwrap()
            .tbs_certificate()
            .validity()
            .not_before,
        dns_name: "atrust.test",
    };
    let policy = Policy::parse(V21.as_bytes(), "atrust.test").unwrap();
    let destination = Destination::route(&policy, "app.example.net", 443, false, |_| None).unwrap();
    let identity = Identity::new("test-sid", "test-user", DEVICE_ID, 1, &mut OsRandom).unwrap();
    let chunk = usize::from(CHUNK_MIB) * MIB;
    let mut payload: Vec<u8> = (0_u8..251).cycle().take(chunk).collect();
    let mut received = vec![0; chunk];
    for trial in 1..=3 {
        for framed in [false, true] {
            let tls = dial(
                address.trim(),
                Duration::from_secs(30),
                verification,
                &mut OsRandom,
            )
            .unwrap();
            let mut stream = Stream::open(tls, &identity, &destination, framed).unwrap();
            for upload in [true, false] {
                let rate = transfer(&mut stream, upload, &mut payload, &mut received);
                eprintln!("trial {trial}, framed={framed}, upload={upload}: {rate:.2} MiB/s");
                assert!(
                    rate >= minimum,
                    "sustained throughput below {minimum} MiB/s"
                );
            }
            stream.write_all(b"E").unwrap();
            stream.flush().unwrap();
        }
    }
    assert!(server.0.wait().unwrap().success());
    fs::remove_dir_all(directory).unwrap();
}

//! **DNS** — the flagship: a real RFC 1035 message, header to answer, including the feature
//! that motivated bnb's free cursor seeking — **name-compression pointers**.
//!
//! It folds the bnb stack: the 16-bit flags word is a `#[bitfield]` of two `#[derive(BitEnum)]`s
//! and six bools; the four sections are `count`-driven `Vec`s of nested `#[bin]` records; domain
//! names are a length-prefixed label list parsed by a custom `parse_with`/`write_with` codec;
//! and a compressed answer name is a **pointer back to an earlier offset**, which the parser
//! *follows by seeking* and then resumes — the in-memory cursor makes that pointer-chase free
//! (`seek_to_bit`), no second pass.
//!
//! It finishes by **sending the query and receiving the answer over a real UDP loopback socket**
//! (DNS is datagram-based, so each message is one packet — no framing), the same `#[bin]` codec
//! on both ends.
//!
//! Output goes through `tracing`. Run with: `cargo run -p bitsandbytes --example dns`

use bnb::{BitEnum, BitReader, Sink, Source, bin, bitfield, u3, u4};
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;
use tracing::info;

// --- the 16-bit flags word: a bitfield of two enums + six bools ----------------

/// 4-bit DNS opcode. `catch_all` preserves an unknown opcode (dual-use).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum OpCode {
    Query,
    IQuery,
    Status,
    #[catch_all]
    Other(u4),
}

/// 4-bit response code. `catch_all` preserves an unknown rcode.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    NxDomain,
    #[catch_all]
    Other(u4),
}

/// The DNS header flags (RFC 1035 §4.1.1), MSB-first, big-endian — `QR Opcode AA TC RD RA Z RCODE`.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    qr: bool,       // 0 = query, 1 = response
    opcode: OpCode, // 4
    aa: bool,       // authoritative answer
    tc: bool,       // truncated
    rd: bool,       // recursion desired
    ra: bool,       // recursion available
    z: u3,          // reserved (must be 0)
    rcode: RCode,   // 4  → 1+4+1+1+1+1+3+4 = 16
}

// --- domain names: a custom codec (and the compression-pointer chase) ----------

/// Read a DNS name into its labels. A name is a run of length-prefixed labels ending in a
/// zero byte — *or* a **compression pointer** (top two bits set) giving a 14-bit offset to an
/// earlier name in the message, which we follow by seeking and then resume after the pointer.
fn read_name<S: Source>(r: &mut S) -> Result<Vec<String>, bnb::BitError> {
    let mut labels = Vec::new();
    let mut resume_at: Option<usize> = None; // where to continue after the first pointer
    let mut hops = 0;
    loop {
        let n: u8 = r.read()?;
        if n == 0 {
            break; // the root label terminates the name
        }
        if n & 0xC0 == 0xC0 {
            // A pointer: 14-bit offset = (low 6 bits of n) << 8 | next byte.
            let lo: u8 = r.read()?;
            let offset = (((n & 0x3F) as usize) << 8) | lo as usize;
            // Remember where the *main* stream continues (only for the first jump), then seek.
            resume_at.get_or_insert_with(|| r.bit_pos());
            hops += 1;
            if hops > 128 {
                break; // defensive: don't chase a pointer loop forever (dual-use)
            }
            r.seek_to_bit(offset * 8)?; // the cursor jumps — free, it's in-memory
            continue;
        }
        // A normal label: `n` bytes of (ASCII) text.
        let mut bytes = Vec::with_capacity(n as usize);
        for _ in 0..n {
            bytes.push(r.read::<u8>()?);
        }
        labels.push(String::from_utf8_lossy(&bytes).into_owned());
    }
    if let Some(pos) = resume_at {
        r.seek_to_bit(pos)?; // restore the cursor to just past the pointer we followed
    }
    Ok(labels)
}

/// Write a name as length-prefixed labels + a root terminator. We always emit the
/// **uncompressed** form (valid, if larger) — compression is a decode-time optimization.
fn write_name<K: Sink>(labels: &[String], w: &mut K) -> Result<(), bnb::BitError> {
    for label in labels {
        w.write(label.len() as u8)?;
        for &b in label.as_bytes() {
            w.write(b)?;
        }
    }
    w.write(0u8) // root
}

/// Join labels back into a dotted name for display.
fn dotted(labels: &[String]) -> String {
    labels.join(".")
}

// --- the records and the message -----------------------------------------------

/// A question: a name + query type/class.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Question {
    #[br(parse_with = read_name)]
    #[bw(write_with = write_name)]
    name: Vec<String>,
    qtype: u16,
    qclass: u16,
}

/// A resource record: a name (often compressed), type/class/TTL, and length-prefixed rdata.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    #[br(parse_with = read_name)]
    #[bw(write_with = write_name)]
    name: Vec<String>,
    rtype: u16,
    rclass: u16,
    ttl: u32,
    #[br(temp)]
    #[bw(calc = self.rdata.len() as u16)]
    rdlength: u16,
    #[br(count = rdlength)]
    rdata: Vec<u8>,
}

/// A whole DNS message: the header, then the four `count`-driven sections. Each `count` is read
/// into a temp and recomputed on write from the matching `Vec`'s length, so they can't drift.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Message {
    id: u16,
    flags: Flags,
    #[br(temp)]
    #[bw(calc = self.questions.len() as u16)]
    qdcount: u16,
    #[br(temp)]
    #[bw(calc = self.answers.len() as u16)]
    ancount: u16,
    #[br(temp)]
    #[bw(calc = self.authority.len() as u16)]
    nscount: u16,
    #[br(temp)]
    #[bw(calc = self.additional.len() as u16)]
    arcount: u16,
    #[br(count = qdcount)]
    #[nested]
    questions: Vec<Question>,
    #[br(count = ancount)]
    #[nested]
    #[builder(default)]
    answers: Vec<Record>,
    #[br(count = nscount)]
    #[nested]
    #[builder(default)]
    authority: Vec<Record>,
    #[br(count = arcount)]
    #[nested]
    #[builder(default)]
    additional: Vec<Record>,
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A tiny DNS server: receive one query datagram, answer it with a single A record, send it back.
fn serve_one(sock: &UdpSocket) -> std::io::Result<()> {
    let mut buf = [0u8; 512]; // classic DNS-over-UDP messages fit in 512 bytes
    let (n, client) = sock.recv_from(&mut buf)?;
    let query = Message::decode_exact(&buf[..n]).expect("decode query");
    let q = query.questions[0].clone();
    let name = q.name.clone();
    let response = Message::builder()
        .id(query.id) // echo the transaction id
        .flags(Flags::new().with_qr(true).with_rd(true).with_ra(true)) // a recursive response
        .questions(vec![q])
        .answers(vec![Record {
            name,
            rtype: 1,  // A
            rclass: 1, // IN
            ttl: 60,
            rdata: vec![93, 184, 216, 34],
        }])
        .build()
        .expect("build response");
    sock.send_to(&response.to_bytes().expect("encode response"), client)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    // ===== build + encode a query with the builder (answers/authority/additional default) =====
    let query = Message::builder()
        .id(0x1234)
        .flags(Flags::new().with_rd(true)) // standard recursive A? query
        .questions(vec![Question {
            name: vec!["example".into(), "com".into()],
            qtype: 1,  // A
            qclass: 1, // IN
        }])
        .build()?;
    let q_bytes = query.to_bytes()?;
    info!(
        question = %dotted(&query.questions[0].name),
        bytes = %hex(&q_bytes),
        "built query",
    );
    assert_eq!(Message::decode_exact(&q_bytes)?, query); // round-trips

    // ===== decode a real response that uses a compression pointer =====
    // id, flags(0x8180: response+RD+RA), qd=1 an=1 ns=0 ar=0, question "example.com" A IN, then
    // an answer whose NAME is `c0 0c` — a pointer to offset 0x0c (12), the question name —
    // followed by A IN, ttl 60, rdata 93.184.216.34.
    let wire: &[u8] = &[
        0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // header
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm',
        0x00, // qname @12
        0x00, 0x01, 0x00, 0x01, // qtype=A qclass=IN
        0xc0, 0x0c, // answer name: POINTER to offset 12 (the question name)
        0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, // type=A class=IN ttl=60
        0x00, 0x04, 0x5d, 0xb8, 0xd8, 0x22, // rdlength=4, rdata=93.184.216.34
    ];
    info!(
        len = wire.len(),
        "decoding a response with a compressed answer name"
    );

    let resp = Message::decode_exact(wire)?;
    info!("the full decoded structure:\n{resp:#?}"); // the whole message, pretty-printed
    let answer = &resp.answers[0];
    let ip = &answer.rdata;
    info!(
        qr = resp.flags.qr(),
        opcode = ?resp.flags.opcode(),
        ra = resp.flags.ra(),
        rcode = ?resp.flags.rcode(),
        question = %dotted(&resp.questions[0].name),
        // `name` was the 2-byte pointer `c0 0c` on the wire — the parser SEEKED back to
        // offset 12 and reconstructed the full name from there:
        answer_name = %dotted(&answer.name),
        ttl = answer.ttl,
        address = %format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]),
        "decoded response (compression pointer followed)",
    );
    assert_eq!(dotted(&answer.name), "example.com"); // the pointer resolved to the question name
    assert_eq!(answer.rdata, vec![93, 184, 216, 34]);

    // The decoded value round-trips through a *re-encode* (now uncompressed — `c0 0c` becomes
    // the full labels again), so it's struct-identical though not byte-identical:
    let reencoded = resp.to_bytes()?;
    assert_eq!(Message::decode_exact(&reencoded)?, resp);
    info!(
        on_wire = wire.len(),
        re_encoded = reencoded.len(),
        "round-trips by value; re-encode is uncompressed (larger), as expected",
    );

    // Show the seek explicitly: read just the answer name (the `c0 0c` at offset 29) on its own.
    let mut r = BitReader::new(wire);
    r.seek_to_bit(29 * 8)?;
    assert_eq!(read_name(&mut r)?, vec!["example", "com"]);

    // ===== send + receive over a real UDP loopback socket =====
    // DNS is datagram-based, so each message is exactly one UDP packet — no framing needed. A
    // tiny server thread answers one query; the client sends the query built above and decodes
    // the reply. Both sides run the same `#[bin]` codec.
    let server = UdpSocket::bind("127.0.0.1:0")?; // ephemeral loopback port
    let server_addr = server.local_addr()?;
    let server_thread = thread::spawn(move || serve_one(&server));

    let client = UdpSocket::bind("127.0.0.1:0")?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?; // safety net, not normally hit
    client.send_to(&q_bytes, server_addr)?;
    info!(to = %server_addr, bytes = q_bytes.len(), "client → query over UDP loopback");

    let mut inbox = [0u8; 512];
    let (n, from) = client.recv_from(&mut inbox)?;
    let reply = Message::decode_exact(&inbox[..n])?;
    let a = &reply.answers[0];
    info!(
        from = %from,
        id = %format!("0x{:04x}", reply.id),
        name = %dotted(&a.name),
        address = %format!("{}.{}.{}.{}", a.rdata[0], a.rdata[1], a.rdata[2], a.rdata[3]),
        "client ← response decoded",
    );
    assert_eq!(a.name, query.questions[0].name); // the server echoed our question's name
    server_thread.join().expect("server thread panicked")?;

    info!("all checks passed");
    Ok(())
}

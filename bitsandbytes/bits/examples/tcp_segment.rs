//! A TCP-segment header that exercises all of `bits` together: `#[bitflags]` for
//! the control flags, `#[bitfield]` + `#[derive(BitsBuilder)]` for the
//! data-offset/flags word (with a required-field builder), and `#[binrw]` for the
//! whole header — no `map` glue anywhere.
//!
//! Run with: `cargo run -p bits --example tcp_segment`

use binrw::{BinRead, BinWrite, binrw, io::Cursor};
use bits::{BitsBuilder, bitfield, bitflags, u4};

/// The 8 TCP control flags.
#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Control {
    fin: bool,
    syn: bool,
    rst: bool,
    psh: bool,
    ack: bool,
    urg: bool,
    ece: bool,
    cwr: bool,
}

/// Data offset (4) + reserved (4) + the 8 control flags = a 16-bit word
/// (simplified: real TCP splits the reserved bits with the NS flag). The builder
/// requires `data_offset` and `control`; `reserved` defaults.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(BitsBuilder, Clone, Copy, Debug, PartialEq, Eq)]
struct OffsetFlags {
    data_offset: u4,
    #[builder(default)]
    reserved: u4,
    control: Control,
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
struct TcpHeader {
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    offset_flags: OffsetFlags, // bitfield-of-flags embeds directly
    window: u16,
    checksum: u16,
    urgent: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build the offset/flags word: data offset required, reserved defaulted, and
    // a SYN|ACK control set assembled with the flag operators.
    let offset_flags = OffsetFlags::builder()
        .data_offset(u4::new(5))
        .control(Control::SYN | Control::ACK)
        .build()?;

    println!(
        "control flags set: {:?}",
        offset_flags.control().iter().collect::<Vec<_>>()
    );

    let header = TcpHeader {
        src_port: 443,
        dst_port: 51000,
        seq: 0x1000,
        ack: 0x2000,
        offset_flags,
        window: 65535,
        checksum: 0,
        urgent: 0,
    };

    let mut buf = Cursor::new(Vec::new());
    header.write(&mut buf)?;
    let bytes = buf.into_inner();
    println!("encoded {}-byte TCP header: {bytes:02x?}", bytes.len());

    let parsed = TcpHeader::read(&mut Cursor::new(&bytes))?;
    let of = parsed.offset_flags;
    println!(
        "decoded: {} -> {}, data_offset={}, SYN={}, ACK={}, FIN={}",
        parsed.src_port,
        parsed.dst_port,
        of.data_offset(),
        of.control().syn(),
        of.control().ack(),
        of.control().fin(),
    );
    assert!(of.control().contains(Control::SYN | Control::ACK));
    assert!(!of.control().fin());

    // A required field omitted is caught at build() time.
    let err = OffsetFlags::builder().control(Control::empty()).build();
    println!("omitting data_offset -> {err:?}");
    assert!(err.is_err());
    Ok(())
}

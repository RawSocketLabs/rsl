use bnb::bin;

#[bin(ctx(fid: u8, code: u8))]
enum MissingCondition {
    #[bin(tag(fid = 0x00, code = 0x00))]
    First(u8),
    #[bin(tag(code = 0x01))]
    Second(u8),
}

#[bin(ctx(fid: u8, code: u8))]
enum DuplicateCondition {
    #[bin(tag(fid = 0x00, fid = 0x01))]
    First(u8),
}

#[bin(ctx(fid: u8, code: u8))]
enum MissingCapture {
    #[bin(tag(fid = 0x00, code = 0x00))]
    First(u8),
    #[catch_all]
    Unknown { code: u8, raw: u8 },
}

#[bin(ctx(fid: u8, code: u8))]
enum CaptureWithCodec {
    #[bin(tag(fid = 0x00, code = 0x00))]
    First(u8),
    #[catch_all]
    Unknown { #[br(temp)] fid: u8, code: u8, raw: u8 },
}

#[bin(ctx(fid: u8, code: u8))]
enum UnknownParameter {
    #[bin(tag(other = 0x00, code = 0x00))]
    First(u8),
}

#[bin(ctx(fid: u8, code: u8), tag = code)]
enum MixedSyntax {
    #[bin(tag(fid = 0x00, code = 0x00))]
    First(u8),
}

fn main() {}

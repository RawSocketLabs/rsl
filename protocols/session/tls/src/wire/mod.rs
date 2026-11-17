//! RFC 5246 wire types. Codecs only: no I/O, no cryptography, no policy.

mod content_type;
mod protocol_version;
mod record_header;

pub use content_type::ContentType;
pub use protocol_version::ProtocolVersion;
pub use record_header::RecordHeader;

//! The whole DNS message (RFC 1035 §4.1).

use crate::header::{Header, State};
use crate::name::CompressionDict;
use crate::question::Question;
use crate::record::Record;
use bnb::bin;
use bnb::bitstream::{BitEncode, BitWriter};

/// A complete DNS message: a header followed by the four sections, each sized by the
/// header's corresponding count.
///
/// Decode follows name-compression pointers inline, so every `Name` in a decoded message
/// is fully resolved. The header's four section counts are
/// [`WireLen`](bnb::WireLen)-derived (`auto_len` below): a freshly-built message's counts
/// fill themselves from the sections on encode — no sync step — while a
/// [`set`](bnb::WireLen::set) count is honored verbatim (dual-use forging).
//~ models rfc1035#4.1 part="Message format"
#[bin(big, auto_len(
    header.qdcount = count(questions),
    header.ancount = count(answers),
    header.nscount = count(authorities),
    header.arcount = count(additional),
))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The 12-byte header.
    pub header: Header,
    /// The question section.
    #[br(count = header.qdcount.to_count())]
    pub questions: Vec<Question>,
    /// The answer section.
    #[br(count = header.ancount.to_count())]
    pub answers: Vec<Record>,
    /// The authority (name-server) section.
    #[br(count = header.nscount.to_count())]
    pub authorities: Vec<Record>,
    /// The additional section.
    #[br(count = header.arcount.to_count())]
    pub additional: Vec<Record>,
}

impl Message {
    /// Assemble a message whose header counts auto-derive from its sections: it resets the
    /// four counts to [`auto()`](bnb::WireLen::auto), so encoding fills them from the
    /// section lengths. To forge a header whose counts *disagree* with the sections
    /// (dual-use), build the `Message` directly and [`set`](bnb::WireLen::set) a count.
    #[must_use]
    pub fn assemble(
        mut header: Header,
        questions: Vec<Question>,
        answers: Vec<Record>,
        authorities: Vec<Record>,
        additional: Vec<Record>,
    ) -> Self {
        header.qdcount = bnb::WireLen::auto();
        header.ancount = bnb::WireLen::auto();
        header.nscount = bnb::WireLen::auto();
        header.arcount = bnb::WireLen::auto();
        Message {
            header,
            questions,
            answers,
            authorities,
            additional,
        }
    }

    /// A minimal recursive query for a single question (id + one question; RD set).
    #[must_use]
    pub fn query(id: u16, question: Question) -> Self {
        let state = State::new()
            .with_response(false)
            .with_recursion_desired(true);
        let header = Header {
            id,
            state,
            qdcount: bnb::WireLen::auto(),
            ancount: bnb::WireLen::auto(),
            nscount: bnb::WireLen::auto(),
            arcount: bnb::WireLen::auto(),
        };
        Message::assemble(header, vec![question], vec![], vec![], vec![])
    }

    /// Encode with **name compression** (RFC 1035 §4.1.4): a name whose label-suffix
    /// already appeared earlier in the message is written as a pointer to that first
    /// occurrence. Seeds a fresh [`CompressionDict`] into the sink's scratch and drives the
    /// ordinary codec — every name shares the one dictionary because they all write through
    /// the same sink.
    ///
    /// The dual of [`to_bytes`](Self::to_bytes) (uncompressed): both decode back to the
    /// same `Message` (decode follows pointers inline), but the compressed form is smaller
    /// when names repeat.
    ///
    /// # Errors
    /// Propagates any encode error (e.g. a label over 63 bytes).
    pub fn to_compressed_bytes(&self) -> Result<Vec<u8>, bnb::bitstream::BitError> {
        let mut w = BitWriter::with_layout(<Message as BitEncode>::LAYOUT)
            .with_scratch(Box::new(CompressionDict::new()));
        self.bit_encode(&mut w)?;
        Ok(w.into_bytes())
    }
}

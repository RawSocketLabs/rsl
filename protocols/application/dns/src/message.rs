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
/// is fully resolved. Encode writes names **uncompressed** (Increment 1). Construct via
/// [`Message::new`] to keep the header counts in sync with the sections.
//~ models rfc1035#4.1 part="Message format"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The 12-byte header.
    pub header: Header,
    /// The question section.
    #[br(count = header.qdcount)]
    pub questions: Vec<Question>,
    /// The answer section.
    #[br(count = header.ancount)]
    pub answers: Vec<Record>,
    /// The authority (name-server) section.
    #[br(count = header.nscount)]
    pub authorities: Vec<Record>,
    /// The additional section.
    #[br(count = header.arcount)]
    pub additional: Vec<Record>,
}

impl Message {
    /// Assemble a message, deriving the header's section counts from the sections
    /// (overwriting whatever counts the passed-in header carried) so the wire form is
    /// self-consistent. To forge a header whose counts *disagree* with the sections
    /// (dual-use), use the generated [`Message::new`] and set the counts directly.
    #[must_use]
    pub fn assemble(
        mut header: Header,
        questions: Vec<Question>,
        answers: Vec<Record>,
        authorities: Vec<Record>,
        additional: Vec<Record>,
    ) -> Self {
        header.qdcount = questions.len() as u16;
        header.ancount = answers.len() as u16;
        header.nscount = authorities.len() as u16;
        header.arcount = additional.len() as u16;
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
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
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

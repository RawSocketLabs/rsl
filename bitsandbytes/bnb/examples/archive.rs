//! **archive** — random-access reading with `SeekReader`: a container whose header is an index
//! of `(offset, length)` records that we seek to and read **out of order**. The large-file /
//! container case — you jump around a `Read + Seek` source instead of streaming it front to
//! back. (`SeekReader::new(File)` works identically; a `Cursor` keeps the example self-contained.)
//!
//! Run with: `cargo run -p bitsandbytes --example archive`

use bnb::{SeekReader, Source, bin};
use std::io::Cursor;

/// One index entry: where a blob lives in the file (byte offset from the start) and its length.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Entry {
    offset: u32,
    len: u16,
}

/// The archive header: a count-driven index. Blobs live at the offsets the entries point at —
/// not necessarily in order.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Index {
    #[br(temp)]
    #[bw(calc = self.entries.len() as u8)]
    count: u8,
    #[br(count = count)]
    entries: Vec<Entry>,
}

fn main() -> Result<(), bnb::BitError> {
    let blob_a = b"hello".as_slice();
    let blob_b = b"world!!".as_slice();

    // header = count(1) + 2 * (offset u32 + len u16 = 6) = 13 bytes; blobs follow.
    let header_len = 1 + 2 * 6;
    let off_a = header_len as u32;
    let off_b = off_a + blob_a.len() as u32;
    let index = Index {
        entries: vec![
            Entry {
                offset: off_a,
                len: blob_a.len() as u16,
            },
            Entry {
                offset: off_b,
                len: blob_b.len() as u16,
            },
        ],
    };

    let mut file = index.to_bytes()?;
    file.extend_from_slice(blob_a);
    file.extend_from_slice(blob_b);
    println!("archive: {} bytes  {file:02x?}", file.len());

    // Read it back over a seekable source (a Cursor here; a std::fs::File is the same).
    let mut src = SeekReader::new(Cursor::new(file));
    let index = Index::decode(&mut src)?;
    println!("{index:#?}");

    // Random access: seek to each entry's offset and read its blob — reversed, to prove it.
    for entry in index.entries.iter().rev() {
        src.seek_to_bit(entry.offset as usize * 8)?;
        let mut blob = Vec::with_capacity(entry.len as usize);
        for _ in 0..entry.len {
            blob.push(src.read::<u8>()?);
        }
        println!(
            "  @{:>2} ({} bytes): {:?}",
            entry.offset,
            entry.len,
            String::from_utf8_lossy(&blob)
        );
    }

    println!("all checks passed");
    Ok(())
}

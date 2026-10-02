// --- Standard library ---
use ::std::io;

// --- Workspace dependencies ---
#[cfg(feature = "blocking")]
use bnb::{BitDecode, BitEncode};

// --- Internal modules ---
#[cfg(feature = "blocking")]
use super::MAX_FRAME_LEN;
use super::Stream;
#[cfg(feature = "blocking")]
use crate::error::Error;

#[cfg(feature = "blocking")]
impl<S: io::Read> Stream<S> {
    pub(crate) fn read_message<T: BitDecode + BitEncode>(&mut self) -> Result<T, Error> {
        let mut bytes = [0; MAX_FRAME_LEN];
        bnb::net::read_message(&mut self.inner, &mut self.buffered, &mut bytes).map_err(Error::from)
    }
}

#[cfg(feature = "blocking")]
impl<S: io::Write> Stream<S> {
    /// Encode a complete handshake message and flush it before the next protocol step.
    pub(crate) fn write_message<T: BitEncode>(&mut self, message: &T) -> Result<(), Error> {
        self.inner
            .write_all(&bnb::bitstream::encode_to_vec(message, T::LAYOUT)?)?;
        self.inner.flush()?;
        Ok(())
    }
}

impl<S: io::Read> io::Read for Stream<S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        match self.read_buffered(bytes)? {
            0 => self.inner.read(bytes),
            count => Ok(count),
        }
    }
}

impl<S: io::Write> io::Write for Stream<S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

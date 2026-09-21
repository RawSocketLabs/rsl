#[cfg(feature = "blocking")]
use super::MAX_FRAME_LEN;
use super::Stream;
#[cfg(feature = "blocking")]
use crate::error::Error;
use ::std::io;
#[cfg(feature = "blocking")]
use bnb::{BitDecode, BitEncode};

#[cfg(feature = "blocking")]
impl<S: io::Read> Stream<S> {
    pub(crate) fn read_message<T: BitDecode + BitEncode>(&mut self) -> Result<T, Error> {
        let mut bytes = [0; MAX_FRAME_LEN];
        bnb::net::read_message(&mut self.inner, &mut self.buffered, &mut bytes).map_err(Error::from)
    }
}

impl<S: io::Read> io::Read for Stream<S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(self.buffered_len()?);
        if count == 0 {
            self.inner.read(bytes)
        } else {
            self.read_buffered(&mut bytes[..count])?;
            Ok(count)
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

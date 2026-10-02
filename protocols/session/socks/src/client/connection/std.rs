// --- Standard library ---
use std::io::{self, IoSlice, IoSliceMut, Read, Write};

// --- Internal modules ---
use super::Connection;

impl<S: Read> Read for Connection<S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.read(bytes)
    }

    fn read_vectored(&mut self, bytes: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.stream.read_vectored(bytes)
    }
}

impl<S: Write> Write for Connection<S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream.write(bytes)
    }

    fn write_vectored(&mut self, bytes: &[IoSlice<'_>]) -> io::Result<usize> {
        self.stream.write_vectored(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

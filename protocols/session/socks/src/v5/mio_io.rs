use crate::{Stream, error::Error, io::stream::MAX_FRAME_LEN};
use bnb::{BitDecode, BitEncode};
use mio::Interest;
use std::io::{self, Read, Write};

pub(crate) struct Io<S> {
    stream: Option<Stream<S>>,
    output: Vec<u8>,
    written: usize,
    pub(crate) interest: Interest,
    budget: usize,
    pub(crate) again: bool,
}

impl<S: Read + Write> Io<S> {
    pub(crate) fn new(stream: S) -> Self {
        Self {
            stream: Some(Stream::new(stream)),
            output: Vec::new(),
            written: 0,
            interest: Interest::READABLE,
            budget: 64,
            again: false,
        }
    }

    pub(crate) fn stream(&mut self) -> Result<&mut Stream<S>, Error> {
        self.stream.as_mut().ok_or(Error::InvalidState)
    }

    pub(crate) fn take(&mut self) -> Option<Stream<S>> {
        self.stream.take()
    }

    pub(crate) fn begin(&mut self) {
        self.budget = 64;
        self.again = false;
    }

    fn spend(&mut self) -> bool {
        self.again = self.budget == 0;
        if self.again {
            false
        } else {
            self.budget -= 1;
            true
        }
    }

    pub(crate) fn close(&mut self) {
        self.stream = None;
        self.output.clear();
        self.again = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.stream.is_some()
    }

    pub(crate) fn queue<T: BitEncode>(&mut self, message: &T) -> Result<(), Error> {
        self.output = bnb::bitstream::encode_to_vec(message, T::LAYOUT)?;
        self.written = 0;
        self.interest = Interest::WRITABLE;
        Ok(())
    }

    pub(crate) fn flush(&mut self) -> Result<bool, Error> {
        while self.written < self.output.len() {
            if !self.spend() {
                return Ok(false);
            }
            let stream = self.stream.as_mut().ok_or(Error::InvalidState)?;
            match stream.write(&self.output[self.written..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                Ok(count) => self.written += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        loop {
            if !self.spend() {
                return Ok(false);
            }
            match self.stream()?.flush() {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        self.output.clear();
        self.written = 0;
        Ok(true)
    }

    pub(crate) fn receive<T: BitDecode + BitEncode>(&mut self) -> Result<Option<T>, Error> {
        self.interest = Interest::READABLE;
        let stream = self.stream.as_mut().ok_or(Error::InvalidState)?;
        let mut reader = Budget {
            reader: &mut stream.inner,
            remaining: &mut self.budget,
            again: &mut self.again,
        };
        let mut scratch = [0; MAX_FRAME_LEN];
        match bnb::net::read_message(&mut reader, &mut stream.buffered, &mut scratch)
            .map_err(Error::from)
        {
            Ok(message) => Ok(Some(message)),
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

// A budget yield must schedule an immediate continuation, unlike real WouldBlock.
struct Budget<'a, S> {
    reader: &'a mut S,
    remaining: &'a mut usize,
    again: &'a mut bool,
}
impl<S: Read> Read for Budget<'_, S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if *self.remaining == 0 {
            *self.again = true;
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            *self.remaining -= 1;
            self.reader.read(bytes)
        }
    }
}

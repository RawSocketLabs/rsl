//! Shared scalar checks for explicit guided-message methods, never decode hooks.
use crate::error::Error;

pub(super) fn version(actual: u8, expected: u8) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Version { expected, actual })
    }
}

pub(super) fn reserved(value: u8) -> Result<(), Error> {
    if value == 0 {
        Ok(())
    } else {
        Err(Error::Reserved(value))
    }
}

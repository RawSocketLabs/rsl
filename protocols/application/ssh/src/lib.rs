// TODO(hygiene): WIP crate; clippy::all ratcheted off pending implementation.
#![allow(clippy::all)]

mod encrypt;

pub struct Header {
    pub total_length: u32,
    pub padding_length: u8,
    pub payload: Vec<u8>,
    pub padding_data: Vec<u8>,
    pub mac: Vec<u8>,
}

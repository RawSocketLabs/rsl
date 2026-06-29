//! `DatagramSocket` is sealed — a downstream crate cannot implement it for its own type.
use bnb::DatagramSocket;
use std::io;
use std::net::SocketAddr;

struct MySocket;

impl DatagramSocket for MySocket {
    type Addr = SocketAddr;
    fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unimplemented!()
    }
    fn send_to(&self, _buf: &[u8], _addr: &SocketAddr) -> io::Result<usize> {
        unimplemented!()
    }
}

fn main() {}

use std::io::BufWriter;
use std::net::{IpAddr, Ipv4Addr, TcpStream};

use binrw::{io::NoSeek, BinWrite, BinWriterExt};

use crate::v1::message::Message;

pub struct Client {
    pub host: IpAddr,
    pub port: u16,
    pub transport: Transport,
}

impl Client {
    pub fn new() -> Client {
        Client {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 445,
            transport: Transport::Tcp(None),
        }
    }

    pub fn connect(&mut self, share: &str) {
        // TODO: Figure out if this is possible without NoSeek? or does NoSeek actually make the
        // most sense here (specifically for writeable events)?
        let mut writer = NoSeek::new(TcpStream::connect((self.host, self.port)).unwrap());
        Message::negotiate().write(&mut writer).unwrap();
    }

    fn tcp_connect(&mut self) {
        self.transport = Transport::Tcp(Some(TcpStream::connect((self.host, self.port)).unwrap()));
    }

    fn tcp_disconnect(&mut self) {}
}

pub enum Transport {
    Tcp(Option<TcpStream>),
    //NetBios,
    //AsyncTcp(),
    //AsyncNetBios,
}

pub enum Version {
    V1,
    V2,
    V3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client() {
        let mut client = Client::new();
        client.connect("C$");
    }
}

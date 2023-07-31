use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};

use binrw::BinRead;
use binrw::{io::Cursor, io::NoSeek, BinWrite};

use crate::v1::message::Header;
use crate::v1::message::Message;

pub struct Client {
    pub host: IpAddr,
    pub port: u16,
    pub transport: Transport,
    pub stream: Option<NoSeek<TcpStream>>,
    pub version: Option<Version>,
}

impl Client {
    pub fn new(host: IpAddr, port: u16, transport: Transport) -> Client {
        Client {
            host,
            port,
            transport,
            stream: None,
            version: None,
        }
    }

    pub fn connect(&mut self, share: &str) {
        self.tcp_connect();
        println!("{share}");
        let mut buffer = Cursor::new(Vec::new());
        buffer.write(&vec![0x00, 0x00, 0x00, 0x2F]).unwrap();
        Message::negotiate().write(&mut buffer).unwrap();
        self.stream
            .as_mut()
            .unwrap()
            .write_all(&buffer.into_inner())
            .unwrap();
        println!("Negotiate Message Sent");
        let mut buf = [0; 36];
        self.stream.as_mut().unwrap().read(&mut buf).unwrap();
        let header = Header::read(&mut Cursor::new(&buf[4..])).unwrap();
        println!("{:#?}", header);
    }

    fn tcp_connect(&mut self) {
        let tcp_stream = TcpStream::connect((self.host, self.port)).unwrap();
        let stream = NoSeek::new(tcp_stream);
        self.stream = Some(stream);
    }

    //fn tcp_disconnect(&mut self) {}
}

pub enum Transport {
    Tcp, //NetBios,
         //AsyncTcp,
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
    fn test_tcp_transport_client() {
        let mut client = Client::new("127.0.0.1".parse().unwrap(), 445, Transport::Tcp);
        client.connect("C$");
    }
}

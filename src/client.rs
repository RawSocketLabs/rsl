// Std
//use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};

// 3rd Party
use binrw::BinWrite;
use binrw::{io::BufReader, io::Cursor, io::NoSeek};

// Protocol Crates
use nbt::nbt::*;
use nbt::BinWrite as NbtBinWrite;

// Internal
//use crate::v1::message::cmd::NegotiateResponse;
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

    pub fn connect(&mut self, _share: &str) {
        // TODO: Make a stream split method. Should return a writer and a reader
        //self.tcp_connect();
        //let stream = self.stream();

        // Create a message buffer
        //let mut message = Cursor::new(Vec::new());
        //Message::negotiate().write(&mut message).unwrap();

        // Create a netbios buffer
        //let mut header = Cursor::new(Vec::new());
        //NetbiosHeader::header(message).write(&mut header).unwrap();

        //// Write the netbios header and message to the stream
        //stream.write_all(&header.into_inner()).unwrap();
        //stream.write_all(&message.into_inner()).unwrap();

        //// Read the response
        //let mut reader = BufReader::new(stream);
        //let netbios: NetbiosMessage = NetbiosMessage::read(&mut reader).unwrap();
        //let response: NegotiateResponse = Message::from(netbios.message).try_into().unwrap();

        // TODO: Fix this to be wrapped in a netbios message

        // TODO: Use a persistent write buffer
        // NOTE: Need to figure out a good way to do a buffer that
        // does not continually grow
        let mut smb_buf = Cursor::new(Vec::new());
        Message::negotiate().write(&mut smb_buf).unwrap();
        println!("MESSAGE SIZE: {}", smb_buf.get_ref().len());

        let session = SessionBuilder::default()
            .packet_type(PacketType::Message)
            .flags(Flags::new().with_reserved_checked(0).unwrap())
            .payload(smb_buf.into_inner())
            .build()
            .unwrap();

        let mut nbt_buf = Cursor::new(Vec::new());
        session.write(&mut nbt_buf).unwrap();
        println!("{:?}", nbt_buf.into_inner());
        println!("{:02x?}", session.as_bytes());

        //println!("SESSION MESSAGE TYPE: {:?}", session.packet_type);
        //println!("SESSION FLAGS: {:?}", session.flags);
        //println!("SESSION LENGTH: {:#02x}", session.length);
        //println!("SESSION PAYLOAD: {:?}", session.payload);

        // Accept the netbios header
        //let mut buffer = [0u8; 4];
        //stream.read_exact(&mut buffer).unwrap();

        // Accept the negotiate response
        //let mut reader = BufReader::new(stream);
        //let message: NegotiateResponse = Message::read(&mut reader).unwrap().try_into().unwrap();

        //println!("{:#?}", message);
    }

    fn stream(&mut self) -> &mut NoSeek<TcpStream> {
        self.stream.as_mut().unwrap()
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

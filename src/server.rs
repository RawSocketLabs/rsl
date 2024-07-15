use std::net::TcpStream;

use binrw::{io::BufReader, io::NoSeek, io::Read, io::Write, BinRead, BinWrite};

use crate::v5::*;

fn handle_client(mut stream: TcpStream) {
    let mut reader = BufReader::new(NoSeek::new(&stream));
    let mut writer = NoSeek::new(&stream);

    let id = Identifier::read(&mut reader).unwrap();
    println!("Received: {:?}", id);

    let offer = OfferBuilder::default()
        .method(Method::NoAuth)
        .build()
        .unwrap();

    offer.write_be(&mut writer).unwrap();

    let request = Request::read(&mut reader).unwrap();
    println!("Received: {:?}", request);

    let mut conn = match request.dest_addr {
        Address::V4(addr) => TcpStream::connect(format!("{}:{}", addr, request.dest_port))
            .expect("Failed to connect to target"),
        Address::Domain(ref domain) => TcpStream::connect(format!(
            "{}:{}",
            String::from_utf8(domain.domain.clone()).unwrap(),
            request.dest_port
        ))
        .expect("Failed to connect to target"),
        _ => panic!("Unsupported address type"),
    };

    let reply = ReplyBuilder::default()
        .reply(Response::Succeeded)
        .address_type(request.address_type)
        .bind_addr(request.dest_addr)
        .bind_port(request.dest_port)
        .build()
        .unwrap();

    println!("Replying: {:?}", reply);
    reply.write_be(&mut writer).unwrap();

    let mut buffer = [0; 4096];

    stream.set_nonblocking(true).unwrap();
    conn.set_nonblocking(true).unwrap();

    loop {
        if let Ok(num) = stream.read(&mut buffer) {
            conn.write_all(&buffer[..num]).unwrap();
        }

        if let Ok(num) = conn.read(&mut buffer) {
            stream.write_all(&buffer[..num]).unwrap();
        }
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn curl() {
        let listener = TcpListener::bind("127.0.0.1:9000").unwrap();

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            std::thread::spawn(move || {
                handle_client(stream);
            });
        }
    }
}

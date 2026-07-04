//! End-to-end resolver test over real loopback UDP: a fake server thread receives the query
//! and replies, exercising the full `Resolver::query` path (random id, socket, send/recv,
//! validation, answer extraction) — no external network.
#![cfg(feature = "client")]

mod e2e {
    use dns::{
        Header, Message, QClass, QType, Question, RClass, RData, RType, Record, Resolver, State,
        WireLen,
    };
    use std::net::{Ipv4Addr, UdpSocket};

    #[test]
    fn resolver_resolves_over_loopback_udp() {
        // A minimal "server": bind a UDP socket, and in a thread answer one query, echoing its
        // id and returning a single A record.
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = server.recv_from(&mut buf).unwrap();
            let query = Message::decode_exact(&buf[..n]).unwrap();
            assert_eq!(query.questions[0].name.to_string(), "example.com");

            let answer = Record {
                name: "example.com".parse().unwrap(),
                rtype: RType::A,
                class: RClass::Internet,
                ttl: 60,
                rdlength: WireLen::auto(),
                data: RData::A(Ipv4Addr::new(93, 184, 216, 34)),
            };
            let header = Header {
                id: query.header.id, // echo the query id
                state: State::new().with_response(true),
                qdcount: WireLen::auto(),
                ancount: WireLen::auto(),
                nscount: WireLen::auto(),
                arcount: WireLen::auto(),
            };
            let resp = Message::assemble(
                header,
                vec![Question {
                    name: "example.com".parse().unwrap(),
                    qtype: QType::A,
                    qclass: QClass::Internet,
                }],
                vec![answer],
                vec![],
                vec![],
            );
            server.send_to(&resp.to_bytes().unwrap(), from).unwrap();
        });

        let resolver = Resolver::new(server_addr);
        let ips = resolver.resolve_ipv4("example.com").unwrap();
        assert_eq!(ips, vec![Ipv4Addr::new(93, 184, 216, 34)]);

        handle.join().unwrap();
    }
}

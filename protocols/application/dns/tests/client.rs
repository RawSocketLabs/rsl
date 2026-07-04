//! End-to-end resolver tests over real loopback sockets: fake server threads receive the
//! query and reply, exercising the full `Resolver` UDP and TCP paths — no external network.
#![cfg(feature = "client")]

mod e2e {
    use dns::{
        Header, Message, QClass, QType, Question, RClass, RData, RType, Record, Resolver, State,
        WireLen,
    };
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener, UdpSocket};

    /// A response `Message` echoing `id` with one A record for `example.com`, optionally
    /// with the TC (truncated) bit set.
    fn a_response(id: u16, ip: Ipv4Addr, truncated: bool) -> Message {
        let answer = Record {
            name: "example.com".parse().unwrap(),
            rtype: RType::A,
            class: RClass::Internet,
            ttl: 60,
            rdlength: WireLen::auto(),
            data: RData::A(ip),
        };
        let header = Header {
            id,
            state: State::new().with_response(true).with_truncated(truncated),
            qdcount: WireLen::auto(),
            ancount: WireLen::auto(),
            nscount: WireLen::auto(),
            arcount: WireLen::auto(),
        };
        Message::assemble(
            header,
            vec![Question {
                name: "example.com".parse().unwrap(),
                qtype: QType::A,
                qclass: QClass::Internet,
            }],
            vec![answer],
            vec![],
            vec![],
        )
    }

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

    #[test]
    fn resolver_queries_over_loopback_tcp() {
        // A fake DNS-over-TCP server: accept one connection, read the 2-byte length prefix +
        // query, and reply with a length-prefixed response echoing the query id.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut len_buf = [0u8; 2];
            conn.read_exact(&mut len_buf).unwrap();
            let n = usize::from(u16::from_be_bytes(len_buf));
            let mut query_buf = vec![0u8; n];
            conn.read_exact(&mut query_buf).unwrap();
            let query = Message::decode_exact(&query_buf).unwrap();

            let resp = a_response(query.header.id, Ipv4Addr::new(10, 0, 0, 1), false)
                .to_bytes()
                .unwrap();
            conn.write_all(&(resp.len() as u16).to_be_bytes()).unwrap();
            conn.write_all(&resp).unwrap();
        });

        let resolver = Resolver::new(server_addr);
        let resp = resolver.query_tcp("example.com", QType::A).unwrap();
        assert_eq!(resp.answers[0].data, RData::A(Ipv4Addr::new(10, 0, 0, 1)));

        handle.join().unwrap();
    }

    #[test]
    fn query_falls_back_to_tcp_on_a_truncated_udp_response() {
        // A UDP socket and a TCP listener on the *same* port: the UDP side replies with the TC
        // bit set, so `query` must fall back to TCP, which serves the full answer.
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = udp.local_addr().unwrap();
        let tcp = TcpListener::bind(addr).unwrap();

        let udp_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = udp.recv_from(&mut buf).unwrap();
            let query = Message::decode_exact(&buf[..n]).unwrap();
            // Truncated (TC) response — forces the TCP fallback.
            let resp = a_response(query.header.id, Ipv4Addr::LOCALHOST, true)
                .to_bytes()
                .unwrap();
            udp.send_to(&resp, from).unwrap();
        });
        let tcp_thread = std::thread::spawn(move || {
            let (mut conn, _) = tcp.accept().unwrap();
            let mut len_buf = [0u8; 2];
            conn.read_exact(&mut len_buf).unwrap();
            let mut query_buf = vec![0u8; usize::from(u16::from_be_bytes(len_buf))];
            conn.read_exact(&mut query_buf).unwrap();
            let query = Message::decode_exact(&query_buf).unwrap();
            // The full (untruncated) answer over TCP.
            let resp = a_response(query.header.id, Ipv4Addr::new(8, 8, 8, 8), false)
                .to_bytes()
                .unwrap();
            conn.write_all(&(resp.len() as u16).to_be_bytes()).unwrap();
            conn.write_all(&resp).unwrap();
        });

        let ips = Resolver::new(addr).resolve_ipv4("example.com").unwrap();
        assert_eq!(ips, vec![Ipv4Addr::new(8, 8, 8, 8)]); // came from the TCP fallback

        udp_thread.join().unwrap();
        tcp_thread.join().unwrap();
    }
}

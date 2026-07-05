//! The typed read accessors on `Message` — extract answer records by `RData` variant.

mod integration {
    use dns::{
        Header, Message, QClass, QType, Question, RClass, RData, RType, Record, State, WireLen,
    };
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn record(name: &str, rtype: RType, data: RData) -> Record {
        Record {
            name: name.parse().unwrap(),
            rtype,
            class: RClass::Internet,
            ttl: 60,
            rdlength: WireLen::auto(),
            data,
        }
    }

    fn response(answers: Vec<Record>) -> Message {
        let header = Header {
            id: 0x1234,
            state: State::new().with_response(true),
            qdcount: WireLen::auto(),
            ancount: WireLen::auto(),
            nscount: WireLen::auto(),
            arcount: WireLen::auto(),
        };
        let q = Question {
            name: "example.com".parse().unwrap(),
            qtype: QType::A,
            qclass: QClass::Internet,
        };
        Message::assemble(header, vec![q], answers, vec![], vec![])
    }

    #[test]
    fn typed_accessors_extract_by_rdata_variant() {
        let msg = response(vec![
            record("example.com", RType::A, RData::A(Ipv4Addr::new(1, 2, 3, 4))),
            record("example.com", RType::A, RData::A(Ipv4Addr::new(5, 6, 7, 8))),
            record("example.com", RType::AAAA, RData::Aaaa(Ipv6Addr::LOCALHOST)),
            record(
                "example.com",
                RType::CNAME,
                RData::Cname("cdn.example.com".parse().unwrap()),
            ),
        ]);

        assert_eq!(
            msg.a_records(),
            vec![Ipv4Addr::new(1, 2, 3, 4), Ipv4Addr::new(5, 6, 7, 8)]
        );
        assert_eq!(msg.aaaa_records(), vec![Ipv6Addr::LOCALHOST]);
        assert_eq!(msg.cnames().len(), 1);
        assert_eq!(msg.cnames()[0].to_string(), "cdn.example.com");
        assert!(msg.mx_records().is_empty());
        // `records(rtype)` filters by the declared type.
        assert_eq!(msg.records(RType::A).count(), 2);
        assert_eq!(msg.records(RType::AAAA).count(), 1);
    }

    #[test]
    fn empty_answers_give_empty_views() {
        let msg = response(vec![]);
        assert!(msg.a_records().is_empty());
        assert!(msg.aaaa_records().is_empty());
        assert_eq!(msg.records(RType::A).count(), 0);
    }
}

use crate::dns::{encode_domain_name, Answer, DnsParser, Question};

use std::net::UdpSocket;

/// Resolves INternet A records for `domain` using the DNS server `dns`
/// todo:
/// - error handling with own error and success types
/// - resolve callers should not necessarily error if our dns request times out
///   or at least send the correct dns response op code back to the requesting client
///   -- this will still result in the same result for the request client but it will be faster
pub fn resolve(
    domain: &str,
    dns: &str,
    id: Option<u16>,
    socket: Option<UdpSocket>,
) -> Result<(Vec<Answer>, Vec<u8>), Box<dyn std::error::Error>> {
    let socket = socket.unwrap_or_else(|| UdpSocket::bind(("0.0.0.0", 0)).unwrap());

    let request = generate_request(domain, id);
    let addr = (dns, 53);
    if let Err(e) = socket.send_to(&request, addr) {
        println!(
            "Failed to send request for {} to {:?}: {:?}",
            domain, addr, e
        );
        // return read timeout error
        return Err(e.into());
    }

    let mut buffer = (0..512).into_iter().map(|_| 0).collect::<Vec<_>>();
    let (datagram_size, _) = socket.recv_from(&mut buffer).map_err(|e| {
        println!(
            "Failed to receive response for {} from {:?}: {:?}",
            domain, addr, e
        );
        e
    })?;
    buffer.truncate(datagram_size);

    let mut parser = DnsParser::new(buffer);
    let header = parser.parse_header();

    for _ in 0..header.question_count {
        parser.parse_question();
    }

    let answers = (0..header.answer_count)
        .map(|_| parser.parse_answer())
        .collect::<Vec<_>>();

    Ok((answers, parser.buf))
}

pub fn resolve_pipe(
    dns_query: &[u8],
    dns: &str,
    existing_socket: Option<UdpSocket>,
) -> Result<(Vec<Answer>, Vec<u8>), Box<dyn std::error::Error>> {
    let socket = existing_socket.unwrap_or_else(|| UdpSocket::bind(("0.0.0.0", 0)).unwrap());

    let addr = (dns, 53000);
    if let Err(e) = socket.send_to(&dns_query, addr) {
        println!("Failed to pipe DNS query to {:?}: {:?}", addr, e);
        // return read timeout error
        return Err(e.into());
    }

    let mut buffer = (0..512).into_iter().map(|_| 0).collect::<Vec<_>>();
    let (datagram_size, _) = socket.recv_from(&mut buffer).map_err(|e| {
        println!("Failed to receive response from {:?}: {:?}", addr, e);
        e
    })?;
    buffer.truncate(datagram_size);

    let mut parser = DnsParser::new(buffer);
    let header = parser.parse_header();

    for _ in 0..header.question_count {
        parser.parse_question();
    }

    let answers = (0..header.answer_count)
        .map(|_| parser.parse_answer())
        .collect::<Vec<_>>();

    Ok((answers, parser.buf))
}

pub fn parse_query(buf: [u8; 512]) -> Result<(u16, Question), Box<dyn std::error::Error>> {
    let mut parser = DnsParser::new(buf.to_vec());
    let header = parser.parse_header();
    Ok((header.id, parser.parse_question()))
}

/// Generates a DNS query for INternet A records
pub(crate) fn generate_request(domain: &str, id: Option<u16>) -> Vec<u8> {
    let id = id
        .map(|n| ((n >> 8) as u8, (n & 0xFF) as u8))
        .unwrap_or((0x10, 0x01));
    const QTYPE: [u8; 2] = [0x00, 0x01];
    const QCLASS: [u8; 2] = [0x00, 0x01];
    let request_header: [u8; 12] = [
        id.0, id.1, // identification
        0x01, 0x00, // flags
        0x00, 0x01, // question section
        0x00, 0x00, // answer section
        0x00, 0x00, // authority section
        0x00, 0x00, // additional section
    ];
    let mut request = vec![];
    request.extend(request_header);
    request.extend(encode_domain_name(domain));
    request.extend(QTYPE);
    request.extend(QCLASS);
    request
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::{resolve, Answer};

    const DNS_SERVERS: [&str; 1] = ["1.1.1.1"];

    #[test]
    fn test_resolve_a_records() {
        for dns_root in DNS_SERVERS {
            let (answers, _) = resolve("www.example.com", dns_root, None, None).unwrap();
            if let Some(Answer::A { ipv4, .. }) = answers.last() {
                assert_eq!(&Ipv4Addr::new(93, 184, 216, 34), ipv4);
            }

            let (answers, _) = resolve("www.maximumstock.net", dns_root, None, None).unwrap();
            let expected = vec![Ipv4Addr::new(154, 53, 57, 10)];

            for answer in &answers {
                if let Answer::A { ipv4, .. } = answer {
                    assert!(expected.contains(ipv4));
                }
            }
        }
    }
}

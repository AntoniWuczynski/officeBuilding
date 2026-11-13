use super::{
    BODY_MAX, HEAD_MAX, Header, Method, Refusal, Request, admit, client_gone, only, parse_head,
    read_request_within, write_chunk, write_end, write_head, write_local,
};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const SHORT: Duration = Duration::from_millis(300);
const LONG: Duration = Duration::from_secs(5);

fn header(name: &str, value: &str) -> Header {
    Header {
        name: name.to_owned(),
        value: value.as_bytes().to_vec(),
    }
}

/// A connected pair on loopback: (client, server, the listener's port).
fn pair() -> (TcpStream, TcpStream, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (server, _) = listener.accept().unwrap();
    (client, server, port)
}

/// Writes each piece with a pause between, on its own thread, and keeps the client open after.
fn send_in_pieces(mut client: TcpStream, pieces: Vec<Vec<u8>>) -> thread::JoinHandle<TcpStream> {
    thread::spawn(move || {
        for piece in pieces {
            client.write_all(&piece).unwrap();
            thread::sleep(Duration::from_millis(40));
        }
        client
    })
}

fn read_all(mut client: TcpStream) -> Vec<u8> {
    client.set_read_timeout(Some(LONG)).unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).unwrap();
    out
}

#[test]
fn only_takes_a_header_sent_exactly_once() {
    let once = [header("a", "1"), header("b", "2")];
    let twice = [header("a", "1"), header("a", "2")];
    assert_eq!(only(&once, "a"), Some(&b"1"[..]));
    assert_eq!(only(&once, "c"), None);
    assert_eq!(only(&twice, "a"), None);
}

#[test]
fn debug_never_prints_a_value_or_the_body() {
    let login = header("authorization", "Bearer invented-login-0001");
    let text = format!("{login:?}");
    assert!(text.contains("authorization"), "{text}");
    assert!(text.contains("26"), "{text}");
    assert!(!text.contains("invented"), "{text}");
    assert!(!text.contains("Bearer"), "{text}");

    let request = Request {
        method: Method::Post,
        path: "/s/route-1/v1/messages".to_owned(),
        query: None,
        headers: vec![login, header("x-office-building-route", "invented-secret")],
        body: br#"{"model":"invented-body-text"}"#.to_vec(),
    };
    let text = format!("{request:?}");
    assert!(text.contains("/s/route-1/v1/messages"), "{text}");
    for hidden in ["invented", "Bearer", "model"] {
        assert!(!text.contains(hidden), "{hidden} in {text}");
    }
}

type Shape = (Method, &'static str, Option<&'static str>);
type HeadRow = (&'static str, Vec<u8>, Result<Option<Shape>, Refusal>);
type AdmitRow<'a> = (&'a str, Vec<(&'a str, &'a str)>, Result<usize, Refusal>);

#[test]
fn parse_head_table() {
    let many = {
        let lines: Vec<String> = (0..97).map(|n| format!("x-h{n}: v")).collect();
        format!("GET / HTTP/1.1\r\n{}\r\n\r\n", lines.join("\r\n")).into_bytes()
    };
    let full = {
        let mut head = b"GET / HTTP/1.1\r\nx-long: ".to_vec();
        head.resize(HEAD_MAX, b'a');
        head
    };
    let complete = b"POST /s/r1/v1/messages HTTP/1.1\r\nHost: 127.0.0.1:1\r\n\r\n".to_vec();
    let rows: Vec<HeadRow> = vec![
        (
            "complete",
            complete.clone(),
            Ok(Some((Method::Post, "/s/r1/v1/messages", None))),
        ),
        (
            "partial",
            b"POST /v1/messages HTTP/1.1\r\nHost: 127".to_vec(),
            Ok(None),
        ),
        ("97 headers", many, Err(Refusal::HeadTooLarge)),
        ("fills the buffer", full, Err(Refusal::HeadTooLarge)),
        (
            "HTTP/1.0",
            b"GET / HTTP/1.0\r\n\r\n".to_vec(),
            Err(Refusal::Version),
        ),
        (
            "HTTP/2 preface",
            b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".to_vec(),
            Err(Refusal::Malformed),
        ),
        (
            "no leading slash",
            b"GET v1/messages HTTP/1.1\r\n\r\n".to_vec(),
            Err(Refusal::Malformed),
        ),
        (
            "a space in the target",
            b"GET /a b HTTP/1.1\r\n\r\n".to_vec(),
            Err(Refusal::Malformed),
        ),
        (
            "a query",
            b"POST /s/r1/v1/messages?beta=true HTTP/1.1\r\n\r\n".to_vec(),
            Ok(Some((Method::Post, "/s/r1/v1/messages", Some("beta=true")))),
        ),
        (
            "HEAD",
            b"HEAD /s/r1/api/hello HTTP/1.1\r\n\r\n".to_vec(),
            Ok(Some((Method::Head, "/s/r1/api/hello", None))),
        ),
        (
            "GET",
            b"GET /x HTTP/1.1\r\n\r\n".to_vec(),
            Ok(Some((Method::Get, "/x", None))),
        ),
        (
            "another method",
            b"PUT /x HTTP/1.1\r\n\r\n".to_vec(),
            Ok(Some((Method::Other, "/x", None))),
        ),
    ];
    for (name, bytes, expected) in rows {
        let got = parse_head(&bytes)
            .map(|head| head.map(|(request, _)| (request.method, request.path, request.query)));
        let expected = expected.map(|head| {
            head.map(|(method, path, query)| (method, path.to_owned(), query.map(str::to_owned)))
        });
        assert_eq!(got, expected, "{name}");
    }
    let used = parse_head(&complete).unwrap().unwrap().1;
    assert_eq!(used, complete.len());
}

#[test]
fn admit_table() {
    const PORT: u16 = 4000;
    let host = ("host", "127.0.0.1:4000");
    let over = (BODY_MAX + 1).to_string();
    let at = BODY_MAX.to_string();
    let rows: Vec<AdmitRow> = vec![
        (
            "origin",
            vec![host, ("origin", "https://example.com")],
            Err(Refusal::Origin),
        ),
        ("no host", vec![("content-length", "2")], Err(Refusal::Host)),
        (
            "another port",
            vec![("host", "127.0.0.1:4001")],
            Err(Refusal::Host),
        ),
        (
            "localhost",
            vec![("host", "localhost:4000")],
            Err(Refusal::Host),
        ),
        ("two hosts", vec![host, host], Err(Refusal::Host)),
        (
            "chunked alone",
            vec![host, ("transfer-encoding", "chunked")],
            Err(Refusal::LengthRequired),
        ),
        (
            "chunked beside a length",
            vec![
                host,
                ("content-length", "5"),
                ("transfer-encoding", "chunked"),
            ],
            Err(Refusal::LengthRequired),
        ),
        (
            "gzip",
            vec![host, ("content-encoding", "gzip"), ("content-length", "5")],
            Err(Refusal::Encoding),
        ),
        (
            "identity",
            vec![
                host,
                ("content-encoding", "identity"),
                ("content-length", "5"),
            ],
            Ok(5),
        ),
        (
            "two lengths",
            vec![host, ("content-length", "5"), ("content-length", "5")],
            Err(Refusal::Malformed),
        ),
        (
            "letters",
            vec![host, ("content-length", "abc")],
            Err(Refusal::Malformed),
        ),
        (
            "a plus sign",
            vec![host, ("content-length", "+5")],
            Err(Refusal::Malformed),
        ),
        (
            "one over the cap",
            vec![host, ("content-length", over.as_str())],
            Err(Refusal::TooLarge),
        ),
        (
            "past a usize",
            vec![host, ("content-length", "99999999999999999999999999")],
            Err(Refusal::TooLarge),
        ),
        (
            "at the cap",
            vec![host, ("content-length", at.as_str())],
            Ok(BODY_MAX),
        ),
        ("no length", vec![host], Ok(0)),
    ];
    for (name, headers, expected) in rows {
        let request = Request {
            method: Method::Post,
            path: "/".to_owned(),
            query: None,
            headers: headers.iter().map(|(n, v)| header(n, v)).collect(),
            body: Vec::new(),
        };
        assert_eq!(admit(&request, PORT), expected, "{name}");
    }
}

fn head_for(port: u16, extra: &str, length: usize) -> Vec<u8> {
    format!(
        "POST /s/r1/v1/messages?beta=true HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{extra}Content-Length: {length}\r\n\r\n"
    )
    .into_bytes()
}

#[test]
fn reads_a_whole_request() {
    let (mut client, mut server, port) = pair();
    let mut bytes = head_for(port, "", 7);
    bytes.extend_from_slice(b"{\"a\":1}");
    client.write_all(&bytes).unwrap();
    let request = read_request_within(&mut server, port, LONG, LONG, &|_| true).unwrap();
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.path, "/s/r1/v1/messages");
    assert_eq!(request.query.as_deref(), Some("beta=true"));
    assert_eq!(request.body, b"{\"a\":1}");
    let names: Vec<&str> = request.headers.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["host", "content-type", "content-length"]);
    assert_eq!(request.only("content-type"), Some(&b"application/json"[..]));
    assert!(request.has("host"));
}

#[test]
fn reads_a_head_in_two_writes_and_a_body_in_three() {
    let (client, mut server, port) = pair();
    let head = head_for(port, "", 9);
    let (first, second) = head.split_at(20);
    let writer = send_in_pieces(
        client,
        vec![
            first.to_vec(),
            second.to_vec(),
            b"abc".to_vec(),
            b"def".to_vec(),
            b"ghi".to_vec(),
        ],
    );
    let request = read_request_within(&mut server, port, LONG, LONG, &|_| true).unwrap();
    assert_eq!(request.body, b"abcdefghi");
    drop(writer.join().unwrap());
}

#[test]
fn answers_expect_continue_before_the_body_is_read() {
    let (mut client, mut server, port) = pair();
    let reader =
        thread::spawn(move || read_request_within(&mut server, port, LONG, LONG, &|_| true));
    client
        .write_all(&head_for(port, "Expect: 100-Continue\r\n", 5))
        .unwrap();
    client.set_read_timeout(Some(LONG)).unwrap();
    let mut interim = [0_u8; 25];
    client.read_exact(&mut interim).unwrap();
    assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
    client.write_all(b"hello").unwrap();
    assert_eq!(reader.join().unwrap().unwrap().body, b"hello");
}

#[test]
fn a_body_cut_short_is_gone() {
    let (mut client, mut server, port) = pair();
    let mut bytes = head_for(port, "", 10);
    bytes.extend_from_slice(b"abcd");
    client.write_all(&bytes).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    assert_eq!(
        read_request_within(&mut server, port, LONG, LONG, &|_| true),
        Err(Refusal::Gone)
    );
}

#[test]
fn a_head_slower_than_the_wait_is_gone() {
    let (mut client, mut server, port) = pair();
    client.write_all(b"POST /s/r1/v1/mess").unwrap();
    let started = Instant::now();
    assert_eq!(
        read_request_within(&mut server, port, SHORT, LONG, &|_| true),
        Err(Refusal::Gone)
    );
    assert!(started.elapsed() < LONG, "{:?}", started.elapsed());
    drop(client);
}

#[test]
fn a_refusal_on_the_socket_reads_no_body() {
    let (mut client, mut server, port) = pair();
    client
        .write_all(&head_for(port, "Origin: https://example.com\r\n", 5))
        .unwrap();
    assert_eq!(
        read_request_within(&mut server, port, LONG, LONG, &|_| true),
        Err(Refusal::Origin)
    );
}

#[test]
fn header_names_are_lower_cased() {
    let (mut client, mut server, port) = pair();
    let bytes = format!(
        "GET /x HTTP/1.1\r\nHOST: 127.0.0.1:{port}\r\nX-Claude-Code-Session-Id: AbC\r\n\r\n"
    );
    client.write_all(bytes.as_bytes()).unwrap();
    let request = read_request_within(&mut server, port, LONG, LONG, &|_| true).unwrap();
    assert_eq!(request.only("x-claude-code-session-id"), Some(&b"AbC"[..]));
    assert!(request.body.is_empty());
}

#[test]
fn write_local_is_a_complete_answer() {
    let (client, mut server, _) = pair();
    write_local(&mut server, 503, &[("retry-after", "1")], b"{}").unwrap();
    drop(server);
    assert_eq!(
        String::from_utf8(read_all(client)).unwrap(),
        "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\ncontent-length: 2\r\nretry-after: 1\r\nconnection: close\r\n\r\n{}"
    );
}

/// Serves one request on its own thread with a relayed reply, ended or not.
fn relay_once(ends: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_within(&mut stream, port, LONG, LONG, &|_| true).unwrap();
        let headers = [
            header("content-type", "text/event-stream"),
            header("x-bad", "a\r\nb"),
            header("request-id", "req_invented"),
        ];
        write_head(&mut stream, 200, &headers).unwrap();
        write_chunk(&mut stream, b"hello ").unwrap();
        write_chunk(&mut stream, b"").unwrap();
        write_chunk(&mut stream, b"world").unwrap();
        if ends {
            write_end(&mut stream).unwrap();
        }
        stream.shutdown(Shutdown::Both).unwrap();
    });
    port
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

#[test]
fn a_relayed_reply_reads_back_as_chunked() {
    let port = relay_once(true);
    let mut reply = client()
        .get(format!("http://127.0.0.1:{port}/x"))
        .call()
        .unwrap();
    assert_eq!(reply.status(), 200);
    let headers = reply.headers();
    assert_eq!(headers["transfer-encoding"], "chunked");
    assert_eq!(headers["connection"], "close");
    assert_eq!(headers["request-id"], "req_invented");
    assert!(headers.get("x-bad").is_none());
    let mut body = String::new();
    reply
        .body_mut()
        .as_reader()
        .read_to_string(&mut body)
        .unwrap();
    assert_eq!(body, "hello world");
}

#[test]
fn a_relayed_reply_without_its_end_is_cut_short() {
    let port = relay_once(false);
    let mut reply = client()
        .get(format!("http://127.0.0.1:{port}/x"))
        .call()
        .unwrap();
    let mut body = Vec::new();
    assert!(reply.body_mut().as_reader().read_to_end(&mut body).is_err());
}

/// Polls `client_gone` until it says `want` or a second passes.
fn settles_to(stream: &TcpStream, want: bool) -> bool {
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        if client_gone(stream) == want {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn client_gone_sees_a_closed_peer_only() {
    let (mut client, server, _) = pair();
    assert!(!client_gone(&server), "open");
    client.write_all(b"extra").unwrap();
    thread::sleep(Duration::from_millis(50));
    assert!(!client_gone(&server), "bytes waiting");

    let (client, server, _) = pair();
    drop(client);
    assert!(settles_to(&server, true), "closed");
}

#[test]
fn a_head_the_listener_does_not_admit_reads_no_body() {
    let (mut client, mut server, port) = pair();
    client.write_all(&head_for(port, "", 5_000_000)).unwrap();
    assert_eq!(
        read_request_within(&mut server, port, LONG, LONG, &|_| false),
        Err(Refusal::Unadmitted)
    );
}

#[test]
fn a_body_trickled_past_the_wait_is_gone() {
    let (mut client, mut server, port) = pair();
    client.write_all(&head_for(port, "", 100)).unwrap();
    client.write_all(b"a").unwrap();
    let started = Instant::now();
    let reader = thread::spawn(move || {
        let read = read_request_within(&mut server, port, LONG, SHORT, &|_| true);
        (read, server)
    });
    let trickle = thread::spawn(move || {
        for _ in 0..8 {
            thread::sleep(SHORT / 4);
            if client.write_all(b"b").is_err() {
                break;
            }
        }
        client
    });
    let (read, _server) = reader.join().unwrap();
    assert_eq!(read, Err(Refusal::Gone));
    assert!(started.elapsed() < LONG, "{:?}", started.elapsed());
    drop(trickle.join().unwrap());
}

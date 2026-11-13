use super::{
    ANTHROPIC, Bases, Canned, FakeUpstream, LOCAL_HEADER, OPENAI, OPENROUTER, SendError, Step,
    Upstream, UreqUpstream, outbound, relays, returned, send_error, translated, url,
};
use crate::routing::config::ConnectionKind;
use crate::routing::wire::Header;
use crate::secrets::Secret;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const KINDS: [ConnectionKind; 5] = [
    ConnectionKind::ClaudeSubscription,
    ConnectionKind::AnthropicApiKey,
    ConnectionKind::OpenRouter,
    ConnectionKind::OpenAiApiKey,
    ConnectionKind::ChatGptSignIn,
];
const LOGIN: &str = "Bearer login-from-the-desk";
const CLIENT_KEY: &str = "client-key-value";
const CLIENT_COOKIE: &str = "session=client-cookie";
const ROUTE_SECRET: &str = "route-secret-value";
const KEY: &str = "saved-key-value";

fn header(name: &str, value: &str) -> Header {
    Header {
        name: name.to_owned(),
        value: value.as_bytes().to_vec(),
    }
}

fn names(headers: &[Header]) -> Vec<&str> {
    headers.iter().map(|h| h.name.as_str()).collect()
}

/// The full header set a desk sends, as probed (T2), plus the local ones, credentials and a
/// header nobody knows.
fn client() -> Vec<Header> {
    [
        ("accept", "application/json"),
        ("accept-encoding", "gzip, deflate, br, zstd"),
        ("authorization", LOGIN),
        ("content-type", "application/json"),
        ("user-agent", "claude-cli/test (external, cli)"),
        ("x-app", "cli"),
        ("x-claude-code-session-id", "session-1"),
        ("x-claude-code-agent-id", "agent-1"),
        ("x-claude-code-request-class", "main"),
        ("x-stainless-lang", "js"),
        ("anthropic-beta", "oauth-2025-04-20,other-beta"),
        ("anthropic-version", "2023-06-01"),
        ("anthropic-dangerous-direct-browser-access", "true"),
        (LOCAL_HEADER, ROUTE_SECRET),
        ("host", "127.0.0.1:4000"),
        ("content-length", "2"),
        ("connection", "keep-alive"),
        ("cookie", CLIENT_COOKIE),
        ("x-api-key", CLIENT_KEY),
        ("proxy-authorization", "Basic proxy"),
        ("x-new-thing", "1"),
    ]
    .iter()
    .map(|(name, value)| header(name, value))
    .collect()
}

#[test]
fn relays_and_url_follow_the_kind() {
    let bases = Bases::default();
    assert_eq!(
        bases,
        Bases {
            anthropic: ANTHROPIC.to_owned(),
            openrouter: OPENROUTER.to_owned(),
            openai: OPENAI.to_owned(),
        }
    );
    let rows = [
        (
            ConnectionKind::ClaudeSubscription,
            true,
            Some("https://api.anthropic.com/v1/messages?beta=true"),
        ),
        (
            ConnectionKind::AnthropicApiKey,
            true,
            Some("https://api.anthropic.com/v1/messages?beta=true"),
        ),
        (
            ConnectionKind::OpenRouter,
            true,
            Some("https://openrouter.ai/api/v1/messages?beta=true"),
        ),
        (
            ConnectionKind::OpenAiApiKey,
            true,
            Some("https://api.openai.com/v1/responses"),
        ),
        (ConnectionKind::ChatGptSignIn, false, None),
    ];
    for (kind, relayed, want) in rows {
        assert_eq!(relays(kind), relayed, "{kind:?}");
        assert_eq!(
            url(&bases, kind, Some("beta=true")).as_deref(),
            want,
            "{kind:?}"
        );
    }
    assert_eq!(
        url(&bases, ConnectionKind::OpenRouter, None).as_deref(),
        Some("https://openrouter.ai/api/v1/messages")
    );
    // The client's query is Anthropic's: it never reaches OpenAI.
    for query in [None, Some("beta=true"), Some("x=1&beta=true")] {
        let got = url(&bases, ConnectionKind::OpenAiApiKey, query);
        assert_eq!(
            got.as_deref(),
            Some("https://api.openai.com/v1/responses"),
            "{query:?}"
        );
        assert!(!got.unwrap_or_default().contains("beta"), "{query:?}");
    }
    let local = Bases {
        anthropic: "http://127.0.0.1:1".into(),
        openrouter: "http://127.0.0.1:2".into(),
        openai: "http://127.0.0.1:3".into(),
    };
    assert_eq!(
        url(&local, ConnectionKind::AnthropicApiKey, None).as_deref(),
        Some("http://127.0.0.1:1/v1/messages")
    );
    assert_eq!(
        url(&local, ConnectionKind::OpenAiApiKey, Some("beta=true")).as_deref(),
        Some("http://127.0.0.1:3/v1/responses")
    );
}

#[test]
fn outbound_per_kind() {
    let key = Secret::new(KEY.into());
    let forwarded = [
        "accept",
        "content-type",
        "user-agent",
        "x-app",
        "x-claude-code-session-id",
        "x-claude-code-agent-id",
        "x-claude-code-request-class",
        "x-stainless-lang",
        "anthropic-beta",
        "anthropic-version",
        "anthropic-dangerous-direct-browser-access",
    ];
    let subscription = [
        "accept",
        "accept-encoding",
        "authorization",
        "content-type",
        "user-agent",
        "x-app",
        "x-claude-code-session-id",
        "x-claude-code-agent-id",
        "x-claude-code-request-class",
        "x-stainless-lang",
        "anthropic-beta",
        "anthropic-version",
        "anthropic-dangerous-direct-browser-access",
        "x-new-thing",
    ];
    let anthropic_key: Vec<&str> = forwarded
        .iter()
        .copied()
        .chain(["x-new-thing", "x-api-key", "accept-encoding"])
        .collect();
    let openrouter: Vec<&str> = forwarded
        .iter()
        .copied()
        .chain(["authorization", "accept-encoding"])
        .collect();
    let rows: [(ConnectionKind, Option<Vec<&str>>); 5] = [
        (
            ConnectionKind::ClaudeSubscription,
            Some(subscription.to_vec()),
        ),
        (ConnectionKind::AnthropicApiKey, Some(anthropic_key)),
        (ConnectionKind::OpenRouter, Some(openrouter)),
        (
            ConnectionKind::OpenAiApiKey,
            Some(vec![
                "content-type",
                "accept",
                "authorization",
                "accept-encoding",
            ]),
        ),
        (ConnectionKind::ChatGptSignIn, None),
    ];
    for (kind, want) in rows {
        let got = outbound(kind, &client(), Some(&key));
        assert_eq!(got.as_deref().map(names), want, "{kind:?}");
    }
}

#[test]
fn the_hard_line() {
    let key = Secret::new(KEY.into());
    let client = client();
    for kind in KINDS {
        for key in [Some(&key), None] {
            let Some(sent) = outbound(kind, &client, key) else {
                continue;
            };
            let carries = |value: &str| sent.iter().any(|h| h.value == value.as_bytes());
            assert_eq!(
                carries(LOGIN),
                kind == ConnectionKind::ClaudeSubscription,
                "{kind:?}: the login crosses for the subscription only"
            );
            for value in [CLIENT_KEY, CLIENT_COOKIE, ROUTE_SECRET] {
                assert!(!carries(value), "{kind:?} sent a client value it must not");
            }
            assert!(
                !sent.iter().any(|h| h.name == LOCAL_HEADER),
                "{kind:?} sent the local header"
            );
            // F13: the client is not read, so not one value of its own crosses to OpenAI. Its
            // `content-type` happens to equal the router's own.
            if kind == ConnectionKind::OpenAiApiKey {
                assert_eq!(outbound(kind, &[], key).as_ref(), Some(&sent));
                for h in client.iter().filter(|h| h.value != b"application/json") {
                    assert!(!carries(&String::from_utf8_lossy(&h.value)), "{}", h.name);
                }
            }
            if kind == ConnectionKind::ClaudeSubscription {
                for h in &sent {
                    assert!(
                        client.contains(h),
                        "the subscription added or changed {}",
                        h.name
                    );
                }
            }
        }
    }
}

#[test]
fn a_key_goes_only_to_its_own_kind_in_its_own_header() {
    let key = Secret::new(KEY.into());
    let client = client();
    let value_of = |headers: &[Header], name: &str| -> Vec<String> {
        headers
            .iter()
            .filter(|h| h.name == name)
            .map(|h| String::from_utf8_lossy(&h.value).into_owned())
            .collect()
    };
    let subscription = outbound(ConnectionKind::ClaudeSubscription, &client, Some(&key)).unwrap();
    assert!(!subscription.iter().any(|h| h.value == KEY.as_bytes()));
    assert_eq!(value_of(&subscription, "authorization"), [LOGIN]);

    let anthropic = outbound(ConnectionKind::AnthropicApiKey, &client, Some(&key)).unwrap();
    assert_eq!(value_of(&anthropic, "x-api-key"), [KEY]);
    assert!(value_of(&anthropic, "authorization").is_empty());
    assert_eq!(value_of(&anthropic, "accept-encoding"), ["identity"]);

    let openrouter = outbound(ConnectionKind::OpenRouter, &client, Some(&key)).unwrap();
    assert_eq!(
        value_of(&openrouter, "authorization"),
        [format!("Bearer {KEY}")]
    );
    assert!(value_of(&openrouter, "x-api-key").is_empty());
    assert_eq!(value_of(&openrouter, "accept-encoding"), ["identity"]);

    let openai = outbound(ConnectionKind::OpenAiApiKey, &client, Some(&key)).unwrap();
    let pairs: Vec<(&str, String)> = openai
        .iter()
        .map(|h| {
            (
                h.name.as_str(),
                String::from_utf8_lossy(&h.value).into_owned(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("content-type", "application/json".to_owned()),
            ("accept", "text/event-stream".to_owned()),
            ("authorization", format!("Bearer {KEY}")),
            ("accept-encoding", "identity".to_owned()),
        ]
    );

    for kind in [
        ConnectionKind::AnthropicApiKey,
        ConnectionKind::OpenRouter,
        ConnectionKind::OpenAiApiKey,
    ] {
        assert_eq!(outbound(kind, &client, None), None, "{kind:?} with no key");
    }
    assert_eq!(
        outbound(ConnectionKind::ChatGptSignIn, &client, Some(&key)),
        None
    );
    assert!(outbound(ConnectionKind::ClaudeSubscription, &client, None).is_some());
}

#[test]
fn returned_drops_hop_headers_and_the_length_only() {
    let upstream: Vec<Header> = [
        ("content-type", "text/event-stream"),
        ("connection", "keep-alive"),
        ("content-encoding", "gzip"),
        ("transfer-encoding", "chunked"),
        ("retry-after", "3"),
        ("content-length", "10"),
        ("x-should-retry", "false"),
        ("keep-alive", "timeout=5"),
        ("request-id", "req_1"),
        ("anthropic-ratelimit-unified-status", "rejected"),
    ]
    .iter()
    .map(|(name, value)| header(name, value))
    .collect();
    assert_eq!(
        names(&returned(&upstream)),
        [
            "content-type",
            "content-encoding",
            "retry-after",
            "x-should-retry",
            "request-id",
            "anthropic-ratelimit-unified-status",
        ]
    );
}

/// Name, the request ids the provider sent, the head expected.
type HeadRow<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [&'a str]);

#[test]
fn a_translated_head_carries_three_headers_at_most() {
    let base = ["content-type: text/event-stream", "cache-control: no-cache"];
    let provider = [
        ("content-type", "text/event-stream; charset=utf-8"),
        ("openai-processing-ms", "12"),
        ("x-ratelimit-remaining-requests", "9"),
        ("set-cookie", "c=1"),
    ];
    let rows: [HeadRow; 3] = [
        ("no request id", &[], &base),
        (
            "one",
            &[("x-request-id", "req_test_1")],
            &[
                "content-type: text/event-stream",
                "cache-control: no-cache",
                "x-request-id: req_test_1",
            ],
        ),
        (
            "two",
            &[("x-request-id", "req_a"), ("x-request-id", "req_b")],
            &base,
        ),
    ];
    for (name, ids, want) in rows {
        let upstream: Vec<Header> = provider
            .iter()
            .chain(ids)
            .map(|(n, v)| header(n, v))
            .collect();
        let got: Vec<String> = translated(&upstream)
            .iter()
            .map(|h| format!("{}: {}", h.name, String::from_utf8_lossy(&h.value)))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn send_error_is_conservative() {
    let io = |kind: ErrorKind| ureq::Error::Io(std::io::Error::from(kind));
    let http = ureq::http::Request::builder()
        .header("bad\nname", "x")
        .body(())
        .unwrap_err();
    let rows = [
        (
            "host not found",
            ureq::Error::HostNotFound,
            SendError::NotSent,
        ),
        (
            "connection failed",
            ureq::Error::ConnectionFailed,
            SendError::NotSent,
        ),
        (
            "bad uri",
            ureq::Error::BadUri("x".into()),
            SendError::NotSent,
        ),
        ("http", ureq::Error::Http(http), SendError::NotSent),
        (
            "proxy url",
            ureq::Error::InvalidProxyUrl,
            SendError::NotSent,
        ),
        ("tls", ureq::Error::Tls("x"), SendError::NotSent),
        ("tls required", ureq::Error::TlsRequired, SendError::NotSent),
        (
            "https only",
            ureq::Error::RequireHttpsOnly("x".into()),
            SendError::NotSent,
        ),
        (
            "resolve",
            ureq::Error::Timeout(ureq::Timeout::Resolve),
            SendError::NotSent,
        ),
        (
            "connect",
            ureq::Error::Timeout(ureq::Timeout::Connect),
            SendError::NotSent,
        ),
        (
            "refused",
            io(ErrorKind::ConnectionRefused),
            SendError::NotSent,
        ),
        ("addr", io(ErrorKind::AddrNotAvailable), SendError::NotSent),
        (
            "network",
            io(ErrorKind::NetworkUnreachable),
            SendError::NotSent,
        ),
        ("host", io(ErrorKind::HostUnreachable), SendError::NotSent),
        ("reset", io(ErrorKind::ConnectionReset), SendError::Dropped),
        ("eof", io(ErrorKind::UnexpectedEof), SendError::Dropped),
        ("pipe", io(ErrorKind::BrokenPipe), SendError::Dropped),
        (
            "send request",
            ureq::Error::Timeout(ureq::Timeout::SendRequest),
            SendError::Dropped,
        ),
        (
            "recv response",
            ureq::Error::Timeout(ureq::Timeout::RecvResponse),
            SendError::Dropped,
        ),
        (
            "per read",
            ureq::Error::Timeout(ureq::Timeout::PerRead),
            SendError::Dropped,
        ),
        ("status", ureq::Error::StatusCode(500), SendError::Dropped),
        (
            "redirects",
            ureq::Error::TooManyRedirects,
            SendError::Dropped,
        ),
        (
            "redirect failed",
            ureq::Error::RedirectFailed,
            SendError::Dropped,
        ),
    ];
    for (name, error, want) in &rows {
        assert_eq!(send_error(error), *want, "{name}");
    }
}

#[test]
fn the_real_agent_is_direct_https_only_and_never_redirects() {
    let upstream = UreqUpstream::new();
    let config = upstream.agent.config();
    assert!(config.proxy().is_none());
    assert_eq!(config.max_redirects(), 0);
    assert!(config.https_only());
    let got = upstream.post("http://127.0.0.1:9/v1/messages", &[], b"{}");
    assert_eq!(got.err(), Some(SendError::NotSent));
}

/// What a fake server does after it has read one request.
enum Then {
    Write(Vec<u8>),
    Close,
}

/// A one-connection server on loopback. It reads one request, hands its bytes back on the
/// channel and then does `then`. Returns its base address.
fn fake_server(then: Then) -> (String, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_one(&mut stream);
        tx.send(request).unwrap();
        if let Then::Write(reply) = then {
            let _ = stream.write_all(&reply);
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    (format!("http://127.0.0.1:{port}"), rx)
}

/// One request's bytes: the head, then as many body bytes as its `content-length` says.
fn read_one(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0, "the client closed before its request ended");
        buf.extend_from_slice(&chunk[..n]);
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut parsed = httparse::Request::new(&mut headers);
        if let httparse::Status::Complete(head) = parsed.parse(&buf).unwrap() {
            let length: usize = parsed
                .headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                .map_or(0, |h| {
                    std::str::from_utf8(h.value).unwrap().parse().unwrap()
                });
            while buf.len() < head + length {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                buf.extend_from_slice(&chunk[..n]);
            }
            return buf;
        }
    }
}

fn post(base: &str, headers: &[Header], body: &[u8]) -> Result<super::Reply, SendError> {
    UreqUpstream::plain().post(&format!("{base}/v1/messages?beta=true"), headers, body)
}

fn read_body(reply: super::Reply) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut reader = reply.body;
    reader.read_to_end(&mut body).map(|_| body)
}

#[test]
fn headers_and_body_arrive_as_sent() {
    let (base, seen) = fake_server(Then::Write(
        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nrequest-id: req_1\r\ncontent-length: 2\r\n\r\nok".to_vec(),
    ));
    let headers = [
        header("content-type", "application/json"),
        header("anthropic-beta", "a,b"),
        header("x-claude-code-session-id", "session-1"),
        header("user-agent", "claude-cli/test"),
    ];
    let reply = post(&base, &headers, b"{\"model\":\"m\"}").unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(
        names(&reply.headers),
        ["content-type", "request-id", "content-length"]
    );
    assert_eq!(read_body(reply).unwrap(), b"ok");

    let request = String::from_utf8(seen.recv().unwrap()).unwrap();
    assert!(
        request.starts_with("POST /v1/messages?beta=true HTTP/1.1\r\n"),
        "{request}"
    );
    for line in [
        "content-type: application/json\r\n",
        "anthropic-beta: a,b\r\n",
        "x-claude-code-session-id: session-1\r\n",
        "user-agent: claude-cli/test\r\n",
        "content-length: 13\r\n",
    ] {
        assert!(request.contains(line), "missing {line:?}");
    }
    assert!(
        !request.contains("office-building"),
        "the client's agent wins"
    );
    assert!(request.ends_with("\r\n\r\n{\"model\":\"m\"}"));
}

#[test]
fn a_redirect_is_returned_and_never_followed() {
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    let elsewhere = second.local_addr().unwrap().port();
    let (base, seen) = fake_server(Then::Write(
        format!(
            "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://127.0.0.1:{elsewhere}/v1/messages\r\ncontent-length: 0\r\n\r\n"
        )
        .into_bytes(),
    ));
    let reply = post(&base, &[], b"{}").unwrap();
    assert_eq!(reply.status, 307);
    seen.recv().unwrap();
    second.set_nonblocking(true).unwrap();
    assert_eq!(
        second.accept().err().map(|e| e.kind()),
        Some(ErrorKind::WouldBlock),
        "the second server saw a connection"
    );
}

#[test]
fn a_429_is_a_reply() {
    let (base, _seen) = fake_server(Then::Write(
        b"HTTP/1.1 429 Too Many Requests\r\nanthropic-ratelimit-unified-status: rejected\r\ncontent-length: 2\r\n\r\n{}".to_vec(),
    ));
    let reply = post(&base, &[], b"{}").unwrap();
    assert_eq!(reply.status, 429);
    assert_eq!(
        names(&reply.headers)[0],
        "anthropic-ratelimit-unified-status"
    );
    assert_eq!(read_body(reply).unwrap(), b"{}");
}

#[test]
fn a_chunked_body_arrives_in_order_and_a_cut_one_fails() {
    let (base, _seen) = fake_server(Then::Write(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n2\r\nab\r\n3\r\ncde\r\n0\r\n\r\n"
            .to_vec(),
    ));
    let reply = post(&base, &[], b"{}").unwrap();
    assert_eq!(read_body(reply).unwrap(), b"abcde");

    let (base, _seen) = fake_server(Then::Write(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n2\r\nab\r\n5\r\ncd".to_vec(),
    ));
    let reply = post(&base, &[], b"{}").unwrap();
    assert!(read_body(reply).is_err(), "a body cut inside a chunk");
}

#[test]
fn an_encoded_body_comes_back_as_written() {
    let gzip = [0x1f_u8, 0x8b, 0x08, 0x00, 0x01, 0x02, 0x03, 0xff];
    let mut reply =
        b"HTTP/1.1 200 OK\r\ncontent-encoding: gzip\r\ncontent-length: 8\r\n\r\n".to_vec();
    reply.extend_from_slice(&gzip);
    let (base, _seen) = fake_server(Then::Write(reply));
    let reply = post(&base, &[header("accept-encoding", "gzip")], b"{}").unwrap();
    assert_eq!(read_body(reply).unwrap(), gzip);
}

#[test]
fn a_closed_port_is_not_sent_and_a_silent_close_is_dropped() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert_eq!(
        post(&format!("http://127.0.0.1:{port}"), &[], b"{}").err(),
        Some(SendError::NotSent)
    );

    let (base, seen) = fake_server(Then::Close);
    assert_eq!(post(&base, &[], b"{}").err(), Some(SendError::Dropped));
    assert!(!seen.recv().unwrap().is_empty(), "the request did leave");
}

#[test]
fn the_fake_records_and_plays_its_script() {
    let (gate, held) = mpsc::channel();
    let held = std::sync::Mutex::new(Some(held));
    let fake = FakeUpstream::new(move |sent| {
        if sent.url.contains("openrouter") {
            return Err(SendError::NotSent);
        }
        let mut body = vec![Step::Bytes(b"one".to_vec())];
        if let Some(held) = crate::sync::lock(&held).take() {
            body.push(Step::Hold(held));
        }
        body.push(Step::Bytes(b"two".to_vec()));
        body.push(Step::Break);
        Ok(Canned {
            status: 200,
            headers: vec![("content-type", "text/event-stream".into())],
            body,
        })
    });
    let sent_headers = [header("x-api-key", KEY)];
    let reply = fake
        .post(
            "https://api.anthropic.com/v1/messages",
            &sent_headers,
            b"{}",
        )
        .unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(names(&reply.headers), ["content-type"]);
    let mut body = reply.body;
    let mut buf = [0_u8; 16];
    assert_eq!(body.read(&mut buf).unwrap(), 3);
    assert_eq!(&buf[..3], b"one");
    let reader = thread::spawn(move || {
        let n = body.read(&mut buf).unwrap();
        let rest = body.read(&mut buf).map_err(|e| e.kind());
        (n, rest)
    });
    thread::sleep(Duration::from_millis(50));
    assert!(!reader.is_finished(), "the hold blocks");
    gate.send(()).unwrap();
    let (n, rest) = reader.join().unwrap();
    assert_eq!(n, 3);
    assert_eq!(rest, Err(ErrorKind::ConnectionReset));

    assert_eq!(
        fake.post("https://openrouter.ai/api/v1/messages", &[], b"[]")
            .err(),
        Some(SendError::NotSent)
    );
    let sent = fake.sent();
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .any(|s| s.url.contains("anthropic") && s.headers == sent_headers && s.body == b"{}")
    );
    assert!(
        sent.iter()
            .any(|s| s.url.contains("openrouter") && s.body == b"[]")
    );
}

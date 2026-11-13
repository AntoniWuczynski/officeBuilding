//! HTTP/1.1 on the loopback socket: reading one request and checking it, writing a local answer
//! and a relayed reply. One request per connection, and every response carries `connection: close`.

use std::fmt;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

pub const HEAD_MAX: usize = 32 * 1024;
pub const HEADERS_MAX: usize = 96;
pub const BODY_MAX: usize = 64 * 1024 * 1024;
/// The whole head.
pub const HEAD_WAIT: Duration = Duration::from_secs(10);
/// The whole body.
pub const READ_WAIT: Duration = Duration::from_secs(60);
/// Each write to the client.
pub const WRITE_WAIT: Duration = Duration::from_secs(60);
/// How long a refused connection keeps taking what the client still sends.
pub const LINGER_WAIT: Duration = Duration::from_secs(2);

/// One header as it crossed the socket. `name` is lower-case. `Debug` prints the name and the
/// value's length, never the value: a header can carry a login or the route secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Header")
            .field("name", &self.name)
            .field("value_len", &self.value.len())
            .finish()
    }
}

/// The value of `name` (lower-case) when `headers` carry it exactly once.
#[must_use]
pub fn only<'h>(headers: &'h [Header], name: &str) -> Option<&'h [u8]> {
    let mut found = headers.iter().filter(|header| header.name == name);
    let first = found.next()?;
    found.next().is_none().then_some(first.value.as_slice())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Other,
}

/// One request off the loopback socket. `Debug` prints the method, the path, the header count
/// and the body length, never a header value or the body.
#[derive(PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    /// The path without its query, starting with `/`.
    pub path: String,
    /// The text after `?`, as sent.
    pub query: Option<String>,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("headers", &self.headers.len())
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}

impl Request {
    /// The value of `name` when the request carries it exactly once.
    #[must_use]
    pub fn only(&self, name: &str) -> Option<&[u8]> {
        only(&self.headers, name)
    }

    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.headers.iter().any(|header| header.name == name)
    }
}

/// Why a request was turned away before anything looked at its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Malformed,
    HeadTooLarge,
    Version,
    Origin,
    Host,
    LengthRequired,
    Encoding,
    TooLarge,
    /// The head named no desk the listener serves.
    Unadmitted,
    Gone,
}

/// Read one request. `port` is the listener's own. `head_wait` bounds the whole head and
/// `body_wait` the whole body. `admits` sees the head before any body byte is read or any room
/// is made for it, and a head it turns away is `Refusal::Unadmitted`. Writes
/// `HTTP/1.1 100 Continue` after that when the request carries `expect: 100-continue`.
pub(crate) fn read_request_within(
    stream: &mut TcpStream,
    port: u16,
    head_wait: Duration,
    body_wait: Duration,
    admits: &dyn Fn(&Request) -> bool,
) -> Result<Request, Refusal> {
    let deadline = Instant::now() + head_wait;
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 8 * 1024];
    let (mut request, used) = loop {
        let room = (HEAD_MAX - buf.len()).min(chunk.len());
        let read = read_before(stream, deadline, &mut chunk[..room])?;
        buf.extend_from_slice(&chunk[..read]);
        if let Some(head) = parse_head(&buf)? {
            break head;
        }
    };
    let length = admit(&request, port)?;
    if !admits(&request) {
        return Err(Refusal::Unadmitted);
    }
    let mut body = buf.split_off(used);
    body.truncate(length);
    let expects = request.headers.iter().any(|header| {
        header.name == "expect" && header.value.eq_ignore_ascii_case(b"100-continue")
    });
    if expects {
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .map_err(|_| Refusal::Gone)?;
    }
    // The buffer grows with the bytes that arrive, never to the declared length at once.
    let deadline = Instant::now() + body_wait;
    while body.len() < length {
        let room = (length - body.len()).min(chunk.len());
        let read = read_before(stream, deadline, &mut chunk[..room])?;
        body.extend_from_slice(&chunk[..read]);
    }
    request.body = body;
    Ok(request)
}

/// One read into `into` that gives up at `deadline`. Zero bytes and any error are `Gone`.
fn read_before(
    stream: &mut TcpStream,
    deadline: Instant,
    into: &mut [u8],
) -> Result<usize, Refusal> {
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(Refusal::Gone)?;
        stream
            .set_read_timeout(Some(left))
            .map_err(|_| Refusal::Gone)?;
        match stream.read(into) {
            Ok(0) => return Err(Refusal::Gone),
            Ok(read) => return Ok(read),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Err(Refusal::Gone),
        }
    }
}

/// After a refusal answer: close the write side, then read and drop what the client still sends,
/// at most `BODY_MAX` bytes and until `LINGER_WAIT`, so closing with unread bytes does not reset
/// the connection before the client has read the answer.
pub fn linger(stream: &mut TcpStream) {
    if stream.shutdown(Shutdown::Write).is_err() {
        return;
    }
    let deadline = Instant::now() + LINGER_WAIT;
    let mut chunk = [0_u8; 8 * 1024];
    let mut dropped = 0;
    while dropped < BODY_MAX {
        match read_before(stream, deadline, &mut chunk) {
            Ok(read) => dropped += read,
            Err(_) => return,
        }
    }
}

/// The head at the start of `buf` and its length, None while more bytes are needed. Checks the
/// version and the target. The body is left empty.
fn parse_head(buf: &[u8]) -> Result<Option<(Request, usize)>, Refusal> {
    let mut slots = [httparse::EMPTY_HEADER; HEADERS_MAX];
    let mut parsed = httparse::Request::new(&mut slots);
    let used = match parsed.parse(buf) {
        Ok(httparse::Status::Complete(used)) => used,
        Ok(httparse::Status::Partial) if buf.len() >= HEAD_MAX => {
            return Err(Refusal::HeadTooLarge);
        }
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(httparse::Error::TooManyHeaders) => return Err(Refusal::HeadTooLarge),
        Err(_) => return Err(Refusal::Malformed),
    };
    if parsed.version != Some(1) {
        return Err(Refusal::Version);
    }
    let target = parsed.path.ok_or(Refusal::Malformed)?;
    if !target.starts_with('/') || !target.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(Refusal::Malformed);
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query.to_owned())),
        None => (target, None),
    };
    let method = match parsed.method {
        Some("GET") => Method::Get,
        Some("HEAD") => Method::Head,
        Some("POST") => Method::Post,
        _ => Method::Other,
    };
    let headers = parsed
        .headers
        .iter()
        .map(|header| Header {
            name: header.name.to_ascii_lowercase(),
            value: header.value.to_vec(),
        })
        .collect();
    let request = Request {
        method,
        path: path.to_owned(),
        query,
        headers,
        body: Vec::new(),
    };
    Ok(Some((request, used)))
}

/// The checks after the head parsed, in order. Returns the body's length.
fn admit(parsed: &Request, port: u16) -> Result<usize, Refusal> {
    if parsed.has("origin") {
        return Err(Refusal::Origin);
    }
    let host = format!("127.0.0.1:{port}");
    if parsed.only("host") != Some(host.as_bytes()) {
        return Err(Refusal::Host);
    }
    if parsed.has("transfer-encoding") {
        return Err(Refusal::LengthRequired);
    }
    if parsed.headers.iter().any(|header| {
        header.name == "content-encoding" && !header.value.eq_ignore_ascii_case(b"identity")
    }) {
        return Err(Refusal::Encoding);
    }
    let mut lengths = parsed
        .headers
        .iter()
        .filter(|header| header.name == "content-length");
    let Some(length) = lengths.next() else {
        return Ok(0);
    };
    if lengths.next().is_some()
        || length.value.is_empty()
        || !length.value.iter().all(u8::is_ascii_digit)
    {
        return Err(Refusal::Malformed);
    }
    // Plain ASCII digits that do not fit a usize are past the cap too.
    let length = std::str::from_utf8(&length.value)
        .ok()
        .and_then(|digits| digits.parse::<usize>().ok())
        .ok_or(Refusal::TooLarge)?;
    if length > BODY_MAX {
        return Err(Refusal::TooLarge);
    }
    Ok(length)
}

/// A complete local answer: `content-type: application/json`, `content-length`, `connection: close`.
pub fn write_local(
    stream: &mut TcpStream,
    status: u16,
    extra: &[(&str, &str)],
    body: &[u8],
) -> io::Result<()> {
    let mut out = status_line(status);
    put_header(&mut out, "content-type", b"application/json");
    put_header(
        &mut out,
        "content-length",
        body.len().to_string().as_bytes(),
    );
    for (name, value) in extra {
        put_header(&mut out, name, value.as_bytes());
    }
    put_header(&mut out, "connection", b"close");
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    stream.write_all(&out)
}

/// The head of a relayed reply: `headers` as given, then `transfer-encoding: chunked` and
/// `connection: close`.
pub fn write_head(stream: &mut TcpStream, status: u16, headers: &[Header]) -> io::Result<()> {
    let mut out = status_line(status);
    for header in headers {
        put_header(&mut out, &header.name, &header.value);
    }
    put_header(&mut out, "transfer-encoding", b"chunked");
    put_header(&mut out, "connection", b"close");
    out.extend_from_slice(b"\r\n");
    stream.write_all(&out)
}

/// One chunk, written with a single `write_all`. An empty slice writes nothing.
pub fn write_chunk(stream: &mut TcpStream, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    let mut out = format!("{:x}\r\n", bytes.len()).into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\r\n");
    stream.write_all(&out)
}

/// The clean terminator `0\r\n\r\n`. Only ever written after a clean upstream end.
pub fn write_end(stream: &mut TcpStream) -> io::Result<()> {
    stream.write_all(b"0\r\n\r\n")
}

/// Whether the client has closed its side. Never blocks. Bytes waiting are not a disconnect.
#[must_use]
pub fn client_gone(stream: &TcpStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut byte = [0_u8; 1];
    let gone = match stream.peek(&mut byte) {
        Ok(read) => read == 0,
        Err(e) => !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
    };
    stream.set_nonblocking(false).is_err() || gone
}

fn status_line(status: u16) -> Vec<u8> {
    format!("HTTP/1.1 {status} {}\r\n", reason(status)).into_bytes()
}

const fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        529 => "Overloaded",
        _ => "Status",
    }
}

/// `name: value\r\n`, or nothing when either holds a line break.
fn put_header(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    let breaks = |byte: &u8| matches!(byte, b'\r' | b'\n');
    if name.bytes().any(|byte| breaks(&byte)) || value.iter().any(breaks) {
        return;
    }
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(value);
    out.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests;

//! The router's accept loop: `127.0.0.1` only, one thread and one request per connection, and a
//! cap on live connections answered with a 503 beyond it.

use super::answers;
use super::wire::{self, HEAD_WAIT, READ_WAIT, Request, WRITE_WAIT};
use crate::manager::socket::Seat;
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// "Tens, not eight" (brief, E19).
pub const STREAMS_MAX: usize = 32;
/// A pause after a failed accept, so a persistent failure does not spin.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);
/// How long the accept thread spends writing a busy answer.
const BUSY_WAIT: Duration = Duration::from_secs(1);

/// What answers one request. The router implements it (relay.rs).
pub trait Handler: Send + Sync + 'static {
    /// Whether the head names a desk this handler serves. Asked before the body is read.
    fn admits(&self, head: &Request) -> bool;
    fn handle(&self, request: Request, stream: &mut TcpStream);
}

/// The caps and waits of one listener. `listen` uses the real ones, tests small ones.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub streams: usize,
    pub head_wait: Duration,
    pub read_wait: Duration,
}

/// Bind `127.0.0.1:0`, start the accept thread ("routing-accept") and return the port.
/// The thread runs until the process ends.
pub fn listen(handler: Arc<dyn Handler>) -> Result<u16, String> {
    listen_within(
        handler,
        Limits {
            streams: STREAMS_MAX,
            head_wait: HEAD_WAIT,
            read_wait: READ_WAIT,
        },
    )
}

/// `listen` with its caps and waits given.
pub(crate) fn listen_within(handler: Arc<dyn Handler>, limits: Limits) -> Result<u16, String> {
    let opened = |e: std::io::Error| format!("the building's router could not open a port ({e})");
    let listener = bind().map_err(opened)?;
    let port = listener.local_addr().map_err(opened)?.port();
    thread::Builder::new()
        .name("routing-accept".to_owned())
        .spawn(move || accept(&listener, port, &handler, limits))
        .map(drop)
        .map_err(|e| format!("the building's router could not start ({e})"))?;
    Ok(port)
}

/// The literal loopback address with a port the system picks. Never `0.0.0.0`, `localhost` or `::1`.
fn bind() -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
}

/// Follows `manager::socket::accept`, with a busy answer where that one closes silently: a silent
/// close makes the client retry blind.
fn accept(listener: &TcpListener, port: u16, handler: &Arc<dyn Handler>, limits: Limits) {
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            thread::sleep(ACCEPT_PAUSE);
            continue;
        };
        if open.fetch_add(1, Ordering::SeqCst) >= limits.streams {
            open.fetch_sub(1, Ordering::SeqCst);
            busy(&mut stream);
            continue;
        }
        let seat = Seat(Arc::clone(&open));
        let handler = Arc::clone(handler);
        // A failed spawn drops the closure with the stream and the seat, so the spare copy is
        // what is left to answer on.
        let spare = stream.try_clone();
        let spawned = thread::Builder::new()
            .name("routing-conn".to_owned())
            .spawn(move || {
                let _seat = seat;
                connection(stream, port, handler.as_ref(), limits);
            });
        if let (Err(_), Ok(mut spare)) = (spawned, spare) {
            busy(&mut spare);
        }
    }
}

/// `answers::busy()` (503, `retry-after: 1`), written within `BUSY_WAIT`, then closed.
fn busy(stream: &mut TcpStream) {
    if stream.set_write_timeout(Some(BUSY_WAIT)).is_ok() {
        let _unsent = answers::busy().write(stream);
    }
    let _closed = stream.shutdown(Shutdown::Both);
}

/// One connection: read one request, refuse it or hand it over, then close.
fn connection(mut stream: TcpStream, port: u16, handler: &dyn Handler, limits: Limits) {
    if stream.set_nodelay(true).is_err() || stream.set_write_timeout(Some(WRITE_WAIT)).is_err() {
        return;
    }
    match wire::read_request_within(
        &mut stream,
        port,
        limits.head_wait,
        limits.read_wait,
        &|head| handler.admits(head),
    ) {
        Ok(request) => handler.handle(request, &mut stream),
        Err(refusal) => {
            if let Some(answer) = answers::transport(refusal)
                && answer.write(&mut stream).is_ok()
            {
                wire::linger(&mut stream);
            }
        }
    }
    let _closed = stream.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::wire::{BODY_MAX, HEAD_MAX, Method};
    use std::io::{Read, Write};
    use std::sync::Mutex;
    use std::sync::mpsc;
    use std::time::Instant;

    /// Records each request's method, path and body, waits for the gate when there is one, and
    /// answers 200 `{}`.
    #[derive(Default)]
    struct Recorder {
        seen: Mutex<Vec<(Method, String, Vec<u8>)>>,
        gate: Option<Mutex<mpsc::Receiver<()>>>,
    }

    impl Handler for Recorder {
        fn admits(&self, _head: &Request) -> bool {
            true
        }

        fn handle(&self, request: Request, stream: &mut TcpStream) {
            crate::sync::lock(&self.seen).push((request.method, request.path, request.body));
            if let Some(gate) = &self.gate {
                let _ = crate::sync::lock(gate).recv();
            }
            let _ = wire::write_local(stream, 200, &[], b"{}");
        }
    }

    impl Recorder {
        fn count(&self) -> usize {
            crate::sync::lock(&self.seen).len()
        }
    }

    fn limits(streams: usize, head_wait: Duration) -> Limits {
        Limits {
            streams,
            head_wait,
            read_wait: Duration::from_secs(5),
        }
    }

    fn start(recorder: &Arc<Recorder>, limits: Limits) -> u16 {
        let handler: Arc<dyn Handler> = recorder.clone();
        listen_within(handler, limits).unwrap()
    }

    fn connect(port: u16) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
    }

    fn post(port: u16, extra: &str, body: &str) -> Vec<u8> {
        format!(
            "POST /s/r/v1/messages HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\n{extra}content-length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    /// Everything the server wrote until it closed.
    fn answer(mut stream: TcpStream) -> String {
        let mut got = Vec::new();
        let _ = stream.read_to_end(&mut got);
        String::from_utf8(got).unwrap()
    }

    fn wait_for(what: &str, done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(10), "{what}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn the_listener_is_on_loopback_with_a_real_port() {
        let listener = bind().unwrap();
        let address = listener.local_addr().unwrap();
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert_ne!(address.port(), 0);

        let recorder = Arc::new(Recorder::default());
        let handler: Arc<dyn Handler> = recorder.clone();
        let port = listen(handler).unwrap();
        assert_ne!(port, 0);
        let stream = connect(port);
        assert_eq!(
            stream.peer_addr().unwrap().to_string(),
            format!("127.0.0.1:{port}")
        );
    }

    #[test]
    fn a_request_reaches_the_handler_once_and_the_reply_ends_the_connection() {
        let recorder = Arc::new(Recorder::default());
        let port = start(&recorder, limits(4, HEAD_WAIT));
        let mut stream = connect(port);
        stream.write_all(&post(port, "", "{\"a\":1}")).unwrap();
        let got = answer(stream);
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "{got}");
        assert!(got.contains("connection: close\r\n"), "{got}");
        assert!(got.ends_with("\r\n\r\n{}"), "{got}");
        assert_eq!(
            *crate::sync::lock(&recorder.seen),
            [(
                Method::Post,
                "/s/r/v1/messages".to_owned(),
                b"{\"a\":1}".to_vec()
            )]
        );
    }

    #[test]
    fn refusals_never_reach_the_handler() {
        let recorder = Arc::new(Recorder::default());
        let port = start(&recorder, limits(4, HEAD_WAIT));
        let mut big_head = b"POST / HTTP/1.1\r\nx-pad: ".to_vec();
        big_head.resize(HEAD_MAX, b'a');
        let rows: [(&str, Vec<u8>, &str); 5] = [
            ("origin", post(port, "origin: http://example.invalid\r\n", "{}"), "403"),
            (
                "wrong host",
                b"POST / HTTP/1.1\r\nhost: localhost\r\ncontent-length: 0\r\n\r\n".to_vec(),
                "403",
            ),
            ("head too large", big_head, "431"),
            (
                "body over the cap",
                format!(
                    "POST / HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ncontent-length: {}\r\n\r\n",
                    BODY_MAX + 1
                )
                .into_bytes(),
                "413",
            ),
            (
                "transfer-encoding",
                format!(
                    "POST / HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ntransfer-encoding: chunked\r\n\r\n"
                )
                .into_bytes(),
                "411",
            ),
        ];
        for (name, request, status) in rows {
            let mut stream = connect(port);
            stream.write_all(&request).unwrap();
            let got = answer(stream);
            assert!(
                got.starts_with(&format!("HTTP/1.1 {status} ")),
                "{name}: {got}"
            );
            assert!(got.contains("\"type\":\"error\""), "{name}: {got}");
        }
        assert_eq!(recorder.count(), 0);
    }

    #[test]
    fn beyond_the_cap_a_connection_is_told_to_wait_and_a_freed_seat_serves_again() {
        let (gate, held) = mpsc::channel();
        let recorder = Arc::new(Recorder {
            gate: Some(Mutex::new(held)),
            ..Recorder::default()
        });
        let port = start(&recorder, limits(2, HEAD_WAIT));
        let held: Vec<TcpStream> = (0..2)
            .map(|_| {
                let mut stream = connect(port);
                stream.write_all(&post(port, "", "{}")).unwrap();
                stream
            })
            .collect();
        wait_for("both held requests reach the handler", || {
            recorder.count() == 2
        });

        let third = answer(connect(port));
        assert!(third.starts_with("HTTP/1.1 503 "), "{third}");
        assert!(third.contains("retry-after: 1\r\n"), "{third}");
        assert!(
            third.contains("content-type: application/json\r\n"),
            "{third}"
        );
        assert!(third.contains("\"type\":\"overloaded_error\""), "{third}");
        assert_eq!(recorder.count(), 2, "the third never reached the handler");

        // Let one go, and keep enough tokens for the other held one and the next served.
        for _ in 0..3 {
            gate.send(()).unwrap();
        }
        let mut held = held.into_iter();
        let first = answer(held.next().unwrap());
        assert!(first.starts_with("HTTP/1.1 200 "), "{first}");

        let mut served = None;
        for _ in 0..100 {
            let mut stream = connect(port);
            stream.write_all(&post(port, "", "{}")).unwrap();
            let got = answer(stream);
            if got.starts_with("HTTP/1.1 200 ") {
                served = Some(got);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(served.is_some(), "a freed seat serves the next connection");
        drop(held);
    }

    /// Serves no desk.
    struct Stranger;

    impl Handler for Stranger {
        fn admits(&self, _head: &Request) -> bool {
            false
        }

        fn handle(&self, _request: Request, _stream: &mut TcpStream) {
            unreachable!("an unadmitted head never reaches the handler");
        }
    }

    #[test]
    fn an_unadmitted_request_with_a_large_body_still_gets_its_answer() {
        let port = listen_within(Arc::new(Stranger), limits(4, HEAD_WAIT)).unwrap();
        for _ in 0..5 {
            let stream = connect(port);
            let mut sender = stream.try_clone().unwrap();
            let body = "x".repeat(4 * 1024 * 1024);
            let request = post(port, "", &body);
            let sending = thread::spawn(move || sender.write_all(&request).is_ok());
            thread::sleep(Duration::from_millis(50));
            let mut got = Vec::new();
            let mut reader = stream;
            let read = reader.read_to_end(&mut got);
            assert!(read.is_ok(), "{read:?}");
            let got = String::from_utf8(got).unwrap();
            assert!(got.starts_with("HTTP/1.1 400 "), "{got}");
            assert!(got.contains("did not recognise this desk"), "{got}");
            assert!(sending.join().unwrap(), "the whole body was taken");
        }
    }

    #[test]
    fn silence_is_closed_after_the_head_wait() {
        let recorder = Arc::new(Recorder::default());
        let port = start(&recorder, limits(4, Duration::from_millis(200)));
        let started = Instant::now();
        let got = answer(connect(port));
        assert_eq!(got, "", "closed with no answer");
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(recorder.count(), 0);
    }
}

//! Server-sent events: `Events` splits an upstream body into events as the bytes arrive, and
//! `frame` writes one event as Anthropic frames it. Only the framing U1's streaming example
//! shows, and no more of the event-stream format than that needs: an event's type is read from
//! its JSON (U2: every event has `type`), so `event:` and every other field are ignored.

use std::io::{self, ErrorKind, Read};

/// The most one event may hold. `response.created` and `response.completed` repeat the
/// request's tools and the reply's output.
pub const EVENT_MAX: usize = 16 * 1024 * 1024;

/// The most one read asks for.
const READ_SIZE: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// One event's data: its `data:` lines joined with a line break.
    Event(Vec<u8>),
    /// The body ended. Whatever was pending without its blank line is discarded.
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Broken {
    /// A read failed: the upstream dropped.
    Upstream,
    /// One event passed `EVENT_MAX`.
    Oversize,
}

/// Splits an upstream body into events as the bytes arrive. Never reads ahead of what it
/// returns: a read is made only when no whole event is pending.
pub struct Events<R> {
    from: R,
    /// Bytes read and not yet taken as lines: the start of a line whose end has not arrived.
    pending: Vec<u8>,
    /// How much of `pending` is known to hold no line break, so a long line is scanned once.
    scanned: usize,
    /// The data of the event being read.
    data: Vec<u8>,
    /// Whether the event being read has had a `data:` line.
    has_data: bool,
    cap: usize,
}

impl<R: Read> Events<R> {
    pub fn new(from: R) -> Self {
        Self::with_cap(from, EVENT_MAX)
    }

    fn with_cap(from: R, cap: usize) -> Self {
        Self {
            from,
            pending: Vec::new(),
            scanned: 0,
            data: Vec::new(),
            has_data: false,
            cap,
        }
    }

    /// The next event, as soon as it is whole.
    #[expect(
        clippy::should_implement_trait,
        reason = "the end of the body and a broken body are results, not the end of an iterator"
    )]
    pub fn next(&mut self) -> Result<Next, Broken> {
        let mut buf = vec![0; READ_SIZE];
        loop {
            if let Some(event) = self.take_event() {
                return Ok(Next::Event(event));
            }
            if self.data.len() + self.pending.len() > self.cap {
                return Err(Broken::Oversize);
            }
            let read = read_some(&mut self.from, &mut buf).map_err(|_| Broken::Upstream)?;
            let Some(bytes) = buf.get(..read).filter(|bytes| !bytes.is_empty()) else {
                self.pending.clear();
                self.scanned = 0;
                self.data.clear();
                self.has_data = false;
                return Ok(Next::End);
            };
            self.pending.extend_from_slice(bytes);
        }
    }

    /// Takes the whole lines pending, up to and including the blank line that ends an event
    /// with data. None: no such event is pending yet.
    fn take_event(&mut self) -> Option<Vec<u8>> {
        let mut start = 0;
        let mut from = self.scanned;
        let event = loop {
            let Some(at) = self
                .pending
                .get(from..)
                .and_then(|rest| rest.iter().position(|&b| b == b'\n'))
            else {
                break None;
            };
            let end = from + at;
            let line = self.pending.get(start..end).unwrap_or_default();
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            start = end + 1;
            from = start;
            if line.is_empty() {
                // A blank line ends an event. One with no data is skipped.
                if self.has_data {
                    self.has_data = false;
                    break Some(std::mem::take(&mut self.data));
                }
            } else if let Some(value) = line.strip_prefix(b"data:") {
                let value = value.strip_prefix(b" ").unwrap_or(value);
                if self.has_data {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
                self.has_data = true;
            }
            // Anything else is a comment (`:`) or a field this route does not read.
        };
        self.pending.drain(..start);
        self.scanned = if event.is_some() {
            0
        } else {
            self.pending.len()
        };
        event
    }
}

/// One event as Anthropic frames it (U11): `event: <event>`, `data: <data>`, a blank line.
#[must_use]
pub fn frame(event: &str, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(event.len() + data.len() + 16);
    out.extend_from_slice(b"event: ");
    out.extend_from_slice(event.as_bytes());
    out.extend_from_slice(b"\ndata: ");
    out.extend_from_slice(data);
    out.extend_from_slice(b"\n\n");
    out
}

/// One read, retried when it was interrupted.
pub(crate) fn read_some(from: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match from.read(buf) {
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            read => return read,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::upstream::{Step, scripted_body as body};
    use std::sync::mpsc;
    use std::time::Duration;

    const ONE: &[u8] = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\n";

    fn bytes(chunks: &[&[u8]]) -> Vec<Step> {
        chunks.iter().map(|c| Step::Bytes(c.to_vec())).collect()
    }

    /// Every `next` until the body ends or breaks.
    fn all(steps: Vec<Step>) -> (Vec<Vec<u8>>, Result<Next, Broken>) {
        all_capped(steps, EVENT_MAX)
    }

    fn all_capped(steps: Vec<Step>, cap: usize) -> (Vec<Vec<u8>>, Result<Next, Broken>) {
        let mut events = Events::with_cap(body(steps), cap);
        let mut seen = Vec::new();
        loop {
            match events.next() {
                Ok(Next::Event(data)) => seen.push(data),
                last => return (seen, last),
            }
        }
    }

    fn texts(events: &[Vec<u8>]) -> Vec<String> {
        events
            .iter()
            .map(|e| String::from_utf8_lossy(e).into_owned())
            .collect()
    }

    /// A table row: its name, the script, the events expected, and how the body ends.
    type Row = (
        &'static str,
        Vec<Step>,
        &'static [&'static str],
        Result<Next, Broken>,
    );

    #[test]
    fn splits_events_as_the_format_says() {
        let rows: [Row; 12] = [
            (
                "one event",
                bytes(&[ONE]),
                &[r#"{"type":"response.created"}"#],
                Ok(Next::End),
            ),
            (
                "two events in one read",
                bytes(&[b"data: a\n\ndata: b\n\n"]),
                &["a", "b"],
                Ok(Next::End),
            ),
            (
                "crlf line ends",
                bytes(&[b"event: x\r\ndata: a\r\n\r\ndata: b\r\n\r\n"]),
                &["a", "b"],
                Ok(Next::End),
            ),
            (
                "comment, event and id lines change nothing",
                bytes(&[b": keep-alive\nevent: x\nid: 7\ndata: a\nretry: 10\n\n"]),
                &["a"],
                Ok(Next::End),
            ),
            (
                "two data lines are joined with a line break",
                bytes(&[b"data: a\ndata: b\n\n"]),
                &["a\nb"],
                Ok(Next::End),
            ),
            (
                "data with and without the space",
                bytes(&[b"data:a\n\ndata: b\n\ndata:  c\n\n"]),
                &["a", "b", " c"],
                Ok(Next::End),
            ),
            (
                "an event with no data is skipped",
                bytes(&[b"event: ping\n\n: c\n\ndata: a\n\n"]),
                &["a"],
                Ok(Next::End),
            ),
            (
                "the body ends at an event boundary",
                bytes(&[b"data: a\n\n"]),
                &["a"],
                Ok(Next::End),
            ),
            (
                "the body ends inside an event: the pending bytes are discarded",
                bytes(&[b"data: a\n\ndata: b\n"]),
                &["a"],
                Ok(Next::End),
            ),
            (
                "the body ends inside a line",
                bytes(&[b"data: a\n\ndata: b"]),
                &["a"],
                Ok(Next::End),
            ),
            (
                "a read that fails",
                vec![Step::Bytes(b"data: a\n\ndata: b".to_vec()), Step::Break],
                &["a"],
                Err(Broken::Upstream),
            ),
            ("an empty body", Vec::new(), &[], Ok(Next::End)),
        ];
        for (name, steps, expected, last) in rows {
            let (seen, end) = all(steps);
            assert_eq!(texts(&seen), expected.to_vec(), "{name}");
            assert_eq!(end, last, "{name}");
        }
    }

    #[test]
    fn one_event_split_across_two_reads_at_every_byte() {
        for at in 0..=ONE.len() {
            let (head, tail) = ONE.split_at(at);
            let (seen, end) = all(bytes(&[head, tail]));
            assert_eq!(
                texts(&seen),
                vec![r#"{"type":"response.created"}"#],
                "split at {at}"
            );
            assert_eq!(end, Ok(Next::End), "split at {at}");
        }
    }

    #[test]
    fn an_event_over_the_cap_is_oversize() {
        let rows: [(&str, Vec<Step>, Result<Next, Broken>); 4] = [
            (
                "a line at the cap",
                bytes(&[b"data: 12345678", b"\n\n"]),
                Ok(Next::End),
            ),
            (
                "one data line over the cap",
                bytes(&[b"data: 123456789", b"\n\n"]),
                Err(Broken::Oversize),
            ),
            (
                "data lines that pass the cap together",
                bytes(&[b"data: 1234567\n", b"data: 1234567\n", b"\n"]),
                Err(Broken::Oversize),
            ),
            (
                "an endless line with no break",
                bytes(&[b": 123456", b"7890123"]),
                Err(Broken::Oversize),
            ),
        ];
        for (name, steps, last) in rows {
            let (_, end) = all_capped(steps, 14);
            assert_eq!(end, last, "{name}");
        }
    }

    #[test]
    fn next_returns_the_first_event_without_waiting_for_the_second() {
        let (open, gate) = mpsc::channel();
        let steps = vec![
            Step::Bytes(b"data: first\n\n".to_vec()),
            Step::Hold(gate),
            Step::Bytes(b"data: second\n\n".to_vec()),
        ];
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut events = Events::new(body(steps));
            for _ in 0..3 {
                if tx.send(events.next()).is_err() {
                    return;
                }
            }
        });
        let wait = Duration::from_secs(5);
        assert_eq!(
            rx.recv_timeout(wait),
            Ok(Ok(Next::Event(b"first".to_vec())))
        );
        open.send(()).expect("open the gate");
        assert_eq!(
            rx.recv_timeout(wait),
            Ok(Ok(Next::Event(b"second".to_vec())))
        );
        assert_eq!(rx.recv_timeout(wait), Ok(Ok(Next::End)));
        reader.join().expect("reader thread");
    }

    #[test]
    fn frame_writes_one_event() {
        assert_eq!(
            frame("message_stop", br#"{"type":"message_stop"}"#),
            b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_vec()
        );
    }
}

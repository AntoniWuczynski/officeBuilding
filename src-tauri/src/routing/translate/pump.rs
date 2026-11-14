//! The commit boundary on a translated route (section 9): an upstream Responses stream in,
//! Anthropic's stream out through a `Sink`. Nothing is written before the first translated bytes
//! (F1), so every earlier failure can still be answered with a true status. The clean terminator
//! is written only after a reply that ran to its end. No socket here: the relay's sink is the
//! client's.

use std::io::{self, Read};

use super::reply::{End, Failure, Reply};
use super::sse::{Broken, Events, Next};

/// Where translated bytes go. The relay's is the client's socket. Tests record the calls.
pub trait Sink {
    /// The response head. Called once, straight before the first `send`.
    fn open(&mut self) -> io::Result<()>;
    fn send(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// The clean terminator. Called only after a reply that ran to its end.
    fn close(&mut self) -> io::Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The reply ran to its end, and the client has its terminator.
    Clean,
    /// The provider reported a failure, or sent something this route cannot read or carry.
    /// `committed`: translated bytes had reached the client.
    Failed { failure: Failure, committed: bool },
    /// The upstream broke, or its body ended with no terminal event.
    Broke { committed: bool },
    /// A write to the client failed.
    ClientLeft,
}

/// Read `from` as a Responses stream and write its translation to `to`. One `send` per upstream
/// event that yields bytes, straight away. Nothing is buffered across events.
pub fn pump(from: impl Read, to: &mut dyn Sink) -> Ended {
    let mut events = Events::new(from);
    let mut reply = Reply::new();
    let mut committed = false;
    loop {
        let data = match events.next() {
            Err(Broken::Upstream) | Ok(Next::End) => return Ended::Broke { committed },
            Err(Broken::Oversize) => {
                return Ended::Failed {
                    failure: Failure::Unreadable,
                    committed,
                };
            }
            Ok(Next::Event(data)) => data,
        };
        let step = reply.take(&data);
        if !step.out.is_empty() {
            if !committed {
                if to.open().is_err() {
                    return Ended::ClientLeft;
                }
                committed = true;
            }
            if to.send(&step.out).is_err() {
                return Ended::ClientLeft;
            }
        }
        match step.end {
            None => {}
            Some(End::Clean) => {
                return if to.close().is_err() {
                    Ended::ClientLeft
                } else {
                    Ended::Clean
                };
            }
            Some(End::Failed(failure)) => return Ended::Failed { failure, committed },
        }
    }
}

#[cfg(test)]
mod tests {
    // These replies are written from OpenAI's published documentation (U1 "Create a model
    // response", U2 "Responses streaming events", U4 "Error codes"), each is cited, and none was
    // captured from a real account. Every id is invented. `@MODEL@` stands for the model the
    // fixture settings map, so no model name is written here.

    use super::*;
    use crate::routing::fixtures;
    use crate::routing::translate::sse::EVENT_MAX;
    use crate::routing::upstream::{Step, scripted_body as body};

    /// The model A Human mapped for the fixture's `OpenAI` connection.
    fn model() -> String {
        fixtures::oai_target().model
    }

    // U1, the streaming example of "Create a model response".
    const CREATED: &str = r#"{"type":"response.created","response":{"id":"resp_inv0101","object":"response","status":"in_progress","error":null,"incomplete_details":null,"model":"@MODEL@","output":[],"usage":null},"sequence_number":0}"#;
    const IN_PROGRESS: &str = r#"{"type":"response.in_progress","response":{"id":"resp_inv0101","object":"response","status":"in_progress","model":"@MODEL@","output":[],"usage":null},"sequence_number":1}"#;
    const ITEM_ADDED: &str = r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_inv0101","type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":2}"#;
    const PART_ADDED: &str = r#"{"type":"response.content_part.added","item_id":"msg_inv0101","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]},"sequence_number":3}"#;
    const DELTA_1: &str = r#"{"type":"response.output_text.delta","item_id":"msg_inv0101","output_index":0,"content_index":0,"delta":"Hi","sequence_number":4}"#;
    const DELTA_2: &str = r#"{"type":"response.output_text.delta","item_id":"msg_inv0101","output_index":0,"content_index":0,"delta":" there!","sequence_number":5}"#;
    const TEXT_DONE: &str = r#"{"type":"response.output_text.done","item_id":"msg_inv0101","output_index":0,"content_index":0,"text":"Hi there!","sequence_number":6}"#;
    const PART_DONE: &str = r#"{"type":"response.content_part.done","item_id":"msg_inv0101","output_index":0,"content_index":0,"part":{"type":"output_text","text":"Hi there!","annotations":[]},"sequence_number":7}"#;
    const ITEM_DONE: &str = r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_inv0101","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hi there!","annotations":[]}]},"sequence_number":8}"#;
    const COMPLETED: &str = r#"{"type":"response.completed","response":{"id":"resp_inv0101","object":"response","status":"completed","error":null,"incomplete_details":null,"model":"@MODEL@","usage":{"input_tokens":37,"output_tokens":11,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}},"sequence_number":9}"#;

    // U2, `response.failed` with a code from `response.error.code`'s list.
    const FAILED: &str = r#"{"type":"response.failed","response":{"id":"resp_inv0101","object":"response","status":"failed","error":{"code":"server_error","message":"The model failed to generate a response."},"incomplete_details":null,"model":"@MODEL@","output":[],"usage":null},"sequence_number":10}"#;
    // U2, the `error` event (`code`, `message`, `param`), with a code from U4 (U5: an error after
    // the stream began "can arrive as a stream event"). The message is invented.
    const ERROR_USED_UP: &str = r#"{"type":"error","code":"credit_balance_exhausted","message":"Your organization has no prepaid credits remaining.","param":null,"sequence_number":10}"#;

    /// A whole text reply.
    const PLAIN: &[&str] = &[
        CREATED,
        IN_PROGRESS,
        ITEM_ADDED,
        PART_ADDED,
        DELTA_1,
        DELTA_2,
        TEXT_DONE,
        PART_DONE,
        ITEM_DONE,
        COMPLETED,
    ];
    /// Nothing that yields bytes yet, up to and including the content part and before any delta.
    const BEFORE: &[&str] = &[CREATED, IN_PROGRESS, ITEM_ADDED, PART_ADDED];
    /// Up to and including the first text delta.
    const AFTER: &[&str] = &[CREATED, IN_PROGRESS, ITEM_ADDED, PART_ADDED, DELTA_1];

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Open,
        Send(Vec<u8>),
        Close,
    }

    /// Keeps every call that succeeded, and fails its `fail_at`th call (from 0).
    #[derive(Default)]
    struct Recorder {
        calls: Vec<Call>,
        fail_at: Option<usize>,
        made: usize,
    }

    impl Recorder {
        fn failing_at(n: usize) -> Self {
            Self {
                fail_at: Some(n),
                ..Self::default()
            }
        }

        fn record(&mut self, call: Call) -> io::Result<()> {
            let n = self.made;
            self.made += 1;
            if self.fail_at == Some(n) {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the client left"));
            }
            self.calls.push(call);
            Ok(())
        }

        fn sent(&self) -> Vec<u8> {
            self.calls
                .iter()
                .filter_map(|c| match c {
                    Call::Send(bytes) => Some(bytes.as_slice()),
                    Call::Open | Call::Close => None,
                })
                .flatten()
                .copied()
                .collect()
        }
    }

    impl Sink for Recorder {
        fn open(&mut self) -> io::Result<()> {
            self.record(Call::Open)
        }
        fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.record(Call::Send(bytes.to_vec()))
        }
        fn close(&mut self) -> io::Result<()> {
            self.record(Call::Close)
        }
    }

    fn data(event: &str) -> Vec<u8> {
        event.replace("@MODEL@", &model()).into_bytes()
    }

    /// One read per event, framed as U1's streaming example frames it.
    fn script(events: &[&str]) -> Vec<Step> {
        events
            .iter()
            .map(|e| {
                let mut bytes = b"data: ".to_vec();
                bytes.extend(data(e));
                bytes.extend(b"\n\n");
                Step::Bytes(bytes)
            })
            .collect()
    }

    /// What the reducer makes of each event that yields bytes, in order.
    fn translated(events: &[&str]) -> Vec<Call> {
        let mut reply = Reply::new();
        events
            .iter()
            .map(|e| reply.take(&data(e)).out)
            .filter(|out| !out.is_empty())
            .map(Call::Send)
            .collect()
    }

    fn joined(head: &[&str], tail: Vec<Step>) -> Vec<Step> {
        let mut steps = script(head);
        steps.extend(tail);
        steps
    }

    /// One data line longer than the cap, in reads of 64 KiB.
    fn oversize() -> Vec<Step> {
        const CHUNK: usize = 64 * 1024;
        let mut steps = vec![Step::Bytes(b"data: ".to_vec())];
        steps.extend((0..=EVENT_MAX / CHUNK).map(|_| Step::Bytes(vec![b'a'; CHUNK])));
        steps
    }

    /// Every way a stream fails: (name, what follows the prefix, the failure, or None for a break).
    fn failing_tails() -> Vec<(&'static str, Vec<Step>, Option<Failure>)> {
        vec![
            (
                "response.failed",
                script(&[FAILED]),
                Some(Failure::Failed {
                    code: Some("server_error".to_owned()),
                    message: Some("The model failed to generate a response.".to_owned()),
                }),
            ),
            (
                "an error event",
                script(&[ERROR_USED_UP]),
                Some(Failure::Error {
                    code: Some("credit_balance_exhausted".to_owned()),
                    message: Some("Your organization has no prepaid credits remaining.".to_owned()),
                }),
            ),
            (
                "data that cannot be read",
                vec![Step::Bytes(b"data: {not json\n\n".to_vec())],
                Some(Failure::Unreadable),
            ),
            (
                "an event over the cap",
                oversize(),
                Some(Failure::Unreadable),
            ),
            ("a body that ends with no terminal event", Vec::new(), None),
            ("a read that fails", vec![Step::Break], None),
        ]
    }

    fn expected(failure: Option<Failure>, committed: bool) -> Ended {
        match failure {
            Some(failure) => Ended::Failed { failure, committed },
            None => Ended::Broke { committed },
        }
    }

    #[test]
    fn a_clean_reply_opens_sends_each_event_and_closes() {
        let mut sink = Recorder::default();
        let ended = pump(body(script(PLAIN)), &mut sink);
        assert_eq!(ended, Ended::Clean);
        let mut calls = vec![Call::Open];
        calls.extend(translated(PLAIN));
        calls.push(Call::Close);
        assert_eq!(sink.calls, calls);
        assert!(
            String::from_utf8_lossy(&sink.sent())
                .ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
            "the reply ends with message_stop"
        );
    }

    #[test]
    fn nothing_is_written_before_the_first_output() {
        let mut sink = Recorder::default();
        let ended = pump(body(joined(BEFORE, vec![Step::Break])), &mut sink);
        assert_eq!(ended, Ended::Broke { committed: false });
        assert!(sink.calls.is_empty(), "{:?}", sink.calls);
    }

    #[test]
    fn a_failure_before_the_first_byte_writes_nothing() {
        for (name, tail, failure) in failing_tails() {
            let mut sink = Recorder::default();
            let ended = pump(body(joined(BEFORE, tail)), &mut sink);
            assert_eq!(ended, expected(failure, false), "{name}");
            assert!(sink.calls.is_empty(), "{name}: {:?}", sink.calls);
        }
    }

    #[test]
    fn a_failure_after_the_first_byte_never_closes() {
        let mut first = vec![Call::Open];
        first.extend(translated(AFTER));
        for (name, tail, failure) in failing_tails() {
            let mut sink = Recorder::default();
            let ended = pump(body(joined(AFTER, tail)), &mut sink);
            assert_eq!(ended, expected(failure, true), "{name}");
            assert_eq!(sink.calls, first, "{name}");
        }
    }

    #[test]
    fn no_failing_script_ends_cleanly() {
        for head in [BEFORE, AFTER] {
            for (name, tail, _) in failing_tails() {
                let mut sink = Recorder::default();
                let ended = pump(body(joined(head, tail)), &mut sink);
                assert_ne!(ended, Ended::Clean, "{name}");
                assert!(!sink.calls.contains(&Call::Close), "{name}");
                let sent = String::from_utf8_lossy(&sink.sent()).into_owned();
                assert!(!sent.contains("message_stop"), "{name}");
                assert!(!sent.contains("message_delta"), "{name}");
            }
        }
    }

    #[test]
    fn a_client_that_leaves_ends_the_pump() {
        let sends = translated(PLAIN).len();
        let close = sends + 1;
        let rows = [
            ("on open", 0),
            ("on the first send", 1),
            ("on a later send", 2),
            ("on close", close),
        ];
        for (name, fail_at) in rows {
            let mut sink = Recorder::failing_at(fail_at);
            let ended = pump(body(script(PLAIN)), &mut sink);
            assert_eq!(ended, Ended::ClientLeft, "{name}");
            assert_eq!(
                sink.made,
                fail_at + 1,
                "{name}: no call after the failed one"
            );
            assert!(!sink.calls.contains(&Call::Close), "{name}");
        }
    }
}

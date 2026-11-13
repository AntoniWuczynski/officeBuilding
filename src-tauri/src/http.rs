//! One HTTPS JSON POST, behind a trait so tests never open a socket.

use crate::secrets::Secret;
use std::fmt;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// The largest answer read, in bytes.
const MAX_BODY: u64 = 4 * 1024 * 1024;

/// A JSON POST.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub url: &'a str,
    /// Sent as the `Authorization` header, exactly as given.
    pub authorization: Option<&'a Secret>,
    /// The JSON body.
    pub body: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    /// The `Retry-After` header, when it is a whole number of seconds.
    pub retry_after: Option<Duration>,
    pub body: String,
}

/// Why no reply came. Fixed words: nothing from the request is ever echoed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    Offline,
    TimedOut,
    Tls,
    TooLarge,
    Other,
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Offline => "it could not be reached",
            Self::TimedOut => "it took too long to answer",
            Self::Tls => "the secure connection failed",
            Self::TooLarge => "its answer was too large",
            Self::Other => "the request failed",
        })
    }
}

pub trait HttpClient: Send + Sync {
    /// Any status is a reply. Only a request that got no answer is an error.
    fn post_json(&self, request: &Request<'_>) -> Result<Reply, HttpError>;
}

/// The real client: `ureq` on the system's TLS and root certificates.
pub struct UreqClient {
    agent: ureq::Agent,
}

impl UreqClient {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .https_only(true)
            .max_redirects(0)
            .user_agent("office-building")
            .tls_config(tls())
            .build()
            .into();
        Self { agent }
    }
}

impl HttpClient for UreqClient {
    fn post_json(&self, request: &Request<'_>) -> Result<Reply, HttpError> {
        let mut builder = self
            .agent
            .post(request.url)
            .header("Content-Type", "application/json");
        if let Some(secret) = request.authorization {
            builder = builder.header("Authorization", secret.expose());
        }
        let mut response = builder.send(request.body).map_err(|e| reason(&e))?;
        let status = response.status().as_u16();
        let retry_after = retry_after(
            response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
        );
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|e| reason(&e))?;
        Ok(Reply {
            status,
            retry_after,
            body,
        })
    }
}

/// The TLS settings every HTTPS client in the app uses. `native-tls-no-default` bundles no roots:
/// the default `RootCerts::WebPki` would panic at connect time, so the platform verifier is
/// required here.
pub(crate) fn tls() -> TlsConfig {
    TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build()
}

/// Fixed words for a failed request. The error itself is never formatted: some variants
/// echo the URL.
fn reason(error: &ureq::Error) -> HttpError {
    match error {
        ureq::Error::Timeout(_) => HttpError::TimedOut,
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            HttpError::Offline
        }
        ureq::Error::Tls(_) | ureq::Error::NativeTls(_) | ureq::Error::TlsRequired => {
            HttpError::Tls
        }
        ureq::Error::BodyExceedsLimit(_) => HttpError::TooLarge,
        _ => HttpError::Other,
    }
}

/// A `Retry-After` value that is a whole number of seconds. A date or junk is `None`.
pub(crate) fn retry_after(value: Option<&str>) -> Option<Duration> {
    value?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// What a fake was asked to send.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub url: String,
    pub authorization: Option<String>,
    pub body: String,
}

#[cfg(test)]
type Answer = Box<dyn Fn(&Sent) -> Result<Reply, HttpError> + Send + Sync>;

/// For tests: records every request and answers from a closure. Nothing is sent.
#[cfg(test)]
pub struct FakeHttp {
    sent: std::sync::Mutex<Vec<Sent>>,
    reply: Answer,
}

#[cfg(test)]
impl FakeHttp {
    pub fn new(reply: impl Fn(&Sent) -> Result<Reply, HttpError> + Send + Sync + 'static) -> Self {
        Self {
            sent: std::sync::Mutex::new(Vec::new()),
            reply: Box::new(reply),
        }
    }

    /// Every request so far, in order.
    pub fn sent(&self) -> Vec<Sent> {
        crate::sync::lock(&self.sent).clone()
    }
}

#[cfg(test)]
impl HttpClient for FakeHttp {
    fn post_json(&self, request: &Request<'_>) -> Result<Reply, HttpError> {
        let sent = Sent {
            url: request.url.to_string(),
            authorization: request.authorization.map(|s| s.expose().to_string()),
            body: request.body.to_string(),
        };
        crate::sync::lock(&self.sent).push(sent.clone());
        (self.reply)(&sent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_reads_whole_seconds_only() {
        assert_eq!(retry_after(Some("30")), Some(Duration::from_secs(30)));
        assert_eq!(retry_after(Some(" 30 ")), Some(Duration::from_secs(30)));
        assert_eq!(retry_after(Some("0")), Some(Duration::ZERO));
        assert_eq!(retry_after(None), None);
        assert_eq!(retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), None);
        assert_eq!(retry_after(Some("-1")), None);
        assert_eq!(retry_after(Some("soon")), None);
    }

    #[test]
    fn reason_maps_each_failure_to_fixed_words() {
        let cases = [
            (
                ureq::Error::Timeout(ureq::Timeout::Global),
                HttpError::TimedOut,
            ),
            (ureq::Error::HostNotFound, HttpError::Offline),
            (ureq::Error::ConnectionFailed, HttpError::Offline),
            (
                ureq::Error::Io(std::io::Error::other("reset")),
                HttpError::Offline,
            ),
            (ureq::Error::Tls("x"), HttpError::Tls),
            (ureq::Error::TlsRequired, HttpError::Tls),
            (ureq::Error::BodyExceedsLimit(1), HttpError::TooLarge),
            (ureq::Error::StatusCode(500), HttpError::Other),
        ];
        for (error, want) in &cases {
            assert_eq!(reason(error), *want);
        }
    }

    #[test]
    fn http_error_words() {
        let words: Vec<String> = [
            HttpError::Offline,
            HttpError::TimedOut,
            HttpError::Tls,
            HttpError::TooLarge,
            HttpError::Other,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            words,
            [
                "it could not be reached",
                "it took too long to answer",
                "the secure connection failed",
                "its answer was too large",
                "the request failed",
            ]
        );
    }

    #[test]
    fn real_client_refuses_plain_http_before_connecting() {
        let client = UreqClient::new(Duration::from_secs(5));
        let got = client.post_json(&Request {
            url: "http://127.0.0.1:9/",
            authorization: None,
            body: "{}",
        });
        assert_eq!(got, Err(HttpError::Other));
    }

    #[test]
    fn fake_records_requests_in_order_and_plays_replies() {
        let fake = FakeHttp::new(|sent| {
            if sent.body == "{}" {
                Ok(Reply {
                    status: 200,
                    retry_after: None,
                    body: "ok".into(),
                })
            } else {
                Err(HttpError::Offline)
            }
        });
        let key = Secret::new("test-key".into());
        let first = fake.post_json(&Request {
            url: "https://example.invalid/a",
            authorization: Some(&key),
            body: "{}",
        });
        let second = fake.post_json(&Request {
            url: "https://example.invalid/b",
            authorization: None,
            body: "[]",
        });
        assert_eq!(first.map(|r| r.body), Ok("ok".to_string()));
        assert_eq!(second, Err(HttpError::Offline));
        assert_eq!(
            fake.sent(),
            vec![
                Sent {
                    url: "https://example.invalid/a".into(),
                    authorization: Some("test-key".into()),
                    body: "{}".into(),
                },
                Sent {
                    url: "https://example.invalid/b".into(),
                    authorization: None,
                    body: "[]".into(),
                },
            ]
        );
    }
}

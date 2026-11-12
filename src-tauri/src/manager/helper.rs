//! The helper mode of the app's own binary: Claude Code starts it as the manager's MCP server,
//! and it relays standard input and output to the running app's socket.

use super::auth::{TOKEN_ENV, well_formed};
use super::socket::{AUTH_WAIT, Hello, Welcome, read_line, write_line};
use crate::secrets::Secret;
use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Instant;

/// The first argument that makes the app's binary run as a manager's helper.
pub const MODE: &str = "manager-mcp";

/// The longest answer to the token the helper reads.
const WELCOME_MAX: usize = 64;

/// Why the helper stopped, with its exit code.
#[derive(Debug)]
struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Run as a helper: connect to `socket`, present the token from the environment, then relay
/// standard input to the socket and the socket to standard output. Returns the exit code.
#[must_use]
pub fn run(socket: Option<OsString>) -> i32 {
    match start(
        socket,
        std::env::var(TOKEN_ENV).ok(),
        io::stdin(),
        io::stdout(),
    ) {
        Ok(()) => 0,
        Err(failure) => {
            eprintln!("office-building manager-mcp: {}", failure.message);
            failure.code
        }
    }
}

/// `run` with the token passed in, so tests need not touch the environment.
fn start(
    socket: Option<OsString>,
    token: Option<String>,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> Result<(), Failure> {
    let socket = socket
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| Failure::new(2, "no socket path was given"))?;
    let token = token
        .filter(|token| well_formed(token))
        .map(Secret::new)
        .ok_or_else(|| {
            Failure::new(
                2,
                "the building's token is not in this helper's environment",
            )
        })?;
    let stream = UnixStream::connect(&socket)
        .map_err(|e| Failure::new(1, format!("the building is not running ({e})")))?;
    relay(stream, token.expose(), input, output).map_err(|message| Failure::new(1, message))
}

/// The relay itself, for tests: `token` is presented on `stream`, then `input` is copied to it
/// and what it sends is copied to `output`, until either side closes.
pub fn relay(
    stream: UnixStream,
    token: &str,
    mut input: impl Read + Send + 'static,
    mut output: impl Write,
) -> Result<(), String> {
    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("could not use the connection to the building ({e})"))?;
    write_line(
        &mut writer,
        &Hello {
            token: Cow::Borrowed(token),
        },
    )
    .map_err(|e| format!("the building did not take the token ({e})"))?;
    let mut reader = BufReader::new(stream);
    let welcome: Welcome = read_line(
        &mut reader,
        WELCOME_MAX,
        Some(Instant::now() + AUTH_WAIT * 2),
    )
    .and_then(|line| serde_json::from_str(&line).ok())
    .ok_or("the building did not answer this helper")?;
    if !welcome.ok {
        return Err("the building refused this helper".to_string());
    }
    reader
        .get_ref()
        .set_read_timeout(None)
        .map_err(|e| format!("could not use the connection to the building ({e})"))?;

    thread::Builder::new()
        .name("manager-helper-input".to_string())
        .spawn(move || {
            let _copied = io::copy(&mut input, &mut writer);
            let _closed = writer.shutdown(Shutdown::Write);
        })
        .map_err(|e| format!("could not start the helper's input thread ({e})"))?;

    loop {
        let chunk = reader
            .fill_buf()
            .map_err(|e| format!("the connection to the building broke ({e})"))?;
        if chunk.is_empty() {
            return Ok(());
        }
        output
            .write_all(chunk)
            .and_then(|()| output.flush())
            .map_err(|e| format!("could not write to the manager ({e})"))?;
        let read = chunk.len();
        reader.consume(read);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::auth::{Registry, new_token};
    use crate::manager::socket::Server;
    use crate::manager::socket::tests::{Echo, short_dir};
    use std::fs;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    fn serving(tag: &str) -> (PathBuf, PathBuf, Arc<Registry>) {
        let base = short_dir(tag);
        let server = Server::bind(&base.join("ipc")).unwrap();
        let path = server.path().to_path_buf();
        let registry = Arc::new(Registry::default());
        server
            .serve(Arc::clone(&registry), Arc::new(Echo::default()))
            .unwrap();
        (base, path, registry)
    }

    fn code(socket: Option<&Path>, token: Option<String>) -> (i32, String) {
        let failure = start(
            socket.map(|path| path.as_os_str().to_os_string()),
            token,
            io::empty(),
            Vec::new(),
        )
        .unwrap_err();
        (failure.code, failure.message)
    }

    #[test]
    fn relay_returns_the_sessions_replies_and_ends_with_the_input() {
        let (base, path, registry) = serving("h1");
        let token = registry.admit("p-1").unwrap();
        let mut output = Vec::new();
        relay(
            UnixStream::connect(&path).unwrap(),
            token.expose(),
            Cursor::new(b"one\ntwo\n".to_vec()),
            &mut output,
        )
        .unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "echo:one\necho:two\n");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn start_relays_with_a_good_token() {
        let (base, path, registry) = serving("h2");
        let token = registry.admit("p-1").unwrap();
        let mut output = Vec::new();
        start(
            Some(path.as_os_str().to_os_string()),
            Some(token.expose().to_string()),
            Cursor::new(b"hi\n".to_vec()),
            &mut output,
        )
        .unwrap();
        assert_eq!(output, b"echo:hi\n");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_refused_token_is_an_error() {
        let (base, path, registry) = serving("h3");
        let _floor = registry.admit("p-1").unwrap();
        let stranger = new_token().unwrap();
        let mut output = Vec::new();
        let error = relay(
            UnixStream::connect(&path).unwrap(),
            stranger.expose(),
            Cursor::new(b"one\n".to_vec()),
            &mut output,
        )
        .unwrap_err();
        assert_eq!(error, "the building refused this helper");
        assert!(output.is_empty());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn start_needs_an_absolute_socket_path_and_a_well_formed_token() {
        let token = || Some(new_token().unwrap().expose().to_string());
        let absolute = std::env::temp_dir().join("ob-no-such.sock");

        let (code_none, message) = code(None, token());
        assert_eq!(code_none, 2);
        assert_eq!(message, "no socket path was given");
        assert_eq!(code(Some(Path::new("ipc/m.sock")), token()).0, 2);

        let (code_missing, message) = code(Some(&absolute), None);
        assert_eq!(code_missing, 2);
        assert_eq!(
            message,
            "the building's token is not in this helper's environment"
        );
        assert_eq!(code(Some(&absolute), Some("abc".to_string())).0, 2);
        assert_eq!(code(Some(&absolute), Some("A".repeat(64))).0, 2);

        let (code_down, message) = code(Some(&absolute), token());
        assert_eq!(code_down, 1);
        assert!(message.starts_with("the building is not running ("));
    }
}

//! The Unix socket a manager's helper reaches the app on. Owner-only, no network, and each
//! connection is bound to one floor by the token it presents first. Nothing here logs.

use super::auth::Registry;
use crate::secrets::Secret;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub const AUTH_WAIT: Duration = Duration::from_secs(5);
pub const AUTH_LINE_MAX: usize = 256;
pub const LINE_MAX: usize = 65_536;
pub const MAX_CONNECTIONS: usize = 8;
pub const SOCKET_NAME: &str = "m.sock";

/// The longest socket path accepted, under macOS's 104-byte `sun_path`.
const PATH_MAX: usize = 100;
const FOLDER_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;
/// How long a refused helper waits for its answer.
const REFUSAL_PAUSE: Duration = Duration::from_millis(300);
/// A pause after a failed accept, so a persistent failure does not spin.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// The helper's first line.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Hello<'a> {
    #[serde(borrow)]
    pub token: Cow<'a, str>,
}

/// The app's answer.
#[derive(Serialize, Deserialize)]
pub(super) struct Welcome {
    pub ok: bool,
}

/// What answers a manager's lines once its connection is bound to a floor.
pub trait Session: Send + Sync {
    /// The reply to one line, or None when it needs none.
    fn handle(&self, floor: &str, line: &str) -> Option<String>;
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Open the socket in `dir`, following section 3.2 of the issue #4 spec. `dir`'s parent
    /// must exist.
    pub fn bind(dir: &Path) -> Result<Self, String> {
        if !dir.parent().is_some_and(Path::is_dir) {
            return Err("the app's data folder is not there".to_string());
        }
        match fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => return Err("the socket's folder is not a plain folder".to_string()),
            Err(e) if e.kind() == ErrorKind::NotFound => DirBuilder::new()
                .mode(FOLDER_MODE)
                .create(dir)
                .map_err(|e| format!("could not make the socket's folder ({e})"))?,
            Err(e) => return Err(format!("could not read the socket's folder ({e})")),
        }
        fs::set_permissions(dir, Permissions::from_mode(FOLDER_MODE))
            .map_err(|e| format!("could not make the socket's folder private ({e})"))?;
        if !has_mode(dir, FOLDER_MODE, true) {
            return Err("the socket's folder could not be made private".to_string());
        }

        let path = dir.join(SOCKET_NAME);
        if path.as_os_str().len() > PATH_MAX {
            return Err("the app's data folder path is too long for a socket".to_string());
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_socket() => {
                if UnixStream::connect(&path).is_ok() {
                    return Err("another copy of the app is already serving the socket".to_string());
                }
                fs::remove_file(&path)
                    .map_err(|e| format!("could not clear the last launch's socket ({e})"))?;
            }
            Ok(_) => {
                return Err("something that is not a socket is at the socket's path".to_string());
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("could not read the socket's path ({e})")),
        }

        let listener =
            UnixListener::bind(&path).map_err(|e| format!("could not open the socket ({e})"))?;
        fs::set_permissions(&path, Permissions::from_mode(SOCKET_MODE))
            .map_err(|e| format!("could not make the socket private ({e})"))?;
        if !has_mode(&path, SOCKET_MODE, false) {
            return Err("the socket could not be made private".to_string());
        }
        Ok(Self { listener, path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Serve connections on a thread named "manager-socket" until the app exits.
    pub fn serve(self, registry: Arc<Registry>, session: Arc<dyn Session>) -> Result<(), String> {
        thread::Builder::new()
            .name("manager-socket".to_string())
            .spawn(move || accept(&self.listener, &registry, &session))
            .map(drop)
            .map_err(|e| format!("could not start the socket's thread ({e})"))
    }
}

/// Whether `path` is (not following a link) a folder or not, with exactly `mode`.
fn has_mode(path: &Path, mode: u32, folder: bool) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.file_type().is_dir() == folder && meta.permissions().mode() & 0o777 == mode
    })
}

/// A place among `MAX_CONNECTIONS`, given back when the connection's thread ends.
struct Seat(Arc<AtomicUsize>);

impl Drop for Seat {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept(listener: &UnixListener, registry: &Arc<Registry>, session: &Arc<dyn Session>) {
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            thread::sleep(ACCEPT_PAUSE);
            continue;
        };
        if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let seat = Seat(Arc::clone(&open));
        let registry = Arc::clone(registry);
        let session = Arc::clone(session);
        // A failed spawn drops the closure, which closes the stream and frees the seat.
        let _detached = thread::Builder::new()
            .name("manager-conn".to_string())
            .spawn(move || {
                let _seat = seat;
                connection(stream, &registry, session.as_ref(), AUTH_WAIT);
            });
    }
}

/// One connection: authenticate within `auth_wait`, then answer its lines for its floor while
/// its token stays registered.
fn connection(stream: UnixStream, registry: &Registry, session: &dyn Session, auth_wait: Duration) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let deadline = Instant::now() + auth_wait;
    let Some((floor, token)) = read_line(&mut reader, AUTH_LINE_MAX, Some(deadline))
        .and_then(|line| authenticate(registry, &line))
    else {
        thread::sleep(REFUSAL_PAUSE);
        let _refused = write_line(&mut writer, &Welcome { ok: false });
        return;
    };
    if write_line(&mut writer, &Welcome { ok: true }).is_err()
        || reader.get_ref().set_read_timeout(None).is_err()
    {
        return;
    }
    while let Some(line) = read_line(&mut reader, LINE_MAX, None) {
        if !registry.holds(&floor, token.expose()) {
            return;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(mut reply) = session.handle(&floor, &line) {
            reply.push('\n');
            if writer.write_all(reply.as_bytes()).is_err() {
                return;
            }
        }
    }
}

/// The floor and token of a `Hello` line whose token is registered.
fn authenticate(registry: &Registry, line: &str) -> Option<(String, Secret)> {
    let hello: Hello<'_> = serde_json::from_str(line).ok()?;
    let floor = registry.floor_of(&hello.token)?;
    Some((floor, Secret::new(hello.token.into_owned())))
}

pub(super) fn write_line<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)
}

/// One line of at most `max` bytes, without its newline, read before `deadline` when there is
/// one. None at end of input, on a read error or timeout, for a longer line and for bytes that
/// are not UTF-8. With no deadline the stream's read timeout is left as it is.
pub(super) fn read_line(
    reader: &mut BufReader<UnixStream>,
    max: usize,
    deadline: Option<Instant>,
) -> Option<String> {
    let mut line = Vec::new();
    loop {
        if let Some(deadline) = deadline {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|left| !left.is_zero())?;
            reader.get_ref().set_read_timeout(Some(left)).ok()?;
        }
        let available = reader.fill_buf().ok()?;
        if available.is_empty() {
            return None;
        }
        let (taken, done) = match available.iter().position(|&b| b == b'\n') {
            Some(at) => (at, true),
            None => (available.len(), false),
        };
        line.extend_from_slice(available.get(..taken)?);
        reader.consume(if done { taken + 1 } else { taken });
        if line.len() > max {
            return None;
        }
        if done {
            return String::from_utf8(line).ok();
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::fs::symlink;
    use std::sync::Mutex;

    /// A short folder under the temp folder, so socket paths stay under the limit.
    pub(in crate::manager) fn short_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ob{tag}{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// Records `(floor, line)` and echoes the line.
    #[derive(Default)]
    pub(in crate::manager) struct Echo {
        pub seen: Mutex<Vec<(String, String)>>,
    }

    impl Session for Echo {
        fn handle(&self, floor: &str, line: &str) -> Option<String> {
            crate::sync::lock(&self.seen).push((floor.to_string(), line.to_string()));
            Some(format!("echo:{line}"))
        }
    }

    pub(in crate::manager) fn hello(token: &Secret) -> String {
        format!("{{\"token\":\"{}\"}}\n", token.expose())
    }

    /// A connection handled on its own thread, with the client's end returned.
    fn open(registry: &Arc<Registry>, session: &Arc<Echo>, wait: Duration) -> UnixStream {
        let (client, server) = UnixStream::pair().unwrap();
        let (registry, session) = (Arc::clone(registry), Arc::clone(session));
        thread::spawn(move || connection(server, &registry, session.as_ref(), wait));
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        client
    }

    fn reply(client: &mut BufReader<UnixStream>) -> String {
        let mut line = String::new();
        client.read_line(&mut line).unwrap();
        line
    }

    fn at_end(client: &mut BufReader<UnixStream>) -> bool {
        let mut rest = Vec::new();
        client.read_to_end(&mut rest).is_ok_and(|n| n == 0)
    }

    fn refused(registry: &Arc<Registry>, first: &[u8]) {
        let session = Arc::new(Echo::default());
        let mut client = open(registry, &session, AUTH_WAIT);
        let _ = client.write_all(first);
        let mut client = BufReader::new(client);
        assert_eq!(reply(&mut client), "{\"ok\":false}\n");
        assert!(at_end(&mut client), "the connection is closed");
        assert!(crate::sync::lock(&session.seen).is_empty());
    }

    #[test]
    fn bind_makes_a_private_folder_and_socket() {
        let base = short_dir("b1");
        let dir = base.join("ipc");
        let server = Server::bind(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(server.path()), 0o600);
        assert!(
            fs::symlink_metadata(server.path())
                .unwrap()
                .file_type()
                .is_socket()
        );
        assert_eq!(server.path(), dir.join(SOCKET_NAME));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_tightens_an_open_folder() {
        let base = short_dir("b2");
        let dir = base.join("ipc");
        DirBuilder::new().mode(0o755).create(&dir).unwrap();
        fs::set_permissions(&dir, Permissions::from_mode(0o755)).unwrap();
        Server::bind(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_refuses_a_folder_that_is_a_link() {
        let base = short_dir("b3");
        let real = base.join("real");
        fs::create_dir(&real).unwrap();
        let dir = base.join("ipc");
        symlink(&real, &dir).unwrap();
        assert!(Server::bind(&dir).is_err());
        assert!(!real.join(SOCKET_NAME).exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_refuses_a_link_at_the_socket_path_and_leaves_its_target() {
        let base = short_dir("b4");
        let dir = base.join("ipc");
        DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let target = base.join("target");
        fs::write(&target, "keep").unwrap();
        symlink(&target, dir.join(SOCKET_NAME)).unwrap();
        assert!(Server::bind(&dir).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
        assert!(
            fs::symlink_metadata(dir.join(SOCKET_NAME))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_refuses_a_regular_file_at_the_socket_path_and_leaves_it() {
        let base = short_dir("b5");
        let dir = base.join("ipc");
        DirBuilder::new().mode(0o700).create(&dir).unwrap();
        fs::write(dir.join(SOCKET_NAME), "keep").unwrap();
        assert!(Server::bind(&dir).is_err());
        assert_eq!(fs::read_to_string(dir.join(SOCKET_NAME)).unwrap(), "keep");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_replaces_a_stale_socket() {
        let base = short_dir("b6");
        let dir = base.join("ipc");
        drop(Server::bind(&dir).unwrap());
        assert!(
            dir.join(SOCKET_NAME).exists(),
            "the old socket file stays behind"
        );
        // A child another test forks holds a copy of the old listener until it execs, so the
        // old socket can answer for a moment after the drop.
        let server = (0..50)
            .find_map(|_| {
                let bound = Server::bind(&dir).ok();
                if bound.is_none() {
                    thread::sleep(Duration::from_millis(20));
                }
                bound
            })
            .unwrap();
        assert!(UnixStream::connect(server.path()).is_ok());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_leaves_a_live_socket_alone() {
        let base = short_dir("b9");
        let dir = base.join("ipc");
        let live = Server::bind(&dir).unwrap();
        let error = Server::bind(&dir).err().unwrap();
        assert_eq!(
            error,
            "another copy of the app is already serving the socket"
        );
        assert!(UnixStream::connect(live.path()).is_ok());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_refuses_a_path_over_100_bytes() {
        let base = short_dir("b7");
        let dir = base.join("d".repeat(100));
        let error = Server::bind(&dir).err().unwrap();
        assert_eq!(error, "the app's data folder path is too long for a socket");
        assert!(!dir.join(SOCKET_NAME).exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn bind_wants_the_folders_parent() {
        let base = short_dir("b8");
        assert!(Server::bind(&base.join("gone").join("ipc")).is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_right_token_is_welcomed_and_its_lines_arrive_with_its_floor() {
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        let session = Arc::new(Echo::default());
        let mut client = open(&registry, &session, AUTH_WAIT);
        client.write_all(hello(&token).as_bytes()).unwrap();
        client.write_all(b"one\n\ntwo\n").unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        assert_eq!(reply(&mut reader), "{\"ok\":true}\n");
        assert_eq!(reply(&mut reader), "echo:one\n");
        assert_eq!(reply(&mut reader), "echo:two\n");
        client.shutdown(std::net::Shutdown::Write).unwrap();
        assert!(at_end(&mut reader));
        assert_eq!(
            *crate::sync::lock(&session.seen),
            [
                ("p-1".to_string(), "one".to_string()),
                ("p-1".to_string(), "two".to_string())
            ]
        );
    }

    #[test]
    fn two_tokens_get_their_own_floors() {
        let registry = Arc::new(Registry::default());
        let one = registry.admit("p-1").unwrap();
        let two = registry.admit("p-2").unwrap();
        let session = Arc::new(Echo::default());
        for (token, line) in [(&one, "a"), (&two, "b")] {
            let mut client = open(&registry, &session, AUTH_WAIT);
            client
                .write_all(format!("{}{line}\n", hello(token)).as_bytes())
                .unwrap();
            let mut reader = BufReader::new(client);
            assert_eq!(reply(&mut reader), "{\"ok\":true}\n");
            assert_eq!(reply(&mut reader), format!("echo:{line}\n"));
        }
        assert_eq!(
            *crate::sync::lock(&session.seen),
            [
                ("p-1".to_string(), "a".to_string()),
                ("p-2".to_string(), "b".to_string())
            ]
        );
    }

    #[test]
    fn bad_first_lines_are_refused_and_closed() {
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        let started = Instant::now();
        refused(
            &registry,
            format!("{{\"token\":\"{}\"}}\n", "0".repeat(64)).as_bytes(),
        );
        assert!(started.elapsed() >= REFUSAL_PAUSE, "a refusal waits");
        refused(
            &registry,
            format!("{{\"token\":\"{}\",\"floor\":\"p-1\"}}\n", token.expose()).as_bytes(),
        );
        refused(&registry, b"not json\n");
        refused(&registry, b"\n");
        let long = format!(
            "{{\"token\":\"{}\",\"pad\":\"{}\"}}\n",
            token.expose(),
            " ".repeat(300)
        );
        refused(&registry, long.as_bytes());
        refused(&registry, format!("{}\n", " ".repeat(300)).as_bytes());
    }

    #[test]
    fn silence_is_closed_after_the_wait() {
        let registry = Arc::new(Registry::default());
        let session = Arc::new(Echo::default());
        let started = Instant::now();
        let mut client = BufReader::new(open(&registry, &session, Duration::from_millis(100)));
        assert_eq!(reply(&mut client), "{\"ok\":false}\n");
        assert!(at_end(&mut client));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_trickled_first_line_still_has_to_finish_in_time() {
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        let session = Arc::new(Echo::default());
        let mut client = open(&registry, &session, Duration::from_millis(300));
        let line = hello(&token);
        let (head, tail) = line.split_at(10);
        client.write_all(head.as_bytes()).unwrap();
        thread::sleep(Duration::from_millis(200));
        client.write_all(tail.get(..5).unwrap().as_bytes()).unwrap();
        thread::sleep(Duration::from_millis(200));
        let _ = client.write_all(tail.get(5..).unwrap().as_bytes());
        let mut reader = BufReader::new(client);
        assert_eq!(reply(&mut reader), "{\"ok\":false}\n");
    }

    #[test]
    fn a_revoked_token_loses_its_connection_at_the_next_line() {
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        let session = Arc::new(Echo::default());
        let mut client = open(&registry, &session, AUTH_WAIT);
        client
            .write_all(format!("{}one\n", hello(&token)).as_bytes())
            .unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        assert_eq!(reply(&mut reader), "{\"ok\":true}\n");
        assert_eq!(reply(&mut reader), "echo:one\n");
        registry.revoke("p-1");
        client.write_all(b"two\n").unwrap();
        assert!(at_end(&mut reader));
        assert_eq!(crate::sync::lock(&session.seen).len(), 1);
    }

    #[test]
    fn a_line_over_the_limit_or_not_utf8_closes_the_connection() {
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        let session = Arc::new(Echo::default());
        let exact = "x".repeat(LINE_MAX);
        for (bad, answered) in [
            (format!("{exact}\n").into_bytes(), 1),
            (format!("{exact}y\n").into_bytes(), 0),
            (b"\xff\xfe\n".to_vec(), 0),
        ] {
            let mut client = open(&registry, &session, AUTH_WAIT);
            client.write_all(hello(&token).as_bytes()).unwrap();
            let mut reader = BufReader::new(client.try_clone().unwrap());
            assert_eq!(reply(&mut reader), "{\"ok\":true}\n");
            let writer = thread::spawn(move || {
                let _ = client.write_all(&bad);
                client
            });
            let mut answers = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                answers += 1;
                if answers == answered {
                    let _ = writer.join().unwrap().shutdown(std::net::Shutdown::Write);
                    assert!(at_end(&mut reader));
                    break;
                }
            }
            assert_eq!(answers, answered);
        }
    }

    #[test]
    fn the_ninth_connection_is_closed_and_a_place_frees_up() {
        let base = short_dir("s9");
        let server = Server::bind(&base.join("ipc")).unwrap();
        let path = server.path().to_path_buf();
        let registry = Arc::new(Registry::default());
        let token = registry.admit("p-1").unwrap();
        server
            .serve(Arc::clone(&registry), Arc::new(Echo::default()))
            .unwrap();

        let welcomed = |token: &Secret| -> Option<UnixStream> {
            // macOS answers EINVAL to a socket option once the app has closed the stream.
            let mut client = UnixStream::connect(&path).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(10)))
                .ok()?;
            let _ = client.write_all(hello(token).as_bytes());
            let mut reader = BufReader::new(client.try_clone().ok()?);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            (line == "{\"ok\":true}\n").then_some(client)
        };
        let mut eight: Vec<UnixStream> = (0..MAX_CONNECTIONS)
            .map(|_| welcomed(&token).unwrap())
            .collect();

        let mut ninth = UnixStream::connect(&path).unwrap();
        // EINVAL here means the app has closed it already, which is what is wanted.
        let _ = ninth.set_read_timeout(Some(Duration::from_secs(10)));
        let mut rest = Vec::new();
        assert_eq!(
            ninth.read_to_end(&mut rest).unwrap(),
            0,
            "closed at once, unanswered"
        );

        drop(eight.pop());
        let mut again = None;
        for _ in 0..100 {
            again = welcomed(&token);
            if again.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(again.is_some(), "a place is free again");
        drop(eight);
        fs::remove_dir_all(&base).unwrap();
    }
}

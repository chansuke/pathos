//! The Unix socket on which the window answers agents (ADR 35).

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::protocol::{Request, Response};

/// The longest request line, in bytes.
pub const MAX_REQUEST: u64 = 1 << 20;

/// Where the window listens: `$PATHOS_SOCKET`, or `pathos/agent.sock` in
/// `$XDG_RUNTIME_DIR` or else in the temporary directory. `env` looks up a
/// variable; the window and `pathos-mcp` pass a lookup of their own environment.
pub fn socket_path(env: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    if let Some(path) = env("PATHOS_SOCKET") {
        return PathBuf::from(path);
    }
    let directory = env("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    directory.join("pathos").join("agent.sock")
}

#[derive(Debug)]
pub enum BindError {
    /// Another window answers on the socket.
    InUse,
    /// The directory of the socket can be read by other users.
    NotPrivate(PathBuf),
    Io(io::Error),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::InUse => f.write_str("another window answers agents"),
            BindError::NotPrivate(directory) => {
                write!(f, "{} is open to other users", directory.display())
            }
            BindError::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for BindError {}

impl From<io::Error> for BindError {
    fn from(error: io::Error) -> Self {
        BindError::Io(error)
    }
}

/// A request waiting for the window's response.
#[derive(Debug)]
pub struct Call {
    pub request: Request,
    reply: mpsc::Sender<Response>,
}

impl Call {
    pub fn reply(self, response: Response) {
        // The agent may have gone, and then nobody waits for the response.
        let _ = self.reply.send(response);
    }
}

/// Accepts connections on a thread and hands their requests over in order.
#[derive(Debug)]
pub struct Listener {
    path: PathBuf,
    calls: UnboundedReceiver<Call>,
}

impl Listener {
    /// Listens on `path`, creating its directory readable only by the user.
    /// A socket left by a window that has gone is replaced.
    pub fn bind(path: &Path) -> Result<Listener, BindError> {
        if let Some(directory) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            match fs::DirBuilder::new().mode(0o700).create(directory) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let metadata = fs::symlink_metadata(directory)?;
                    if !directory_is_private(&metadata, current_uid()) {
                        return Err(BindError::NotPrivate(directory.to_path_buf()));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        if fs::symlink_metadata(path).is_ok() {
            if UnixStream::connect(path).is_ok() {
                return Err(BindError::InUse);
            }
            fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        let (sender, calls) = unbounded();
        thread::Builder::new()
            .name("pathos-agents".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let sender = sender.clone();
                    if sender.is_closed() {
                        break;
                    }
                    let _ = thread::Builder::new()
                        .name("pathos-agent".into())
                        .spawn(move || serve(stream, &sender));
                }
            })?;
        Ok(Listener {
            path: path.to_path_buf(),
            calls,
        })
    }

    /// The next request, in the order they arrive.
    pub async fn next(&mut self) -> Option<Call> {
        self.calls.next().await
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Whether an existing socket directory is a real directory owned by `owner`
/// with no group or other permissions (ADR 35).
fn directory_is_private(metadata: &fs::Metadata, owner: u32) -> bool {
    metadata.is_dir() && metadata.permissions().mode() & 0o077 == 0 && metadata.uid() == owner
}

/// The effective user id of this process, to verify that an existing socket
/// directory belongs to the same user (ADR 35).
fn current_uid() -> u32 {
    // SAFETY: POSIX `geteuid` takes no arguments, cannot fail, and returns
    // `uid_t` (`u32` on Unix platforms matching `MetadataExt::uid`).
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: `geteuid` has no preconditions and never fails.
    unsafe { geteuid() }
}

/// Answers the requests of one connection, a line at a time.
fn serve(stream: UnixStream, calls: &UnboundedSender<Call>) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    loop {
        line.clear();
        let read = (&mut reader).take(MAX_REQUEST + 1).read_line(&mut line)?;
        if read == 0 {
            return Ok(());
        }
        if read as u64 > MAX_REQUEST {
            write_line(&mut writer, &Response::error("the request is too long"))?;
            return Ok(());
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Err(error) => Response::error(format!("not a request: {error}")),
            Ok(request) => {
                let (reply, response) = mpsc::channel();
                if calls.unbounded_send(Call { request, reply }).is_err() {
                    return Ok(());
                }
                match response.recv() {
                    Ok(response) => response,
                    Err(_) => return Ok(()),
                }
            }
        };
        write_line(&mut writer, &response)?;
    }
}

fn write_line(writer: &mut impl Write, response: &Response) -> io::Result<()> {
    let mut line = serde_json::to_string(response).map_err(io::Error::other)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()
}

/// Sends `request` to the window listening on `path` and waits for its
/// response.
pub fn request(path: &Path, request: &Request) -> io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    let mut line = serde_json::to_string(request).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response)?;
    if response.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the window closed the connection",
        ));
    }
    serde_json::from_str(&response).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::Proposals;
    use crate::protocol::Status;
    use pathos_core::Document;

    #[test]
    fn the_socket_path_follows_the_given_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert_eq!(
            socket_path(env(&[
                ("PATHOS_SOCKET", "/a/b.sock"),
                ("XDG_RUNTIME_DIR", "/run")
            ])),
            PathBuf::from("/a/b.sock")
        );
        assert_eq!(
            socket_path(env(&[("XDG_RUNTIME_DIR", "/run/user/1")])),
            PathBuf::from("/run/user/1/pathos/agent.sock")
        );
        assert_eq!(
            socket_path(env(&[])),
            std::env::temp_dir().join("pathos").join("agent.sock")
        );
    }

    /// A fresh directory for one test. Its socket goes in `pathos` inside
    /// it, which [`Listener::bind`] creates.
    fn directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pathos-agent-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// Answers the calls on `listener` on a thread, as the window does.
    fn answer(mut listener: Listener, mut document: Document) -> thread::JoinHandle<Document> {
        thread::spawn(move || {
            let mut proposals = Proposals::default();
            futures::executor::block_on(async {
                while let Some(call) = listener.next().await {
                    let stop = call.request == Request::Proposal { id: u64::MAX };
                    let response = proposals.handle(&document, call.request.clone());
                    call.reply(response);
                    if proposals.pending(&document) > 0 {
                        proposals.apply(&mut document).unwrap();
                    }
                    if stop {
                        break;
                    }
                }
            });
            document
        })
    }

    #[test]
    fn an_agent_reads_the_project_and_proposes_through_the_socket() {
        let path = directory("socket-round-trip")
            .join("pathos")
            .join("agent.sock");
        let listener = Listener::bind(&path).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&path), 0o600);
        assert!(matches!(Listener::bind(&path), Err(BindError::InUse)));

        let window = answer(listener, Document::default());
        let Response::Project(state) = request(&path, &Request::Project).unwrap() else {
            panic!("not a project");
        };
        assert_eq!(state.edits, 0);
        let proposed = request(
            &path,
            &Request::Propose {
                summary: "a track".into(),
                commands: vec!["add-track".into()],
                anchor: None,
            },
        )
        .unwrap();
        assert_eq!(proposed, Response::Proposed { id: 0 });
        assert_eq!(
            request(&path, &Request::Proposal { id: 0 }).unwrap(),
            Response::Proposal {
                id: 0,
                status: Status::Applied
            }
        );
        // A line that is not a request is answered without the window.
        let mut stream = UnixStream::connect(&path).unwrap();
        stream.write_all(b"{\"request\":\"nothing\"}\n").unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        assert!(line.starts_with(r#"{"response":"error","message":"not a request"#));

        request(&path, &Request::Proposal { id: u64::MAX }).unwrap();
        let document = window.join().unwrap();
        assert_eq!(document.save(), "add-track\n");
    }

    #[test]
    fn a_socket_left_behind_is_replaced_and_removed_on_drop() {
        let private = directory("socket-left-behind").join("pathos");
        fs::DirBuilder::new().mode(0o700).create(&private).unwrap();
        let path = private.join("agent.sock");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        let listener = Listener::bind(&path).unwrap();
        drop(listener);
        assert!(!path.exists());
    }

    #[test]
    fn a_directory_open_to_others_is_refused() {
        let open = directory("socket-open-directory").join("pathos");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            Listener::bind(&open.join("agent.sock")),
            Err(BindError::NotPrivate(_))
        ));
    }

    #[test]
    fn a_request_with_no_window_fails() {
        let path = directory("socket-no-window")
            .join("pathos")
            .join("agent.sock");
        assert!(request(&path, &Request::Project).is_err());
    }

    #[test]
    fn directory_privacy_checks_type_permissions_and_owner() {
        let base = directory("socket-privacy-predicate");
        let private = base.join("private");
        fs::DirBuilder::new().mode(0o700).create(&private).unwrap();
        let meta = fs::symlink_metadata(&private).unwrap();
        let uid = current_uid();
        assert_eq!(meta.uid(), uid);
        assert!(directory_is_private(&meta, uid));
        assert!(!directory_is_private(&meta, uid.wrapping_add(1)));

        let open = base.join("open");
        fs::DirBuilder::new().mode(0o755).create(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        let open_meta = fs::symlink_metadata(&open).unwrap();
        assert!(!directory_is_private(&open_meta, uid));

        let link = base.join("link");
        std::os::unix::fs::symlink(&private, &link).unwrap();
        let link_meta = fs::symlink_metadata(&link).unwrap();
        assert!(!directory_is_private(&link_meta, uid));
        assert!(matches!(
            Listener::bind(&link.join("agent.sock")),
            Err(BindError::NotPrivate(_))
        ));
    }
}

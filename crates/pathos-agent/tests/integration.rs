//! End-to-end integration tests wiring `mcp::serve`, the Unix socket
//! (`request` and `Listener`), `Proposals::handle`, and `pathos_measure`.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use pathos_agent::mcp;
use pathos_agent::{
    BindError, Listener, MAX_REQUEST, Proposals, Request, Response, Status, request,
};
use pathos_core::{Document, Edit};
use serde_json::{Value, json};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

/// A temporary directory for one test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pathos-agent-it-{}-{id}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test temp directory");
        TempDir(dir)
    }

    fn socket(&self) -> PathBuf {
        self.0.join("pathos").join("agent.sock")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn document(lines: &[&str]) -> Document {
    let mut doc = Document::default();
    for line in lines {
        doc.edit(line.parse::<Edit>().expect("valid test edit"))
            .expect("accepted test edit");
    }
    doc
}

/// Runs a window-like server loop answering `Request`s through `Proposals`
/// until the sentinel `Request::Proposal { id: u64::MAX }` arrives.
fn spawn_window(mut listener: Listener, document: Document) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut proposals = Proposals::default();
        futures::executor::block_on(async {
            while let Some(call) = listener.next().await {
                let stop = call.request == Request::Proposal { id: u64::MAX };
                let response = proposals.handle(&document, call.request.clone());
                call.reply(response);
                if stop {
                    break;
                }
            }
        });
    })
}

fn run_mcp(socket: &std::path::Path, messages: &[Value]) -> Vec<Value> {
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
    let mut output = Vec::new();
    mcp::serve(input.as_bytes(), &mut output, |req| request(socket, req))
        .expect("mcp::serve succeeds");
    String::from_utf8(output)
        .expect("mcp output is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("mcp output line is JSON"))
        .collect()
}

#[test]
fn mcp_round_trip_over_unix_socket_including_measurements() {
    let temp = TempDir::new("mcp-round-trip");
    let path = temp.socket();
    let listener = Listener::bind(&path).unwrap();
    let doc = document(&["add-track", "add-clip 0 0 3840", "add-note 0 0 3840 69 100"]);
    let window = spawn_window(listener, doc);

    let replies = run_mcp(
        &path,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "2025-06-18"}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                   "params": {"name": "get_project", "arguments": {}}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                   "params": {"name": "get_measurements", "arguments": {}}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                   "params": {"name": "propose_commands",
                              "arguments": {"summary": "lower track",
                                            "commands": ["set-track-volume 0 500"]}}}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                   "params": {"name": "get_proposal", "arguments": {"id": 0}}}),
            json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                   "params": {"name": "report_finding",
                              "arguments": {"summary": "loud bar", "track": 0,
                                            "start": 0, "end": 3840}}}),
        ],
    );
    assert_eq!(replies.len(), 6);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");

    assert_eq!(replies[1]["result"]["isError"], false);
    let project_text = replies[1]["result"]["content"][0]["text"].as_str().unwrap();
    let Response::Project(state) = serde_json::from_str(project_text).unwrap() else {
        panic!("expected Project response");
    };
    assert_eq!(state.tracks.len(), 1);
    assert_eq!(state.clips.len(), 1);

    assert_eq!(replies[2]["result"]["isError"], false);
    let report: pathos_measure::Report =
        serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report.seconds, 2.0);
    assert_eq!(report.tracks.len(), 1);
    assert!(report.master.loudness.is_some());

    assert_eq!(replies[3]["result"]["isError"], false);
    let proposed: Response =
        serde_json::from_str(replies[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(proposed, Response::Proposed { id: 0 });

    assert_eq!(replies[4]["result"]["isError"], false);
    let status: Response =
        serde_json::from_str(replies[4]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        status,
        Response::Proposal {
            id: 0,
            status: Status::Pending
        }
    );

    assert_eq!(replies[5]["result"]["isError"], false);
    let finding: Response =
        serde_json::from_str(replies[5]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(finding, Response::Proposed { id: 1 });

    request(&path, &Request::Proposal { id: u64::MAX }).unwrap();
    window.join().unwrap();
}

#[test]
fn mcp_measurements_over_socket_reports_too_long_and_invalid_file() {
    // TooLong through Proposals::handle -> Request::File -> pathos_measure::measure
    let temp = TempDir::new("mcp-too-long");
    let path = temp.socket();
    let listener = Listener::bind(&path).unwrap();
    // 120 BPM: 1_152_960 ticks is 600.5 seconds (> 600s limit).
    let long_doc = document(&["add-track", "add-clip 0 0 1152960"]);
    let window = spawn_window(listener, long_doc);

    let call = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "get_measurements", "arguments": {}}
    });
    let replies = run_mcp(&path, std::slice::from_ref(&call));
    assert_eq!(replies[0]["result"]["isError"], true);
    let message = replies[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("more than the 600"), "{message}");

    request(&path, &Request::Proposal { id: u64::MAX }).unwrap();
    window.join().unwrap();

    // Invalid file payload sent over the socket.
    let bad_temp = TempDir::new("mcp-invalid-file");
    let bad_path = bad_temp.socket();
    let mut bad_listener = Listener::bind(&bad_path).unwrap();
    let server = thread::spawn(move || {
        futures::executor::block_on(async {
            if let Some(call) = bad_listener.next().await {
                call.reply(Response::File {
                    text: "not-a-valid-command\n".into(),
                });
            }
        });
    });
    let replies = run_mcp(&bad_path, &[call]);
    assert_eq!(replies[0]["result"]["isError"], true);
    let message = replies[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        message,
        "the project file does not load: line 1: not an edit"
    );
    server.join().unwrap();
}

#[test]
fn oversized_request_exceeding_max_request_is_rejected() {
    let temp = TempDir::new("max-request");
    let path = temp.socket();
    let listener = Listener::bind(&path).unwrap();
    let window = spawn_window(listener, Document::default());

    let mut stream = UnixStream::connect(&path).unwrap();
    let mut oversized = vec![b'a'; MAX_REQUEST as usize + 16];
    oversized.push(b'\n');
    stream.write_all(&oversized).unwrap();
    stream.flush().unwrap();

    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).unwrap();
    let response: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(response, Response::error("the request is too long"));

    // Subsequent normal requests still succeed.
    assert!(matches!(
        request(&path, &Request::Project).unwrap(),
        Response::Project(_)
    ));

    request(&path, &Request::Proposal { id: u64::MAX }).unwrap();
    window.join().unwrap();
}

#[test]
fn multiple_sequential_requests_work_on_one_socket_connection() {
    let temp = TempDir::new("multi-request");
    let path = temp.socket();
    let listener = Listener::bind(&path).unwrap();
    let window = spawn_window(listener, Document::default());

    let mut stream = UnixStream::connect(&path).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut send = |req: &Request| -> Response {
        let mut line = serde_json::to_string(req).unwrap();
        line.push('\n');
        stream.write_all(line.as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap()
    };

    assert!(matches!(send(&Request::Project), Response::Project(_)));
    assert_eq!(
        send(&Request::Propose {
            summary: "add track".into(),
            commands: vec!["add-track".into()],
            anchor: None,
        }),
        Response::Proposed { id: 0 }
    );
    assert_eq!(
        send(&Request::Proposal { id: 0 }),
        Response::Proposal {
            id: 0,
            status: Status::Pending
        }
    );
    assert_eq!(
        send(&Request::Proposal { id: u64::MAX }),
        Response::error(format!("no proposal {}", u64::MAX))
    );
    window.join().unwrap();
}

#[test]
fn client_disconnect_and_closed_listener_channel_are_handled_cleanly() {
    let temp = TempDir::new("disconnect-and-close");
    let path = temp.socket();
    let listener = Listener::bind(&path).unwrap();
    let window = spawn_window(listener, Document::default());

    // Client connects and disconnects immediately without sending anything.
    drop(UnixStream::connect(&path).unwrap());

    // Client sends a request and disconnects before reading the reply.
    {
        let mut stream = UnixStream::connect(&path).unwrap();
        stream.write_all(b"{\"request\":\"project\"}\n").unwrap();
        stream.flush().unwrap();
    }

    // Server still handles subsequent requests normally.
    assert!(matches!(
        request(&path, &Request::Project).unwrap(),
        Response::Project(_)
    ));
    request(&path, &Request::Proposal { id: u64::MAX }).unwrap();
    window.join().unwrap();

    // Dropping a Call without replying closes the connection cleanly (UnexpectedEof).
    let drop_temp = TempDir::new("dropped-call");
    let drop_path = drop_temp.socket();
    let mut drop_listener = Listener::bind(&drop_path).unwrap();
    let dropper = thread::spawn(move || {
        futures::executor::block_on(async {
            if let Some(call) = drop_listener.next().await {
                drop(call);
            }
        });
    });
    let err = request(&drop_path, &Request::Project).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    dropper.join().unwrap();

    // Dropping the Listener closes the unbounded channel; an already-open
    // connection receives EOF on its next request instead of hanging.
    let closed_temp = TempDir::new("closed-channel");
    let closed_path = closed_temp.socket();
    let mut closed_listener = Listener::bind(&closed_path).unwrap();
    let mut stream = UnixStream::connect(&closed_path).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    // Synchronize on one round-trip so the per-connection thread is active.
    let handle = thread::spawn(move || {
        futures::executor::block_on(async {
            let call = closed_listener.next().await.unwrap();
            call.reply(Response::File {
                text: String::new(),
            });
            drop(closed_listener);
        });
    });
    stream.write_all(b"{\"request\":\"file\"}\n").unwrap();
    stream.flush().unwrap();
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert_eq!(
        serde_json::from_str::<Response>(&first).unwrap(),
        Response::File {
            text: String::new()
        }
    );
    handle.join().unwrap();

    // Now the listener's channel is closed; sending another line sees EOF.
    let _ = stream.write_all(b"{\"request\":\"project\"}\n");
    let _ = stream.flush();
    let mut second = String::new();
    let bytes = reader.read_line(&mut second).unwrap();
    assert_eq!(bytes, 0);
}

#[test]
fn symlink_parent_directory_is_refused() {
    let temp = TempDir::new("symlink-parent");
    let real = temp.0.join("real");
    fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
    let link = temp.0.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert!(matches!(
        Listener::bind(&link.join("agent.sock")),
        Err(BindError::NotPrivate(_))
    ));
}

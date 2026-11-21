//! An MCP server over standard input and output that relays each tool call to
//! the window as a request on its socket (ADR 35).

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::protocol::{Request, Response};

/// The versions of the protocol this server speaks, the latest last.
const VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

const COMMANDS: &str = "Each command is a line of a pathos project file, words \
separated by spaces and numbers in decimal. Ticks count 960 to a quarter note; \
tracks count from 0; a clip's notes start relative to the clip. The commands \
are: add-track; add-clip TRACK START LENGTH (the clip gets next_clip_id); \
move-clip ID TRACK START; split-clip ID TICK (the right part gets \
next_clip_id); delete-clip ID; set-tempo TICK US_PER_QUARTER; remove-tempo \
TICK; add-note ID START LENGTH PITCH VELOCITY (pitch 0-127, velocity 1-127, \
ending within the clip, not overlapping a note of the same pitch); remove-note \
ID START PITCH; set-track-volume TRACK VOLUME (thousandths of a gain of 1, up \
to 2000); mute-track TRACK; unmute-track TRACK; solo-track TRACK; \
unsolo-track TRACK; set-track-pan TRACK PAN (0 left, 100 center, 200 right); \
set-master-volume VOLUME. Clips on a track must not overlap and nothing may \
lie beyond tick 2^40.";

fn tools() -> Value {
    json!([
        {
            "name": "get_project",
            "description": "Read the project open in pathos: its tracks, master \
                volume, tempo map, clips with their notes, the id the next clip \
                receives, and the number of edits so far.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "propose_commands",
            "description": format!(
                "Propose commands to the project open in pathos. The window shows \
                the summary and what the commands change, and the user applies \
                or dismisses them; nothing changes until then. The proposal is \
                refused if a command is rejected, and it goes stale if the \
                project changes before the user applies it. Returns the \
                proposal's id. {COMMANDS}"
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "What the commands do and why, for the user."
                    },
                    "commands": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "The commands, applied in order."
                    }
                },
                "required": ["summary", "commands"]
            }
        },
        {
            "name": "get_proposal",
            "description": "Tell whether a proposal is pending, applied, \
                dismissed, or stale because the project changed first.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "integer", "minimum": 0}},
                "required": ["id"]
            },
            "annotations": {"readOnlyHint": true}
        }
    ])
}

/// Serves MCP messages from `input`, one JSON-RPC message per line, until it
/// ends. `call` sends a request to the window.
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
    mut call: impl FnMut(&Request) -> io::Result<Response>,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = answer(&line, &mut call) {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The reply to one message, or none for a notification or a reply.
fn answer(line: &str, call: &mut impl FnMut(&Request) -> io::Result<Response>) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(failure(Value::Null, -32700, &error.to_string())),
    };
    let id = message.get("id").cloned()?;
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to a request of ours; this server sends none.
        return None;
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|asked| VERSIONS.contains(asked))
                .unwrap_or(VERSIONS[VERSIONS.len() - 1]);
            success(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "pathos", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "pathos is a DAW. Read the open project with \
                        get_project before proposing commands, and propose changes \
                        with propose_commands; the user applies them in the window."
                }),
            )
        }
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({"tools": tools()})),
        "tools/call" => match tool_request(&params) {
            Err(message) => failure(id, -32602, &message),
            Ok(request) => success(id, tool_result(call(&request))),
        },
        _ => failure(id, -32601, &format!("no method {method}")),
    })
}

/// The request a tool call makes.
fn tool_request(params: &Value) -> Result<Request, String> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let argument = |key: &str| {
        arguments
            .get(key)
            .ok_or_else(|| format!("{name} needs {key}"))
    };
    match name {
        "get_project" => Ok(Request::Project),
        "propose_commands" => {
            let summary = argument("summary")?
                .as_str()
                .ok_or("summary must be a string")?
                .to_string();
            let commands = argument("commands")?
                .as_array()
                .and_then(|commands| {
                    commands
                        .iter()
                        .map(|command| command.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or("commands must be a list of strings")?;
            Ok(Request::Propose { summary, commands })
        }
        "get_proposal" => Ok(Request::Proposal {
            id: argument("id")?
                .as_u64()
                .ok_or("id must be a whole number")?,
        }),
        _ => Err(format!("no tool {name:?}")),
    }
}

/// The result of a tool call: the window's response as JSON text, an error
/// when the window refused the request or cannot be reached.
fn tool_result(response: io::Result<Response>) -> Value {
    let (text, error) = match response {
        Err(error) => (
            format!("pathos cannot be reached; is its window open? ({error})"),
            true,
        ),
        Ok(Response::Error { message }) => (message, true),
        Ok(response) => (
            serde_json::to_string_pretty(&response).unwrap_or_default(),
            false,
        ),
    };
    json!({"content": [{"type": "text", "text": text}], "isError": error})
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn failure(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Status;

    /// The replies to `messages` from a server whose window answers with
    /// `respond`, and the requests it received.
    fn exchange(
        messages: &[Value],
        mut respond: impl FnMut(&Request) -> io::Result<Response>,
    ) -> (Vec<Value>, Vec<Request>) {
        let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
        let mut output = Vec::new();
        let mut requests = Vec::new();
        serve(input.as_bytes(), &mut output, |request| {
            requests.push(request.clone());
            respond(request)
        })
        .unwrap();
        let replies = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (replies, requests)
    }

    #[test]
    fn it_initializes_and_lists_its_tools() {
        let (replies, requests) = exchange(
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                       "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                                  "clientInfo": {"name": "test", "version": "0"}}}),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                json!({"jsonrpc": "2.0", "id": "three", "method": "ping"}),
                json!({"jsonrpc": "2.0", "id": 4, "method": "initialize",
                       "params": {"protocolVersion": "1999-01-01"}}),
            ],
            |_| unreachable!(),
        );
        assert!(requests.is_empty());
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(replies[0]["result"]["capabilities"], json!({"tools": {}}));
        let names: Vec<&str> = replies[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["get_project", "propose_commands", "get_proposal"]);
        assert_eq!(
            replies[2],
            json!({"jsonrpc": "2.0", "id": "three", "result": {}})
        );
        assert_eq!(replies[3]["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn a_tool_call_is_a_request_to_the_window() {
        let (replies, requests) = exchange(
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": "propose_commands",
                                  "arguments": {"summary": "a track",
                                                "commands": ["add-track"]}}}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                       "params": {"name": "get_proposal", "arguments": {"id": 0}}}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                       "params": {"name": "get_proposal", "arguments": {"id": 7}}}),
            ],
            |request| {
                Ok(match request {
                    Request::Propose { .. } => Response::Proposed { id: 0 },
                    Request::Proposal { id: 0 } => Response::Proposal {
                        id: 0,
                        status: Status::Pending,
                    },
                    _ => Response::error("no proposal 7"),
                })
            },
        );
        assert_eq!(
            requests,
            [
                Request::Propose {
                    summary: "a track".into(),
                    commands: vec!["add-track".into()]
                },
                Request::Proposal { id: 0 },
                Request::Proposal { id: 7 },
            ]
        );
        let text = |reply: &Value| {
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let proposed: Response = serde_json::from_str(&text(&replies[0])).unwrap();
        assert_eq!(proposed, Response::Proposed { id: 0 });
        assert_eq!(replies[0]["result"]["isError"], false);
        assert!(text(&replies[1]).contains("\"pending\""));
        assert_eq!(text(&replies[2]), "no proposal 7");
        assert_eq!(replies[2]["result"]["isError"], true);
    }

    #[test]
    fn bad_calls_are_errors_and_an_absent_window_is_a_failed_tool() {
        let (replies, requests) = exchange(
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": "get_project"}}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                       "params": {"name": "propose_commands",
                                  "arguments": {"summary": "x", "commands": [1]}}}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                       "params": {"name": "delete_everything"}}),
                json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list"}),
            ],
            |_| Err(io::Error::from(io::ErrorKind::NotFound)),
        );
        assert_eq!(requests, [Request::Project]);
        assert_eq!(replies[0]["result"]["isError"], true);
        assert!(
            replies[0]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("pathos cannot be reached")
        );
        assert_eq!(replies[1]["error"]["code"], -32602);
        assert_eq!(
            replies[1]["error"]["message"],
            "commands must be a list of strings"
        );
        assert_eq!(replies[2]["error"]["code"], -32602);
        assert_eq!(replies[3]["error"]["code"], -32601);

        let mut output = Vec::new();
        serve("not json\n".as_bytes(), &mut output, |_| unreachable!()).unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            (reply["id"].clone(), reply["error"]["code"].clone()),
            (Value::Null, json!(-32700))
        );
    }
}

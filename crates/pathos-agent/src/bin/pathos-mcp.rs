//! An MCP server over standard input and output for the pathos window that is
//! open (ADR 35). Register it with an MCP client, such as
//! `claude mcp add pathos -- pathos-mcp`.

use std::io;
use std::process::ExitCode;

#[expect(
    clippy::disallowed_methods,
    reason = "a bin reads the environment and passes it in"
)]
fn main() -> ExitCode {
    let path = pathos_agent::socket_path(|name| std::env::var_os(name));
    let served = pathos_agent::mcp::serve(io::stdin().lock(), io::stdout().lock(), |request| {
        pathos_agent::request(&path, request)
    });
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pathos-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}

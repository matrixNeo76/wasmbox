//! `wasmbox-mcp` — server MCP stdio persistente sopra `wasmbox-core`
//! (docs/agent-loop-plan.md, piano agent-loop v0.9 approvato).
//!
//! Contratto (stdout = protocollo, SEMPRE; log su stderr):
//! - `initialize` → serverInfo + capabilities {tools};
//! - `notifications/initialized` (notifica) → nessuna risposta;
//! - `tools/list` → esattamente UN tool: `wasmbox_run`;
//! - `tools/call` `wasmbox_run` → envelope JSON capato (envelope.rs);
//! - metodi/requests sconosciuti → error JSON-RPC `method_not_found`
//!   (mai panic, mai stdout extra).
//!
//! Il dispatch vive in `server.rs` (pubblico dalla lib, quindi testabile
//! a livello contratto): questo binario è solo il loop stdio.

use std::io::{BufRead, Write};

use wasmbox_mcp::protocol::PROTOCOL_VERSION;
use wasmbox_mcp::server;

const VERSION: &str = "0.1.0";

fn main() -> std::process::ExitCode {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = std::io::stdout();
    let mut runner = wasmbox_mcp::runner::Runner::new();

    eprintln!("[wasmbox-mcp] v{VERSION} (MCP {PROTOCOL_VERSION} stdio) pronto");

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF: pipe chiusa dal client → exit pulito
            Ok(_) => {}
            Err(e) => {
                eprintln!("[wasmbox-mcp] lettura stdin fallita: {e}");
                return std::process::ExitCode::from(1);
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(resp) = server::handle(trimmed, &mut runner) {
            if writeln!(stdout, "{resp}").is_err() {
                break; // stdout chiuso: il client è andato via
            }
            let _ = stdout.flush();
        }
    }
    std::process::ExitCode::from(0)
}

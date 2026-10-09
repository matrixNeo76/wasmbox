//! CLI di prova per `wasmbox-core`: esegue un guest .wasm su input da argv o stdin.
//!
//! Comportamento pensato per **agenti AI e script**:
//! - exit code deterministici (vedi `ExitCode` sotto);
//! - `--json` ⇒ esito machine-readable su stdout (una riga JSON, niente altro
//!   su stdout; i log vanno su stderr);
//! - senza `--json` ⇒ output del guest su stdout, messaggi di errore su stderr.
//!
//! La CLI NON aggiunge protocolli né logica di dominio (i divieti del blueprint
//! restano validi): il `Handler` implementato qui è l'eco più banale possibile,
//! come `examples/host-run` — è la dimostrazione che la sandbox gira in un
//! processo separato con una CLI e con exit code prevedibili.
//!
//! Uso:
//! ```sh
//! wasmbox-cli run guest.wasm "ciao"        # input da argv
//! echo -n "ciao" | wasmbox-cli run guest.wasm       # input da stdin
//! wasmbox-cli run guest.wasm "ciao" --json # output JSON per agenti
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

/// Handler della CLI: eco, con log su stderr (mai stdout, che è per l'output).
struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        let req = String::from_utf8_lossy(request);
        eprintln!("[ask] richieste → {req}");
        Ok(req.to_uppercase().into_bytes())
    }
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    args.retain(|a| a != "--json");

    match run(args) {
        Ok(Ok(output)) => {
            if json {
                let out = String::from_utf8_lossy(&output);
                println!("{{\"ok\":true,\"output\":\"{}\"}}", escape_json(&out));
            } else if let Err(e) = std::io::stdout().write_all(&output) {
                return fail(json, &format!("scrittura output fallita: {e}"));
            }
            ExitCode::from(0)
        }
        Ok(Err(e)) => sandbox_failure(json, &e),
        Err(msg) => fail(json, &msg), // errore d'uso o di I/O locale
    }
}

/// Parsifica gli argomenti restanti: [run] <guest.wasm> [input]
/// Se non c'è input da argv e stdin non è un TTY, legge stdin.
fn run(args: Vec<String>) -> Result<Result<Vec<u8>, SandboxError>, String> {
    let mut it = args.into_iter().fuse();
    match it.next().as_deref() {
        Some("run") => {}
        Some(other) => {
            return Err(format!(
                "comando sconosciuto: {other} (comandi disponibili: run)"
            ))
        }
        None => return Err("nessun comando (uso: wasmbox-cli run <guest.wasm> [input])".into()),
    }

    let wasm_path: PathBuf = it
        .next()
        .ok_or("manca <guest.wasm> (uso: wasmbox-cli run <guest.wasm> [input])")?
        .into();

    let input_bytes: Vec<u8> = match it.next() {
        Some(s) => s.into_bytes(),
        None if !std::io::IsTerminal::is_terminal(&std::io::stdin()) => {
            // input da pipe/stdin (per agenti AI e script)
            use std::io::Read;
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("lettura stdin fallita: {e}"))?;
            buf
        }
        None => Vec::new(),
    };

    let wasm = std::fs::read(&wasm_path)
        .map_err(|e| format!("impossibile leggere {}: {e}", wasm_path.display()))?;
    eprintln!("[load] guest {} ({} byte)", wasm_path.display(), wasm.len());

    // Configuro la sandbox: cache in una subdir temporanea specifica della CLI.
    let config = SandboxConfig {
        cache_dir: Some(std::env::temp_dir().join("wasmbox-cli-cache")),
        ..Default::default()
    };

    let engine =
        SandboxEngine::new(&wasm, config).map_err(|e| format!("iniezione sandbox fallita: {e}"))?;
    eprintln!("[run] input {} byte", input_bytes.len());

    let mut handler = EchoHandler;
    Ok(engine.run(&input_bytes, &mut handler))
}

/// Mappa un `SandboxError` a exit code deterministici + messaggio su stderr
/// (o JSON `\{\"ok\":false\}` su stdout se `--json`).
fn sandbox_failure(json: bool, e: &SandboxError) -> ExitCode {
    use wasmbox_core::SandboxError as S;
    let (code, kind) = match e {
        S::FuelExhausted => (ExitCode::from(3), "fuel_exhausted"),
        S::Timeout => (ExitCode::from(4), "timeout"),
        S::GuestOutOfMemory => (ExitCode::from(5), "guest_out_of_memory"),
        S::AskLimitExceeded(_) | S::PayloadTooLarge { .. } => {
            (ExitCode::from(6), "ask_limit_or_payload")
        }
        S::Host(_) => (ExitCode::from(7), "host_handler_error"),
        S::MissingExport(_) => (ExitCode::from(8), "missing_export"),
        S::InvalidWasm(_) => (ExitCode::from(9), "invalid_wasm"),
        S::EngineInit(_) | S::MemoryAccess(_) | S::Execution(_) => {
            (ExitCode::from(10), "execution_or_engine_error")
        }
    };
    let msg = e.to_string();
    if json {
        println!(
            "{{\"ok\":false,\"error\":\"{kind}\",\"message\":\"{}\"}}",
            escape_json(&msg)
        );
    } else {
        eprintln!("errore [{kind}]: {msg}");
    }
    code
}

/// Errore di uso/I-O: exit 2, messaggio stderr (o JSON `ok:false`).
fn fail(json: bool, msg: &str) -> ExitCode {
    if json {
        println!(
            "{{\"ok\":false,\"error\":\"usage_or_io\",\"message\":\"{}\"}}",
            escape_json(msg)
        );
    } else {
        eprintln!("errore d'uso: {msg}");
    }
    ExitCode::from(2)
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

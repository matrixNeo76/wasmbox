//! Host dello scenario B — un `HostHandler` con protocollo "tool" reale
//! sopra `ask` (via 2 di [`docs/integration.md`](../../docs/integration.md)).
//!
//! Il guest trasporta una richiesta come `tool:<nome>:<argomenti>`; l'host
//! la decodifica, esegue il tool richiesto e codifica `tool_result:<…>`.
//! Due metriche contate lato host (chiamate e byte) dimostrano l'ispezione
//! dello stato dell'handler DOPO la run (per questo `&mut dyn`, non `Box`).
//!
//! I tool demo sono puri (niente I/O esterno): `soma` (2 interi separati da
//! 'x'), `reverse` (stringa invertita), `len` (lunghezza). Un tool sciocco
//! (`boom`) fa fallire di proposito il lato host per mostrare
//! `SandboxError::Host` → exit 7 in CLI / errore tipizzato da libreria.
//!
//! Uso (dalla root, dopo `sh scripts/scenarios/build-scenarios.sh`):
//! ```sh
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "soma:7x35"
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "reverse:ciao"
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "boom!"
//! ```

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

const MAX_ASK_CALLS: u32 = 16;

struct ToolHandler {
    calls: u32,
    bytes_in: usize,
    bytes_out: usize,
}

impl HostHandler for ToolHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        self.calls += 1;
        if self.calls > MAX_ASK_CALLS {
            // L'host può anche rifiutare: diventa SandboxError::Host lato core.
            return Err(HostError::Custom(format!(
                "budget tool locale superato ({MAX_ASK_CALLS} chiamate)"
            )));
        }

        let req = String::from_utf8_lossy(request).to_string();
        self.bytes_in += request.len();

        // --- Protocollo: tool:<nome>:<resto> ---
        let rest = req.strip_prefix("tool:").unwrap_or(&req);
        let (name, arg) = match rest.split_once(':') {
            Some((n, a)) => (n, a),
            // Richiesta senza protocollo: solo eco, come il guest-echo.
            None => (req.as_str(), ""),
        };

        let response = match name {
            "soma" => {
                let (a, b) = arg
                    .split_once('x')
                    .and_then(|(a, b)| {
                        let a: u64 = a.trim().parse().ok()?;
                        let b: u64 = b.trim().parse().ok()?;
                        Some((a, b))
                    })
                    .ok_or_else(|| {
                        HostError::Custom("uso di soma: soma:<a>x<b> con due interi".into())
                    })?;
                format!("tool_result:soma={}", a + b)
            }
            "reverse" => format!(
                "tool_result:reverse={}",
                arg.chars().rev().collect::<String>()
            ),
            "len" => format!("tool_result:len={}", arg.chars().count()),
            // Ogni risposta seguita dal TWO precedente: mantieni keyed protocol.
            _ => return Err(HostError::Custom(format!("tool sconosciuto: {name}"))),
        };

        let bytes = response.into_bytes();
        self.bytes_out += bytes.len();
        Ok(bytes)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let wasm_path = args
        .next()
        .unwrap_or_else(|| "target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm".into());
    let input = args.next().unwrap_or_else(|| "soma:7x35".into());
    let mut input: Vec<u8> = input.into_bytes();
    if !input.starts_with(b"tool:") {
        // Semplifica la demo: prefix tipico del protocollo se l'utente dimentica.
        let p = format!("tool:{req}", req = String::from_utf8_lossy(&input)).into_bytes();
        input = p;
    }

    let wasm = match std::fs::read(&wasm_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("impossibile leggere {}: {e}", wasm_path);
            eprintln!("compila: sh scripts/scenarios/build-scenarios.sh");
            std::process::exit(2);
        }
    };

    let config = SandboxConfig {
        cache_dir: Some(std::env::temp_dir().join("wasmbox-scenario-b-cache")),
        max_ask_calls: MAX_ASK_CALLS + 4, // il tetto "duro" della sandbox, sopra il policy-check locale
        ..Default::default()
    };

    let engine = match SandboxEngine::new(&wasm, config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("inizializzazione sandbox fallita: {e}");
            std::process::exit(1);
        }
    };

    let mut handler = ToolHandler {
        calls: 0,
        bytes_in: 0,
        bytes_out: 0,
    };
    println!("input: {:?}", String::from_utf8_lossy(&input));
    match engine.run(&input, &mut handler) {
        Ok(output) => {
            println!(
                "output: {} ({len} byte)",
                String::from_utf8_lossy(&output),
                len = output.len()
            );
            // Stato dell'handler ispezionabile DOPO la run (&mut dyn, non
            // Box): metriche di sessione senza Nothing buffer in più.
            println!(
                "metriche host: {} chiamate ask, {} byte in, {} byte out",
                handler.calls, handler.bytes_in, handler.bytes_out
            );
        }
        // Err(SandboxError::Host(_)) = errore del TUO handler: exit 7 da API
        // CLI, qui tipizzato e chiaramente separato dagli errori del guest.
        Err(SandboxError::Host(h)) => {
            eprintln!("errore host handler: {h}");
            std::process::exit(7);
        }
        Err(e) => {
            eprintln!("run fallita: {e}");
            std::process::exit(1);
        }
    }
}

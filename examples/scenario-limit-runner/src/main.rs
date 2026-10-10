//! M1 — run deterministica fuel-only per lo scenario C
//! ([docs/extension-plan.md](../../docs/extension-plan.md), §1).
//!
//! Perché esiste: con la `SandboxConfig` default la fuel-bomb esce 3
//! (`fuel_exhausted`) **oppure** 4 (`timeout`): fuel 1e9 e epoch 1 s corrono
//! in parallelo e vince il primo che scatta, dipende dal carico della
//! macchina. Qui il timeout è disattivato, quindi l'unico limite possibile
//! è il fuel ⇒ **exit 3 deterministico**.
//!
//! Facoltativamente esegue anche la fune: `--oom` (permesso OOM con input
//! grande) o `--ask` (flood `ask`, exit 6).
//!
//! Uso (dopo `sh scripts/scenarios/build-scenarios.sh`):
//! ```sh
//! scenario-limit-runner target/scenario-hostile/fuel-bomb.wasm
//! ```
//! Exit: 0 ok · 2 uso/IO · 3 fuel · 9 wasm invalido. Nota: exit 3 qui è
//! "run terminata con FuelExhausted come previsto", NON un errore del
//! runner — lo script scenario-c gestisce la mappatura.

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(request.to_vec())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wasm_path = args.first().cloned().unwrap_or_else(|| {
        eprintln!("uso: scenario-limit-runner <guest.wasm> [--oom | --ask]");
        std::process::exit(2);
    });

    let want_oom = args.iter().any(|a| a == "--oom");
    let want_ask = args.iter().any(|a| a == "--ask");
    if want_oom && want_ask {
        eprintln!("--oom e --ask sono mutuamente esclusivi");
        std::process::exit(2);
    }

    let wasm = match std::fs::read(&wasm_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("impossibile leggere {}: {e}", wasm_path);
            std::process::exit(2);
        }
    };

    // Config fuel-only: NESSUN timeout wall-clock, fuel ridotto.
    let config = SandboxConfig {
        max_fuel: Some(5_000_000),
        epoch_timeout: None, // l'unico limite possibile è il fuel
        max_ask_calls: 1024,
        ..Default::default()
    };

    let engine = match SandboxEngine::new(&wasm, config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("inizializzazione sandbox fallita: {e}");
            std::process::exit(if matches!(e, SandboxError::InvalidWasm(_)) {
                9
            } else {
                1
            });
        }
    };

    let mut handler = EchoHandler;
    let input: Vec<u8> = if want_oom {
        // 20 MiB via argomento non è possibile (limite argv): read dello stdin.
        use std::io::Read;
        let mut buf = Vec::new();
        if std::io::stdin().read_to_end(&mut buf).is_err() || buf.is_empty() {
            eprintln!(
                "--oom richiede input su stdin (es. head -c 20971520 /dev/zero | tr '\\0' 'x')"
            );
            std::process::exit(2);
        }
        buf
    } else {
        b"x".to_vec()
    };

    match engine.run(&input, &mut handler) {
        Ok(out) => {
            println!("{}", String::from_utf8_lossy(&out));
            std::process::exit(0);
        }
        Err(SandboxError::FuelExhausted) if want_ask || !want_oom => {
            // Attesa per la fuel-bomb: 3 = ok
            std::process::exit(3);
        }
        Err(SandboxError::AskLimitExceeded(n)) if want_ask => {
            println!("ask_limit_exceeded ({n})");
            std::process::exit(6);
        }
        Err(e) => {
            eprintln!("run fallita: {e}");
            let code = match e {
                SandboxError::AskLimitExceeded(_) | SandboxError::PayloadTooLarge { .. } => 6,
                SandboxError::GuestOutOfMemory => 5,
                SandboxError::Timeout => 4,
                SandboxError::Host(_) => 7,
                SandboxError::MissingExport(_) => 8,
                SandboxError::InvalidWasm(_) => 9,
                _ => 10,
            };
            std::process::exit(code);
        }
    }
}

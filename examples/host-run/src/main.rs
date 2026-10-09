//! Esempio lato host: come si usa `wasmbox-core` in un'applicazione reale.
//!
//! Uso:
//! ```sh
//! cargo run -p host-run -- <guest.wasm> [input]
//! ```
//! Se non viene passato alcun percorso, viene usato l'artefatto di
//! `guest-echo` (compilato con `sh ./scripts/build.sh`).
//!
//! Il punto chiave: la logica di dominio vive interamente nell'host.
//! Il runtime non interpreta nulla — il `HostHandler` decide cosa fare
//! di ogni richiesta opaca `ask`.

use std::path::PathBuf;

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine};

/// Handler di esempio: fa da eco in maiuscolo e logga la richiesta.
///
/// In un'applicazione vera qui ci sarebbe una chiamata a un LLM, una
/// ricerca, un motore di regole… qualunque cosa, tranne la sandbox.
struct UppercaseHandler;

impl HostHandler for UppercaseHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        let req = String::from_utf8_lossy(request);
        println!(
            "  [host] richiesta ricevuta ({} byte): {req}",
            request.len()
        );
        Ok(req.to_uppercase().into_bytes())
    }
}

fn main() {
    let mut args = std::env::args().skip(1);

    let wasm_path: PathBuf = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/wasm32-unknown-unknown/release/guest_echo.wasm"));
    let input = args.next().unwrap_or_else(|| "hello wasmbox".to_string());

    let wasm = match std::fs::read(&wasm_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!(
                "errore: impossibile leggere il guest da {}: {e}",
                wasm_path.display()
            );
            eprintln!("suggerimento: compila guest-echo con `sh ./scripts/build.sh`");
            std::process::exit(1);
        }
    };

    println!("guest: {} ({} byte)", wasm_path.display(), wasm.len());
    println!("input: {input:?}");

    let config = SandboxConfig {
        cache_dir: Some(std::env::temp_dir().join("wasmbox-example-cache")),
        ..Default::default()
    };

    let engine = match SandboxEngine::new(&wasm, config) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("errore: inizializzazione sandbox fallita: {e}");
            std::process::exit(1);
        }
    };

    let mut handler = UppercaseHandler;
    match engine.run(input.as_bytes(), &mut handler) {
        Ok(output) => {
            let out = String::from_utf8_lossy(&output);
            println!("output host ({byte} byte): {out}", byte = output.len());
        }
        Err(e) => {
            eprintln!("errore: la run è fallita: {e}");
            std::process::exit(1);
        }
    }
}

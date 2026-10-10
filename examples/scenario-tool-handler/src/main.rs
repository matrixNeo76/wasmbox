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
//! Use (dalla root, dopo `sh scripts/scenarios/build-scenarios.sh`):
//! ```sh
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "soma:7x35"
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "reverse:ciao"
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "boom!"
//! ```
//!
//! Feature `llm` (scenario F, docs/extension-plan.md §4): aggiunge il tool
//! `llm:<prompt>` — la chiamata di RETE sta interamente nell'host handler,
//! mai nella sandbox. Richiede `OPENROUTER_API_KEY` nell'ambiente del
//! PROCESSO HOST (lettura solo lato host, mai nel guest). Limiti a difesa:
//! prompt capito a 8 KiB, timeout 30 s, retry 0, modello free-tier
//! default. Nessuna chiave nella risposta o nel guest.

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

const MAX_ASK_CALLS: u32 = 16;

// --- Scenario F: costanti del tool llm (feature "llm") ---
#[cfg(feature = "llm")]
mod llm_tool {
    use super::HostError;

    const MAX_PROMPT_BYTES: usize = 8 * 1024;
    const TIMEOUT_SECS: u64 = 30;
    const MODEL_FREE: &str = "openrouter/auto:free";

    /// Tool `llm:<prompt>`: una singola chiamata OpenRouter.
    ///
    /// Mappature errori → HostError::Custom("llm:<http>:\ <msg>"):
    /// chiave mancante, prompt vuoto/troppo grande, timeout HTTP, provider.
    pub fn call_llm(prompt: &str) -> Result<String, HostError> {
        // API key: SOLO dall'ambiente del processo host (mai nella sandbox).
        let api_key = std::env::var("OPENROUTER_API_KEY").map_err(|_| {
            HostError::Custom("llm:0: OPENROUTER_API_KEY non impostata nel processo host".into())
        })?;
        if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err(HostError::Custom(format!(
                "llm:0: prompt vuoto o > {MAX_PROMPT_BYTES} byte"
            )));
        }

        // Il workspace VIETA serde/serde_json: costruiamo la richiesta a mano.
        let body = format!(
            "{{\"model\":\"{MODEL_FREE}\",\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}",
            json_escape(prompt)
        );
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(TIMEOUT_SECS)))
            .build()
            .new_agent();
        let mut resp = agent
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", &format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .send(&body)
            .map_err(|e| HostError::Custom(format!("llm:0: http {e}")))?;

        // OpenRouter: testo in choices[0].message.content; errori lato body.
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| HostError::Custom(format!("llm:{status}: body read: {e}")))?;
        if status != 200 {
            return Err(HostError::Custom(format!("llm:{status}: {text}")));
        }
        // Estrazione minimal a mano di choices[0].message.content
        // (protocollo del piano; saranno semplificate, per casi negativi
        // usa da estrattore).
        let content = extract_content(&text).ok_or_else(|| {
            HostError::Custom(format!("llm:{status}: risposta json senza content"))
        })?;
        Ok(format!("tool_result:llm={content}"))
    }

    fn json_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
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
        out.push('"');
        out
    }

    /// Estrattore minimale di `choices[0].message.content` (stringa scappata)
    /// senza serde: cerca `"content":"` e copia fino alla `"` non scappata.
    fn extract_content(body: &str) -> Option<String> {
        let target = "\"content\":\"";
        let i = body.find(target)? + target.len();
        let rest = &body[i..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        let cp = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    }
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
        None
    }
}

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
            #[cfg(feature = "llm")]
            "llm" => llm_tool::call_llm(arg)?,
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

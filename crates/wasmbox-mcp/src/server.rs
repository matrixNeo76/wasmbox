//! Modulo server: il dispatch JSON-RPC (una riga → risposta opzionale).
//! Pubblico perché testato a livello contratto dai test d'integrazione;
//! `main.rs` è solo il loop stdio che lo usa.

use crate::envelope::{RunEnvelope, OUTPUT_CAP_DEFAULT, OUTPUT_CAP_MAX};
use crate::protocol::{parse, to_string, Json};
use crate::runner::Runner;

const SERVER_NAME: &str = "wasmbox-mcp";
const SERVER_VERSION: &str = "0.1.0";
const PROTOCOL_VERSION: &str = "2026-07-28";
const TOOL_NAME: &str = "wasmbox_run";

const TOOL_DESCRIPTION: &str = "Esegue un modulo WASM (guest) nella sandbox wasmbox \
con limiti di risorsa SEMPRE attivi (fuel, timeout, memoria, budget ask). \
Ritorna un envelope JSON compatto: {ok, output|error+message, ask_calls, \
duration_ms, output_truncated}. Il guest va in base64 se binario.";

pub fn tool_name() -> &'static str {
    TOOL_NAME
}

pub fn tool_schema() -> Json {
    Json::obj(&[
        ("name", Json::Str(TOOL_NAME.into())),
        ("description", Json::Str(TOOL_DESCRIPTION.into())),
        (
            "inputSchema",
            Json::obj(&[
                ("type", Json::Str("object".into())),
                ("required", Json::Arr(vec![Json::Str("guest".into())])),
                (
                    "properties",
                    Json::obj(&[
                        (
                            "guest",
                            Json::obj(&[
                                ("type", Json::Str("string".into())),
                                ("description", Json::Str("guest .wasm in base64".into())),
                            ]),
                        ),
                        (
                            "input",
                            Json::obj(&[
                                ("type", Json::Str("string".into())),
                                ("description", Json::Str("input del guest (testo)".into())),
                            ]),
                        ),
                        (
                            "max_fuel",
                            Json::obj(&[
                                ("type", Json::Str("integer".into())),
                                (
                                    "description",
                                    Json::Str(
                                        "budget istruzioni (restringe il default 1e9)".into(),
                                    ),
                                ),
                            ]),
                        ),
                        (
                            "timeout_ms",
                            Json::obj(&[
                                ("type", Json::Str("integer".into())),
                                (
                                    "description",
                                    Json::Str("wall-clock ms (restringe il default 1000)".into()),
                                ),
                            ]),
                        ),
                        (
                            "output_bytes_max",
                            Json::obj(&[
                                ("type", Json::Str("integer".into())),
                                (
                                    "description",
                                    Json::Str("cap output: default 4096, max 1048576".into()),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ])
}

fn error_obj(id: &Json, code: i64, message: &str) -> Json {
    Json::obj(&[
        ("jsonrpc", Json::Str("2.0".into())),
        ("id", id.clone()),
        (
            "error",
            Json::obj(&[
                ("code", Json::Int(code)),
                ("message", Json::Str(message.to_string())),
            ]),
        ),
    ])
}

fn result_obj(id: &Json, result: Json) -> Json {
    Json::obj(&[
        ("jsonrpc", Json::Str("2.0".into())),
        ("id", id.clone()),
        ("result", result),
    ])
}

/// Elabora una riga JSON-RPC. `None` = notifica (nessuna risposta).
pub fn handle(line: &str, runner: &mut Runner) -> Option<String> {
    let msg = match parse(line) {
        Ok(m) => m,
        Err(e) => {
            return Some(to_string(&error_obj(
                &Json::Null,
                -32700,
                &format!("parse error: {e}"),
            )))
        }
    };
    let id = msg.get("id").cloned().unwrap_or(Json::Null);
    let method = match msg.get("method").and_then(Json::as_str) {
        Some(m) => m.to_string(),
        None => {
            // Nessun method: risposta spuria (ha result/error) → ignora; se
            // non è nemmeno una risposta → errore di richiesta.
            if msg.get("result").is_some() || msg.get("error").is_some() {
                return None;
            }
            return Some(to_string(&error_obj(&id, -32600, "richiesta senza method")));
        }
    };
    let is_notification = msg.get("id").is_none();

    match method.as_str() {
        "initialize" => {
            if is_notification {
                return None;
            }
            Some(to_string(&result_obj(
                &id,
                Json::obj(&[
                    ("protocolVersion", Json::Str(PROTOCOL_VERSION.into())),
                    ("capabilities", Json::obj(&[("tools", Json::obj(&[]))])),
                    (
                        "serverInfo",
                        Json::obj(&[
                            ("name", Json::Str(SERVER_NAME.into())),
                            ("version", Json::Str(SERVER_VERSION.into())),
                        ]),
                    ),
                ]),
            )))
        }
        "notifications/initialized" => None, // notifica: mai risposta
        "tools/list" => {
            if is_notification {
                return None;
            }
            Some(to_string(&result_obj(
                &id,
                Json::obj(&[("tools", Json::Arr(vec![tool_schema()]))]),
            )))
        }
        "tools/call" => {
            if is_notification {
                return None;
            }
            Some(to_string(&result_obj(&id, tools_call(&msg, runner))))
        }
        other => {
            if is_notification {
                return None;
            }
            Some(to_string(&error_obj(
                &id,
                -32601,
                &format!("method_not_found: {other}"),
            )))
        }
    }
}

fn tools_call(msg: &Json, runner: &mut Runner) -> Json {
    let params = msg.get("params").cloned().unwrap_or(Json::Null);
    let name = params.get("name").and_then(Json::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(Json::Null);

    let envelope = if name != TOOL_NAME {
        RunEnvelope {
            ok: false,
            error: Some("unknown_tool"),
            message: Some(format!(
                "tool sconosciuto: {name} (disponibile: {TOOL_NAME})"
            )),
            output: None,
            ask_calls: 0,
            duration_ms: 0,
            output_truncated: false,
        }
    } else {
        run_tool(&args, runner)
    };

    // MCP: result = {content: [{type:"text", text: <envelope>}]} — una sola
    // rappresentazione: l'envelope già serializzato È il text.
    Json::obj(&[(
        "content",
        Json::Arr(vec![Json::obj(&[
            ("type", Json::Str("text".into())),
            ("text", Json::Str(envelope.to_json())),
        ])]),
    )])
}

/// Esegue il tool `wasmbox_run` con gli args passati.
fn run_tool(args: &Json, runner: &mut Runner) -> RunEnvelope {
    // Per MCP il guest arriva in base64 (decoder minimale qui sotto,
    // stessa disciplina di `wasmbox-http/src/b64.rs`).
    let guest_b64 = args.get("guest").and_then(Json::as_str).unwrap_or("");
    let input = args.get("input").and_then(Json::as_str).unwrap_or("");
    let max_fuel = args
        .get("max_fuel")
        .and_then(Json::as_i64)
        .map(|v| v as u64);
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Json::as_i64)
        .map(|v| v as u64);
    let cap = args
        .get("output_bytes_max")
        .and_then(Json::as_i64)
        .map(|v| v.clamp(64, OUTPUT_CAP_MAX as i64) as usize)
        .unwrap_or(OUTPUT_CAP_DEFAULT);

    let guest = match b64_decode(guest_b64) {
        Ok(g) => g,
        Err(_) => {
            return RunEnvelope {
                ok: false,
                error: Some("invalid_wasm"),
                message: Some("guest non è base64 valido".into()),
                output: None,
                ask_calls: 0,
                duration_ms: 0,
                output_truncated: false,
            }
        }
    };

    let t = std::time::Instant::now();
    match runner.run(&guest, input.as_bytes(), max_fuel, timeout_ms) {
        Ok((out, ask_calls, duration_ms)) => RunEnvelope::ok(&out, ask_calls, duration_ms, cap),
        Err(e) => RunEnvelope::fail(&e, 0, t.elapsed().as_millis() as u64),
    }
}

/// Decoder base64 minimale: standard alphabet, padding richiesto,
/// caratteri estranei rifiutati.
pub fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::new();
    let mut pad_seen = false;
    for (i, c) in input.chars().enumerate() {
        if c == '=' {
            pad_seen = true;
            continue;
        }
        if pad_seen {
            return Err(format!("base64: carattere dopo il padding (pos {i})"));
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("base64: carattere non valido '{c}' (pos {i})")),
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

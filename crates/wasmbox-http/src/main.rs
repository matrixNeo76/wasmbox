//! wasmbox-http — endpoint HTTP minimale per orchestrazione remota
//! (scenario D, docs/extension-plan.md §2).
//!
//! Rotta unica `POST /run`:

//! ```json
//! {"guest":"<base64>","input":"<base64|null>","limits":{...}}
//! → 200 {"ok":true,"output":"<base64>","ask_calls":n}
//! → 200 {"ok":false,"error":"<snake_case>","message":"…"}
//! → 400 bad_request · 413 guest_too_large · 404 not_found · 405 …
//! ```
//!
//! Server std (`TcpListener` + thread per connessione), zero dipendenze
//! oltre a `wasmbox-core`; TLS/auth NON inclusi (v0.7): usare dietro un
//! reverse-proxy con auth. Bind di default LOOPBACK (127.0.0.1:8130);
//! `--bind 0.0.0.0:PORT` è un atto deliberato e documentato.

mod b64;
mod json;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use json::Json;
use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

const MAX_BODY: usize = 48 * 1024 * 1024; // body (json+base64) ≤ 48 MiB
const MAX_GUEST_BYTES: usize = 32 * 1024 * 1024; // guest decomprim ≤ 32 MiB
const MAX_HEADER_LINES: usize = 128;

struct EchoHandler {
    calls: u32,
}

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        self.calls += 1;
        Ok(request.to_vec())
    }
}

fn main() {
    let mut bind = "127.0.0.1:8130".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => match args.next() {
                Some(v) => bind = v,
                None => {
                    eprintln!("uso: wasmbox-http [--bind HOST:PORT]  (default 127.0.0.1:8130)");
                    std::process::exit(2);
                }
            },
            "--help" | "-h" => {
                println!("wasmbox-http — endpoint HTTP per orchestrazione remota (scenario D)");
                println!("uso: wasmbox-http [--bind HOST:PORT] (default 127.0.0.1:8130, loopback)");
                println!("rotta unica: POST /run (vedi docs/extension-plan.md §2)");
                std::process::exit(0);
            }
            other => {
                eprintln!("argomento sconosciuto: {other} (uso: wasmbox-http [--bind HOST:PORT])");
                std::process::exit(2);
            }
        }
    }

    let listener = match TcpListener::bind(&bind) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind {bind} fallita: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[wasmbox-http] in ascolto su http://{bind} (rotta: POST /run)");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || handle_conn(s));
            }
            Err(e) => eprintln!("[conn] accept fallito: {e}"),
        }
    }
}

fn handle_conn(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let (method, path, version, body) = match read_request(&mut stream) {
        Ok(r) => r,
        Err(e) => {
            return respond(
                stream,
                400,
                r#"{"ok":false,"error":"bad_request"}"#,
                Some(&e),
            )
        }
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return respond(stream, 400, r#"{"ok":false,"error":"bad_request"}"#, None);
    }

    if path == "/healthz" && method == "GET" {
        return respond(stream, 200, r#"{"ok":true}"#, None);
    }

    if path != "/run" {
        return respond(stream, 404, r#"{"ok":false,"error":"not_found"}"#, None);
    }
    if method != "POST" {
        return respond(
            stream,
            405,
            r#"{"ok":false,"error":"method_not_allowed"}"#,
            None,
        );
    }

    let payload = match json::parse(&body) {
        Ok(p) => p,
        Err(e) => {
            return respond(
                stream,
                400,
                r#"{"ok":false,"error":"bad_request"}"#,
                Some(&e),
            )
        }
    };

    dispatch_run(stream, payload);
}

fn read_request(stream: &mut TcpStream) -> Result<(String, String, String, String), String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connessione chiusa prima dell'header completo".into());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = find_subslice(&buf, b"\r\n\r\n") {
            header_end = p;
            break;
        }
        if buf.len() > MAX_BODY {
            return Err("header troppo grande".into());
        }
    }

    let header = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header.split("\r\n");
    let request_line = lines.next().ok_or("richiesta vuota")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("metodo assente")?.to_string();
    let path = parts.next().ok_or("path assente")?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();

    if lines.count() > MAX_HEADER_LINES {
        return Err("troppi header".into());
    }

    // Content-Length obbligatorio (ThreadPool blocking server, niente
    // chunked in v0.7 — curl manda sempre Content-Length su POST).
    let mut content_len: usize = 0;
    for line in header.split("\r\n").skip(1) {
        let (k, v) = line
            .split_once(':')
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim()))
            .unwrap_or_default();
        if k == "content-length" {
            content_len = v
                .parse()
                .map_err(|_| "content-length non valido".to_string())?;
        }
        if k == "transfer-encoding" {
            return Err("Transfer-Encoding non supportato (usare Content-Length)".into());
        }
    }
    if content_len > MAX_BODY {
        return Err("body troppo grande".into());
    }

    while buf.len() < header_end + 4 + content_len {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connessione chiusa prima del body completo".into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8(buf[header_end + 4..header_end + 4 + content_len].to_vec())
        .map_err(|_| "body non è UTF-8".to_string())?;

    Ok((method, path, version, body))
}

fn find_subslice(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn dispatch_run(stream: TcpStream, payload: Json) {
    let guest_b64 = match payload.get("guest").and_then(Json::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => {
            return respond(
                stream,
                400,
                r#"{"ok":false,"error":"bad_request"}"#,
                Some("campo 'guest' mancante, vuoto o non stringa"),
            )
        }
    };

    let guest = match b64::decode(guest_b64) {
        Ok(g) => g,
        Err(e) => {
            return respond(
                stream,
                400,
                r#"{"ok":false,"error":"bad_request"}"#,
                Some(&format!("guest: {e}")),
            )
        }
    };
    if guest.len() > MAX_GUEST_BYTES {
        return respond(
            stream,
            413,
            r#"{"ok":false,"error":"guest_too_large"}"#,
            None,
        );
    }

    let input = match payload.get("input") {
        Some(Json::Null) | None => Vec::new(),
        Some(Json::Str(s)) => match b64::decode(s) {
            Ok(b) => b,
            Err(e) => {
                return respond(
                    stream,
                    400,
                    r#"{"ok":false,"error":"bad_request"}"#,
                    Some(&format!("input: {e}")),
                )
            }
        },
        Some(_) => {
            return respond(
                stream,
                400,
                r#"{"ok":false,"error":"bad_request"}"#,
                Some("'input' deve essere stringa o null"),
            )
        }
    };

    let config = match build_config(&payload) {
        Ok(c) => c,
        Err((http, err, msg)) => return respond(stream, http, err, Some(&msg)),
    };

    let engine = match SandboxEngine::new(&guest, config) {
        Ok(e) => e,
        Err(e) => return sandbox_response(stream, e),
    };

    let mut handler = EchoHandler { calls: 0 };
    match engine.run(&input, &mut handler) {
        Ok(out) => {
            let body = format!(
                "{{\"ok\":true,\"output\":\"{}\",\"ask_calls\":{}}}",
                b64::encode(&out),
                handler.calls
            );
            respond(stream, 200, &body, None)
        }
        Err(e) => sandbox_response(stream, e),
    }
}

fn build_config(payload: &Json) -> Result<SandboxConfig, (u16, &'static str, String)> {
    let mut config = SandboxConfig::default();
    let Some(limits) = payload.get("limits") else {
        return Ok(config);
    };
    if !matches!(limits, Json::Obj(_)) {
        return Err((400, "bad_request", "'limits' deve essere un oggetto".into()));
    }
    for key in [
        "max_fuel",
        "epoch_timeout_ms",
        "max_memory_bytes",
        "max_ask_calls",
        "max_ask_payload_bytes",
    ] {
        match limits.get(key) {
            Some(Json::Null) | None => {}
            Some(Json::Int(n)) => apply_limit(&mut config, key, *n)?,
            Some(_) => {
                return Err((
                    400,
                    "bad_request",
                    format!("{key} deve essere intero o null"),
                ))
            }
        }
    }
    Ok(config)
}

fn apply_limit(
    config: &mut SandboxConfig,
    key: &str,
    n: i64,
) -> Result<(), (u16, &'static str, String)> {
    let negative = n < 0;
    match (key, n) {
        ("max_fuel", _) if negative => Err((400, "bad_request", "max_fuel deve essere ≥ 0".into())),
        ("max_fuel", 0) => {
            config.max_fuel = None;
            Ok(())
        }
        ("max_fuel", n) => {
            config.max_fuel = Some(n as u64);
            Ok(())
        }
        ("epoch_timeout_ms", 0) => {
            config.epoch_timeout = None;
            Ok(())
        }
        ("epoch_timeout_ms", n) => {
            config.epoch_timeout = Some(Duration::from_millis(n as u64));
            Ok(())
        }
        ("max_memory_bytes", n) => {
            if negative {
                Err((
                    400,
                    "bad_request",
                    "max_memory_bytes deve essere ≥ 0".into(),
                ))
            } else {
                config.max_memory_bytes = n as usize;
                Ok(())
            }
        }
        ("max_ask_calls", 0) => {
            config.max_ask_calls = u32::MAX;
            Ok(())
        }
        ("max_ask_calls", n) => {
            if negative {
                Err((400, "bad_request", "max_ask_calls deve essere ≥ 0".into()))
            } else {
                config.max_ask_calls = n as u32;
                Ok(())
            }
        }
        ("max_ask_payload_bytes", n) => {
            if negative {
                Err((
                    400,
                    "bad_request",
                    "max_ask_payload_bytes deve essere ≥ 0".into(),
                ))
            } else if n == 0 {
                config.max_ask_payload_bytes = usize::MAX;
                Ok(())
            } else {
                config.max_ask_payload_bytes = n as usize;
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

fn sandbox_response(stream: TcpStream, e: SandboxError) {
    // Una sola mappatura 1:1 con i nomi JSON della CLI (wasmbox-cli main.rs).
    let (code, error) = match &e {
        SandboxError::FuelExhausted => (3, "fuel_exhausted"),
        SandboxError::Timeout => (4, "timeout"),
        SandboxError::GuestOutOfMemory => (5, "guest_out_of_memory"),
        SandboxError::AskLimitExceeded(_) => (6, "ask_limit_exceeded"),
        SandboxError::PayloadTooLarge { .. } => (6, "payload_too_large"),
        SandboxError::Host(_) => (7, "host_error"),
        SandboxError::MissingExport(_) => (8, "missing_export"),
        SandboxError::InvalidWasm(_) => (9, "invalid_wasm"),
        SandboxError::EngineInit(_)
        | SandboxError::MemoryAccess(_)
        | SandboxError::Execution(_) => (10, "execution_error"),
    };
    let body = format!(
        "{{\"ok\":false, \"error\":\"{error}\", \"message\":{}, \"exit_code\":{code}}}",
        json::escape_str(&e.to_string())
    );
    // 200 con ok:false: l'errore È la risposta (il protocollo della CLI).
    respond(stream, 200, &body, None)
}

fn respond(mut stream: TcpStream, status: u16, body: &str, message: Option<&str>) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    };
    // Se c'è un messaggio e il body non lo contiene già, lo compone:
    let body_with_msg: String = if let Some(m) = message {
        if body.contains("\"message\"") {
            body.to_string()
        } else {
            // I body 4xx sono tutti {"ok":false,"error":"…"} → aggiunge message.
            format!(
                "{{\"ok\":false,\"error\":\"bad_request\",\"message\":{}}}",
                json::escape_str(m)
            )
        }
    } else {
        body.to_string()
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_with_msg.len(),
        body_with_msg
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

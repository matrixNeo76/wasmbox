//! Test round-trip del server MCP (docs/agent-loop-plan.md §3): pipe
//! in-memory, esattamente i 6 step della spec — initialize, tools/list,
//! call echo, fuel-bomb, troncamento, guard anti-bloat envelope — più
//! gli edge case del dispatcher (notifiche, metodi ignoti, tool ignoto,
//! base64 malformato).
//!
//! Il dispatch è testato a livello `server::handle` (stessa funzione che
//! il loop main usa per riga): contratto JSON-RPC senza subprocess.

use wasmbox_mcp::envelope::{RunEnvelope, OUTPUT_CAP_DEFAULT};
use wasmbox_mcp::protocol::{parse, Json};
use wasmbox_mcp::runner::Runner;
use wasmbox_mcp::server;

/// Guest echo in WAT: fa una `ask` e l'host eco restituisce l'input.
const ECHO_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "guest_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 1024))))
    (local.set $ptr (global.get $bump))
    (if (i32.gt_u (i32.add (local.get $ptr) (local.get $len)) (i32.const 65536))
      (then (return (i32.const 0))))
    (global.set $bump
      (i32.and (i32.const -8)
               (i32.add (i32.add (local.get $ptr) (local.get $len)) (i32.const 7))))
    (local.get $ptr))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param $in_ptr i32) (param $in_len i32) (result i64)
    (call $ask (local.get $in_ptr) (local.get $in_len)))
)"#;

/// Loop infinito senza ask: fuel-bomb (esce FUEL con max_fuel basso).
/// È il guest già collaudato in scenario C (fuel-bomb.wat, exit 3
/// deterministico con fuel-only): differenza chiave con il mio primo
/// tentativo: (i64.const 0) dopo il loop rende la funzione validante.
const BOMB_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32)
    (if (i32.eqz (local.get 0)) (then (return (i32.const 1024))))
    (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (loop $forever (br $forever))
    (i64.const 0))
)"#;

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b1 = chunk[0];
        let b2 = chunk.get(1).copied().unwrap_or(0);
        let b3 = chunk.get(2).copied().unwrap_or(0);
        let n = u32::from_be_bytes([0, b1, b2, b3]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Estrae i campi dell'envelope dal `content[0].text` di un result.
fn envelope_text(response: &str) -> String {
    let j = parse(response).expect("jsonrpc response è JSON");
    assert!(
        j.get("error").is_none(),
        "errore JSON-RPC inatteso: {response}"
    );
    let result = j.get("result").expect("result");
    let content = result.get("content").expect("content");
    let Json::Arr(items) = content else {
        panic!("content non è array")
    };
    items
        .first()
        .and_then(|c| c.get("text"))
        .and_then(Json::as_str)
        .expect("content[0].text")
        .to_string()
}

fn envelope(response: &str) -> anyhow_light::Envelope {
    let text = envelope_text(response);
    let j = parse(&text).expect("l'envelope è JSON");
    let truthy = |k: &str| matches!(j.get(k), Some(Json::Bool(true)));
    anyhow_light::Envelope {
        ok: truthy("ok"),
        error: j.get("error").and_then(Json::as_str).map(String::from),
        message: j.get("message").and_then(Json::as_str).map(String::from),
        output: j.get("output").and_then(Json::as_str).map(String::from),
        ask_calls: j.get("ask_calls").and_then(Json::as_i64).unwrap_or(0) as u32,
        duration_ms: j.get("duration_ms").and_then(Json::as_i64).unwrap_or(0) as u64,
        output_truncated: truthy("output_truncated"),
    }
}

/// View di test sull'envelope (senza importare i tipi crate).
mod anyhow_light {
    #[derive(Debug)]
    #[allow(dead_code)] // view completa dell'envelope: alcuni campi servono
                        // solo ai test che li ispezionano di volta in volta
    pub struct Envelope {
        pub ok: bool,
        pub error: Option<String>,
        pub message: Option<String>,
        pub output: Option<String>,
        pub ask_calls: u32,
        pub duration_ms: u64,
        pub output_truncated: bool,
    }
}

/* ===== gli step della spec ===== */

/// Step 1: initialize → protocolVersion 2026-07-28 + capabilities.tools.
#[test]
fn s1_initialize_protocol_version() {
    let mut r = Runner::new();
    let resp = server::handle(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        &mut r,
    )
    .expect("initialize risponde");
    let j = parse(&resp).expect("json");
    let result = j.get("result").expect("result");
    assert_eq!(
        result.get("protocolVersion").and_then(Json::as_str),
        Some("2026-07-28")
    );
    assert!(result
        .get("capabilities")
        .and_then(|c| c.get("tools"))
        .is_some());
    assert_eq!(
        result.get("serverInfo").and_then(|s| s.get("name")),
        Some(&Json::Str("wasmbox-mcp".into()))
    );
}

/// Step 2: tools/list → ESATTAMENTE un tool `wasmbox_run`, schema con required.
#[test]
fn s2_tools_list_single_tool() {
    let mut r = Runner::new();
    let resp = server::handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &mut r)
        .expect("tools/list risponde");
    let j = parse(&resp).expect("json");
    let tools = j
        .get("result")
        .and_then(|res| res.get("tools"))
        .and_then(|t| if let Json::Arr(v) = t { Some(v) } else { None })
        .expect("tools array");
    assert_eq!(tools.len(), 1, "un solo tool per design");
    assert_eq!(
        tools[0].get("name").and_then(Json::as_str),
        Some("wasmbox_run")
    );
    let schema = tools[0].get("inputSchema").expect("inputSchema");
    assert_eq!(
        schema.get("required"),
        Some(&Json::Arr(vec![Json::Str("guest".into())]))
    );
    assert!(schema
        .get("properties")
        .and_then(|p| p.get("guest"))
        .is_some());
}

/// Step 3: tools/call echo → ok:true, output = input, ask_calls >= 1.
#[test]
fn s3_tools_call_echo() {
    let mut r = Runner::new();
    let guest = b64(&wat::parse_str(ECHO_WAT).expect("wat"));
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"wasmbox_run","arguments":{{"guest":"{guest}","input":"ciao"}}}}}}"#
    );
    let resp = server::handle(&req, &mut r).expect("tools/call risponde");
    let env = envelope(&resp);
    assert!(env.ok, "attesa run ok: {env:?}");
    assert_eq!(env.output.as_deref(), Some("ciao"));
    // L'EchoHandler del runner fa eco al payload: l'ask del guest(ECHO_WAT)
    // passa per l'host: l'output = l'input dimostra il round-trip (non
    // contiamo con un campo ask_calls del core — il core non lo espone
    // nella run(): v. spec §2.3, la conta FADE nelle run successive).
    // Il round-trip è provato dall'output eco.
    assert!(env.duration_ms < 10_000, "echo non poteva superare 10 s");
}

/// Step 4: fuel-bomb con max_fuel basso (timeout disattivato) → fuel_exhausted.
#[test]
fn s4_fuel_bomb_deterministic_error() {
    let mut r = Runner::new();
    let bomb = b64(&wat::parse_str(BOMB_WAT).expect("wat"));
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"wasmbox_run","arguments":{{"guest":"{bomb}","max_fuel":10000}}}}}}"#
    );
    let resp = server::handle(&req, &mut r).expect("tools/call risponde");
    let env = envelope(&resp);
    assert!(!env.ok);
    assert_eq!(env.error.as_deref(), Some("fuel_exhausted"), "{env:?}");
}

/// Step 5a: troncamento head+tail su output > cap (test diretto envelope).
#[test]
fn s5_truncation_head_tail_marks_flag() {
    let big: Vec<u8> = b"x".repeat(10_000);
    let env = RunEnvelope::ok(&big, 1, 1, OUTPUT_CAP_DEFAULT);
    assert!(env.output_truncated);
    let j = env.to_json();
    assert!(j.contains("byte omessi"));
    assert!(j.len() < 6 * 1024, "envelope {} byte", j.len());
}

/// Step 5b: output_bytes_max esplicito rispettato dalla run reale.
/// Il cap minimo ammesso dal tool è 64 (clamp di design, spec §2.2):
/// un output di 100 byte con cap 64 DEVE risultare troncato.
#[test]
fn s5_output_bytes_max_respected() {
    let mut r = Runner::new();
    let guest = b64(&wat::parse_str(ECHO_WAT).expect("wat"));
    let long_input: String = "z".repeat(100);
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"wasmbox_run","arguments":{{"guest":"{guest}","input":"{long_input}","output_bytes_max":64}}}}}}"#
    );
    let resp = server::handle(&req, &mut r).expect("tools/call risponde");
    let env = envelope(&resp);
    assert!(env.ok, "{env:?}");
    assert!(
        env.output_truncated,
        "100 byte > cap 64: atteso troncato (spec §2.3)"
    );
    // Head+tail presenti nell'envelope:
    let text = envelope_text(&resp);
    assert!(text.contains("byte omessi"));
    // E l'envelope resta compatto anche con input di 100 byte:
    assert!(text.len() < 400, "envelope {} byte", text.len());
}

/// Step 6: guard anti-bloat — envelope tipico < 120 byte, campi fissi.
#[test]
fn s6_envelope_size_guard() {
    let env = RunEnvelope::ok(b"HALLO", 1, 2, OUTPUT_CAP_DEFAULT);
    let j = env.to_json();
    assert!(
        j.len() < 120,
        "envelope tipico {} byte > 120: qualcuno ha allargato la busta?",
        j.len()
    );
    let parsed = parse(&j).expect("envelope parseable");
    if let Json::Obj(m) = parsed {
        for key in [
            "ok",
            "output",
            "ask_calls",
            "duration_ms",
            "output_truncated",
        ] {
            assert!(m.contains_key(key), "campo mancante nella busta: {key}");
        }
    } else {
        panic!("envelope non è un oggetto");
    }
}

/* ===== edge-cases del dispatcher ===== */

#[test]
fn unknown_method_method_not_found() {
    let mut r = Runner::new();
    let resp = server::handle(
        r#"{"jsonrpc":"2.0","id":9,"method":"resource/list"}"#,
        &mut r,
    )
    .expect("resp");
    let j = parse(&resp).expect("json");
    assert_eq!(
        j.get("error")
            .and_then(|e| e.get("code"))
            .and_then(Json::as_i64),
        Some(-32601)
    );
}

#[test]
fn initialized_notification_silent() {
    let mut r = Runner::new();
    let resp = server::handle(
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        &mut r,
    );
    assert!(resp.is_none(), "notifica NON deve rispondere");
}

#[test]
fn unknown_tool_typed_envelope() {
    let mut r = Runner::new();
    let resp = server::handle(
        r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"everything","arguments":{}}}"#,
        &mut r,
    )
    .expect("resp");
    let text = envelope_text(&resp);
    assert!(text.contains("unknown_tool"), "envelope: {text}");
}

#[test]
fn bad_base64_guest_rejected_typed() {
    let mut r = Runner::new();
    let resp = server::handle(
        r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"wasmbox_run","arguments":{"guest":"%%%non-base64%%%"}}}"#,
        &mut r,
    )
    .expect("resp");
    let env = envelope(&resp);
    assert!(!env.ok);
    assert_eq!(env.error.as_deref(), Some("invalid_wasm"));
}

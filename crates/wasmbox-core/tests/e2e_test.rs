//! Suite end-to-end di `wasmbox-core`.
//!
//! I fixture sono moduli WAT che implementano il contratto ABI:
//! import `env::ask`, export `memory`, `guest_alloc`, `guest_free`,
//! `guest_run`.

use std::path::PathBuf;
use std::time::Duration;

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

/// Handler di test: risponde `host_saw[<request>]`.
struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(format!("host_saw[{}]", String::from_utf8_lossy(request)).into_bytes())
    }
}

/// Handler che fallisce sempre: verifica la propagazione `SandboxError::Host`.
struct FailingHandler;

impl HostHandler for FailingHandler {
    fn ask(&mut self, _request: &[u8]) -> Result<Vec<u8>, HostError> {
        Err(HostError::BudgetExceeded)
    }
}

/// Modulo echo: bump allocator fittizio (base 1024, allineamento 8),
/// `guest_free` no-op, `guest_run` inoltra l'input a `ask` e restituisce
/// la risposta dell'host come output (packed).
const ECHO_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)

  (global $bump (mut i32) (i32.const 1024))

  (func (export "guest_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (if (i32.eqz (local.get $len))
      (then (return (i32.const 1024)))
    )
    (local.set $ptr (global.get $bump))
    ;; se finito lo spazio, restituisci 0 (out of memory)
    (if (i32.gt_u (i32.add (local.get $ptr) (local.get $len)) (i32.const 65536))
      (then (return (i32.const 0)))
    )
    (global.set $bump
      (i32.and (i32.const -8)
               (i32.add (i32.add (local.get $ptr) (local.get $len)) (i32.const 7))))
    (local.get $ptr)
  )

  (func (export "guest_free") (param $ptr i32) (param $len i32))

  (func (export "guest_run") (param $in_ptr i32) (param $in_len i32) (result i64)
    (local $packed i64)
    (local.set $packed (call $ask (local.get $in_ptr) (local.get $in_len)))
    (local.get $packed)
  )
)
"#;

/// Loop infinito: serve per i test di fuel e timeout.
const SPIN_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (loop $l (br $l))
    (i64.const 0)
  )
)
"#;

/// Tre chiamate `ask` senza limiti: supera `max_ask_calls = 2`.
const ASK_THRICE_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (drop (call $ask (i32.const 0) (i32.const 0)))
    (drop (call $ask (i32.const 0) (i32.const 0)))
    (drop (call $ask (i32.const 0) (i32.const 0)))
    (i64.const 0)
  )
)
"#;

/// Una sola `ask` con payload più grande del limite configurato.
const BIG_PAYLOAD_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (drop (call $ask (i32.const 0) (i32.const 2048)))
    (i64.const 0)
  )
)
"#;

/// Modulo senza l'export `guest_free`.
const NO_FREE_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32) (i32.const 1024))
  (func (export "guest_run") (param i32 i32) (result i64) (i64.const 0))
)
"#;

/// `guest_alloc` che restituisce sempre 0: la risposta host non-empty
/// deve produrre `GuestOutOfMemory`.
const ALLOC_ZERO_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (local $packed i64)
    (local.set $packed (call $ask (i32.const 0) (i32.const 0)))
    (local.get $packed)
  )
)
"#;

fn parse(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("parse WAT")
}

fn temp_cache_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wasmbox-{tag}-{}-{}",
        std::process::id(),
        // suffisso per non collidere tra test paralleli nello stesso processo
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create cache dir");
    dir
}

#[test]
fn test_echo_round_trip() {
    let wasm = parse(ECHO_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler;
    let out = engine.run(b"a", &mut handler).expect("run");
    assert_eq!(out, b"host_saw[a]");
}

#[test]
fn test_empty_input_runs() {
    let wasm = parse(ECHO_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler;
    let out = engine.run(b"", &mut handler).expect("run");
    assert_eq!(out, b"host_saw[]");
}

#[test]
fn test_fuel_exhausted() {
    let wasm = parse(SPIN_WAT);
    let config = SandboxConfig {
        max_fuel: Some(10_000),
        epoch_timeout: None,
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config).expect("engine");
    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(matches!(err, SandboxError::FuelExhausted), "got {err:?}");
}

#[test]
fn test_timeout() {
    let wasm = parse(SPIN_WAT);
    let config = SandboxConfig {
        max_fuel: None,
        epoch_timeout: Some(Duration::from_millis(100)),
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config).expect("engine");
    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(matches!(err, SandboxError::Timeout), "got {err:?}");
}

#[test]
fn test_ask_limit_exceeded() {
    let wasm = parse(ASK_THRICE_WAT);
    let config = SandboxConfig {
        max_ask_calls: 2,
        max_fuel: None,
        epoch_timeout: None,
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config).expect("engine");
    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(
        matches!(err, SandboxError::AskLimitExceeded(2)),
        "got {err:?}"
    );
}

#[test]
fn test_payload_too_large() {
    let wasm = parse(BIG_PAYLOAD_WAT);
    let config = SandboxConfig {
        max_ask_payload_bytes: 1024,
        max_fuel: None,
        epoch_timeout: None,
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config).expect("engine");
    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(
        matches!(
            err,
            SandboxError::PayloadTooLarge {
                size: 2048,
                max: 1024
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn test_missing_export() {
    let wasm = parse(NO_FREE_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(
        matches!(err, SandboxError::MissingExport(ref n) if n == "guest_free"),
        "got {err:?}"
    );
}

#[test]
fn test_guest_out_of_memory() {
    let wasm = parse(ALLOC_ZERO_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler; // risposta non-empty ⇒ guest_alloc(>0) == 0
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(matches!(err, SandboxError::GuestOutOfMemory), "got {err:?}");
}

#[test]
fn test_host_error_propagates() {
    let wasm = parse(ECHO_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = FailingHandler;
    let err = engine.run(b"x", &mut handler).unwrap_err();
    match err {
        SandboxError::Host(HostError::BudgetExceeded) => {}
        other => panic!("got {other:?}"),
    }
}

#[test]
fn test_cache_corrupted_file_recompiles() {
    let dir = temp_cache_dir("cache-corrupt");

    let wasm = parse(ECHO_WAT);
    let config = SandboxConfig {
        cache_dir: Some(dir.clone()),
        ..Default::default()
    };

    // Prima run: popola la cache.
    let engine = SandboxEngine::new(&wasm, config.clone()).expect("engine");
    let mut h = EchoHandler;
    let _ = engine.run(b"a", &mut h).expect("run");
    drop(engine);

    // Corrompi il file .cwasm.
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        wasm.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    };
    let cwasm = dir.join(format!("{hash}.cwasm"));
    assert!(cwasm.exists(), "cache file should exist after first run");
    std::fs::write(&cwasm, b"corrupted").unwrap();

    // Seconda run: deve rilevare la corruzione e ricompilare senza panicare.
    let engine2 = SandboxEngine::new(&wasm, config).expect("engine2");
    let mut h = EchoHandler;
    let out = engine2.run(b"b", &mut h).expect("run2");
    assert_eq!(out, b"host_saw[b]");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_error_chain_preserves_typed_error() {
    // Verifica che `e.chain()` preservi l'errore tipizzato attraverso
    // l'incapsulamento di Wasmtime (fix della catena di errori).
    let wasm = parse(ASK_THRICE_WAT);
    let config = SandboxConfig {
        max_ask_calls: 2,
        max_fuel: None,
        epoch_timeout: None,
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config).expect("engine");

    let mut handler = EchoHandler;
    let err = engine.run(b"", &mut handler).unwrap_err();
    assert!(
        matches!(err, SandboxError::AskLimitExceeded(2)),
        "typed error must survive the anyhow chain, got {err:?}"
    );
}

#[test]
fn test_guest_free_called_on_response() {
    // 10 run consecutive: se la risposta non venisse liberata, il bump
    // allocator del WAT esaurirebbe la memoria.
    let wasm = parse(ECHO_WAT);
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler;
    for i in 0..10 {
        let input = format!("msg{i}");
        let out = engine.run(input.as_bytes(), &mut handler).expect("run");
        assert!(out.starts_with(b"host_saw["), "got {out:?}");
    }
}

#[test]
fn test_cache_hit_reuses_cwasm() {
    let dir = temp_cache_dir("cache-hit");

    let wasm = parse(ECHO_WAT);
    let config = SandboxConfig {
        cache_dir: Some(dir.clone()),
        ..Default::default()
    };

    let engine1 = SandboxEngine::new(&wasm, config.clone()).expect("engine1");
    let mut h = EchoHandler;
    assert_eq!(engine1.run(b"a", &mut h).expect("run1"), b"host_saw[a]");
    drop(engine1);

    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read cache dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert!(files
        .iter()
        .any(|f| f.to_string_lossy().ends_with(".cwasm")));

    // Secondo engine: riusa la cache senza errori.
    let engine2 = SandboxEngine::new(&wasm, config).expect("engine2");
    let mut h = EchoHandler;
    assert_eq!(engine2.run(b"b", &mut h).expect("run2"), b"host_saw[b]");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_invalid_wasm_rejected() {
    let result = SandboxEngine::new(b"not a wasm module", SandboxConfig::default());
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("invalid wasm must not compile"),
    };
    assert!(matches!(err, SandboxError::InvalidWasm(_)), "got {err:?}");
}

/// E2E con il guest Rust reale (guest-echo), opzionale: richiede
/// `GUEST_ECHO_WASM` puntato all'artefatto `wasm32-unknown-unknown`.
/// La suite resta verde senza il target installato.
#[test]
fn guest_echo_e2e() {
    let path = match std::env::var("GUEST_ECHO_WASM") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            eprintln!("skip: GUEST_ECHO_WASM non impostato, guest_echo_e2e saltato");
            return;
        }
    };
    if !path.exists() {
        eprintln!("skip: artefatto {} assente", path.display());
        return;
    }
    let wasm = std::fs::read(&path).expect("read guest artifact");
    let engine = SandboxEngine::new(&wasm, SandboxConfig::default()).expect("engine");
    let mut handler = EchoHandler;
    let out = engine.run(b"hello", &mut handler).expect("run");
    assert_eq!(out, b"echo_result:host_saw[inspect:hello]");
}

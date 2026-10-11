//! Test di accettazione del layer WASI opt-in (spec wasi-plan.md §6).
//!
//! I guest sono moduli WAT compilati al volo con la dev-dep `wat` (come la
//! suite del core): hanno l'ABI wasmbox (`memory`, `guest_alloc`,
//! `guest_free`, `guest_run`) più le import WASI che servono al test.

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxError};
use wasmbox_wasi::{WasiCapabilities, WasiEngine};

struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, req: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(format!("host_saw[{}]", String::from_utf8_lossy(req)).into_bytes())
    }
}

const GUEST_ABI_WAT: &str = r#"
(module
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
  (func (export "guest_run") (param i32 i32) (result i64)
    ;; output = "ok" scritto dopo la fine del guest_alloc: restituisce l'input
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get 0)) (i64.const 32))
      (i64.extend_i32_u (local.get 1))))
)
"#;

const GUEST_RANDOM_WAT: &str = r#"
(module
  (import "wasi_snapshot_preview1" "random_get" (func $random_get (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 6144) "ok")   ;; buffer di risposta scritto dopo random_get
  (func (export "guest_alloc") (param $len i32) (result i32)
    (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    ;; chiama random_get sul buffer 1024..1032, poi risponde "ok" (6144,2)
    (drop (call $random_get (i32.const 1024) (i32.const 8)))
    (i64.or
      (i64.shl (i64.extend_i32_u (i32.const 6144)) (i64.const 32))
      (i64.extend_i32_u (i32.const 2))))
)
"#;

const GUEST_STDOUT_WAT: &str = r#"
(module
  (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 2048) "ciao-dal-guest-wasi\n")
  (global $bump (mut i32) (i32.const 4096))
  (func (export "guest_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 1024))))
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (local.get $ptr) (local.get $len)))
    (local.get $ptr))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    ;; iovec singolo: @3072 [ptr=2048, len=20]; nwritten @3076
    (i32.store (i32.const 3072) (i32.const 2048))
    (i32.store (i32.const 3076) (i32.const 20))
    (drop (call $fd_write (i32.const 1) (i32.const 3072) (i32.const 1) (i32.const 3080)))
    ;; ritorna l'input invariato (ABI-packed)
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get 0)) (i64.const 32))
      (i64.extend_i32_u (local.get 1))))
)
"#;

const GUEST_FUEL_BOMB_WAT: &str = r#"
(module
  (import "wasi_snapshot_preview1" "clock_time_get" (func $clock (param i32 i64 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param $len i32) (result i32) (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    (loop $forever (br $forever))
    (i64.const 0))
)
"#;

/// Test 1 della spec: default = denied. Il guest che importa `random_get`
/// con capability di default NON istanzia → errore con "wasi_denied" nel
/// messaggio (strict mode).
#[test]
fn t1_zero_capabilities_denies_imports() {
    let wasm = wat::parse_str(GUEST_RANDOM_WAT).expect("wat");
    let e = WasiEngine::new(
        &wasm,
        &WasiCapabilities::default(),
        SandboxConfig::default(),
    );
    match e {
        Err(SandboxError::EngineInit(msg)) => {
            assert!(
                msg.contains("random_get"),
                "attesa negazione di random_get, trovato: {msg}"
            );
        }
        Err(other) => panic!("atteso EngineInit(wasi_denied), trovato {other:?}"),
        Ok(_) => panic!("il guest con import negato NON deve istanziare"),
    }
}

/// Diagnosi: la lista delle denied import del modulo con default.
#[test]
fn t1b_denied_imports_listed() {
    let wasm = wat::parse_str(GUEST_RANDOM_WAT).expect("wat");
    let e = WasiEngine::new(
        &wasm,
        &WasiCapabilities::default(),
        SandboxConfig::default(),
    );
    let err = e.err().expect("error atteso");
    assert!(err.to_string().contains("random_get"), "{err}");
}

/// Test 3/5 della spec fatto separatamente: clock e random opt-in.
/// Con `allow_random = true` il guest istanzia e random_get risponde errno 0.
#[test]
fn t5_random_get_allowed_allows_instantiation() {
    let wasm = wat::parse_str(GUEST_RANDOM_WAT).expect("wat");
    let caps = WasiCapabilities {
        allow_random: true,
        ..Default::default()
    };
    let engine =
        WasiEngine::new(&wasm, &caps, SandboxConfig::default()).expect("engine con allow_random");
    let mut h = EchoHandler;
    let (out, _stdout) = engine.run(b"x", &mut h).expect("run con allow_random");
    assert_eq!(out, b"ok");
}

/// Test base: guest ABI senza import WASI gira identico al core (echo sul
/// returned packed — e `ask` non è chiamato).
#[test]
fn t2_guest_abi_only_runs_without_wasi() {
    let wasm = wat::parse_str(GUEST_ABI_WAT).expect("wat");
    let engine = WasiEngine::new(
        &wasm,
        &WasiCapabilities::default(),
        SandboxConfig::default(),
    )
    .expect("engine");
    let mut h = EchoHandler;
    let (out, _stdout) = engine.run(b"ciao", &mut h).expect("run");
    assert_eq!(out, b"ciao");
}

/// Test 8 della spec: `env::ask` round-trip in un guest WASI.
#[test]
fn t8_ask_roundtrip_in_wasi_guest() {
    // Handler che registra le richieste viste (run single-thread: una
    // Vec locale si trasferisce all'handler e torna indietro dopo la run).
    struct RecordingHandler {
        seen: Vec<String>,
    }

    impl HostHandler for RecordingHandler {
        fn ask(&mut self, req: &[u8]) -> Result<Vec<u8>, HostError> {
            self.seen.push(String::from_utf8_lossy(req).to_string());
            Ok(format!("host_saw[{}]", String::from_utf8_lossy(req)).into_bytes())
        }
    }

    let wasm = wat::parse_str(GUEST_ASK_WAT).expect("wat");
    let engine = WasiEngine::new(
        &wasm,
        &WasiCapabilities::default(),
        SandboxConfig::default(),
    )
    .expect("engine");
    let mut h = RecordingHandler { seen: Vec::new() };
    let (out, _stdout) = engine.run(b"ping", &mut h).expect("run");
    // L'output della run è la risposta dell'ULTIMA ask: 'f' (mem[102]).
    assert_eq!(out, b"host_saw[f]");
    // E la prima ask ha visto il VERO input del guest (pipeline input
    // → memoria → ask confermata):
    assert_eq!(h.seen.len(), 2);
    assert_eq!(h.seen[0], "ping");
    assert_eq!(h.seen[1], "f");
}

const GUEST_ASK_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 102) "f")          ;; argomento della seconda ask
  (data (i32.const 6656) "host_saw[ping]")  ;; diagnostica, non usata
  (global $bump (mut i32) (i32.const 4096))
  (func (export "guest_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 2048))))
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (local.get $ptr) (local.get $len)))
    (local.get $ptr))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param i32 i32) (result i64)
    ;; due ask: la prima inoltra l'input, la seconda usa 'f' (mem[102])
    (drop (call $ask (local.get 0) (local.get 1)))
    (call $ask (i32.const 102) (i32.const 1)))
)
"#;

/// Test 6 della spec: stdout captured (fd_write → pipe memoria, mai processo host).
#[test]
fn t6_stdout_captured() {
    let wasm = wat::parse_str(GUEST_STDOUT_WAT).expect("wat");
    let engine = WasiEngine::new(
        &wasm,
        &WasiCapabilities::default(),
        SandboxConfig::default(),
    )
    .expect("engine");
    let mut h = EchoHandler;
    let (_out, stdout) = engine.run(b"in", &mut h).expect("run");
    assert_eq!(stdout, b"ciao-dal-guest-wasi\n");
}

/// Test 7 della spec: fuel CONTINUA a funzionare (loop infinito → FuelExhausted).
#[test]
fn t7_fuel_bomb_with_wasi_imports() {
    let wasm = wat::parse_str(GUEST_FUEL_BOMB_WAT).expect("wat");
    // clock concesso dall'host: dimostra che il guest con import concesso
    // istanzia e fuel lo frena comunque.
    let caps = WasiCapabilities {
        allow_clock: true,
        ..Default::default()
    };
    let config = SandboxConfig {
        max_fuel: Some(200_000),
        epoch_timeout: None, // deterministico: solo fuel
        ..Default::default()
    };
    let engine = WasiEngine::new(&wasm, &caps, config).expect("engine");
    let mut h = EchoHandler;
    match engine.run(b"", &mut h) {
        Err(SandboxError::FuelExhausted) => {}
        Err(other) => panic!("atteso FuelExhausted, trovato {other:?}"),
        Ok(_) => panic!("la fuel bomb NON deve completare"),
    }
}

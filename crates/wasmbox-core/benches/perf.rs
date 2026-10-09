//! Benchmark di `wasmbox-core` — misura i numeri che il README dichiara.
//!
//! Solo `std` (nessuna dipendenza aggiuntiva, come da blueprint):
//! - compilazione a freddo vs cache-hit `.cwasm`;
//! - overhead per run con round-trip `ask`;
//! - run senza `ask` (baseline) per isolare il costo dell'host function.
//!
//! Esecuzione: `cargo bench -p wasmbox-core`
//! (target `harness = false`: il binario stampa i numeri e basta).

use std::time::Instant;

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine};

/// Handler minimale: risponde subito, senza alloci significativi.
struct NullHandler;

impl HostHandler for NullHandler {
    fn ask(&mut self, _request: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(b"ok".to_vec())
    }
}

/// Modulo con round-trip `ask` (come ECHO_WAT dei test).
const ASK_WAT: &str = r#"
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
)
"#;

/// Modulo senza `ask`: serve come baseline per isolare il costo round-trip.
const NO_ASK_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "guest_alloc") (param $len i32) (result i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 1024))))
    (i32.const 1024))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param $in_ptr i32) (param $in_len i32) (result i64)
    ;; echo fittizio: restituisce l'input com'è (packed)
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $in_ptr)) (i64.const 32))
      (i64.extend_i32_u (local.get $in_len))))
)
"#;

fn parse(wat: &str) -> Vec<u8> {
    // Parser WAT minimale senza `wat`: il crate di dev-dep `wat` esiste già,
    // lo usiamo — è già nella lista autorizzata dal blueprint.
    wat::parse_str(wat).expect("parse WAT")
}

fn mean_ns(samples: &[u128]) -> f64 {
    samples.iter().sum::<u128>() as f64 / samples.len() as f64
}

fn main() {
    let ask_wasm = parse(ASK_WAT);
    let no_ask_wasm = parse(NO_ASK_WAT);

    // --- 1. Compilazione a freddo (nessuna cache) ---
    let n_cold = 5;
    let mut samples = Vec::with_capacity(n_cold);
    for _ in 0..n_cold {
        let t = Instant::now();
        let engine = SandboxEngine::new(&ask_wasm, SandboxConfig::default()).expect("engine");
        samples.push(t.elapsed().as_nanos());
        drop(engine);
    }
    println!(
        "compile a freddo (nessuna cache): {:>8.3} ms  (n={n_cold}, media)",
        mean_ns(&samples) / 1e6
    );

    // --- 2. Cache hit: secondo `new` con stesso cache_dir rilava il .cwasm ---
    let tmp = std::env::temp_dir().join(format!("wasmbox-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let cached_config = SandboxConfig {
        cache_dir: Some(tmp.clone()),
        ..Default::default()
    };
    // Prima run: popola la cache.
    let engine = SandboxEngine::new(&ask_wasm, cached_config.clone()).expect("engine");
    drop(engine);

    let n_warm = 20;
    let mut samples = Vec::with_capacity(n_warm);
    for _ in 0..n_warm {
        let t = Instant::now();
        let engine = SandboxEngine::new(&ask_wasm, cached_config.clone()).expect("engine warm");
        samples.push(t.elapsed().as_nanos());
        drop(engine);
    }
    println!(
        "compile con cache hit (.cwasm):  {:>8.3} ms  (n={n_warm}, media)",
        mean_ns(&samples) / 1e6
    );
    let _ = std::fs::remove_dir_all(&tmp);

    // --- 3. Overhead per run: con ask vs senza ask ---
    let engine_ask = SandboxEngine::new(&ask_wasm, SandboxConfig::default()).expect("engine");
    let engine_no_ask = SandboxEngine::new(&no_ask_wasm, SandboxConfig::default()).expect("engine");

    let n_run = 1_000;
    let mut h = NullHandler;
    let mut samples = Vec::with_capacity(n_run);
    for _ in 0..n_run {
        let t = Instant::now();
        let _ = engine_ask.run(b"bench", &mut h).expect("run");
        samples.push(t.elapsed().as_nanos());
    }
    let mean_ask = mean_ns(&samples);
    println!(
        "run con round-trip ask:          {:>8.1} us  (n={n_run}, media)",
        mean_ask / 1e3
    );

    let mut samples = Vec::with_capacity(n_run);
    for _ in 0..n_run {
        let t = Instant::now();
        let _ = engine_no_ask.run(b"bench", &mut h).expect("run");
        samples.push(t.elapsed().as_nanos());
    }
    let mean_no_ask = mean_ns(&samples);
    println!(
        "run senza ask (baseline):        {:>8.1} us  (n={n_run}, media)",
        mean_no_ask / 1e3
    );
    println!(
        "overhead del round-trip ask:     {:>8.1} us",
        (mean_ask - mean_no_ask) / 1e3
    );
}

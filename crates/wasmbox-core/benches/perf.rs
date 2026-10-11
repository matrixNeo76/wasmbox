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

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

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

/// Modulo con heap ampio per lo sweep a payload grande: 16 MiB lineari
/// (256 pagine), ceiling dell'allocatore letto con `memory.size` (mai un
/// tetto fisso), bump allocator resetato a ogni `guest_run` così i 500
/// run del sweep non saturano lo heap del guest. L'ask è l'echo del
/// modulo sopra: alloca la risposta col guest_alloc del guest.
const ASK_BIG_WAT: &str = r#"
(module
  (import "env" "ask" (func $ask (param i32 i32) (result i64)))
  (memory (export "memory") 256)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "guest_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (if (i32.eqz (local.get $len)) (then (return (i32.const 1024))))
    (local.set $ptr (global.get $bump))
    (if (i32.gt_u (i32.add (local.get $ptr) (local.get $len))
                  (i32.mul (memory.size) (i32.const 65536)))
      (then (return (i32.const 0))))
    (global.set $bump
      (i32.and (i32.const -8)
               (i32.add (i32.add (local.get $ptr) (local.get $len)) (i32.const 7))))
    (local.get $ptr))
  (func (export "guest_free") (param i32 i32))
  (func (export "guest_run") (param $in_ptr i32) (param $in_len i32) (result i64)
    ;; Nuovo round: lo heap riparte pulito (l'host copia input e
    ;; risposta PRIMA del prossimo guest_alloc, quindi è sicuro).
    (global.set $bump (i32.const 1024))
    ;; risposta = echo dell'input: alloca len e restituisce pack(ptr,len)
    ;; — l'host già copia l'input a in_ptr; per l'echo l'host scrive la
    ;; risposta in un nuovo buffer con guest_alloc (come sempre).
    (call $ask (local.get $in_ptr) (local.get $in_len))
  )
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
    let ask_big_wasm = parse(ASK_BIG_WAT);
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

    // --- 4. Sweep payload `ask`: 64B / 4KB / 64KB / 1MB ---
    // Il benchmark core usa payload "bench" (5 byte); qui misuriamo il
    // costo round-trip al crescere del payload (guest_alloc + copia in/out
    // + handler). Engine DEDICATO con memoria lineare adeguata: il guest
    // echo del bench ha un heap bump da 64KB a offset 1024 — non basta
    // per payload grandi, quindi qui `max_memory_bytes` sale a 4 MiB
    // (limite host pieno e testimonianza che la quota host NON Vincola
    // il guest: è la memoria del modulo a decidere).
    // `max_ask_payload_bytes` di default = 1 MiB: 64KB e 1MB - 1 ci
    // passano; il rifiuto tipizzato oltre-tetto è misurato nel punto 5.
    let sweep_engine = SandboxEngine::new(
        &ask_big_wasm,
        SandboxConfig {
            max_memory_bytes: 16 * 1024 * 1024,
            ..SandboxConfig::default()
        },
    )
    .expect("engine sweep");
    let engine = &sweep_engine;
    let n_run_payload = 500;
    const PAYLOAD_SIZES: &[(&str, usize)] = &[
        ("64B", 64),
        ("4KB", 4 * 1024),
        ("64KB", 64 * 1024),
        ("1MB", 1024 * 1024 - 1),
    ];
    let payload_results: Vec<(&str, f64)> = PAYLOAD_SIZES
        .iter()
        .map(|(label, size)| {
            let payload: Vec<u8> = (0..*size).map(|i| (i % 251) as u8).collect();
            let mut samples = Vec::with_capacity(n_run_payload);
            for _ in 0..n_run_payload {
                let t = Instant::now();
                let _ = engine.run(&payload, &mut h).expect("run su payload");
                samples.push(t.elapsed().as_nanos());
            }
            let mean = mean_ns(&samples);
            println!(
                "round-trip ask payload {label:<4} = {size:>7} byte: {:>8.1} us  (n={n_run_payload}, media)",
                mean / 1e3
            );
            (*label, mean)
        })
        .collect();
    // Scaling: tempo per byte al crescere del payload (coerenza lineare).
    if let (Some((_, t64)), Some((_, t1m))) = (payload_results.first(), payload_results.last()) {
        let b1m = (1024 * 1024 - 1) as f64;
        let ns_per_byte_large = t1m / b1m;
        println!(
            "costo marginale:               {:>5.2} ns/byte oltre il payload 64B (copia RAM→RAM; costo fisso di round-trip ≈ {:.1} µs)",
            ns_per_byte_large,
            (*t64) / 1e3
        );
    }

    // --- 5. Limite in azione: rifiuto tipizzato oltre il tetto payload ---
    // Dimostra che il limite è ENFORCED a runtime e il costo del rifiuto è
    // misurabile: un payload di 1 MiB + 1 supera max_ask_payload_bytes e la
    // sandbox lo rifiuta con errore tipizzato PRIMA di toccare il guest.
    let over_payload: Vec<u8> = vec![0u8; 1024 * 1024 + 1];
    let n_reject = 500;
    let mut samples = Vec::with_capacity(n_reject);
    for _ in 0..n_reject {
        let t = Instant::now();
        let result = engine.run(&over_payload, &mut h);
        let elapsed = t.elapsed();
        // Verifica REALE (non assert eliminabile in release): il rifiuto
        // deve essere tipizzato per contratto, mai un crash o un trap.
        match result {
            Err(SandboxError::PayloadTooLarge { .. }) => {}
            Err(other) => {
                panic!("rifiuto oltre il tetto doveva essere PayloadTooLarge, ottenuto {other}")
            }
            Ok(_) => panic!("payload > max_ask_payload_bytes non doveva essere eseguito"),
        }
        samples.push(elapsed.as_nanos());
    }
    println!(
        "rifiuto tipizzato (1048577 > 1048576 byte): {:>8.1} ns  (n={n_reject}, media)\n     errore restituito: PayloadTooLarge — limite ENFORCED, mai crash",
        mean_ns(&samples),
    );
}

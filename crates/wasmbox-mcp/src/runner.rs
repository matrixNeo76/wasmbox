//! Runner persistente (docs/agent-loop-plan.md §1 gap 3): engine e
//! modulo compilati vivono nel processo e le run successive non pagano
//! più né edizione né instantiate (l'`instance_pre` resta warm nel
//! `SandboxEngine`, già pooling per design del core).
//!
//! Cache dei moduli per hash del contenuto (la `.cwasm` del core è
//! su-disco; qui è in-memoria per il processo vivo). Handler: eco,
//! come la CLI (niente logica di dominio — divieto blueprint n.3).

use std::collections::HashMap;

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine};

pub struct Runner {
    engines: HashMap<String, SandboxEngine>,
    cache_dir: std::path::PathBuf,
}

/// Handler dell'eco (stesso comportamento della CLI: mai in alto).
struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(request.to_vec())
    }
}

/// Hash FNV-1a 64 (nessuna dipendenza; chiave di cache in-process).
fn fnv1a(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

impl Runner {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
            cache_dir: std::env::temp_dir().join("wasmbox-mcp-cache"),
        }
    }

    /// Numero di moduli caldi (per test e diagnostica).
    pub fn warm_modules(&self) -> usize {
        self.engines.len()
    }

    /// Esegue `guest` su `input`. Config: limiti del core di default,
    /// con override consentiti SOLO restrittivi (fuel, timeout).
    ///
    /// Cache: gli engine con default vivono nella mappa per hash; le run
    /// con override costruiscono un engine dedicato (usa comunque la
    /// cache `.cwasm` su disco del core, quindi resterà economico).
    pub fn run(
        &mut self,
        guest: &[u8],
        input: &[u8],
        max_fuel: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> Result<(Vec<u8>, u32, u64), wasmbox_core::SandboxError> {
        // Override esplicito: engine dedicato, senza toccare la cache.
        // (Il fuel/timeout è *config di run*, non di engine, nel core;
        //  un engine caldo ha il suo config: non reinterpretarlo qui.)
        if max_fuel.is_some() || timeout_ms.is_some() {
            let config = SandboxConfig {
                cache_dir: Some(self.cache_dir.clone()),
                max_fuel,
                epoch_timeout: timeout_ms.map(std::time::Duration::from_millis),
                ..SandboxConfig::default()
            };
            let engine = SandboxEngine::new(guest, config)?;
            let mut h = EchoHandler;
            let t = std::time::Instant::now();
            let out = engine.run(input, &mut h)?;
            return Ok((out, 0, t.elapsed().as_millis() as u64));
        }

        // Run di default: engine caldo dalla cache per hash.
        let key = fnv1a(guest);
        if !self.engines.contains_key(&key) {
            let config = SandboxConfig {
                cache_dir: Some(self.cache_dir.clone()),
                ..SandboxConfig::default()
            };
            let engine = SandboxEngine::new(guest, config)?;
            self.engines.insert(key.clone(), engine);
        }
        let engine = self
            .engines
            .get(&key)
            .expect("inserito sopra, stessa chiave");
        let mut h = EchoHandler;
        let t = std::time::Instant::now();
        let out = engine.run(input, &mut h)?;
        Ok((out, 0, t.elapsed().as_millis() as u64))
    }

    /// Svuota la cache dei moduli caldi (--idle-exit la usa).
    pub fn clear(&mut self) {
        self.engines.clear();
    }
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn wasm() -> Vec<u8> {
        wat::parse_str(ECHO_WAT).expect("parse wat")
    }

    #[test]
    fn warm_reuse_two_runs_same_engine() {
        let mut r = Runner::new();
        let g = wasm();
        let (o1, _, _) = r.run(&g, b"uno", None, None).expect("run 1");
        let (o2, _, _) = r.run(&g, b"due", None, None).expect("run 2");
        assert_eq!(o1, b"uno");
        assert_eq!(o2, b"due");
        assert_eq!(r.warm_modules(), 1);
    }

    #[test]
    fn fuel_override_restrictive_calls_dedicated_engine() {
        let mut r = Runner::new();
        let g = wasm();
        let res = r.run(&g, b"x", Some(50_000), None);
        // Con fuel minuscolo, un round-trip ask potrebbe non bastare:
        // qui il modulo fa UNA sola ask, quindi può anche passare.
        let _ = res.expect("run con override");
        assert_eq!(r.warm_modules(), 0); // override: non in cache
    }
}

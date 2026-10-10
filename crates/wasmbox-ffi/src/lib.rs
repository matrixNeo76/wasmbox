//! Scenario E — FFI/C-ABI sopra `wasmbox-core`
//! ([docs/extension-plan.md §3](../../docs/extension-plan.md)).
//!
//! ABI minima:
//!
//! - [`wasmbox_engine_new`] · [`wasmbox_engine_free`]
//! - [`wasmbox_engine_run`] · [`wasmbox_buffer_free`]
//! - [`wasmbox_last_error`] · [`wasmbox_last_error_code`]
//!
//! Invarianti:
//! - una engine NON è thread-safe (documentato);
//! - `run` copia l'output in un buffer allocato dal crate (globale alloc via
//!   `std::alloc`, esposto come `*mut u8` C) e va liberato con
//!   [`wasmbox_buffer_free`];
//! - MAI puntatori alla memoria del guest fuori dal crate;
//! - `last_error` è legale solo SUBITO dopo una run fallita, NULL altrimenti;
//! - handler `ask` interno = ECO (v0.7), nessun callback C (rimandato a v0.8).
//!
//! `wasmbox_status_t`: 0 ok · 1 argomento invalido · 2 init fallito ·
//! 3 run fallita (dettagli via last_error/last_error_code).
//!
//! Exit code stile CLI su last_error_code: 0 ok · 2..10 altrimenti
//! (mappatura 1:1 in wasmbox-cli main.rs).

#![allow(clippy::missing_safety_doc)] // ABI C: la safety è documentata sopra ogni fn

pub use wasmbox_core::{SandboxConfig, SandboxEngine, SandboxError};

use std::cell::Cell;
use std::ffi::c_char;
use wasmbox_core::{HostError, HostHandler};

/// Codici di stato della C-ABI.
#[repr(u32)]
pub enum WasmboxStatus {
    Ok = 0,
    InvalidArg = 1,
    InitFailed = 2,
    RunFailed = 3,
}

struct EchoHandler;

impl HostHandler for EchoHandler {
    fn ask(&mut self, request: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(request.to_vec())
    }
}

/// Opacità garantita: solo il crate ne interpreta il contenuto.
pub struct WasmboxEngine {
    engine: SandboxEngine,
    last_error: Cell<Option<String>>,
    /// codice stile CLI dell'ultimo esito (0 ok; 2..10 altrimenti)
    last_code: Cell<u32>,
}

/// Crea una engine dal wasm; `limits` NULL → `SandboxConfig::default()`.
///
/// Il struct `wasmbox_limits_t` rispecchia i campi di `SandboxConfig`
/// (max_fuel, epoch_timeout_ms*, max_memory_bytes, max_ask_calls,
/// max_ask_payload_bytes): `*` — epoch_timeout_ms = 0 significa "nessun
/// timeout" (fare eco alla semantica dello scenario D). Un valore
/// `max_fuel = 0` significa "nessun fuel" stesso (coerente con §2.2).
#[repr(C)]
pub struct WasmboxLimits {
    pub max_fuel: u64,
    /// 0 → disattivato (None)
    pub epoch_timeout_ms: u64,
    pub epoch_timeout_enabled: u32, // 0 = None, 1 = epoch_timeout_ms
    pub max_memory_bytes: usize,
    pub max_ask_calls: u32,
    pub max_ask_payload_bytes: usize,
}

fn limits_from_c(l: Option<&WasmboxLimits>) -> SandboxConfig {
    match l {
        None => SandboxConfig::default(),
        Some(l) => SandboxConfig {
            max_fuel: if l.max_fuel == 0 {
                None
            } else {
                Some(l.max_fuel)
            },
            epoch_timeout: if l.epoch_timeout_enabled != 0 {
                if l.epoch_timeout_ms == 0 {
                    None
                } else {
                    Some(std::time::Duration::from_millis(l.epoch_timeout_ms))
                }
            } else {
                None
            },
            max_memory_bytes: l.max_memory_bytes,
            max_ask_calls: l.max_ask_calls,
            max_ask_payload_bytes: l.max_ask_payload_bytes,
            ..Default::default()
        },
    }
}

/// Codice stile CLI da `SandboxError` (mappatura 1:1 con wasmbox-cli).
fn cli_exit_code(e: &SandboxError) -> u32 {
    match e {
        SandboxError::FuelExhausted => 3,
        SandboxError::Timeout => 4,
        SandboxError::GuestOutOfMemory => 5,
        SandboxError::AskLimitExceeded(_) | SandboxError::PayloadTooLarge { .. } => 6,
        SandboxError::Host(_) => 7,
        SandboxError::MissingExport(_) => 8,
        SandboxError::InvalidWasm(_) => 9,
        SandboxError::EngineInit(_)
        | SandboxError::MemoryAccess(_)
        | SandboxError::Execution(_) => 10,
    }
}

#[no_mangle]
pub unsafe extern "C" fn wasmbox_engine_new(
    wasm: *const u8,
    wasm_len: usize,
    limits: *const WasmboxLimits,
) -> *mut WasmboxEngine {
    if wasm.is_null() && wasm_len != 0 {
        return std::ptr::null_mut();
    }
    let bytes = if wasm_len == 0 {
        Vec::new()
    } else {
        // SAFETY: `wasm` non-null e `wasm_len` byte leggibili dal chiamante.
        unsafe { std::slice::from_raw_parts(wasm, wasm_len) }.to_vec()
    };
    // SAFETY: `limits` NULL oppure puntatore valido a un `WasmboxLimits`
    // C-side, leggero per valore (tutte le copie).
    let limits_ref = unsafe { limits.as_ref() };
    let config = limits_from_c(limits_ref);

    let engine = match SandboxEngine::new(&bytes, config) {
        Ok(e) => e,
        Err(e) => {
            // init fallito: null; nessun modo per riportare la causa
            // (la engine non esiste). Codice runtime-side spot-on: log stderr.
            eprintln!("wasmbox_ffi: inizializzazione fallita: {e}");
            return std::ptr::null_mut();
        }
    };
    Box::into_raw(Box::new(WasmboxEngine {
        engine,
        last_error: Cell::new(None),
        last_code: Cell::new(0),
    }))
}

/// Libera una engine creata con [`wasmbox_engine_new`]. NULL è un no-op.
#[no_mangle]
pub unsafe extern "C" fn wasmbox_engine_free(engine: *mut WasmboxEngine) {
    if !engine.is_null() {
        // SAFETY: proveniva da `Box::into_raw` in `wasmbox_engine_new`.
        drop(Box::from_raw(engine));
    }
}

#[no_mangle]
pub unsafe extern "C" fn wasmbox_engine_run(
    engine: *mut WasmboxEngine,
    input: *const u8,
    input_len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> u32 {
    // --- 1. Validazione argomenti ---
    if engine.is_null() || out.is_null() || out_len.is_null() {
        return WasmboxStatus::InvalidArg as u32;
    }
    // SAFETY: `engine` proveniva da `Box::into_raw` e nessun'altra thread
    // lo sta usando ora: così è documentato, la engine NON è thread-safe.
    let e = unsafe { &*engine };
    if input.is_null() && input_len != 0 {
        return WasmboxStatus::InvalidArg as u32;
    }
    let input_bytes = if input_len == 0 {
        Vec::new()
    } else {
        // SAFETY: input non-null, input_len byte leggibili dal chiamante.
        unsafe { std::slice::from_raw_parts(input, input_len) }.to_vec()
    };

    // --- 2. Run ---
    let mut handler = EchoHandler;
    match e.engine.run(&input_bytes, &mut handler) {
        Ok(output) => {
            e.last_error.set(None);
            e.last_code.set(0);
            // Alloca il buffer di output nel alloc del crate (Rust global
            // allocator,Marker : libera con wasmbox_buffer_free, non con free())
            let mut buf = output.into_boxed_slice();
            let ptr = buf.as_mut_ptr();
            let len = buf.len();
            // La Box non deve deallocare: usa ManuallyDrop-like via into_raw.
            std::mem::forget(buf);
            // SAFETY: il chiamante ha passato out/out_len validi.
            unsafe {
                *out = ptr;
                *out_len = len;
            }
            WasmboxStatus::Ok as u32
        }
        Err(err) => {
            let code = cli_exit_code(&err);
            e.last_error.set(Some(err.to_string()));
            e.last_code.set(code);
            // SAFETY: scrive solo se out/out_len non-null (già validati sopra).
            unsafe {
                *out = std::ptr::null_mut();
                *out_len = 0;
            }
            WasmboxStatus::RunFailed as u32
        }
    }
}

/// Libera un buffer restituito da [`wasmbox_engine_run`]. NULL/len 0 = no-op.
///
/// NB: NON usare `free()` C / `free()` Python-ctypes: la memoria appartiene
/// all'allocator Rust dell'oggetto restituito.
#[no_mangle]
pub unsafe extern "C" fn wasmbox_buffer_free(buf: *mut u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    // SAFETY: il buffer proviene da `into_boxed_slice` + `mem::forget`
    // in `wasmbox_engine_run` con esattamente questa len.
    drop(Vec::from_raw_parts(buf, len, len));
}

/// run fallita. Indirizzo: allocato con `CString::into_raw` e NON MAI
/// liberato — v0.7 fattoreleak, documentato/innocuo: un piccolo leak per
/// errore; nessun leak su run riuscita.
/// SAFETY: `engine` valido (creato da `wasmbox_engine_new`, NON
/// già liberata) oppure NULL.
#[no_mangle]
pub unsafe extern "C" fn wasmbox_last_error(engine: *mut WasmboxEngine) -> *const c_char {
    if engine.is_null() {
        return std::ptr::null();
    }
    let last = unsafe { (*engine).last_error.take() };
    match last.and_then(|s| std::ffi::CString::new(s).ok()) {
        Some(c) => c.into_raw() as *const c_char,
        None => std::ptr::null(),
    }
}

/// SAFETY: `engine` valido (creato da `wasmbox_engine_new`, NON già
/// liberata) oppure NULL.
#[no_mangle]
pub unsafe extern "C" fn wasmbox_last_error_code(engine: *mut WasmboxEngine) -> u32 {
    if engine.is_null() {
        return 0;
    }
    unsafe { (*engine).last_code.get() }
}

// ---------------------------------------------------------------- tests ----

#[cfg(test)]
mod tests {
    use super::*;

    /// Guest wasm32 reale (compilato da scripts/build.sh). Assente → skip
    /// pulito (return, MAI panic: l'artefatto è opzionale in CI).
    fn guest_echo_bytes() -> Option<Vec<u8>> {
        std::fs::read("../../target/wasm32-unknown-unknown/release/guest_echo.wasm").ok()
    }

    fn call_run(engine: *mut WasmboxEngine, input: &[u8]) -> (u32, Option<Vec<u8>>) {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let st = unsafe {
            wasmbox_engine_run(
                engine,
                input.as_ptr(),
                input.len(),
                &mut out_ptr,
                &mut out_len,
            )
        };
        let buf = if st == WasmboxStatus::Ok as u32 && !out_ptr.is_null() {
            let v = unsafe { std::slice::from_raw_parts(out_ptr, out_len) }.to_vec();
            unsafe { wasmbox_buffer_free(out_ptr, out_len) };
            Some(v)
        } else {
            // Su fallimento out deve essere null e len 0 (contratto §3.2).
            assert!(out_ptr.is_null() && out_len == 0, "su errore out=null/0");
            None
        };
        (st, buf)
    }

    /// Round-trip eco end-to-end su C-ABI: status 0, prefisso echo_result.
    #[test]
    fn echo_roundtrip_c_abi() {
        let Some(wasm) = guest_echo_bytes() else {
            eprintln!("guest_echo.wasm assente: skip");
            return;
        };
        let engine = unsafe { wasmbox_engine_new(wasm.as_ptr(), wasm.len(), std::ptr::null()) };
        assert!(!engine.is_null());
        let (st, out) = call_run(engine, b"ffi-test");
        assert_eq!(st, WasmboxStatus::Ok as u32);
        assert_eq!(unsafe { wasmbox_last_error_code(engine) }, 0);
        let out = out.unwrap();
        assert!(out.starts_with(b"echo_result:"), "out={out:?}");
        assert!(std::str::from_utf8(&out).unwrap().contains("ffi-test"));
        unsafe { wasmbox_engine_free(engine) };
    }

    /// Argomenti illegali → status 1, out=null/0; new su wasm non-leggibile.
    #[test]
    fn invalid_args_rejected() {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let invalid = WasmboxStatus::InvalidArg as u32;
        assert_eq!(
            unsafe {
                wasmbox_engine_run(
                    std::ptr::null_mut(),
                    b"x".as_ptr(),
                    1,
                    &mut out_ptr,
                    &mut out_len,
                )
            },
            invalid
        );
        assert!(out_ptr.is_null() && out_len == 0);
        assert_eq!(WasmboxStatus::RunFailed as u32, 3); // contratto del piano
    }

    /// wasm invalido → new restituisce null (init fallito), senza panic.
    #[test]
    fn invalid_wasm_init_fails_cleanly() {
        let e = unsafe { wasmbox_engine_new(b"non-wasm-bytes".as_ptr(), 13, std::ptr::null()) };
        assert!(e.is_null());
    }
}

//! Guest Wasm di esempio.
//!
//! Compilato per `wasm32-unknown-unknown` come `cdylib`, espone il contratto
//! ABI di wasmbox (`guest_alloc`, `guest_free`, `guest_run`) e importa la
//! sola funzione host `env::ask`.

use std::alloc::{alloc, dealloc, Layout};

#[link(wasm_import_module = "env")]
extern "C" {
    fn ask(req_ptr: i32, req_len: i32) -> i64;
}

const ALIGN: usize = 8;

/// Alloca `len` byte. `len == 0` restituisce un puntatore non-nullo
/// (dangling): evita UB lato host quando si alloca un buffer vuoto.
#[no_mangle]
pub extern "C" fn guest_alloc(len: u32) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    let layout = match Layout::from_size_align(len as usize, ALIGN) {
        Ok(l) => l,
        Err(_) => return std::ptr::null_mut(),
    };
    unsafe { alloc(layout) }
}

/// Libera `len` byte al puntatore `ptr`. No-op per buffer vuoti o null.
///
/// `unsafe` perché dereferenzia un puntatore grezzo: la firma wasm
/// esportata (`(i32 i32)`) resta identica, l'`unsafe` è un concetto solo Rust.
///
/// # Safety
///
/// `ptr` deve essere un puntatore restituito da `guest_alloc` con la stessa
/// `len`, oppure null/`len == 0` (no-op).
#[no_mangle]
pub unsafe extern "C" fn guest_free(ptr: *mut u8, len: u32) {
    if len == 0 || ptr.is_null() {
        return;
    }
    let layout = match Layout::from_size_align(len as usize, ALIGN) {
        Ok(l) => l,
        Err(_) => return,
    };
    unsafe { dealloc(ptr, layout) }
}

/// Entry point: inoltra l'input all'host come `inspect:<input>`, poi compone
/// `echo_result:<risposta host>` e lo restituisce come `(ptr << 32) | len`.
///
/// `unsafe` perché dereferenzia il puntatore di input: la firma wasm
/// esportata (`(i32 i32) -> i64`) resta identica.
///
/// # Safety
///
/// `in_ptr` deve puntare a `in_len` byte leggibili nella memoria del guest
/// (come scritti dall'host), oppure `in_len == 0`.
#[no_mangle]
pub unsafe extern "C" fn guest_run(in_ptr: *mut u8, in_len: u32) -> u64 {
    let input = if in_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(in_ptr, in_len as usize) }
    };

    // Costruisce la richiesta: "inspect:" + input
    let mut req = b"inspect:".to_vec();
    req.extend_from_slice(input);

    // Chiamata all'host
    let packed = unsafe { ask(req.as_ptr() as i32, req.len() as i32) };
    let resp_ptr = (packed >> 32) as u32;
    let resp_len = (packed & 0xFFFF_FFFF) as u32;

    let host_resp = if resp_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(resp_ptr as *const u8, resp_len as usize) }
    };

    // Output: "echo_result:" + risposta
    let mut out = b"echo_result:".to_vec();
    out.extend_from_slice(host_resp);

    // Libera il buffer di risposta allocato dall'host tramite il nostro
    // `guest_alloc`: senza questa chiamata ogni `ask` consuma memoria nel
    // guest fino a esaurirla.
    if resp_len > 0 && resp_ptr != 0 {
        // SAFETY: resp_ptr/resp_len provengono dal buffer allocato dall'host
        // tramite il nostro guest_alloc.
        guest_free(resp_ptr as *mut u8, resp_len);
    }

    let out_len = out.len() as u32;
    let out_ptr = guest_alloc(out_len);
    if !out_ptr.is_null() && out_len > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(out.as_ptr(), out_ptr, out_len as usize);
        }
    }

    ((out_ptr as u64) << 32) | (out_len as u64)
}

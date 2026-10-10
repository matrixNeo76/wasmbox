//! Guest dello scenario B: protocollo "tool" sopra `ask`.
//!
//! Imposta l'input come richiesta tool:
//!   `<tool>:<argomento>`      — es. `soma:7x35`
//! L'host risponde `tool_result:<…>`. Il guest restituisce la risposta
//! com'è: la logica di dominio sta TUTTA nell'host (blueprint: il guest
//! non interpreta nulla, solo trasporta).
//!
//! Uso (dalla root del repo):
//! ```sh
//! cargo run -p scenario-tool-handler -- \
//!   target/wasm32-unknown-unknown/release/scenario_tool_guest.wasm "soma:7x35"
//! ```

use std::alloc::{alloc, dealloc, Layout};

#[link(wasm_import_module = "env")]
extern "C" {
    fn ask(req_ptr: i32, req_len: i32) -> i64;
}

const ALIGN: usize = 8;

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

    // Il guest manda l'input COM'È come richiesta: il protocollo
    // (`tool:<…>`) lo deciderà mai l'host — qui solo trasporto opaco.
    let packed = unsafe { ask(input.as_ptr() as i32, input.len() as i32) };
    let resp_ptr = (packed >> 32) as u32;
    let resp_len = (packed & 0xFFFF_FFFF) as u32;

    let host_resp = if resp_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(resp_ptr as *const u8, resp_len as usize) }
    };

    let out = host_resp.to_vec();

    // Libera il buffer della risposta (allocato dall'host via guest_alloc):
    // senza questa chiamata la memoria cresce ad ogni ask fino a esaurirla.
    if resp_len > 0 && resp_ptr != 0 {
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

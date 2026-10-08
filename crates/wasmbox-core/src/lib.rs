//! # wasmbox-core
//!
//! Esecutore di codice WebAssembly isolato e ad alte prestazioni.
//!
//! Principio architetturale: `wasmbox-core` non sa nulla del mondo esterno.
//! Sa solo eseguire Wasm con limiti di risorsa (fuel, memoria, timeout) e
//! inoltrare richieste opache a un [`HostHandler`] fornito dall'host.
//!
//! Il guest può importare una sola funzione, `env::ask(req_ptr, req_len) -> i64`,
//! e deve esportare `memory`, `guest_alloc`, `guest_free` e `guest_run`.
//! Il runtime non interpreta né la richiesta né la risposta: sono bytes opachi.

pub mod config;
pub mod engine;
pub mod error;
pub mod memory;

pub use config::SandboxConfig;
pub use engine::{HostHandler, SandboxEngine};
pub use error::{HostError, SandboxError};

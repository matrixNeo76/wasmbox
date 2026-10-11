//! Errori del layer WASI — spec wasi-plan.md §4 punto 5.
//!
//! Definiti qui (MAI in wasmbox-core): `error.rs` del core non cambia.

use thiserror::Error;

/// Errore del layer WASI opt-in.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WasiError {
    /// Il guest importa una syscall WASI non concessa dalle capability con
    /// cui l'engine è stato costruito (strict mode). Il payload descrive
    /// la syscall o il gruppo negato.
    #[error("capability WASI negata: {0}")]
    Denied(String),

    /// La directory preopen specificata dall'host non esiste o non è una
    /// directory leggibile.
    #[error("preopen non valido ({0}): la dir host non esiste o non è una directory")]
    PreopenInvalid(String),
}

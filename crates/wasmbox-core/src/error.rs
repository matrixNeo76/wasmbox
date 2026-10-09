use thiserror::Error;

/// Errore restituito dall'host handler associato a una run.
///
/// `Clone` è necessario perché `map_guest_error` riceve un `&wasmtime::Error`
/// e deve produrre un `SandboxError` owned senza poter muovere il valore:
/// la clonazione è l'unica via. Contiene solo `String` e una variante unit,
/// quindi è economico.
#[derive(Debug, Clone, Error)]
pub enum HostError {
    #[error("errore generico dell'host: {0}")]
    Custom(String),
    #[error("budget di chiamate host esaurito")]
    BudgetExceeded,
}

/// Errori tipizzati prodotti dalla sandbox.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("inizializzazione Wasm fallita: {0}")]
    EngineInit(String),
    #[error("compilazione o modulo non valido: {0}")]
    InvalidWasm(String),
    #[error("accesso alla memoria Wasm fallito: {0}")]
    MemoryAccess(String),
    #[error("esportazione richiesta assente nel guest: {0}")]
    MissingExport(String),
    #[error("fuel esaurito: limite di computazione superato")]
    FuelExhausted,
    #[error("timeout wall-clock superato (epoch interrupt)")]
    Timeout,
    #[error("allocazione guest fallita: memoria insufficiente (out of memory)")]
    GuestOutOfMemory,
    #[error("superato il limite massimo di chiamate 'ask' ({0})")]
    AskLimitExceeded(u32),
    #[error("payload 'ask' oltre il limite: {size} byte (max {max})")]
    PayloadTooLarge { size: usize, max: usize },
    #[error("errore restituito dall'host handler: {0}")]
    Host(#[from] HostError),
    #[error("guest trap / errore di esecuzione: {0}")]
    Execution(String),
}

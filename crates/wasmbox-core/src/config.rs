use std::path::PathBuf;
use std::time::Duration;

/// Granularità del ticker epoch e del deadline di timeout.
///
/// Usata sia come `sleep` del ticker unico per engine sia come unità di
/// calcolo del deadline: `ticks = (timeout / EPOCH_TICK).max(1)`.
pub(crate) const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Configurazione dei limiti di una sandbox.
///
/// Tutti i limiti sono opzionali o espliciti: nessun valore è dedotto dal
/// codice guest. Il default è pensato per essere sicuro per default
/// (fuel, timeout e limiti di chiamata attivi).
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Massimo numero di istruzioni eseguibili. `None` disabilita il fuel.
    pub max_fuel: Option<u64>,
    /// Timeout wall-clock della run. `None` disabilita l'epoch interruption.
    pub epoch_timeout: Option<Duration>,
    /// Tetto esplicito della memoria lineare del guest, in byte.
    pub max_memory_bytes: usize,
    /// Dimensione del pooling allocator (memorie, stack, istanze).
    pub pool_size: u32,
    /// Directory opzionale per la cache `.cwasm` precompilata.
    pub cache_dir: Option<PathBuf>,
    /// Massimo numero di chiamate `ask` per run.
    pub max_ask_calls: u32,
    /// Massima dimensione (byte) di una richiesta o risposta `ask`.
    pub max_ask_payload_bytes: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_fuel: Some(1_000_000_000),
            epoch_timeout: Some(Duration::from_millis(1000)),
            max_memory_bytes: 16 * 1024 * 1024,
            pool_size: 8,
            cache_dir: None,
            max_ask_calls: 1024,
            max_ask_payload_bytes: 1024 * 1024,
        }
    }
}

//! Libreria `wasmbox_mcp`: i moduli del server MCP sono qui così i test
//! d'integrazione importano il dispatch senza duplicare codice; il
//! binario (`main.rs`) è solo il loop stdio che usa questa lib.

pub mod envelope;
pub mod protocol;
pub mod runner;
pub mod server;

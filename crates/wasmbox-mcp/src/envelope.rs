//! Envelope di run (docs/agent-loop-plan.md §2.3): la busta compatta che
//! l'host agente riceve a ogni `tools/call`.
//!
//! Regole:
//! - campi e ordine FISSI (`ok, error, message, output, ask_calls,
//!   duration_ms, output_truncated`) — stabili tra versioni (KV-cache);
//! - `output` capato (default 4 KiB, max 1 MiB via `output_bytes_max`):
//!   oltre → head+tail + `output_truncated: true`;
//! - `message` capato a 256 byte;
//! - errori snake_case identici alla CLI (`fuel_exhausted`, `timeout`, …).

use wasmbox_core::SandboxError;

/// Default del cap output (4 KiB), come da spec §2.2.
pub const OUTPUT_CAP_DEFAULT: usize = 4 * 1024;
/// Massimo assoluto richiedibile (`output_bytes_max`): 1 MiB.
pub const OUTPUT_CAP_MAX: usize = 1024 * 1024;
/// Cap sul messaggio di errore.
pub const MESSAGE_CAP: usize = 256;

/// Envelope di risultato. `output` esiste solo su ok; `error`/`message`
/// solo su fallimento. `output_truncated` è false quando la run è ok e
/// il cap non è stato raggiunto (o su errore).
#[derive(Debug, Clone, PartialEq)]
pub struct RunEnvelope {
    pub ok: bool,
    pub error: Option<&'static str>,
    pub message: Option<String>,
    pub output: Option<String>,
    pub ask_calls: u32,
    pub duration_ms: u64,
    pub output_truncated: bool,
}

impl RunEnvelope {
    /// Serializza l'envelope in una riga JSON con ordine di campi fisso.
    pub fn to_json(&self) -> String {
        use crate::protocol::escape_str;
        let mut out = String::with_capacity(96);
        out.push_str("{\"ok\":");
        out.push_str(if self.ok { "true" } else { "false" });
        if let Some(e) = self.error {
            out.push_str(",\"error\":\"");
            out.push_str(e); // snake_case statico, senza escape interne
            out.push('"');
            if let Some(m) = &self.message {
                out.push_str(",\"message\":");
                out.push_str(&escape_str(&cap_str(m, MESSAGE_CAP)));
            }
        }
        if let Some(o) = &self.output {
            out.push_str(",\"output\":");
            out.push_str(&escape_str(o));
        }
        out.push_str(",\"ask_calls\":");
        out.push_str(&self.ask_calls.to_string());
        out.push_str(",\"duration_ms\":");
        out.push_str(&self.duration_ms.to_string());
        out.push_str(",\"output_truncated\":");
        out.push_str(if self.output_truncated {
            "true"
        } else {
            "false"
        });
        out.push('}');
        out
    }

    /// Envelope di successo (con cap head+tail sull'output).
    pub fn ok(output: &[u8], ask_calls: u32, duration_ms: u64, cap: usize) -> Self {
        let (text, truncated) = cap_output(output, cap);
        Self {
            ok: true,
            error: None,
            message: None,
            output: Some(text),
            ask_calls,
            duration_ms,
            output_truncated: truncated,
        }
    }

    /// Envelope di fallimento: errore tipizzato, messaggio capato.
    pub fn fail(e: &SandboxError, ask_calls: u32, duration_ms: u64) -> Self {
        Self {
            ok: false,
            error: Some(error_kind(e)),
            message: Some(e.to_string()),
            output: None,
            ask_calls,
            duration_ms,
            output_truncated: false,
        }
    }
}

/// Tronca al cap con head+tail (come il Bash tool di Claude Code):
/// 3/4 all'inizio, 1/4 alla fine, separatore esplicito in mezzo.
/// Non spezza mai un carattere UTF-8 multi-byte (toeggia al bordo).
fn cap_output(bytes: &[u8], cap: usize) -> (String, bool) {
    if bytes.len() <= cap {
        let lossy = String::from_utf8_lossy(bytes).into_owned();
        return (lossy, false);
    }
    if cap < 32 {
        // Cap troppo piccolo per head+tail sensato: solo head.
        let head = &bytes[..cap];
        return (String::from_utf8_lossy(head).into_owned(), true);
    }
    let head_len = safe_char_boundary(bytes, cap * 3 / 4);
    let tail_len = safe_char_boundary_rev(bytes, cap / 4);
    let head = &bytes[..head_len];
    let tail = &bytes[bytes.len() - tail_len..];
    let omitted = bytes.len() - head_len - tail_len;
    let mut s = String::with_capacity(cap + 64);
    s.push_str(&String::from_utf8_lossy(head));
    s.push_str(&format!("\n…[{omitted} byte omessi]…\n"));
    s.push_str(&String::from_utf8_lossy(tail));
    (s, true)
}

fn cap_str(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let cut = safe_char_boundary(s.as_bytes(), cap);
    format!("{}…", &s[..cut])
}

/// Indice di taglio valido (non spezza UTF-8) partendo da `n` verso sinistra.
fn safe_char_boundary(bytes: &[u8], n: usize) -> usize {
    let mut i = n.min(bytes.len());
    while i > 0 && (bytes[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    i
}

fn safe_char_boundary_rev(bytes: &[u8], n: usize) -> usize {
    let mut i = n.min(bytes.len());
    while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 && i > 0 {
        i -= 1;
    }
    let mut j = i;
    while j < bytes.len() && (bytes[j] & 0xC0) == 0x80 {
        j += 1;
    }
    j - i
}

/// snake_case della CLI/SKILL.md, unica mappatura (fonte: wasmbox-cli).
pub fn error_kind(e: &SandboxError) -> &'static str {
    match e {
        SandboxError::FuelExhausted => "fuel_exhausted",
        SandboxError::Timeout => "timeout",
        SandboxError::GuestOutOfMemory => "guest_out_of_memory",
        SandboxError::AskLimitExceeded(_) | SandboxError::PayloadTooLarge { .. } => {
            "ask_limit_or_payload"
        }
        SandboxError::Host(_) => "host_handler_error",
        SandboxError::MissingExport(_) => "missing_export",
        SandboxError::InvalidWasm(_) => "invalid_wasm",
        SandboxError::EngineInit(_)
        | SandboxError::MemoryAccess(_)
        | SandboxError::Execution(_) => "execution_or_engine_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_ok_shape_and_field_order() {
        let env = RunEnvelope::ok(b"hello", 0, 2, OUTPUT_CAP_DEFAULT);
        assert_eq!(
            env.to_json(),
            r#"{"ok":true,"output":"hello","ask_calls":0,"duration_ms":2,"output_truncated":false}"#
        );
    }

    #[test]
    fn envelope_is_ascii_safe_escapes_quotes() {
        let env = RunEnvelope::ok(b"say \"hi\"\nok", 1, 3, OUTPUT_CAP_DEFAULT);
        let j = env.to_json();
        assert!(j.contains(r#"say \"hi\"\nok"#));
        assert!(!j.contains('\n')); // una riga sola
    }

    #[test]
    fn truncation_head_tail_marks_flag() {
        let big: Vec<u8> = (0..10_000u32).map(|i| b'a' + (i % 26) as u8).collect();
        let env = RunEnvelope::ok(&big, 0, 1, OUTPUT_CAP_DEFAULT);
        assert!(env.output_truncated);
        let j = env.to_json();
        assert!(j.contains("byte omessi"));
        // Envelope resta compatto: ben sotto 6 KiB anche con output enorme.
        assert!(j.len() < 6 * 1024, "envelope {} byte", j.len());
    }

    #[test]
    fn truncation_respects_utf8_boundary() {
        let s = "é".repeat(3000); // 2 byte per char
        let env = RunEnvelope::ok(s.as_bytes(), 0, 0, 2048);
        assert!(env.output_truncated);
        // L'output troncato è sempre UTF-8 valido.
        assert!(String::from_utf8(env.output.clone().unwrap().into_bytes()).is_ok());
    }

    #[test]
    fn failure_envelope_matches_cli_snake_case() {
        let env = RunEnvelope::fail(&SandboxError::FuelExhausted, 0, 500);
        assert_eq!(
            env.to_json(),
            r#"{"ok":false,"error":"fuel_exhausted","message":"fuel esaurito: limite di computazione superato","ask_calls":0,"duration_ms":500,"output_truncated":false}"#
        );
    }
}

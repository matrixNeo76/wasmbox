//! Base64 (RFC 4648) scritto a mano: il crate resta zero-dipendenze oltre a
//! `wasmbox-core` (divieto: niente serde_json/base64 nel workspace; il
//! protocollo è documentato in docs/extension-plan.md §2).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a') as u32 + 26),
        b'0'..=b'9' => Some((c - b'0') as u32 + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodifica standard; accetta stringhe senza padding e 1/2 `=` finali.
/// Err su caratteri estranei.
pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    // Non-alphabet ammesso solo come '=' di padding FINALE (max 2).
    let trimmed = s.trim_end_matches('=');
    let padding_len = s.len() - trimmed.len();
    if padding_len > 2 {
        return Err("base64: troppo padding".into());
    }
    for &c in &s.as_bytes()[..trimmed.len()] {
        if val(c).is_none() {
            return Err(format!("base64: carattere invalido {c:?}"));
        }
    }
    if trimmed.len() % 4 == 1 {
        return Err("base64: lunghezza non valida".into());
    }
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4 + 3);
    for chunk in trimmed.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c).unwrap_or(0) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ascii() {
        assert_eq!(encode(b"hello"), "aGVsbG8=");
        assert_eq!(decode("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn roundtrip_binary() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(&encode(&data)).unwrap(), data);
    }

    #[test]
    fn empty_ok() {
        assert_eq!(encode(b""), "");
        assert_eq!(decode("").unwrap(), b"");
    }

    #[test]
    fn rejects_bad_chars() {
        assert!(decode("####").is_err());
    }
}

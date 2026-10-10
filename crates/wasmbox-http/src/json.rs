//! JSON minimale scritto a mano per il protocollo della rotta `POST /run`
//! (docs/extension-plan.md §2.2). Niente serde nel workspace.
//!
//! Supporto volutamente limitato: oggetti, stringhe (con escape basilare),
//! interi, null. Basta al protocollo, resta piccolo e testabile.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Int(i64),
    Str(String),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    #[allow(dead_code)] // usata dai test; parte del contratto del modulo
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<Json, String> {
    let mut pos = 0usize;
    let v = value(input.as_bytes(), &mut pos)?;
    skip_ws(input.as_bytes(), &mut pos);
    if pos != input.len() {
        return Err(format!(
            "json: contenuto extra dopo l'oggetto (offset {pos})"
        ));
    }
    Ok(v)
}

fn skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && matches!(b[*pos], b' ' | b'\t' | b'\r' | b'\n') {
        *pos += 1;
    }
}

fn value(b: &[u8], pos: &mut usize) -> Result<Json, String> {
    skip_ws(b, pos);
    match b.get(*pos).copied() {
        Some(b'{') => object(b, pos),
        Some(b'"') => Ok(Json::Str(string(b, pos)?)),
        Some(b'n') if b[*pos..].starts_with(b"null") => {
            *pos += 4;
            Ok(Json::Null)
        }
        Some(c) if c == b'-' || c.is_ascii_digit() => number(b, pos),
        other => Err(format!("json: token inatteso {other:?} a offset {pos}")),
    }
}

fn object(b: &[u8], pos: &mut usize) -> Result<Json, String> {
    *pos += 1; // '{'
    let mut map = BTreeMap::new();
    skip_ws(b, pos);
    if b.get(*pos) == Some(&b'}') {
        *pos += 1;
        return Ok(Json::Obj(map));
    }
    loop {
        skip_ws(b, pos);
        if b.get(*pos) != Some(&b'"') {
            return Err(format!("json: attesa chiave a offset {pos}"));
        }
        let key = string(b, pos)?;
        skip_ws(b, pos);
        if b.get(*pos) != Some(&b':') {
            return Err(format!("json: atteso ':' a offset {pos}"));
        }
        *pos += 1;
        let v = value(b, pos)?;
        map.insert(key, v);
        skip_ws(b, pos);
        match b.get(*pos) {
            Some(b',') => *pos += 1,
            Some(b'}') => {
                *pos += 1;
                return Ok(Json::Obj(map));
            }
            other => return Err(format!("json: atteso ',' o '}}', trovato {other:?}")),
        }
    }
}

fn string(b: &[u8], pos: &mut usize) -> Result<String, String> {
    *pos += 1; // '"'
    let mut out = String::new();
    while *pos < b.len() {
        match b[*pos] {
            b'"' => {
                *pos += 1;
                return Ok(out);
            }
            b'\\' => {
                *pos += 1;
                match b.get(*pos).copied() {
                    Some(b'n') => out.push('\n'),
                    Some(b't') => out.push('\t'),
                    Some(b'r') => out.push('\r'),
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(_) => return Err("json: escape non supportato".into()),
                    None => return Err("json: escape troncato".into()),
                }
                *pos += 1;
            }
            c if c < 0x20 => return Err("json: byte di controllo nella stringa".into()),
            c => {
                // Byte ASCII singolo o inizio di UTF-8 multi-byte: copia 1 byte
                // (l'input arriva da read_to_string già validato UTF-8).
                out.push(c as char);
                if c >= 0x80 {
                    return Err("json: stringhe non-ASCII non supportate dal protocollo".into());
                }
                *pos += 1;
            }
        }
    }
    Err("json: stringa non chiusa".into())
}

fn number(b: &[u8], pos: &mut usize) -> Result<Json, String> {
    let start = *pos;
    if b[*pos] == b'-' {
        *pos += 1;
    }
    while *pos < b.len() && b[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start || (*pos == start + 1 && b[start] == b'-') {
        return Err("json: numero malformato".into());
    }
    // Rifiuta esplicitamente esponenti/fraction (non serviti dal protocollo)
    if *pos < b.len() && matches!(b[*pos], b'.' | b'e' | b'E') {
        return Err("json: accettati solo interi".into());
    }
    let negative = b[start] == b'-';
    let digits = if negative {
        &b[start + 1..*pos]
    } else {
        &b[start..*pos]
    };
    // Magnitudine su u128: gestisce sia i positivi che |i64::MIN|.
    let magnitude = digits
        .iter()
        .try_fold(0u128, |acc, &c| {
            acc.checked_mul(10)
                .and_then(|a| a.checked_add(u128::from(c - b'0')))
        })
        .ok_or_else(|| "json: intero fuori range".to_string())?;
    let sign_bound = if negative {
        i64::MAX as u128 + 1
    } else {
        i64::MAX as u128
    };
    if magnitude > sign_bound {
        return Err("json: intero fuori range".into());
    }
    let value = if negative {
        (-(magnitude as i128)) as i64
    } else {
        magnitude as i64
    };
    Ok(Json::Int(value))
}

/// Serializzazione (solo ciò che il server risponde).
pub fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_request() {
        let j = parse(r#"{"guest":"QUJD","input":null}"#).unwrap();
        assert_eq!(j.get("guest").unwrap().as_str(), Some("QUJD"));
        assert_eq!(j.get("input"), Some(&Json::Null));
    }

    #[test]
    fn parse_nested_limits() {
        let j = parse(r#"{"limits":{"max_fuel":123,"epoch_timeout_ms":null}}"#).unwrap();
        assert_eq!(
            j.get("limits").unwrap().get("max_fuel").unwrap().as_i64(),
            Some(123)
        );
    }

    #[test]
    fn rejects_trailing() {
        assert!(parse("{} {}").is_err());
    }

    #[test]
    fn rejects_fraction() {
        assert!(parse(r#"{"a":1.5}"#).is_err());
    }

    #[test]
    fn escape_roundtrip() {
        let s = "a\"b\\c\n";
        let j = parse(&format!("{{\"k\":{}}}", escape_str(s))).unwrap();
        assert_eq!(j.get("k").unwrap().as_str(), Some(s));
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(parse("-42").unwrap(), Json::Int(-42));
    }
}

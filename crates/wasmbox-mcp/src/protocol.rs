//! Subset del protocollo MCP (spec `2026-07-28`) su transport stdio
//! (docs/agent-loop-plan.md §2.4): `initialize`, `notifications/initialized`
//! (notifica), `tools/list`, `tools/call`.
//!
//! JSON-RPC 2.0 scritto a mano (zero dipendenze, stessa disciplina di
//! `wasmbox-http/src/json.rs`): oggetti, stringhe, bool, interi, null.
//! Volutamente limitato: basta al subset, resta piccolo e testabile.

/// Versione del protocollo MCP dichiarata (spec 2026-07-28).
pub const PROTOCOL_VERSION: &str = "2026-07-28";

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
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

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn obj(pairs: &[(&str, Json)]) -> Json {
        Json::Obj(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }
}

pub fn parse(input: &str) -> Result<Json, String> {
    let mut pos = 0usize;
    let v = value(input.as_bytes(), &mut pos)?;
    skip_ws(input.as_bytes(), &mut pos);
    if pos != input.len() {
        return Err(format!(
            "json: contenuto extra dopo il valore (offset {pos})"
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
        Some(b'[') => array(b, pos),
        Some(b'"') => Ok(Json::Str(string(b, pos)?)),
        Some(b't') if b[*pos..].starts_with(b"true") => {
            *pos += 4;
            Ok(Json::Bool(true))
        }
        Some(b'f') if b[*pos..].starts_with(b"false") => {
            *pos += 5;
            Ok(Json::Bool(false))
        }
        Some(b'n') if b[*pos..].starts_with(b"null") => {
            *pos += 4;
            Ok(Json::Null)
        }
        Some(c) if c == b'-' || c.is_ascii_digit() => number(b, pos),
        other => Err(format!("json: token inatteso {other:?} a offset {pos}")),
    }
}

fn array(b: &[u8], pos: &mut usize) -> Result<Json, String> {
    *pos += 1; // '['
    let mut items = Vec::new();
    skip_ws(b, pos);
    if b.get(*pos) == Some(&b']') {
        *pos += 1;
        return Ok(Json::Arr(items));
    }
    loop {
        items.push(value(b, pos)?);
        skip_ws(b, pos);
        match b.get(*pos) {
            Some(b',') => *pos += 1,
            Some(b']') => {
                *pos += 1;
                return Ok(Json::Arr(items));
            }
            other => return Err(format!("json: atteso ',' o ']', trovato {other:?}")),
        }
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
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{c}'),
                    Some(b'u') => {
                        // \uXXXX: decodifica BMP; coppie surrogate non srvite
                        // (l'envelope è ASCII-safe: i payload viaggiano base64).
                        let hex = b
                            .get(*pos + 1..*pos + 5)
                            .ok_or_else(|| "json: escape \\u troncato".to_string())?;
                        let hex = std::str::from_utf8(hex)
                            .map_err(|_| "json: escape \\u non-ASCII".to_string())?;
                        let cp = u32::from_str_radix(hex, 16)
                            .map_err(|_| "json: escape \\u malformato".to_string())?;
                        if (0xD800..0xE000).contains(&cp) {
                            return Err("json: surrogate non supportate".into());
                        }
                        out.push(char::from_u32(cp).ok_or("json: codepoint invalido")?);
                        *pos += 4;
                    }
                    Some(_) => return Err("json: escape non supportato".into()),
                    None => return Err("json: escape troncato".into()),
                }
                *pos += 1;
            }
            c if c < 0x20 => return Err("json: byte di controllo nella stringa".into()),
            c => {
                // ASCII: 1 byte. UTF-8 multi-byte: copia il byte (input arriva
                // da read validato UTF-8, quindi i byte successivi seguono).
                if c >= 0x80 {
                    let start = *pos;
                    let len = utf8_len(c);
                    let chunk = b
                        .get(start..start + len)
                        .ok_or_else(|| "json: UTF-8 troncato".to_string())?;
                    let s = std::str::from_utf8(chunk)
                        .map_err(|_| "json: UTF-8 invalido".to_string())?;
                    out.push_str(s);
                    *pos += len;
                } else {
                    out.push(c as char);
                    *pos += 1;
                }
            }
        }
    }
    Err("json: stringa non chiusa".into())
}

fn utf8_len(first: u8) -> usize {
    if first >= 0xF0 {
        4
    } else if first >= 0xE0 {
        3
    } else {
        2
    }
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
    if *pos < b.len() && matches!(b[*pos], b'.' | b'e' | b'E') {
        return Err("json: accettati solo interi".into());
    }
    let negative = b[start] == b'-';
    let digits = if negative {
        &b[start + 1..*pos]
    } else {
        &b[start..*pos]
    };
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
    let v = if negative {
        (-(magnitude as i128)) as i64
    } else {
        magnitude as i64
    };
    Ok(Json::Int(v))
}

/// Serializzazione (risposte del server + envelope).
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

fn render(v: &Json, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Int(n) => out.push_str(&n.to_string()),
        Json::Str(s) => out.push_str(&escape_str(s)),
        Json::Arr(items) => {
            out.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                render(it, out);
            }
            out.push(']');
        }
        Json::Obj(map) => {
            out.push('{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&escape_str(k));
                out.push(':');
                render(val, out);
            }
            out.push('}');
        }
    }
}

pub fn to_string(v: &Json) -> String {
    let mut out = String::new();
    render(v, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bool_array_null() {
        let j = parse(r#"{"ok":true,"l":[1,2,null],"s":"a\"b"}"#).unwrap();
        assert_eq!(
            j,
            Json::obj(&[
                ("ok", Json::Bool(true)),
                ("l", Json::Arr(vec![Json::Int(1), Json::Int(2), Json::Null])),
                ("s", Json::Str("a\"b".into())),
            ])
        );
        assert_eq!(to_string(&j), r#"{"l":[1,2,null],"ok":true,"s":"a\"b"}"#);
    }

    #[test]
    fn unicode_escape() {
        let j = parse(r#""caf\u00e9""#).unwrap();
        assert_eq!(j, Json::Str("caf\u{e9}".into()));
    }

    #[test]
    fn utf8_multibyte_string() {
        let j = parse("\" Italiano ✓ \"").unwrap();
        assert_eq!(j, Json::Str(" Italiano ✓ ".into()));
    }

    #[test]
    fn rejects_surrogate_and_fraction() {
        assert!(parse(r#""\ud83d""#).is_err());
        assert!(parse(r#"{"a":1.5}"#).is_err());
    }
}

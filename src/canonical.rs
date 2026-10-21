//! Byte-exact equivalent of the reference serialisation:
//! `json.dumps(obj, sort_keys=True, ensure_ascii=True, indent=2, separators=(",", ": ")) + "\n"`.

use crate::Error;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Serialise `value` exactly as the Python reference does, including the trailing newline.
pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>, Error> {
    let mut out = String::new();
    write_value(&mut out, value, 0)?;
    out.push('\n');
    Ok(out.into_bytes())
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn indent(out: &mut String, level: usize) {
    out.push('\n');
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_value(out: &mut String, value: &Value, level: usize) -> Result<(), Error> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if !(n.is_i64() || n.is_u64()) {
                return Err(Error::value(
                    "floating-point numbers are not part of the snapshot format",
                ));
            }
            out.push_str(&n.to_string());
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(out, level + 1);
                write_value(out, item, level + 1)?;
            }
            indent(out, level);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            // serde_json's default Map is a BTreeMap<String, _>: byte order of UTF-8 keys
            // equals code point order, which is what Python's sort_keys uses.
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                indent(out, level + 1);
                write_string(out, key);
                out.push_str(": ");
                write_value(out, item, level + 1)?;
            }
            indent(out, level);
            out.push('}');
        }
    }
    Ok(())
}

/// Python's `ensure_ascii` escaping: everything outside 0x20..=0x7e is escaped, with
/// lowercase hex and UTF-16 surrogate pairs above the Basic Multilingual Plane.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

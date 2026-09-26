//! Windows PowerShell 5.1 `ConvertTo-Json` output, reproduced byte-for-byte.
//!
//! The bridge writes every JSON store with `Write-JsonFile`
//! (`codex-consult-common.ps1`): `ConvertTo-Json -InputObject $o -Depth 20`, then a
//! CRLF->LF pass and one trailing `\n`. On the reference machine that `ConvertTo-Json`
//! is Windows PowerShell 5.1's, backed by `System.Web.Script.Serialization.
//! JavaScriptSerializer` and 5.1's indenter. That combination has three signatures this
//! module reproduces exactly, verified against the real `.collab/c3-design/sessions.json`
//! and `findings.json`:
//!
//! 1. **Column-anchored indentation.** The value of a key is written on the key's own
//!    line, right after `"<key>":  ` (colon, then TWO spaces). A container's members are
//!    indented four columns past the *column of its opening bracket* - not four per depth
//!    level. So `bracket_col = key_indent + quoted_key.len() + 3` (`+1` colon, `+2`
//!    spaces), members sit at `bracket_col + 4`, and the closing bracket sits back at
//!    `bracket_col`. An empty array is `[`, a blank line, then `]` at the bracket column.
//! 2. **Escaping (JavaScriptSerializer).** Only `"` and `\` take short escapes; `<`, `>`,
//!    `&` and `'` are escaped as `<`, `>`, `&`, `'`; control
//!    characters (`< 0x20`) and `U+2028`/`U+2029` as `\uXXXX` (lowercase). Every other
//!    character, including all non-ASCII, is written raw as UTF-8.
//! 3. **.NET number text.** A whole-valued double is written without a decimal point
//!    (`277`, not `277.0`), because the bridge rounds wall times to a `[double]` and
//!    `ConvertTo-Json` prints `277.0` as `277`; a fractional double keeps its shortest
//!    round-trip form (`243.4`). Integers print as-is.
//!
//! The one host-dependent caveat: run under PowerShell 7 the same `Write-JsonFile`
//! produces DIFFERENT bytes (single space after the colon, plain per-level indentation).
//! C3 therefore canonicalises to the 5.1 shape - the shape of the evidence on disk - and
//! the byte-identity acceptance test in `tests/formats.rs` pins it. See
//! `docs/port/contracts.md` for the review question this raises.

use serde::Serialize;
use serde_json::Value;

/// Serialize `value` to the PowerShell-5.1 `ConvertTo-Json` text (no trailing newline).
pub fn to_ps_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    Ok(format_value_root(&v))
}

/// Serialize `value` exactly as `Write-JsonFile` writes a store: the PowerShell-5.1 text,
/// LF line endings, plus the one trailing `\n`. This is the on-disk byte stream.
pub fn to_ps_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut s = to_ps_json(value)?;
    s.push('\n');
    Ok(s.into_bytes())
}

/// Format a parsed [`Value`] as the PowerShell-5.1 text (no trailing newline). Used by the
/// formatter's own tests to prove byte-identity independently of any typed struct.
pub fn format_value_root(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out
}

fn write_value(out: &mut String, value: &Value, col: usize) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{\n\n");
                push_spaces(out, col);
                out.push('}');
                return;
            }
            out.push_str("{\n");
            let last = map.len() - 1;
            let child_indent = col + 4;
            for (i, (k, v)) in map.iter().enumerate() {
                push_spaces(out, child_indent);
                let key_token = escape_json_string(k);
                out.push_str(&key_token);
                out.push_str(":  ");
                // The value's opening bracket lands here; it drives the value's own indent.
                let value_col = child_indent + key_token.len() + 3;
                write_value(out, v, value_col);
                if i != last {
                    out.push(',');
                }
                out.push('\n');
            }
            push_spaces(out, col);
            out.push('}');
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                // PowerShell 5.1 renders @() as `[`, a blank line, then `]`.
                out.push_str("[\n\n");
                push_spaces(out, col);
                out.push(']');
                return;
            }
            out.push_str("[\n");
            let last = arr.len() - 1;
            let child_indent = col + 4;
            for (i, v) in arr.iter().enumerate() {
                push_spaces(out, child_indent);
                write_value(out, v, child_indent);
                if i != last {
                    out.push(',');
                }
                out.push('\n');
            }
            push_spaces(out, col);
            out.push(']');
        }
        Value::String(s) => out.push_str(&escape_json_string(s)),
        Value::Number(n) => out.push_str(&format_number(n)),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
    }
}

fn push_spaces(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

/// .NET/JavaScriptSerializer number text: a whole-valued double drops its `.0`.
fn format_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        let f = n.as_f64().unwrap_or(0.0);
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
            (f as i64).to_string()
        } else {
            // Rust's shortest round-trip form: 243.4 -> "243.4".
            format!("{f}")
        }
    }
}

/// Quote and escape a string the way JavaScriptSerializer does.
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\'' => out.push_str("\\u0027"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn colon_two_spaces_and_column_anchor() {
        let v = json!({"codex": {"tool": "x"}});
        let s = format_value_root(&v);
        // "codex" quoted len = 7; bracket at 0+4+7+3 = 14; child at 18.
        assert_eq!(
            s,
            "{\n    \"codex\":  {\n                  \"tool\":  \"x\"\n              }\n}"
        );
    }

    #[test]
    fn empty_array_has_blank_line() {
        let v = json!({"skipped": []});
        let s = format_value_root(&v);
        // bracket col = 4 + len("\"skipped\"")=9 + 3 = 16
        assert_eq!(s, "{\n    \"skipped\":  [\n\n                ]\n}");
    }

    #[test]
    fn escapes_match_javascriptserializer() {
        let v = json!({"k": "a'b<c>d&e\"f\\g"});
        let s = format_value_root(&v);
        assert!(s.contains("\\u0027"));
        assert!(s.contains("\\u003c"));
        assert!(s.contains("\\u003e"));
        assert!(s.contains("\\u0026"));
        assert!(s.contains("\\\""));
        assert!(s.contains("\\\\"));
    }

    #[test]
    fn whole_double_drops_decimal() {
        let v: Value = serde_json::from_str("277.0").unwrap();
        // serde parses 277.0 as a float; JavaScriptSerializer prints "277".
        assert_eq!(format_value_root(&v), "277");
        let v2: Value = json!(243.4);
        assert_eq!(format_value_root(&v2), "243.4");
        let v3: Value = json!(9);
        assert_eq!(format_value_root(&v3), "9");
    }

    #[test]
    fn non_ascii_stays_raw() {
        let v = json!({"k": "\u{2014}\u{2192}"});
        let s = format_value_root(&v);
        assert!(s.contains('\u{2014}'));
    }
}

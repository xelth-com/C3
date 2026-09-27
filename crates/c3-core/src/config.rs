//! The constrained Codex `config.toml` scanner, ported from `Read-CodexConfigSubset`
//! and its helpers in `codex-consult-common.ps1`.
//!
//! It is not a full TOML parser: it reads the subset the bridge understands (tables,
//! plain key = value with string/scalar values) and marks everything else
//! "unsupported", with the exact wording the PowerShell scanner produces, so a
//! provider table that carries an inline table, a dotted key, an array of tables or a
//! duplicate is reported unusable the same way. Values are masked in error messages
//! (a value may hold a credential).

use std::collections::HashMap;

use regex::Regex;

/// U+001F, the separator that joins table path segments into a map key.
pub const KEY_SEP: char = '\u{1f}';

/// One `key = value` entry inside a table.
#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub line: usize,
    pub kind: String,
    /// The decoded value for a supported string/scalar; `None` otherwise.
    pub value: Option<String>,
    pub supported: bool,
    pub reason: String,
}

/// A TOML table (`''` segments = the top level).
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub segments: Vec<String>,
    pub header_line: usize,
    pub ok: bool,
    pub reason: String,
    /// Insertion-ordered entries, keyed ordinally by their key.
    pub entries: Vec<Entry>,
}

impl Table {
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key == key)
    }
    fn set_unsupported(&mut self, reason: &str) {
        if self.ok {
            self.ok = false;
            self.reason = reason.to_string();
        }
    }
}

/// The scanned config: `Read-CodexConfigSubset`'s result object.
#[derive(Debug, Clone)]
pub struct CodexConfig {
    pub path: String,
    pub exists: bool,
    pub ok: bool,
    pub reason: String,
    /// Insertion-ordered tables, keyed by their segments joined with [`KEY_SEP`].
    order: Vec<String>,
    tables: HashMap<String, Table>,
}

impl CodexConfig {
    fn new(path: &str) -> Self {
        let mut c = CodexConfig {
            path: path.to_string(),
            exists: false,
            ok: true,
            reason: String::new(),
            order: Vec::new(),
            tables: HashMap::new(),
        };
        c.get_table(&[]);
        c
    }

    fn key_of(segments: &[String]) -> String {
        segments.join(&KEY_SEP.to_string())
    }

    fn get_table(&mut self, segments: &[String]) -> &mut Table {
        let key = Self::key_of(segments);
        if !self.tables.contains_key(&key) {
            self.order.push(key.clone());
            self.tables.insert(
                key.clone(),
                Table {
                    name: format_toml_path(segments),
                    segments: segments.to_vec(),
                    header_line: 0,
                    ok: true,
                    reason: String::new(),
                    entries: Vec::new(),
                },
            );
        }
        self.tables.get_mut(&key).unwrap()
    }

    pub fn table_by_key(&self, key: &str) -> Option<&Table> {
        self.tables.get(key)
    }
    pub fn contains_table_key(&self, key: &str) -> bool {
        self.tables.contains_key(key)
    }
    /// Tables in insertion order.
    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.order.iter().filter_map(move |k| self.tables.get(k))
    }
    /// The top-level table (`''`).
    pub fn top(&self) -> Option<&Table> {
        self.tables.get("")
    }
}

// --------------------------------------------------------------------------- helpers

/// `Format-TomlPath`: segments joined by `.`, quoting a segment that is not bare.
pub fn format_toml_path(segments: &[String]) -> String {
    segments
        .iter()
        .map(|seg| {
            if !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                seg.clone()
            } else {
                format!("\"{}\"", seg.replace('\\', "\\\\").replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// `ConvertFrom-TomlEscapes`: decode the escapes of a TOML basic string body.
pub fn convert_from_toml_escapes(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let e = chars[i + 1];
        let decoded: Option<String> = match e {
            'b' => Some('\u{8}'.to_string()),
            't' => Some('\t'.to_string()),
            'n' => Some('\n'.to_string()),
            'f' => Some('\u{c}'.to_string()),
            'r' => Some('\r'.to_string()),
            'e' => Some('\u{1b}'.to_string()),
            '"' => Some('"'.to_string()),
            '\\' => Some('\\'.to_string()),
            'u' | 'U' | 'x' => {
                let n = if e == 'u' {
                    4
                } else if e == 'U' {
                    8
                } else {
                    2
                };
                if i + 2 + n <= chars.len() {
                    let hex: String = chars[i + 2..i + 2 + n].iter().collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            i += 2 + n;
                            out.push(ch);
                            continue;
                        }
                    }
                }
                None
            }
            _ => None,
        };
        match decoded {
            Some(s) => {
                out.push_str(&s);
                i += 2;
            }
            None => {
                // leave the backslash sequence as written
                out.push('\\');
                out.push(e);
                i += 2;
            }
        }
    }
    out
}

fn ws_len(c: &[char], i: usize) -> usize {
    let mut n = 0;
    while i + n < c.len() && (c[i + n] == ' ' || c[i + n] == '\t') {
        n += 1;
    }
    n
}

fn comment_len(c: &[char], i: usize) -> usize {
    // `#[^\n]*`
    if i >= c.len() || c[i] != '#' {
        return 0;
    }
    let mut n = 1;
    while i + n < c.len() && c[i + n] != '\n' {
        n += 1;
    }
    n
}

fn bare_len(c: &[char], i: usize) -> usize {
    let mut n = 0;
    while i + n < c.len() {
        let ch = c[i + n];
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            n += 1;
        } else {
            break;
        }
    }
    n
}

/// A basic string `"((?:[^"\\\n]|\\[^\n])*)"` at `i`: (body_raw, total_len) or None.
fn read_basic(c: &[char], i: usize) -> Option<(String, usize)> {
    if i >= c.len() || c[i] != '"' {
        return None;
    }
    let mut j = i + 1;
    let mut body = String::new();
    while j < c.len() {
        let ch = c[j];
        if ch == '"' {
            return Some((body, j + 1 - i));
        }
        if ch == '\n' {
            return None;
        }
        if ch == '\\' {
            if j + 1 >= c.len() || c[j + 1] == '\n' {
                return None;
            }
            body.push('\\');
            body.push(c[j + 1]);
            j += 2;
            continue;
        }
        body.push(ch);
        j += 1;
    }
    None
}

/// A literal string `'([^'\n]*)'` at `i`.
fn read_literal(c: &[char], i: usize) -> Option<(String, usize)> {
    if i >= c.len() || c[i] != '\'' {
        return None;
    }
    let mut j = i + 1;
    let mut body = String::new();
    while j < c.len() {
        let ch = c[j];
        if ch == '\'' {
            return Some((body, j + 1 - i));
        }
        if ch == '\n' {
            return None;
        }
        body.push(ch);
        j += 1;
    }
    None
}

/// Length of a multi-line string starting at `i` (triple quote of `q`), or None.
fn read_multiline(c: &[char], i: usize, q: char) -> Option<usize> {
    // Opening is three q; scan for a closing run of >=3 q, honouring escapes for '"'.
    let mut j = i + 3;
    let basic = q == '"';
    while j < c.len() {
        let ch = c[j];
        if basic && ch == '\\' {
            j += 2;
            continue;
        }
        if ch == q {
            // count the run
            let mut run = 0;
            while j + run < c.len() && c[j + run] == q {
                run += 1;
            }
            if run >= 3 {
                // consume up to 5 (regex `"{3,5}`); take min(run,5)
                let take = run.min(5);
                return Some(j + take - i);
            }
            j += run;
            continue;
        }
        j += 1;
    }
    None
}

/// Result of `Read-TomlKeyPath`.
struct KeyPath {
    segments: Vec<String>,
    end: usize,
}

fn read_key_path(c: &[char], start: usize) -> Option<KeyPath> {
    let mut segs = Vec::new();
    let mut i = start;
    loop {
        i += ws_len(c, i);
        let bl = bare_len(c, i);
        if bl > 0 {
            segs.push(c[i..i + bl].iter().collect::<String>());
            i += bl;
        } else if let Some((body, len)) = read_basic(c, i) {
            segs.push(convert_from_toml_escapes(&body));
            i += len;
        } else if let Some((body, len)) = read_literal(c, i) {
            segs.push(body);
            i += len;
        } else {
            return None;
        }
        i += ws_len(c, i);
        if i < c.len() && c[i] == '.' {
            i += 1;
            continue;
        }
        break;
    }
    Some(KeyPath {
        segments: segs,
        end: i,
    })
}

struct TomlValue {
    kind: String,
    value: Option<String>,
    supported: bool,
    end: usize,
    lines: usize,
    error: String,
}

fn scalar_kind(token: &str) -> Option<&'static str> {
    // TomlScalarRules, in order.
    thread_local! {
        static RES: Vec<(&'static str, Regex)> = vec![
            ("boolean", Regex::new(r"^(true|false)$").unwrap()),
            ("integer", Regex::new(r"^[+-]?(0|[1-9](_?[0-9])*)$").unwrap()),
            ("integer", Regex::new(r"^0x[0-9A-Fa-f](_?[0-9A-Fa-f])*$").unwrap()),
            ("integer", Regex::new(r"^0o[0-7](_?[0-7])*$").unwrap()),
            ("integer", Regex::new(r"^0b[01](_?[01])*$").unwrap()),
            ("float", Regex::new(r"^[+-]?(0|[1-9](_?[0-9])*)(\.[0-9](_?[0-9])*)?([eE][+-]?[0-9](_?[0-9])*)?$").unwrap()),
            ("float", Regex::new(r"^[+-]?(inf|nan)$").unwrap()),
            ("datetime", Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}([Tt ][0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]+)?)?([Zz]|[+-][0-9]{2}:[0-9]{2})?)?$").unwrap()),
            ("datetime", Regex::new(r"^[0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]+)?)?$").unwrap()),
        ];
    }
    RES.with(|res| {
        for (kind, re) in res.iter() {
            if re.is_match(token) {
                return Some(*kind);
            }
        }
        None
    })
}

fn read_value(c: &[char], start: usize) -> TomlValue {
    let mut v = TomlValue {
        kind: String::new(),
        value: None,
        supported: false,
        end: start,
        lines: 0,
        error: String::new(),
    };
    let len = c.len();
    if start >= len || c[start] == '\n' {
        v.error = "missing value".into();
        return v;
    }
    let ch = c[start];
    if ch == '"' || ch == '\'' {
        // triple?
        if start + 3 <= len && c[start] == ch && c[start + 1] == ch && c[start + 2] == ch {
            match read_multiline(c, start, ch) {
                Some(l) => {
                    v.kind = "multi-line string".into();
                    v.end = start + l;
                    v.lines = c[start..start + l].iter().filter(|&&x| x == '\n').count();
                    return v;
                }
                None => {
                    v.error = "unterminated multi-line string".into();
                    return v;
                }
            }
        }
        if ch == '"' {
            match read_basic(c, start) {
                Some((body, l)) => {
                    v.value = Some(convert_from_toml_escapes(&body));
                    v.kind = "string".into();
                    v.supported = true;
                    v.end = start + l;
                    return v;
                }
                None => {
                    v.error = "unterminated string".into();
                    return v;
                }
            }
        } else {
            match read_literal(c, start) {
                Some((body, l)) => {
                    v.value = Some(body);
                    v.kind = "string".into();
                    v.supported = true;
                    v.end = start + l;
                    return v;
                }
                None => {
                    v.error = "unterminated string".into();
                    return v;
                }
            }
        }
    }
    if ch == '[' || ch == '{' {
        let kind = if ch == '[' { "array" } else { "inline table" };
        let mut depth = 0i32;
        let mut lines = 0usize;
        let mut i = start;
        while i < len {
            let cc = c[i];
            if cc == '\n' {
                lines += 1;
                i += 1;
                continue;
            }
            if cc == '#' {
                i += comment_len(c, i);
                continue;
            }
            if cc == '"' || cc == '\'' {
                let inner = read_value(c, i);
                if !inner.error.is_empty() {
                    v.error = format!("unterminated string inside an {kind}");
                    return v;
                }
                lines += inner.lines;
                i = inner.end;
                continue;
            }
            if cc == '[' || cc == '{' {
                depth += 1;
            } else if cc == ']' || cc == '}' {
                depth -= 1;
                if depth == 0 {
                    v.kind = kind.into();
                    v.end = i + 1;
                    v.lines = lines;
                    return v;
                }
            }
            i += 1;
        }
        v.error = format!("unterminated {kind}");
        return v;
    }
    // scalar: `[^ \t\n#]+`
    let mut sl = 0;
    while start + sl < len {
        let x = c[start + sl];
        if x == ' ' || x == '\t' || x == '\n' || x == '#' {
            break;
        }
        sl += 1;
    }
    let mut token: String = c[start..start + sl].iter().collect();
    let mut end = start + sl;
    // date + time tail: ` [0-9]{2}:[0-9]{2}[^ \t\n#]*`
    if Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
        .unwrap()
        .is_match(&token)
        && end < len
        && c[end] == ' '
    {
        // DateTail regex on chars from end
        let mut k = end + 1;
        // [0-9]{2}:[0-9]{2}
        let rest: String = c[k.min(len)..].iter().collect();
        if Regex::new(r"^[0-9]{2}:[0-9]{2}").unwrap().is_match(&rest) {
            let mut tail_len = 5; // HH:MM
            let mut tk = k + 5;
            while tk < len {
                let x = c[tk];
                if x == ' ' || x == '\t' || x == '\n' || x == '#' {
                    break;
                }
                tk += 1;
                tail_len += 1;
            }
            let _ = tail_len;
            let tail: String = c[end..tk].iter().collect();
            token.push_str(&tail);
            end = tk;
            let _ = &mut k;
        }
    }
    if let Some(kind) = scalar_kind(&token) {
        v.kind = kind.into();
        v.value = Some(token);
        v.supported = true;
        v.end = end;
        return v;
    }
    v.error = "not a value the scanner knows".into();
    v
}

fn toml_line_end(c: &[char], start: usize) -> i64 {
    let mut i = start + ws_len(c, start);
    if i < c.len() && c[i] == '#' {
        i += comment_len(c, i);
    }
    if i >= c.len() || c[i] == '\n' {
        return i as i64;
    }
    -1
}

/// `Format-TomlLineForMessage`: a config line for an error message, its value masked.
pub fn format_toml_line_for_message(line: &str, value_start: i64) -> String {
    let lchars: Vec<char> = line.chars().collect();
    let (head, tail): (String, String) =
        if value_start >= 0 && (value_start as usize) <= lchars.len() {
            let vs = value_start as usize;
            (lchars[..vs].iter().collect(), lchars[vs..].iter().collect())
        } else if line.trim_start().starts_with('[') {
            (line.to_string(), String::new())
        } else {
            match read_key_path(&lchars, 0) {
                Some(kp) if kp.end < lchars.len() && lchars[kp.end] == '=' => (
                    lchars[..kp.end + 1].iter().collect(),
                    lchars[kp.end + 1..].iter().collect(),
                ),
                _ => return "<not a key = value line; content not shown>".to_string(),
            }
        };
    // masker regex over `tail`
    let re = Regex::new(r#""(?:[^"\\]|\\.)*"|'[^']*'|["']|[^\s\[\]\{\},=#"']+"#).unwrap();
    let mut sb = String::new();
    let mut pos = 0usize;
    let bytes_tail = tail.as_str();
    for m in re.find_iter(bytes_tail) {
        sb.push_str(&bytes_tail[pos..m.start()]);
        let val = m.as_str();
        let first = val.chars().next().unwrap();
        if val.chars().count() == 1 && (first == '"' || first == '\'') {
            sb.push_str("...");
            pos = bytes_tail.len();
            break;
        }
        if first == '"' || first == '\'' {
            sb.push(first);
            sb.push_str("...");
            sb.push(first);
        } else {
            sb.push_str("...");
        }
        pos = m.end();
    }
    if pos < bytes_tail.len() {
        sb.push_str(&bytes_tail[pos..]);
    }
    let mut t = format!("{head}{sb}").trim().to_string();
    let tc: Vec<char> = t.chars().collect();
    if tc.len() > 60 {
        t = tc[..60].iter().collect();
    }
    t
}

/// A config for a path with no file (`exists = false`): Codex runs on its defaults.
pub fn config_not_found(path: &str) -> CodexConfig {
    CodexConfig::new(path)
}

/// A config whose file could not be read (`exists = true`, `ok = false`).
pub fn config_unreadable(path: &str, reason: &str) -> CodexConfig {
    let mut c = CodexConfig::new(path);
    c.exists = true;
    c.ok = false;
    c.reason = reason.to_string();
    c
}

/// `Read-CodexConfigSubset`: scan the config file text.
pub fn scan_config_text(path: &str, text: &str) -> CodexConfig {
    let mut result = CodexConfig::new(path);
    result.exists = true;
    // strip BOM
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let s = text.replace("\r\n", "\n");
    let c: Vec<char> = s.chars().collect();
    // per-line char vectors for messages
    let lines: Vec<String> = s.split('\n').map(|x| x.to_string()).collect();
    let len = c.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut current_segs: Vec<String> = Vec::new();

    while i < len {
        i += ws_len(&c, i);
        if i >= len {
            break;
        }
        let ch = c[i];
        if ch == '\n' {
            i += 1;
            line += 1;
            continue;
        }
        if ch == '#' {
            i += comment_len(&c, i);
            continue;
        }
        let line_no = line;
        let line_text = lines.get(line_no - 1).cloned().unwrap_or_default();
        // line start (char index of start of current line)
        let mut line_start = 0usize;
        if i > 0 {
            // last '\n' before i
            for k in (0..i).rev() {
                if c[k] == '\n' {
                    line_start = k + 1;
                    break;
                }
            }
        }
        let fatal = format!(
            "unsupported TOML construct at line {line_no}: {}",
            format_toml_line_for_message(&line_text, -1)
        );
        if ch == '[' {
            let is_aot = i + 1 < len && c[i + 1] == '[';
            let close: &[char] = if is_aot {
                &['[', ']', ']'][1..]
            } else {
                &[']']
            };
            let close_len = if is_aot { 2 } else { 1 };
            let kp = read_key_path(&c, i + close_len);
            let close_ok = match &kp {
                Some(k) => {
                    k.end + close_len <= len
                        && (0..close_len).all(|off| c[k.end + off] == close[off])
                }
                None => false,
            };
            if !close_ok {
                result.ok = false;
                result.reason = fatal;
                break;
            }
            let kp = kp.unwrap();
            let eol = toml_line_end(&c, kp.end + close_len);
            if eol < 0 {
                result.ok = false;
                result.reason = fatal;
                break;
            }
            {
                let name = format_toml_path(&kp.segments);
                let existing_hl = result.get_table(&kp.segments).header_line;
                if is_aot {
                    let r = format!("{fatal} (array of tables)");
                    result.get_table(&kp.segments).set_unsupported(&r);
                } else if existing_hl > 0 {
                    let r = format!(
                        "table [{name}] is defined twice in the Codex config (lines {existing_hl} and {line_no})"
                    );
                    result.get_table(&kp.segments).set_unsupported(&r);
                }
                let t = result.get_table(&kp.segments);
                if t.header_line == 0 {
                    t.header_line = line_no;
                }
            }
            current_segs = kp.segments;
            i = eol as usize;
            continue;
        }
        let kp = read_key_path(&c, i);
        let kp = match kp {
            Some(k) if k.end < len && c[k.end] == '=' => k,
            _ => {
                result.ok = false;
                result.reason = fatal;
                break;
            }
        };
        let mut p = kp.end + 1;
        p += ws_len(&c, p);
        let v = read_value(&c, p);
        if !v.error.is_empty() {
            result.ok = false;
            result.reason = format!("{fatal} ({})", v.error);
            break;
        }
        let eol = toml_line_end(&c, v.end);
        if eol < 0 {
            result.ok = false;
            result.reason = fatal;
            break;
        }
        line += v.lines;
        i = eol as usize;
        let construct = format!(
            "unsupported TOML construct at line {line_no}: {}",
            format_toml_line_for_message(&line_text, (p - line_start) as i64)
        );
        let segs = &kp.segments;
        if segs.len() > 1 {
            let mut target_segs = current_segs.clone();
            target_segs.extend_from_slice(&segs[..segs.len() - 1]);
            let r = format!("{construct} (dotted key)");
            result.get_table(&target_segs).set_unsupported(&r);
            let cur = result.get_table(&current_segs);
            if !cur.contains(&segs[0]) {
                cur.entries.push(Entry {
                    key: segs[0].clone(),
                    line: line_no,
                    kind: "dotted key".into(),
                    value: None,
                    supported: false,
                    reason: format!("{construct} (dotted key)"),
                });
            }
            continue;
        }
        let k = segs[0].clone();
        {
            let cur = result.get_table(&current_segs);
            if let Some(prev) = cur.entry(&k) {
                let prev_line = prev.line;
                let cur_name = cur.name.clone();
                let r = format!(
                    "key '{k}' is defined twice in [{cur_name}] of the Codex config (lines {prev_line} and {line_no})"
                );
                cur.set_unsupported(&r);
                continue;
            }
        }
        let why = if !v.supported {
            format!("{construct} ({})", v.kind)
        } else {
            String::new()
        };
        result.get_table(&current_segs).entries.push(Entry {
            key: k.clone(),
            line: line_no,
            kind: v.kind.clone(),
            value: v.value.clone(),
            supported: v.supported,
            reason: why.clone(),
        });
        if v.kind == "array" || v.kind == "inline table" {
            let mut sub = current_segs.clone();
            sub.push(k);
            result.get_table(&sub).set_unsupported(&why);
        }
    }
    result
}

/// A supported plain-string setting (`Get-TomlString`): (present, value, line, reason).
#[derive(Debug, Clone, Default)]
pub struct TomlString {
    pub present: bool,
    pub value: String,
    pub line: usize,
    pub reason: String,
}

pub fn get_toml_string(table: Option<&Table>, key: &str) -> TomlString {
    let table = match table {
        Some(t) => t,
        None => return TomlString::default(),
    };
    let e = match table.entry(key) {
        Some(e) => e,
        None => return TomlString::default(),
    };
    let reason = if !e.supported {
        e.reason.clone()
    } else if e.kind != "string" {
        format!(
            "'{key}' at line {} of the Codex config is a {}, not a string",
            e.line, e.kind
        )
    } else {
        String::new()
    };
    TomlString {
        present: true,
        value: e.value.clone().unwrap_or_default(),
        line: e.line,
        reason,
    }
}

/// `Get-ProviderNames`: the `<name>` of every `[model_providers.<name>]` table, in
/// scan order.
pub fn provider_names(config: &CodexConfig) -> Vec<String> {
    config
        .tables()
        .filter(|t| t.segments.len() == 2 && t.segments[0] == "model_providers")
        .map(|t| t.segments[1].clone())
        .collect()
}

/// `[model_providers.<name>]` lookup: (found, ok, reason, table-key).
pub struct ProviderTable {
    pub found: bool,
    pub ok: bool,
    pub reason: String,
    pub table_key: Option<String>,
}

pub fn provider_table(config: &CodexConfig, name: &str) -> ProviderTable {
    let key = CodexConfig::key_of(&["model_providers".to_string(), name.to_string()]);
    if !config.contains_table_key(&key) {
        let prefix = format!("{key}{KEY_SEP}");
        for other in config.tables() {
            let ok = CodexConfig::key_of(&other.segments);
            if ok.starts_with(&prefix) {
                let why = if !other.reason.is_empty() {
                    other.reason.clone()
                } else {
                    format!(
                        "unsupported TOML construct at line {}: [{}] declares the provider only through a sub-table",
                        other.header_line, other.name
                    )
                };
                return ProviderTable {
                    found: true,
                    ok: false,
                    reason: why,
                    table_key: None,
                };
            }
        }
        return ProviderTable {
            found: false,
            ok: false,
            reason: String::new(),
            table_key: None,
        };
    }
    let t = config.table_by_key(&key).unwrap();
    let mut why = String::new();
    if !t.ok {
        why = t.reason.clone();
    }
    if why.is_empty() {
        for e in &t.entries {
            if !e.supported {
                why = e.reason.clone();
                break;
            }
        }
    }
    if why.is_empty() {
        let prefix = format!("{key}{KEY_SEP}");
        for other in config.tables() {
            let ok = CodexConfig::key_of(&other.segments);
            if ok.starts_with(&prefix) {
                why = if !other.reason.is_empty() {
                    other.reason.clone()
                } else {
                    format!(
                        "unsupported TOML construct at line {}: [{}] (a sub-table of the provider)",
                        other.header_line, other.name
                    )
                };
                break;
            }
        }
    }
    ProviderTable {
        found: true,
        ok: why.is_empty(),
        reason: why,
        table_key: Some(key),
    }
}

/// `Get-ProviderSetProblem`: can the scanner establish whether the provider is declared?
pub fn provider_set_problem(config: &CodexConfig, name: &str) -> String {
    if let Some(top) = config.top() {
        if let Some(e) = top.entry("model_providers") {
            if e.kind != "dotted key" {
                if !e.reason.is_empty() {
                    return e.reason.clone();
                }
                return format!(
                    "model_providers at line {} is a {}, not a table of providers",
                    e.line, e.kind
                );
            }
        }
    }
    if let Some(mp) = config.table_by_key("model_providers") {
        if !mp.ok {
            return mp.reason.clone();
        }
        if let Some(e) = mp.entry(name) {
            if !e.reason.is_empty() {
                return e.reason.clone();
            }
            return format!(
                "[model_providers] declares {name} at line {} as a {}, not as a table",
                e.line, e.kind
            );
        }
    }
    String::new()
}

/// `ConvertTo-CanonicalBaseUrl`: (url, host).
pub fn canonical_base_url(url: &str) -> (String, String) {
    let u = url.trim();
    let re = Regex::new(r"^([A-Za-z][A-Za-z0-9+.-]*)://([^/?#]*)(.*)$").unwrap();
    let caps = match re.captures(u) {
        Some(c) => c,
        None => return (u.trim_end_matches('/').to_string(), String::new()),
    };
    let scheme = caps[1].to_lowercase();
    let mut authority = caps[2].to_string();
    if let Some(at) = authority.rfind('@') {
        authority = authority[at + 1..].to_string();
    }
    authority = authority.to_lowercase();
    let mut host = authority.clone();
    if host.starts_with('[') {
        if let Some(close) = host.find(']') {
            host = host[..close + 1].to_string();
        }
    } else {
        host = Regex::new(r":[0-9]*$")
            .unwrap()
            .replace(&host, "")
            .to_string();
    }
    let full = format!("{scheme}://{authority}{}", &caps[3]);
    (full.trim_end_matches('/').to_string(), host)
}

/// The pieces of a usable provider table's endpoint identity (`Get-ProviderEndpoint`).
pub struct ProviderEndpoint {
    pub compat: String,
    pub host: String,
    pub base_url: String,
    pub wire_api: String,
    pub error: String,
    /// The raw provider-table echo (`Get-ProviderEndpoint`'s `Config`): the table's declared
    /// keys, ordinal-sorted, secret keys dropped, `base_url` audited (`?...`), booleans/integers
    /// typed. This is what the ledger's `reviewer.provider_config` carries for a user table.
    pub provider_config: serde_json::Value,
}

/// The audited form of a base_url for the provider_config echo / console display: the query
/// string is replaced with `?...` (`-replace '\?.*$', '?...'`).
pub fn audit_base_url(url: &str) -> String {
    match url.find('?') {
        Some(i) => format!("{}?...", &url[..i]),
        None => url.to_string(),
    }
}

/// `Get-ProviderEndpoint`'s `Config`: echo the table's declared keys, ordinal-sorted, secrets
/// dropped, `base_url` audited, booleans/integers typed.
fn provider_config_echo(table: Option<&Table>, audited_base_url: &str) -> serde_json::Value {
    let secret = Regex::new(r"(?i)env_key|api_key|bearer|token|secret|password").unwrap();
    let mut keys: Vec<&str> = table
        .map(|t| t.entries.iter().map(|e| e.key.as_str()).collect())
        .unwrap_or_default();
    keys.sort_unstable(); // ordinal (byte) sort, matching [StringComparer]::Ordinal
    let mut m = serde_json::Map::new();
    if let Some(t) = table {
        for key in keys {
            if secret.is_match(key) {
                continue;
            }
            let e = match t.entry(key) {
                Some(e) => e,
                None => continue,
            };
            let val = if key == "base_url" {
                serde_json::Value::String(audited_base_url.to_string())
            } else if e.kind == "boolean" {
                serde_json::Value::Bool(e.value.as_deref() == Some("true"))
            } else if e.kind == "integer" {
                match e
                    .value
                    .as_deref()
                    .and_then(|v| v.replace('_', "").parse::<i64>().ok())
                {
                    Some(n) => serde_json::Value::Number(n.into()),
                    None => serde_json::Value::String(e.value.clone().unwrap_or_default()),
                }
            } else {
                serde_json::Value::String(e.value.clone().unwrap_or_default())
            };
            m.insert(key.to_string(), val);
        }
    }
    serde_json::Value::Object(m)
}

pub fn provider_endpoint(
    config: &CodexConfig,
    table_key: &str,
    table_name: &str,
    where_: &str,
) -> ProviderEndpoint {
    let table = config.table_by_key(table_key);
    let bu = get_toml_string(table, "base_url");
    let wa = get_toml_string(table, "wire_api");
    let mut r = ProviderEndpoint {
        compat: String::new(),
        host: String::new(),
        base_url: String::new(),
        wire_api: String::new(),
        error: String::new(),
        provider_config: serde_json::Value::Null,
    };
    if !bu.reason.is_empty() {
        r.error = format!("{table_name} in {where_} is not usable - {}", bu.reason);
        return r;
    }
    if !wa.reason.is_empty() {
        r.error = format!("{table_name} in {where_} is not usable - {}", wa.reason);
        return r;
    }
    let (curl, chost) = canonical_base_url(&bu.value);
    let mut wire = "default".to_string();
    let mut wire_label = "(default)".to_string();
    if wa.present {
        wire = wa.value.trim().to_string();
        wire_label = wire.clone();
    }
    r.compat = format!("cc-provider-v1|base_url={curl}|wire_api={wire}");
    r.host = chost;
    r.provider_config = provider_config_echo(table, &audit_base_url(&curl));
    r.base_url = curl;
    r.wire_api = wire_label;
    r
}

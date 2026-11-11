use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum PyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<PyValue>),
    Dict(Vec<(String, PyValue)>),
}

impl PyValue {
    pub fn dict<K: Into<String>>(items: impl IntoIterator<Item = (K, PyValue)>) -> Self {
        PyValue::Dict(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn str(s: impl Into<String>) -> Self {
        PyValue::Str(s.into())
    }

    pub fn list_of_str<S: AsRef<str>>(items: &[S]) -> Self {
        PyValue::List(items.iter().map(|s| PyValue::str(s.as_ref())).collect())
    }
}

impl From<&serde_json::Value> for PyValue {
    fn from(v: &serde_json::Value) -> Self {
        use serde_json::Value as V;
        match v {
            V::Null => PyValue::Null,
            V::Bool(b) => PyValue::Bool(*b),
            V::Number(n) => match n.as_i64() {
                Some(i) => PyValue::Int(i),
                None => PyValue::Float(n.as_f64().unwrap_or(f64::NAN)),
            },
            V::String(s) => PyValue::Str(s.clone()),
            V::Array(a) => PyValue::List(a.iter().map(PyValue::from).collect()),
            V::Object(o) => PyValue::Dict(o.iter().map(|(k, v)| (k.clone(), v.into())).collect()),
        }
    }
}

pub fn dumps(value: &PyValue, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_value(&mut out, value, indent, 0);
    out
}

fn write_value(out: &mut String, v: &PyValue, indent: Option<usize>, level: usize) {
    match v {
        PyValue::Null => out.push_str("null"),
        PyValue::Bool(true) => out.push_str("true"),
        PyValue::Bool(false) => out.push_str("false"),
        PyValue::Int(i) => {
            let _ = write!(out, "{i}");
        }
        PyValue::Float(f) => out.push_str(&float_repr(*f)),
        PyValue::Str(s) => write_str(out, s),
        PyValue::List(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                separator(out, i == 0, indent, level + 1);
                write_value(out, item, indent, level + 1);
            }
            close(out, indent, level);
            out.push(']');
        }
        PyValue::Dict(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, item)) in items.iter().enumerate() {
                separator(out, i == 0, indent, level + 1);
                write_str(out, k);
                out.push_str(": ");
                write_value(out, item, indent, level + 1);
            }
            close(out, indent, level);
            out.push('}');
        }
    }
}

fn separator(out: &mut String, first: bool, indent: Option<usize>, level: usize) {
    match indent {
        None => {
            if !first {
                out.push_str(", ");
            }
        }
        Some(n) => {
            if !first {
                out.push(',');
            }
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', n * level));
        }
    }
}

fn close(out: &mut String, indent: Option<usize>, level: usize) {
    if let Some(n) = indent {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', n * level));
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let sci = format!("{:e}", f.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let sign = if f < 0.0 { "-" } else { "" };
    if !(-4..16).contains(&exp) {
        let (head, tail) = digits.split_at(1);
        let frac = if tail.is_empty() {
            String::new()
        } else {
            format!(".{tail}")
        };
        let esign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{head}{frac}e{esign}{:02}", exp.abs());
    }
    let s = if exp >= 0 {
        let point = exp as usize + 1;
        if digits.len() <= point {
            format!("{digits}{}.0", "0".repeat(point - digits.len()))
        } else {
            format!("{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
    };
    format!("{sign}{s}")
}

pub fn parse_lenient(text: &str) -> serde_json::Result<serde_json::Value> {
    serde_json::from_str(&quote_non_finite(text))
}

fn quote_non_finite(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_str = false;
    let mut escaped = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            rest = &rest[1..];
            continue;
        }
        let token = ["-Infinity", "Infinity", "NaN"]
            .into_iter()
            .find(|t| rest.starts_with(t));
        if let Some(t) = token {
            out.push('"');
            out.push_str(t);
            out.push('"');
            rest = &rest[t.len()..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

pub fn f64_lenient(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => match s.as_str() {
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "NaN" => Some(f64::NAN),
            _ => None,
        },
        _ => None,
    }
}

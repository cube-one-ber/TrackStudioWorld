//! Typed JSON for BYML trees, used by the raw-field editor and the catalog cache.
//!
//! Plain JSON numbers are f32. Other scalars are one-key tagged objects:
//! `{"$s32": 1}`, `{"$u32": 2}`, `{"$u64": "123"}`, `{"$s64": "-1"}`, `{"$f64": 0.5}`,
//! `{"$f32": "nan"}`, `{"$bin": "<base64>"}`. Hash-keyed maps are `{"$h32": {...}}` and
//! `{"$h64": {...}}` with decimal string keys. 64-bit integers are strings so JSON readers
//! that use doubles don't round them.

use crate::byml::Byml;
use crate::error::{Error, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Map, Number, Value, json};
use std::collections::BTreeMap;

const TAGS: [&str; 9] = ["$s32", "$u32", "$s64", "$u64", "$f64", "$f32", "$bin", "$h32", "$h64"];

fn non_finite(v: f64) -> &'static str {
    if v.is_nan() {
        "nan"
    } else if v > 0.0 {
        "inf"
    } else {
        "-inf"
    }
}

/// A plain number, or `{"$f32": "nan" | "inf" | "-inf"}` (JSON has no such numbers).
fn float(v: f64) -> Value {
    Number::from_f64(v).map_or_else(|| json!({"$f32": non_finite(v)}), Value::Number)
}

pub fn to_json(b: &Byml) -> Value {
    match b {
        Byml::Null => Value::Null,
        Byml::Bool(v) => Value::Bool(*v),
        Byml::Str(s) => Value::String(s.clone()),
        Byml::F32(v) => float(*v as f64),
        Byml::I32(v) => json!({"$s32": v}),
        Byml::U32(v) => json!({"$u32": v}),
        Byml::I64(v) => json!({"$s64": v.to_string()}),
        Byml::U64(v) => json!({"$u64": v.to_string()}),
        Byml::F64(v) => json!({"$f64": Number::from_f64(*v).map_or_else(|| json!(non_finite(*v)), Value::Number)}),
        Byml::Bin(d) => json!({"$bin": BASE64.encode(d)}),
        Byml::Array(a) => Value::Array(a.iter().map(to_json).collect()),
        Byml::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
        Byml::Hash32(m) => json!({"$h32": m.iter().map(|(k, v)| (k.to_string(), to_json(v))).collect::<Map<_, _>>()}),
        Byml::Hash64(m) => json!({"$h64": m.iter().map(|(k, v)| (k.to_string(), to_json(v))).collect::<Map<_, _>>()}),
    }
}

fn special(v: &Value) -> Result<f64> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| "bad number".into()),
        Value::String(s) => match s.as_str() {
            "nan" => Ok(f64::NAN),
            "inf" => Ok(f64::INFINITY),
            "-inf" => Ok(f64::NEG_INFINITY),
            _ => s.parse::<f64>().map_err(|e| Error::new(format!("{s}: {e}"))),
        },
        _ => Err(format!("expected a number, got {v}").into()),
    }
}

fn int(v: &Value) -> Result<i128> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
            .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i128))
            .ok_or_else(|| format!("expected an integer, got {n}").into()),
        Value::String(s) => s.trim().parse::<i128>().map_err(|e| format!("{s}: {e}").into()),
        _ => Err(format!("expected an integer, got {v}").into()),
    }
}

fn ranged<T: TryFrom<i128>>(v: &Value, what: &str) -> Result<T> {
    let n = int(v)?;
    T::try_from(n).map_err(|_| format!("{n} is out of range for {what}").into())
}

fn hash_map<K: std::str::FromStr + Ord>(x: &Value) -> Result<BTreeMap<K, Byml>>
where
    K::Err: std::fmt::Display,
{
    let obj = x.as_object().ok_or("a hash map must be an object")?;
    obj.iter().map(|(h, y)| Ok((h.parse::<K>().map_err(|e| Error::new(format!("{h}: {e}")))?, from_json(y)?))).collect()
}

fn tagged(tag: &str, x: &Value) -> Result<Byml> {
    Ok(match tag {
        "$s32" => Byml::I32(ranged(x, "s32")?),
        "$u32" => Byml::U32(ranged(x, "u32")?),
        "$s64" => Byml::I64(ranged(x, "s64")?),
        "$u64" => Byml::U64(ranged(x, "u64")?),
        "$f64" => Byml::F64(special(x)?),
        "$f32" => Byml::F32(special(x)? as f32),
        "$bin" => Byml::Bin(BASE64.decode(x.as_str().unwrap_or("")).map_err(|e| Error::new(e.to_string()))?),
        "$h32" => Byml::Hash32(hash_map(x)?),
        "$h64" => Byml::Hash64(hash_map(x)?),
        _ => unreachable!("tag listed in TAGS"),
    })
}

pub fn from_json(v: &Value) -> Result<Byml> {
    Ok(match v {
        Value::Null => Byml::Null,
        Value::Bool(b) => Byml::Bool(*b),
        Value::String(s) => Byml::Str(s.clone()),
        Value::Number(n) => Byml::F32(n.as_f64().unwrap_or(0.0) as f32),
        Value::Array(a) => Byml::Array(a.iter().map(from_json).collect::<Result<_>>()?),
        Value::Object(o) => {
            if let (1, Some((k, x))) = (o.len(), o.iter().next())
                && TAGS.contains(&k.as_str())
            {
                return tagged(k, x);
            }
            Byml::Map(o.iter().map(|(k, x)| Ok((k.clone(), from_json(x)?))).collect::<Result<_>>()?)
        }
    })
}

/// Plain JSON (numbers untagged), for display and catalog defaults.
pub fn to_plain(b: &Byml) -> Value {
    match b {
        Byml::Null | Byml::Bin(_) => Value::Null,
        Byml::Bool(v) => Value::Bool(*v),
        Byml::Str(s) => Value::String(s.clone()),
        Byml::Array(a) => Value::Array(a.iter().map(to_plain).collect()),
        Byml::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), to_plain(v))).collect()),
        Byml::Hash32(m) => Value::Object(m.iter().map(|(k, v)| (k.to_string(), to_plain(v))).collect()),
        Byml::Hash64(m) => Value::Object(m.iter().map(|(k, v)| (k.to_string(), to_plain(v))).collect()),
        Byml::I32(v) => json!(v),
        Byml::U32(v) => json!(v),
        Byml::I64(v) => json!(v),
        Byml::U64(v) => json!(v),
        Byml::F32(v) => float(*v as f64),
        Byml::F64(v) => float(*v),
    }
}

/// A typed value from text typed into a field. `kind` is a parameter table type (`Bool`,
/// `S32`, `U32`, `F32`, `String`, `Enum`, `Vec3f`) or a BYML type name (`bool`, `s32`, `u32`,
/// `f32`, `f64`, `s64`, `u64`, `vec3`, `str`).
pub fn parse_text(kind: &str, text: &str) -> Result<Byml> {
    let t = text.trim();
    let num = || t.parse::<f64>().map_err(|_| Error::new(format!("'{t}' is not a number")));
    let integer = || -> Result<i128> {
        let n = num()?;
        if n.fract() != 0.0 {
            return Err(format!("'{t}' is not a whole number").into());
        }
        Ok(n as i128)
    };
    Ok(match kind {
        "Bool" | "bool" => match t {
            "true" | "1" | "yes" => Byml::Bool(true),
            "false" | "0" | "no" | "" => Byml::Bool(false),
            _ => return Err(format!("'{t}' is not true or false").into()),
        },
        "S32" | "Enum" | "s32" => Byml::I32(i32::try_from(integer()?).map_err(|_| Error::new(format!("{t} is out of range for s32")))?),
        "U32" | "u32" => Byml::U32(u32::try_from(integer()?).map_err(|_| Error::new(format!("{t} is out of range for u32")))?),
        "F32" | "f32" => Byml::F32(num()? as f32),
        "f64" => Byml::F64(num()?),
        "s64" => Byml::I64(t.parse().map_err(|_| Error::new(format!("'{t}' is not a 64-bit integer")))?),
        "u64" => Byml::U64(t.parse().map_err(|_| Error::new(format!("'{t}' is not an unsigned 64-bit integer")))?),
        "Vec3f" | "vec3" => {
            let v: Vec<f32> = t
                .split([',', ' '])
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<f32>())
                .collect::<Result<_, _>>()
                .map_err(|e| Error::new(e.to_string()))?;
            if v.len() != 3 {
                return Err("expected three numbers".into());
            }
            Byml::floats(&v)
        }
        _ => Byml::Str(t.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_text() {
        let mut m = Byml::map();
        for (k, v) in [
            ("a", Byml::I32(-5)),
            ("b", Byml::U32(u32::MAX)),
            ("c", Byml::I64(i64::MIN)),
            ("d", Byml::U64(u64::MAX)),
            ("e", Byml::F64(0.1)),
            ("f", Byml::F32(f32::NAN)),
            ("g", Byml::F32(f32::NEG_INFINITY)),
            ("h", Byml::Bin(vec![0, 255, 7])),
            ("i", Byml::Hash64([(u64::MAX, Byml::Null)].into_iter().collect())),
            ("j", Byml::Hash32([(3, Byml::Bool(true))].into_iter().collect())),
            ("k", Byml::floats(&[1.5, -2.0, 0.0])),
        ] {
            m.insert(k, v);
        }
        let text = serde_json::to_string(&to_json(&m)).unwrap();
        let back = from_json(&serde_json::from_str(&text).unwrap()).unwrap();
        // NaN != NaN, so compare the bytes the writer produces
        let bytes = |b: &Byml| crate::byml::to_bytes(b, 7, false).unwrap();
        assert_eq!(bytes(&back), bytes(&m));
    }

    #[test]
    fn rejects_out_of_range_and_bad_tags() {
        assert!(from_json(&json!({"$s32": 3_000_000_000u64})).is_err());
        assert!(from_json(&json!({"$u32": -1})).is_err());
        assert!(from_json(&json!({"$h64": {"x": 1}})).is_err());
        assert!(from_json(&json!({"$bin": "!!"})).is_err());
        // a one-key object with an unknown tag is just a map
        assert!(matches!(from_json(&json!({"$other": 1})).unwrap(), Byml::Map(_)));
    }

    #[test]
    fn parses_panel_text() {
        assert_eq!(parse_text("Bool", "yes").unwrap(), Byml::Bool(true));
        assert!(parse_text("Bool", "maybe").is_err());
        assert_eq!(parse_text("S32", " -3 ").unwrap(), Byml::I32(-3));
        assert!(parse_text("S32", "1.5").is_err());
        assert!(parse_text("U32", "-1").is_err());
        assert_eq!(parse_text("u64", "18446744073709551615").unwrap(), Byml::U64(u64::MAX));
        assert_eq!(parse_text("Vec3f", "1, 2 3").unwrap(), Byml::floats(&[1.0, 2.0, 3.0]));
        assert!(parse_text("vec3", "1,2").is_err());
        assert_eq!(parse_text("String", " x ").unwrap(), Byml::Str("x".into()));
    }
}

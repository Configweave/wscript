//! `json` — parse/serialize JSON via the shared `Value` type (PRD §7).

use wscript_core::Module;

use crate::value::DynValue;

fn from_json(v: serde_json::Value) -> DynValue {
    match v {
        serde_json::Value::Null => DynValue::Null,
        serde_json::Value::Bool(b) => DynValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DynValue::Int(i)
            } else {
                DynValue::Float(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        serde_json::Value::String(s) => DynValue::String(s),
        serde_json::Value::Array(items) => {
            DynValue::List(items.into_iter().map(from_json).collect())
        }
        serde_json::Value::Object(entries) => DynValue::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k, from_json(v)))
                .collect(),
        ),
    }
}

pub(crate) fn to_json(v: &DynValue) -> serde_json::Value {
    match v {
        DynValue::Null => serde_json::Value::Null,
        DynValue::Bool(b) => serde_json::Value::Bool(*b),
        DynValue::Int(n) => serde_json::Value::Number((*n).into()),
        DynValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        DynValue::String(s) => serde_json::Value::String(s.clone()),
        DynValue::List(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
        DynValue::Map(entries) => {
            // Sorted for deterministic output.
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort();
            serde_json::Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), to_json(&entries[k])))
                    .collect(),
            )
        }
    }
}

pub fn json() -> Module {
    let mut m = Module::new("json");
    m.doc("JSON parsing and serialization (shared Value type)");
    m.fn_("parse", |s: &str| -> Result<DynValue, String> {
        serde_json::from_str::<serde_json::Value>(s)
            .map(from_json)
            .map_err(|e| e.to_string())
    });
    m.fn_("to_string", |v: DynValue| to_json(&v).to_string());
    m.fn_("to_string_pretty", |v: DynValue| {
        serde_json::to_string_pretty(&to_json(&v)).unwrap_or_default()
    });
    m
}

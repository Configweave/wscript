//! `toml` — parse/serialize TOML via the shared `Value` type (PRD §7).
//!
//! Serialization requires a map at the top level (TOML documents are
//! tables) — hence `Result` returns, unlike `json::to_string`.

use wscript_core::Module;

use crate::value::DynValue;

fn from_toml(v: toml::Value) -> DynValue {
    match v {
        toml::Value::String(s) => DynValue::String(s),
        toml::Value::Integer(n) => DynValue::Int(n),
        toml::Value::Float(f) => DynValue::Float(f),
        toml::Value::Boolean(b) => DynValue::Bool(b),
        // Datetimes flatten to strings (documented mapping).
        toml::Value::Datetime(dt) => DynValue::String(dt.to_string()),
        toml::Value::Array(items) => DynValue::List(items.into_iter().map(from_toml).collect()),
        toml::Value::Table(entries) => DynValue::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k, from_toml(v)))
                .collect(),
        ),
    }
}

fn to_toml(v: &DynValue) -> Result<toml::Value, String> {
    Ok(match v {
        DynValue::Null => return Err("TOML has no null value".into()),
        DynValue::Bool(b) => toml::Value::Boolean(*b),
        DynValue::Int(n) => toml::Value::Integer(*n),
        DynValue::Float(f) => toml::Value::Float(*f),
        DynValue::String(s) => toml::Value::String(s.clone()),
        DynValue::List(items) => {
            let converted: Result<Vec<toml::Value>, String> = items.iter().map(to_toml).collect();
            toml::Value::Array(converted?)
        }
        DynValue::Map(entries) => {
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort();
            let mut table = toml::map::Map::new();
            for k in keys {
                table.insert(k.clone(), to_toml(&entries[k])?);
            }
            toml::Value::Table(table)
        }
    })
}

pub fn toml() -> Module {
    let mut m = Module::new("toml");
    m.doc("TOML parsing and serialization (shared Value type)");
    m.fn_("parse", |s: &str| -> Result<DynValue, String> {
        s.parse::<toml::Value>()
            .map(from_toml)
            .map_err(|e| e.to_string())
    });
    m.fn_("to_string", |v: DynValue| -> Result<String, String> {
        let t = to_toml(&v)?;
        toml::to_string(&t).map_err(|e| e.to_string())
    });
    m.fn_(
        "to_string_pretty",
        |v: DynValue| -> Result<String, String> {
            let t = to_toml(&v)?;
            toml::to_string_pretty(&t).map_err(|e| e.to_string())
        },
    );
    m
}

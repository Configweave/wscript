//! The shared dynamic `Value` type (PRD §7): json/toml/xml all parse into
//! this one enum so formats compose (parse JSON, emit TOML). It is the
//! *one* dynamically-typed escape hatch in the language; accessors return
//! `Option`.

use std::collections::HashMap;

use wscript_core::Module;
use wscript_macros::Script;

/// Script-side name: `Value`.
#[derive(Script, Debug, Clone, PartialEq)]
#[script(crate_path = "wscript_core", name = "Value")]
pub enum DynValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<DynValue>),
    Map(HashMap<String, DynValue>),
}

impl DynValue {
    pub fn get(&self, key: &str) -> Option<&DynValue> {
        match self {
            DynValue::Map(m) => m.get(key),
            _ => None,
        }
    }
}

/// The `value` module: registers the `Value` enum and its accessor
/// methods. json/toml/xml reference the type in their signatures, but the
/// accessors come from registering this module.
pub fn value() -> Module {
    let mut m = Module::new("value");
    m.doc("The shared dynamic Value type used by json/toml/xml");
    m.ty::<DynValue>()
        .method("get", |v: &DynValue, key: &str| -> Option<DynValue> {
            v.get(key).cloned()
        })
        .method("at", |v: &DynValue, idx: i64| -> Option<DynValue> {
            match v {
                DynValue::List(items) if idx >= 0 => items.get(idx as usize).cloned(),
                _ => None,
            }
        })
        .method("keys", |v: &DynValue| -> Vec<String> {
            match v {
                DynValue::Map(m) => {
                    let mut keys: Vec<String> = m.keys().cloned().collect();
                    keys.sort();
                    keys
                }
                _ => Vec::new(),
            }
        })
        .method("len", |v: &DynValue| -> i64 {
            match v {
                DynValue::List(items) => items.len() as i64,
                DynValue::Map(m) => m.len() as i64,
                DynValue::String(s) => s.chars().count() as i64,
                _ => 0,
            }
        })
        .method("is_null", |v: &DynValue| matches!(v, DynValue::Null))
        .method("as_bool", |v: &DynValue| -> Option<bool> {
            match v {
                DynValue::Bool(b) => Some(*b),
                _ => None,
            }
        })
        .method("as_int", |v: &DynValue| -> Option<i64> {
            match v {
                DynValue::Int(n) => Some(*n),
                _ => None,
            }
        })
        .method("as_float", |v: &DynValue| -> Option<f64> {
            match v {
                DynValue::Float(f) => Some(*f),
                DynValue::Int(n) => Some(*n as f64),
                _ => None,
            }
        })
        .method("as_string", |v: &DynValue| -> Option<String> {
            match v {
                DynValue::String(s) => Some(s.clone()),
                _ => None,
            }
        })
        .method("as_list", |v: &DynValue| -> Option<Vec<DynValue>> {
            match v {
                DynValue::List(items) => Some(items.clone()),
                _ => None,
            }
        })
        .method(
            "as_map",
            |v: &DynValue| -> Option<HashMap<String, DynValue>> {
                match v {
                    DynValue::Map(m) => Some(m.clone()),
                    _ => None,
                }
            },
        );
    m
}

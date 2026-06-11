//! Host-boundary types: errors, the callable interface for registered
//! functions, and the `FromValue` / `IntoValue` conversion traits (PRD §6.3).
//!
//! The conversion impls here cover primitives and `String`; containers,
//! tuples and derived types are added by the interop layer (M4) and the
//! `#[derive(Script)]` macro.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::defs::{DEF_OPTION, DEF_RESULT, DefTable, TAG_ERR, TAG_NONE, TAG_OK, TAG_SOME};
use crate::types::Type;
use crate::value::{Key, Value};

/// An error crossing the host boundary. Host `Err` returns become script
/// `Result::Err(string)` values; conversion failures and aliasing
/// violations also land here (PRD §6.5: `Err`, never panics).
#[derive(Debug, Clone)]
pub struct HostError {
    pub message: String,
}

impl HostError {
    pub fn msg(message: impl Into<String>) -> HostError {
        HostError {
            message: message.into(),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HostError {}

/// Services the VM exposes to host functions while they run.
pub trait HostCtx {
    fn defs(&self) -> &DefTable;
    /// Render a value the way `print` would.
    fn display_value(&self, v: &Value) -> String;
}

/// A registered host function, type-erased. The typed registration sugar
/// (`Module::fn_`) wraps plain Rust closures into this via `FromValue` /
/// `IntoValue`. `Send + Sync` so a `Context` can be shared across threads
/// (PRD §4.3) — the *values* never cross threads, only the code.
pub trait HostCallable: Send + Sync {
    fn call(&self, ctx: &mut dyn HostCtx, args: Vec<Value>) -> Result<Value, HostError>;
}

impl<F> HostCallable for F
where
    F: Fn(&mut dyn HostCtx, Vec<Value>) -> Result<Value, HostError> + Send + Sync,
{
    fn call(&self, ctx: &mut dyn HostCtx, args: Vec<Value>) -> Result<Value, HostError> {
        self(ctx, args)
    }
}

/// Conversion from a script value to a Rust value.
pub trait FromValue: Sized {
    fn from_value(v: Value) -> Result<Self, HostError>;
}

/// Conversion from a Rust value into a script value.
pub trait IntoValue {
    fn into_value(self) -> Result<Value, HostError>;
}

fn type_mismatch(expected: &str, got: &Value) -> HostError {
    HostError::msg(format!(
        "type mismatch at host boundary: expected {expected}, got {}",
        got.kind_name()
    ))
}

macro_rules! prim_conv {
    ($rust:ty, $variant:ident, $name:literal) => {
        impl FromValue for $rust {
            fn from_value(v: Value) -> Result<Self, HostError> {
                match v {
                    Value::$variant(x) => Ok(x),
                    other => Err(type_mismatch($name, &other)),
                }
            }
        }
        impl IntoValue for $rust {
            fn into_value(self) -> Result<Value, HostError> {
                Ok(Value::$variant(self))
            }
        }
    };
}

prim_conv!(i64, Int, "int");
prim_conv!(f64, Float, "float");
prim_conv!(bool, Bool, "bool");
prim_conv!(char, Char, "char");

impl FromValue for () {
    fn from_value(v: Value) -> Result<Self, HostError> {
        match v {
            Value::Unit => Ok(()),
            other => Err(type_mismatch("unit", &other)),
        }
    }
}

impl IntoValue for () {
    fn into_value(self) -> Result<Value, HostError> {
        Ok(Value::Unit)
    }
}

impl FromValue for String {
    fn from_value(v: Value) -> Result<Self, HostError> {
        match v {
            Value::Str(s) => Ok(s.to_string()),
            other => Err(type_mismatch("string", &other)),
        }
    }
}

impl IntoValue for String {
    fn into_value(self) -> Result<Value, HostError> {
        Ok(Value::Str(Rc::from(self.as_str())))
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Result<Value, HostError> {
        Ok(Value::Str(Rc::from(self)))
    }
}

impl FromValue for Value {
    fn from_value(v: Value) -> Result<Self, HostError> {
        Ok(v)
    }
}

impl IntoValue for Value {
    fn into_value(self) -> Result<Value, HostError> {
        Ok(self)
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(v: Value) -> Result<Self, HostError> {
        match v {
            Value::List(items) => {
                let items = items.borrow();
                items.iter().map(|v| T::from_value(v.clone())).collect()
            }
            other => Err(type_mismatch("List", &other)),
        }
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Result<Value, HostError> {
        let items: Result<Vec<Value>, HostError> =
            self.into_iter().map(IntoValue::into_value).collect();
        Ok(Value::new_list(items?))
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: Value) -> Result<Self, HostError> {
        match &v {
            Value::Enum(e) if e.def == DEF_OPTION => {
                if e.tag == TAG_NONE {
                    Ok(None)
                } else {
                    let payload = e.fields.borrow()[0].clone();
                    Ok(Some(T::from_value(payload)?))
                }
            }
            _ => Err(type_mismatch("Option", &v)),
        }
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Result<Value, HostError> {
        match self {
            None => Ok(Value::new_enum(DEF_OPTION, TAG_NONE, vec![])),
            Some(x) => Ok(Value::new_enum(DEF_OPTION, TAG_SOME, vec![x.into_value()?])),
        }
    }
}

impl<T: FromValue, E: FromValue> FromValue for Result<T, E> {
    fn from_value(v: Value) -> Result<Self, HostError> {
        match &v {
            Value::Enum(e) if e.def == DEF_RESULT => {
                let payload = e.fields.borrow()[0].clone();
                if e.tag == TAG_OK {
                    Ok(Ok(T::from_value(payload)?))
                } else {
                    Ok(Err(E::from_value(payload)?))
                }
            }
            _ => Err(type_mismatch("Result", &v)),
        }
    }
}

impl<T: IntoValue, E: IntoValue> IntoValue for Result<T, E> {
    fn into_value(self) -> Result<Value, HostError> {
        match self {
            Ok(x) => Ok(Value::new_enum(DEF_RESULT, TAG_OK, vec![x.into_value()?])),
            Err(e) => Ok(Value::new_enum(DEF_RESULT, TAG_ERR, vec![e.into_value()?])),
        }
    }
}

/// Marker for the wisp-permitted map key types (`int`, `bool`, `char`,
/// `string`).
pub trait MapKeyType: FromValue + IntoValue {
    fn into_key(self) -> Result<Key, HostError>;
    fn from_key(k: Key) -> Result<Self, HostError>;
}

macro_rules! map_key {
    ($rust:ty) => {
        impl MapKeyType for $rust {
            fn into_key(self) -> Result<Key, HostError> {
                let v = self.into_value()?;
                Key::from_value(&v).ok_or_else(|| HostError::msg("invalid map key"))
            }
            fn from_key(k: Key) -> Result<Self, HostError> {
                Self::from_value(k.to_value())
            }
        }
    };
}

map_key!(i64);
map_key!(bool);
map_key!(char);
map_key!(String);

impl<K, V, S> FromValue for std::collections::HashMap<K, V, S>
where
    K: MapKeyType + std::hash::Hash + Eq,
    V: FromValue,
    S: std::hash::BuildHasher + Default,
{
    fn from_value(v: Value) -> Result<Self, HostError> {
        match v {
            Value::Map(entries) => entries
                .borrow()
                .iter()
                .map(|(k, v)| Ok((K::from_key(k.clone())?, V::from_value(v.clone())?)))
                .collect(),
            other => Err(type_mismatch("Map", &other)),
        }
    }
}

impl<K, V, S> IntoValue for std::collections::HashMap<K, V, S>
where
    K: MapKeyType,
    V: IntoValue,
    S: std::hash::BuildHasher,
{
    fn into_value(self) -> Result<Value, HostError> {
        let mut out = BTreeMap::new();
        for (k, v) in self {
            out.insert(k.into_key()?, v.into_value()?);
        }
        Ok(Value::new_map(out))
    }
}

/// A Rust type with a wisp-level type, used to capture host signatures at
/// registration time so the checker can verify call sites (PRD §6.1).
/// Implemented for primitives/containers here and by `#[derive(Script)]`.
pub trait ScriptType {
    /// The wisp type this Rust type appears as in signatures. `defs` is
    /// consulted/extended for nominal (derived) types.
    fn script_type(defs: &mut DefTable) -> Type;
}

macro_rules! script_type {
    ($rust:ty, $t:expr) => {
        impl ScriptType for $rust {
            fn script_type(_defs: &mut DefTable) -> Type {
                $t
            }
        }
    };
}

script_type!(i64, Type::Int);
script_type!(f64, Type::Float);
script_type!(bool, Type::Bool);
script_type!(char, Type::Char);
script_type!((), Type::Unit);
script_type!(String, Type::Str);
script_type!(&str, Type::Str);

impl<T: ScriptType> ScriptType for Vec<T> {
    fn script_type(defs: &mut DefTable) -> Type {
        Type::List(Box::new(T::script_type(defs)))
    }
}

impl<T: ScriptType> ScriptType for Option<T> {
    fn script_type(defs: &mut DefTable) -> Type {
        Type::Option(Box::new(T::script_type(defs)))
    }
}

impl<T: ScriptType, E: ScriptType> ScriptType for Result<T, E> {
    fn script_type(defs: &mut DefTable) -> Type {
        let t = T::script_type(defs);
        let e = E::script_type(defs);
        Type::Result(Box::new(t), Box::new(e))
    }
}

impl<K: ScriptType, V: ScriptType, S> ScriptType for std::collections::HashMap<K, V, S> {
    fn script_type(defs: &mut DefTable) -> Type {
        let k = K::script_type(defs);
        let v = V::script_type(defs);
        Type::Map(Box::new(k), Box::new(v))
    }
}

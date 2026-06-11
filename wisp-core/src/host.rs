//! Host-boundary types: errors, the callable interface for registered
//! functions, and the `FromValue` / `IntoValue` conversion traits (PRD §6.3).
//!
//! Conversions receive the context's [`DefTable`] so `#[derive(Script)]`
//! types can locate their registered definition (by Rust `TypeId`) in
//! whatever context the value crosses.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::defs::{DEF_OPTION, DEF_RESULT, DefTable, TAG_ERR, TAG_NONE, TAG_OK, TAG_SOME};
use crate::types::Type;
use crate::value::{Key, Value};

/// An error crossing the host boundary. Conversion failures and aliasing
/// violations land here (PRD §6.5: `Err`, never panics).
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
    fn from_value(v: Value, defs: &DefTable) -> Result<Self, HostError>;
}

/// Conversion from a Rust value into a script value.
pub trait IntoValue {
    fn into_value(self, defs: &DefTable) -> Result<Value, HostError>;
}

pub fn type_mismatch(expected: &str, got: &Value) -> HostError {
    HostError::msg(format!(
        "type mismatch at host boundary: expected {expected}, got {}",
        got.kind_name()
    ))
}

macro_rules! prim_conv {
    ($rust:ty, $variant:ident, $name:literal) => {
        impl FromValue for $rust {
            fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
                match v {
                    Value::$variant(x) => Ok(x),
                    other => Err(type_mismatch($name, &other)),
                }
            }
        }
        impl IntoValue for $rust {
            fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
                Ok(Value::$variant(self))
            }
        }
    };
}

prim_conv!(i64, Int, "int");
prim_conv!(f64, Float, "float");
prim_conv!(bool, Bool, "bool");
prim_conv!(char, Char, "char");

macro_rules! int_conv {
    ($rust:ty) => {
        impl FromValue for $rust {
            fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
                match v {
                    Value::Int(x) => <$rust>::try_from(x).map_err(|_| {
                        HostError::msg(format!(
                            "int {x} does not fit in {}",
                            stringify!($rust)
                        ))
                    }),
                    other => Err(type_mismatch("int", &other)),
                }
            }
        }
        impl IntoValue for $rust {
            fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
                Ok(Value::Int(self as i64))
            }
        }
        impl ScriptType for $rust {
            fn script_type(_defs: &mut DefTable) -> Type {
                Type::Int
            }
        }
    };
}

int_conv!(i8);
int_conv!(i16);
int_conv!(i32);
int_conv!(u8);
int_conv!(u16);
int_conv!(u32);
int_conv!(usize);

impl FromValue for f32 {
    fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
        match v {
            Value::Float(x) => Ok(x as f32),
            other => Err(type_mismatch("float", &other)),
        }
    }
}

impl IntoValue for f32 {
    fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
        Ok(Value::Float(self as f64))
    }
}

impl ScriptType for f32 {
    fn script_type(_defs: &mut DefTable) -> Type {
        Type::Float
    }
}

impl FromValue for () {
    fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
        match v {
            Value::Unit => Ok(()),
            other => Err(type_mismatch("unit", &other)),
        }
    }
}

impl IntoValue for () {
    fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
        Ok(Value::Unit)
    }
}

impl FromValue for String {
    fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
        match v {
            Value::Str(s) => Ok(s.to_string()),
            other => Err(type_mismatch("string", &other)),
        }
    }
}

impl IntoValue for String {
    fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
        Ok(Value::Str(Rc::from(self.as_str())))
    }
}

impl IntoValue for &str {
    fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
        Ok(Value::Str(Rc::from(self)))
    }
}

impl FromValue for Value {
    fn from_value(v: Value, _defs: &DefTable) -> Result<Self, HostError> {
        Ok(v)
    }
}

impl IntoValue for Value {
    fn into_value(self, _defs: &DefTable) -> Result<Value, HostError> {
        Ok(self)
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(v: Value, defs: &DefTable) -> Result<Self, HostError> {
        match v {
            Value::List(items) => {
                let items = items.borrow();
                items
                    .iter()
                    .map(|v| T::from_value(v.clone(), defs))
                    .collect()
            }
            other => Err(type_mismatch("List", &other)),
        }
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self, defs: &DefTable) -> Result<Value, HostError> {
        let items: Result<Vec<Value>, HostError> =
            self.into_iter().map(|x| x.into_value(defs)).collect();
        Ok(Value::new_list(items?))
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: Value, defs: &DefTable) -> Result<Self, HostError> {
        match &v {
            Value::Enum(e) if e.def == DEF_OPTION => {
                if e.tag == TAG_NONE {
                    Ok(None)
                } else {
                    let payload = e.fields.borrow()[0].clone();
                    Ok(Some(T::from_value(payload, defs)?))
                }
            }
            _ => Err(type_mismatch("Option", &v)),
        }
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self, defs: &DefTable) -> Result<Value, HostError> {
        match self {
            None => Ok(Value::new_enum(DEF_OPTION, TAG_NONE, vec![])),
            Some(x) => Ok(Value::new_enum(
                DEF_OPTION,
                TAG_SOME,
                vec![x.into_value(defs)?],
            )),
        }
    }
}

impl<T: FromValue, E: FromValue> FromValue for Result<T, E> {
    fn from_value(v: Value, defs: &DefTable) -> Result<Self, HostError> {
        match &v {
            Value::Enum(e) if e.def == DEF_RESULT => {
                let payload = e.fields.borrow()[0].clone();
                if e.tag == TAG_OK {
                    Ok(Ok(T::from_value(payload, defs)?))
                } else {
                    Ok(Err(E::from_value(payload, defs)?))
                }
            }
            _ => Err(type_mismatch("Result", &v)),
        }
    }
}

impl<T: IntoValue, E: IntoValue> IntoValue for Result<T, E> {
    fn into_value(self, defs: &DefTable) -> Result<Value, HostError> {
        match self {
            Ok(x) => Ok(Value::new_enum(
                DEF_RESULT,
                TAG_OK,
                vec![x.into_value(defs)?],
            )),
            Err(e) => Ok(Value::new_enum(
                DEF_RESULT,
                TAG_ERR,
                vec![e.into_value(defs)?],
            )),
        }
    }
}

/// Marker for the wisp-permitted map key types (`int`, `bool`, `char`,
/// `string`).
pub trait MapKeyType: FromValue + IntoValue {
    fn into_key(self, defs: &DefTable) -> Result<Key, HostError>;
    fn from_key(k: Key, defs: &DefTable) -> Result<Self, HostError>;
}

macro_rules! map_key {
    ($rust:ty) => {
        impl MapKeyType for $rust {
            fn into_key(self, defs: &DefTable) -> Result<Key, HostError> {
                let v = self.into_value(defs)?;
                Key::from_value(&v).ok_or_else(|| HostError::msg("invalid map key"))
            }
            fn from_key(k: Key, defs: &DefTable) -> Result<Self, HostError> {
                Self::from_value(k.to_value(), defs)
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
    fn from_value(v: Value, defs: &DefTable) -> Result<Self, HostError> {
        match v {
            Value::Map(entries) => entries
                .borrow()
                .iter()
                .map(|(k, v)| {
                    Ok((
                        K::from_key(k.clone(), defs)?,
                        V::from_value(v.clone(), defs)?,
                    ))
                })
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
    fn into_value(self, defs: &DefTable) -> Result<Value, HostError> {
        let mut out = BTreeMap::new();
        for (k, v) in self {
            out.insert(k.into_key(defs)?, v.into_value(defs)?);
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

/// Marker for `#[derive(Script)] #[script(opaque)]` handle types: they
/// cross the boundary by reference (a live handle), expose methods only,
/// and are the valid receivers of `Module::ty::<T>().method(...)`.
pub trait ScriptOpaque: ScriptType + Sized + 'static {}

// ----------------------------------------------- derive(Script) support

/// Find-or-register a host struct def (used by `#[derive(Script)]`).
/// `fields` runs only on first registration (placeholder-first, so
/// recursive types work). Panics on a script-visible name collision
/// between two different Rust types — a host programming error caught at
/// registration time.
pub fn register_host_struct(
    defs: &mut DefTable,
    name: &str,
    tid: std::any::TypeId,
    opaque: bool,
    fields: impl FnOnce(&mut DefTable) -> Vec<(String, Type)>,
) -> crate::defs::DefId {
    if let Some(id) = defs.by_rust_type(tid) {
        return id;
    }
    assert_name_free(defs, name);
    let id = defs.push(crate::defs::DefKind::Struct(crate::defs::StructDef {
        name: name.to_string(),
        fields: vec![],
        opaque,
        host: true,
        rust_type: Some(tid),
    }));
    let resolved = fields(defs);
    if let crate::defs::DefKind::Struct(sd) = &mut defs.defs[id.index()] {
        sd.fields = resolved;
    }
    id
}

/// Find-or-register a host enum def (used by `#[derive(Script)]`).
pub fn register_host_enum(
    defs: &mut DefTable,
    name: &str,
    tid: std::any::TypeId,
    variants: impl FnOnce(&mut DefTable) -> Vec<crate::defs::VariantDef>,
) -> crate::defs::DefId {
    if let Some(id) = defs.by_rust_type(tid) {
        return id;
    }
    assert_name_free(defs, name);
    let id = defs.push(crate::defs::DefKind::Enum(crate::defs::EnumDef {
        name: name.to_string(),
        variants: vec![],
        host: true,
        rust_type: Some(tid),
    }));
    let resolved = variants(defs);
    if let crate::defs::DefKind::Enum(ed) = &mut defs.defs[id.index()] {
        ed.variants = resolved;
    }
    id
}

fn assert_name_free(defs: &DefTable, name: &str) {
    let taken = defs.defs.iter().any(|d| match d {
        crate::defs::DefKind::Struct(s) => s.name == name,
        crate::defs::DefKind::Enum(e) => e.name == name,
        crate::defs::DefKind::Trait(t) => t.name == name,
    });
    assert!(
        !taken,
        "cannot register host type `{name}`: the name is already taken in this context"
    );
}

/// Locate the registered def for a derived type, for conversions.
pub fn lookup_def<T: 'static>(defs: &DefTable) -> Result<crate::defs::DefId, HostError> {
    defs.by_rust_type(std::any::TypeId::of::<T>())
        .ok_or_else(|| {
            HostError::msg(format!(
                "type {} is not registered in this context (register it with \
                 Module::ty or Context::register_type)",
                std::any::type_name::<T>()
            ))
        })
}

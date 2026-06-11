//! The runtime value model (PRD §4, §5.2).
//!
//! Primitives live inline in registers; everything else is an `Rc`-managed
//! reference type with free aliased mutation (interior `RefCell`s). Values
//! are deliberately `!Send` — one VM per thread (PRD §4.3).

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::{Rc, Weak};

use crate::bytecode::Const;
use crate::defs::{DefId, DefKind, DefTable, VariantKind};

/// A runtime value.
#[derive(Debug, Clone)]
pub enum Value {
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    /// Immutable string; `Rc` makes clones cheap (PRD §3.2).
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    /// Ordered map for deterministic iteration. Key types are restricted by
    /// the checker to `int`, `bool`, `char`, `string`.
    Map(Rc<RefCell<BTreeMap<Key, Value>>>),
    Struct(Rc<StructInstance>),
    Enum(Rc<EnumInstance>),
    Closure(Rc<Closure>),
    /// Opaque host handle (`#[script(opaque)]`).
    Opaque(Rc<OpaqueCell>),
    /// A concrete value coerced to `dyn Trait`, carrying its vtable id.
    Dyn(Rc<DynObj>),
    /// `weak[T]`.
    WeakRef(WeakValue),
    /// Internal: a mutable box for closure-captured locals. Never observable
    /// as a script type; only `CellGet`/`CellSet`/`MakeClosure` touch it.
    Cell(Rc<RefCell<Value>>),
}

/// Map key — the hashable/orderable subset of values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Int(i64),
    Bool(bool),
    Char(char),
    Str(Rc<str>),
}

impl Key {
    pub fn from_value(v: &Value) -> Option<Key> {
        match v {
            Value::Int(n) => Some(Key::Int(*n)),
            Value::Bool(b) => Some(Key::Bool(*b)),
            Value::Char(c) => Some(Key::Char(*c)),
            Value::Str(s) => Some(Key::Str(s.clone())),
            _ => None,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Key::Int(n) => Value::Int(*n),
            Key::Bool(b) => Value::Bool(*b),
            Key::Char(c) => Value::Char(*c),
            Key::Str(s) => Value::Str(s.clone()),
        }
    }
}

#[derive(Debug)]
pub struct StructInstance {
    pub def: DefId,
    pub fields: RefCell<Vec<Value>>,
}

#[derive(Debug)]
pub struct EnumInstance {
    pub def: DefId,
    pub tag: u32,
    pub fields: RefCell<Vec<Value>>,
}

#[derive(Debug)]
pub struct Closure {
    pub proto: u32,
    pub captures: Vec<Rc<RefCell<Value>>>,
}

/// A live host value held by handle. Borrow conflicts at the host boundary
/// surface as `Err`, never panics (PRD §6.5) — hence `try_borrow` at every
/// access site.
pub struct OpaqueCell {
    pub def: DefId,
    pub cell: RefCell<Box<dyn Any>>,
}

impl fmt::Debug for OpaqueCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpaqueCell(def={:?})", self.def)
    }
}

#[derive(Debug)]
pub struct DynObj {
    pub vtable: u32,
    pub inner: Value,
}

/// The weak counterpart of each reference-type payload.
#[derive(Debug, Clone)]
pub enum WeakValue {
    List(Weak<RefCell<Vec<Value>>>),
    Map(Weak<RefCell<BTreeMap<Key, Value>>>),
    Struct(Weak<StructInstance>),
    Enum(Weak<EnumInstance>),
    Closure(Weak<Closure>),
    Opaque(Weak<OpaqueCell>),
    Dyn(Weak<DynObj>),
}

impl WeakValue {
    pub fn upgrade(&self) -> Option<Value> {
        match self {
            WeakValue::List(w) => w.upgrade().map(Value::List),
            WeakValue::Map(w) => w.upgrade().map(Value::Map),
            WeakValue::Struct(w) => w.upgrade().map(Value::Struct),
            WeakValue::Enum(w) => w.upgrade().map(Value::Enum),
            WeakValue::Closure(w) => w.upgrade().map(Value::Closure),
            WeakValue::Opaque(w) => w.upgrade().map(Value::Opaque),
            WeakValue::Dyn(w) => w.upgrade().map(Value::Dyn),
        }
    }
}

impl Value {
    pub fn from_const(c: &Const) -> Value {
        match c {
            Const::Unit => Value::Unit,
            Const::Int(n) => Value::Int(*n),
            Const::Float(f) => Value::Float(*f),
            Const::Bool(b) => Value::Bool(*b),
            Const::Char(c) => Value::Char(*c),
            Const::Str(s) => Value::Str(Rc::from(&**s)),
        }
    }

    pub fn new_struct(def: DefId, fields: Vec<Value>) -> Value {
        Value::Struct(Rc::new(StructInstance {
            def,
            fields: RefCell::new(fields),
        }))
    }

    pub fn new_enum(def: DefId, tag: u32, fields: Vec<Value>) -> Value {
        Value::Enum(Rc::new(EnumInstance {
            def,
            tag,
            fields: RefCell::new(fields),
        }))
    }

    pub fn new_list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn new_map(entries: BTreeMap<Key, Value>) -> Value {
        Value::Map(Rc::new(RefCell::new(entries)))
    }

    /// `weak(x)`: downgrade. Returns `None` for non-reference values (the
    /// checker rejects those; this is the runtime backstop).
    pub fn downgrade(&self) -> Option<WeakValue> {
        match self {
            Value::List(rc) => Some(WeakValue::List(Rc::downgrade(rc))),
            Value::Map(rc) => Some(WeakValue::Map(Rc::downgrade(rc))),
            Value::Struct(rc) => Some(WeakValue::Struct(Rc::downgrade(rc))),
            Value::Enum(rc) => Some(WeakValue::Enum(Rc::downgrade(rc))),
            Value::Closure(rc) => Some(WeakValue::Closure(Rc::downgrade(rc))),
            Value::Opaque(rc) => Some(WeakValue::Opaque(Rc::downgrade(rc))),
            Value::Dyn(rc) => Some(WeakValue::Dyn(Rc::downgrade(rc))),
            _ => None,
        }
    }

    /// `same(a, b)`: reference identity for reference types; value equality
    /// for primitives (documented behaviour of the builtin).
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Unit, Value::Unit) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => Rc::ptr_eq(a, b),
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
            (Value::Struct(a), Value::Struct(b)) => Rc::ptr_eq(a, b),
            (Value::Enum(a), Value::Enum(b)) => Rc::ptr_eq(a, b),
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::Opaque(a), Value::Opaque(b)) => Rc::ptr_eq(a, b),
            (Value::Dyn(a), Value::Dyn(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Human-readable rendering used by `print`, `str()` and derived
    /// `Display` (debug-ish for structs/enums, PRD §3.8). Top-level strings
    /// render bare; strings nested in structures render quoted.
    pub fn display(&self, defs: &DefTable) -> String {
        let mut out = String::new();
        self.fmt_into(defs, &mut out, false);
        out
    }

    fn fmt_into(&self, defs: &DefTable, out: &mut String, nested: bool) {
        use std::fmt::Write;
        match self {
            Value::Unit => out.push_str("()"),
            Value::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Value::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    let _ = write!(out, "{f:.1}");
                } else {
                    let _ = write!(out, "{f}");
                }
            }
            Value::Bool(b) => {
                let _ = write!(out, "{b}");
            }
            Value::Char(c) => {
                if nested {
                    let _ = write!(out, "{c:?}");
                } else {
                    out.push(*c);
                }
            }
            Value::Str(s) => {
                if nested {
                    let _ = write!(out, "{s:?}");
                } else {
                    out.push_str(s);
                }
            }
            Value::List(items) => {
                out.push('[');
                for (i, v) in items.borrow().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    v.fmt_into(defs, out, true);
                }
                out.push(']');
            }
            Value::Map(entries) => {
                out.push_str("#{");
                for (i, (k, v)) in entries.borrow().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    k.to_value().fmt_into(defs, out, true);
                    out.push_str(": ");
                    v.fmt_into(defs, out, true);
                }
                out.push('}');
            }
            Value::Struct(s) => {
                let name = defs.name_of(s.def).to_string();
                out.push_str(&name);
                if let Some(DefKind::Struct(sd)) = defs.defs.get(s.def.index())
                    && sd.opaque
                {
                    out.push_str(" { <opaque> }");
                    return;
                }
                out.push_str(" { ");
                let field_names: Vec<String> = defs
                    .as_struct(s.def)
                    .map(|sd| sd.fields.iter().map(|(n, _)| n.clone()).collect())
                    .unwrap_or_default();
                for (i, v) in s.fields.borrow().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if let Some(n) = field_names.get(i) {
                        out.push_str(n);
                        out.push_str(": ");
                    }
                    v.fmt_into(defs, out, true);
                }
                out.push_str(" }");
            }
            Value::Enum(e) => {
                let (variant, kind, names) = match defs.as_enum(e.def) {
                    Some(ed) => {
                        let v = &ed.variants[e.tag as usize];
                        (
                            v.name.clone(),
                            v.kind,
                            v.fields.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
                        )
                    }
                    None => (format!("<tag {}>", e.tag), VariantKind::Tuple, vec![]),
                };
                // Builtin Option/Result render without the enum name prefix
                // (matches how scripts write them: `Some(1)`, not
                // `Option::Some(1)`).
                let enum_name = defs.name_of(e.def);
                if enum_name != "Option" && enum_name != "Result" {
                    out.push_str(enum_name);
                    out.push_str("::");
                }
                out.push_str(&variant);
                let fields = e.fields.borrow();
                match kind {
                    VariantKind::Unit => {}
                    VariantKind::Tuple => {
                        out.push('(');
                        for (i, v) in fields.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            v.fmt_into(defs, out, true);
                        }
                        out.push(')');
                    }
                    VariantKind::Struct => {
                        out.push_str(" { ");
                        for (i, v) in fields.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            if let Some(n) = names.get(i) {
                                out.push_str(n);
                                out.push_str(": ");
                            }
                            v.fmt_into(defs, out, true);
                        }
                        out.push_str(" }");
                    }
                }
            }
            Value::Closure(c) => {
                let _ = write!(out, "<fn #{}>", c.proto);
            }
            Value::Opaque(o) => {
                let _ = write!(out, "<{}>", defs.name_of(o.def));
            }
            Value::Dyn(d) => d.inner.fmt_into(defs, out, nested),
            Value::WeakRef(_) => out.push_str("<weak>"),
            Value::Cell(c) => c.borrow().fmt_into(defs, out, nested),
        }
    }

    /// Name of the value's runtime shape, for fault messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::Unit => "unit",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Char(_) => "char",
            Value::Str(_) => "string",
            Value::List(_) => "List",
            Value::Map(_) => "Map",
            Value::Struct(_) => "struct",
            Value::Enum(_) => "enum",
            Value::Closure(_) => "fn",
            Value::Opaque(_) => "opaque",
            Value::Dyn(_) => "dyn",
            Value::WeakRef(_) => "weak",
            Value::Cell(_) => "cell",
        }
    }
}

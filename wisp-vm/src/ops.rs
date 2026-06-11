//! Structural value operations: equality, ordering, deep clone, display.
//!
//! These descend values structurally; when they reach a struct/enum whose
//! type has a *custom* (non-derived) `Eq`/`Ord`/`Display` impl, they call
//! it through `CompiledUnit::impls`. Derived impls are pure structure.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::rc::Rc;

use wisp_core::defs::{DefKind, VariantKind};
use wisp_core::value::Value;

use crate::{RuntimeError, Vm};

impl Vm {
    pub(crate) fn value_eq(&mut self, a: &Value, b: &Value) -> Result<bool, RuntimeError> {
        Ok(match (a, b) {
            (Value::Unit, Value::Unit) => true,
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Char(x), Value::Char(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::List(x), Value::List(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                let xs = x.borrow().clone();
                let ys = y.borrow().clone();
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for (xv, yv) in xs.iter().zip(ys.iter()) {
                    if !self.value_eq(xv, yv)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Map(x), Value::Map(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                let xs = x.borrow().clone();
                let ys = y.borrow().clone();
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for ((kx, vx), (ky, vy)) in xs.iter().zip(ys.iter()) {
                    if kx != ky || !self.value_eq(vx, vy)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Struct(x), Value::Struct(y)) => {
                if x.def != y.def {
                    return Ok(false);
                }
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                if let Some(&proto) = self.unit().impls.eq.get(&x.def.0) {
                    let r = self.call_proto_nested(
                        proto,
                        vec![Value::Struct(x.clone()), Value::Struct(y.clone())],
                    )?;
                    return match r {
                        Value::Bool(b) => Ok(b),
                        _ => Err(self.fault("custom Eq impl did not return bool")),
                    };
                }
                let xs = x.fields.borrow().clone();
                let ys = y.fields.borrow().clone();
                for (xv, yv) in xs.iter().zip(ys.iter()) {
                    if !self.value_eq(xv, yv)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Enum(x), Value::Enum(y)) => {
                if x.def != y.def || x.tag != y.tag {
                    return Ok(false);
                }
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                if let Some(&proto) = self.unit().impls.eq.get(&x.def.0) {
                    let r = self.call_proto_nested(
                        proto,
                        vec![Value::Enum(x.clone()), Value::Enum(y.clone())],
                    )?;
                    return match r {
                        Value::Bool(b) => Ok(b),
                        _ => Err(self.fault("custom Eq impl did not return bool")),
                    };
                }
                let xs = x.fields.borrow().clone();
                let ys = y.fields.borrow().clone();
                for (xv, yv) in xs.iter().zip(ys.iter()) {
                    if !self.value_eq(xv, yv)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Dyn(x), Value::Dyn(y)) => {
                let (ix, iy) = (x.inner.clone(), y.inner.clone());
                self.value_eq(&ix, &iy)?
            }
            (Value::Closure(x), Value::Closure(y)) => Rc::ptr_eq(x, y),
            (Value::Opaque(x), Value::Opaque(y)) => Rc::ptr_eq(x, y),
            _ => false,
        })
    }

    /// Three-way comparison: -1 / 0 / 1.
    pub(crate) fn value_cmp(&mut self, a: &Value, b: &Value) -> Result<i64, RuntimeError> {
        let ord = match (a, b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.total_cmp(y),
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            (Value::Char(x), Value::Char(y)) => x.cmp(y),
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            (Value::Unit, Value::Unit) => Ordering::Equal,
            (Value::List(x), Value::List(y)) => {
                let xs = x.borrow().clone();
                let ys = y.borrow().clone();
                for (xv, yv) in xs.iter().zip(ys.iter()) {
                    let c = self.value_cmp(xv, yv)?;
                    if c != 0 {
                        return Ok(c);
                    }
                }
                xs.len().cmp(&ys.len())
            }
            (Value::Struct(x), Value::Struct(y)) if x.def == y.def => {
                if let Some(&proto) = self.unit().impls.cmp.get(&x.def.0) {
                    let r = self.call_proto_nested(
                        proto,
                        vec![Value::Struct(x.clone()), Value::Struct(y.clone())],
                    )?;
                    return match r {
                        Value::Int(n) => Ok(n.signum()),
                        _ => Err(self.fault("custom Ord impl did not return int")),
                    };
                }
                let xs = x.fields.borrow().clone();
                let ys = y.fields.borrow().clone();
                for (xv, yv) in xs.iter().zip(ys.iter()) {
                    let c = self.value_cmp(xv, yv)?;
                    if c != 0 {
                        return Ok(c);
                    }
                }
                Ordering::Equal
            }
            (Value::Enum(x), Value::Enum(y)) if x.def == y.def => {
                if let Some(&proto) = self.unit().impls.cmp.get(&x.def.0) {
                    let r = self.call_proto_nested(
                        proto,
                        vec![Value::Enum(x.clone()), Value::Enum(y.clone())],
                    )?;
                    return match r {
                        Value::Int(n) => Ok(n.signum()),
                        _ => Err(self.fault("custom Ord impl did not return int")),
                    };
                }
                match x.tag.cmp(&y.tag) {
                    Ordering::Equal => {
                        let xs = x.fields.borrow().clone();
                        let ys = y.fields.borrow().clone();
                        for (xv, yv) in xs.iter().zip(ys.iter()) {
                            let c = self.value_cmp(xv, yv)?;
                            if c != 0 {
                                return Ok(c);
                            }
                        }
                        Ordering::Equal
                    }
                    other => other,
                }
            }
            _ => {
                return Err(self.fault(format!(
                    "cannot order {} and {}",
                    a.kind_name(),
                    b.kind_name()
                )));
            }
        };
        Ok(match ord {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
    }

    /// Deep clone (derive `Clone`, PRD §3.8). Structs/enums/lists/maps are
    /// recreated recursively; strings are immutable (shared); functions,
    /// opaque handles, weak refs and dyn values copy the reference.
    pub(crate) fn deep_clone(&mut self, v: &Value) -> Result<Value, RuntimeError> {
        Ok(match v {
            Value::List(items) => {
                let snapshot = items.borrow().clone();
                let cloned: Result<Vec<Value>, RuntimeError> =
                    snapshot.iter().map(|x| self.deep_clone(x)).collect();
                Value::new_list(cloned?)
            }
            Value::Map(entries) => {
                let snapshot = entries.borrow().clone();
                let mut out = BTreeMap::new();
                for (k, x) in snapshot.iter() {
                    out.insert(k.clone(), self.deep_clone(x)?);
                }
                Value::new_map(out)
            }
            Value::Struct(s) => {
                let snapshot = s.fields.borrow().clone();
                let cloned: Result<Vec<Value>, RuntimeError> =
                    snapshot.iter().map(|x| self.deep_clone(x)).collect();
                Value::new_struct(s.def, cloned?)
            }
            Value::Enum(e) => {
                let snapshot = e.fields.borrow().clone();
                let cloned: Result<Vec<Value>, RuntimeError> =
                    snapshot.iter().map(|x| self.deep_clone(x)).collect();
                Value::new_enum(e.def, e.tag, cloned?)
            }
            other => other.clone(),
        })
    }

    /// Display with custom `Display` impls (used by print/str/fmt).
    pub(crate) fn display_value(&mut self, v: &Value) -> Result<String, RuntimeError> {
        let mut out = String::new();
        self.fmt_value(v, &mut out, false)?;
        Ok(out)
    }

    fn fmt_value(&mut self, v: &Value, out: &mut String, nested: bool) -> Result<(), RuntimeError> {
        // Custom Display impls take priority for nominal types.
        let custom = match v {
            Value::Struct(s) => self.unit().impls.display.get(&s.def.0).copied(),
            Value::Enum(e) => self.unit().impls.display.get(&e.def.0).copied(),
            Value::Dyn(d) => {
                let inner = d.inner.clone();
                return self.fmt_value(&inner, out, nested);
            }
            _ => None,
        };
        if let Some(proto) = custom {
            let r = self.call_proto_nested(proto, vec![v.clone()])?;
            return match r {
                Value::Str(s) => {
                    out.push_str(&s);
                    Ok(())
                }
                _ => Err(self.fault("custom Display impl did not return string")),
            };
        }
        match v {
            Value::List(items) => {
                out.push('[');
                let snapshot = items.borrow().clone();
                for (i, x) in snapshot.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    self.fmt_value(x, out, true)?;
                }
                out.push(']');
                Ok(())
            }
            Value::Map(entries) => {
                out.push_str("#{");
                let snapshot = entries.borrow().clone();
                for (i, (k, x)) in snapshot.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    self.fmt_value(&k.to_value(), out, true)?;
                    out.push_str(": ");
                    self.fmt_value(x, out, true)?;
                }
                out.push('}');
                Ok(())
            }
            Value::Struct(s) => {
                let (name, field_names) = {
                    let defs = &self.unit().defs;
                    let name = defs.name_of(s.def).to_string();
                    let names: Vec<String> = defs
                        .as_struct(s.def)
                        .map(|sd| sd.fields.iter().map(|(n, _)| n.clone()).collect())
                        .unwrap_or_default();
                    (name, names)
                };
                let opaque = matches!(
                    self.unit().defs.get(s.def),
                    DefKind::Struct(sd) if sd.opaque
                );
                out.push_str(&name);
                if opaque {
                    out.push_str(" { <opaque> }");
                    return Ok(());
                }
                out.push_str(" { ");
                let snapshot = s.fields.borrow().clone();
                for (i, x) in snapshot.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if let Some(n) = field_names.get(i) {
                        out.push_str(n);
                        out.push_str(": ");
                    }
                    self.fmt_value(x, out, true)?;
                }
                out.push_str(" }");
                Ok(())
            }
            Value::Enum(e) => {
                let (enum_name, vname, kind, field_names) = {
                    let defs = &self.unit().defs;
                    let enum_name = defs.name_of(e.def).to_string();
                    let (vname, kind, names) = defs
                        .as_enum(e.def)
                        .and_then(|ed| ed.variants.get(e.tag as usize))
                        .map(|vd| {
                            (
                                vd.name.clone(),
                                vd.kind,
                                vd.fields.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
                            )
                        })
                        .unwrap_or((format!("<tag {}>", e.tag), VariantKind::Tuple, vec![]));
                    (enum_name, vname, kind, names)
                };
                if enum_name != "Option" && enum_name != "Result" {
                    out.push_str(&enum_name);
                    out.push_str("::");
                }
                out.push_str(&vname);
                let snapshot = e.fields.borrow().clone();
                match kind {
                    VariantKind::Unit => {}
                    VariantKind::Tuple => {
                        out.push('(');
                        for (i, x) in snapshot.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            self.fmt_value(x, out, true)?;
                        }
                        out.push(')');
                    }
                    VariantKind::Struct => {
                        out.push_str(" { ");
                        for (i, x) in snapshot.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            if let Some(n) = field_names.get(i) {
                                out.push_str(n);
                                out.push_str(": ");
                            }
                            self.fmt_value(x, out, true)?;
                        }
                        out.push_str(" }");
                    }
                }
                Ok(())
            }
            other => {
                out.push_str(&other_display(other, self, nested));
                Ok(())
            }
        }
    }
}

fn other_display(v: &Value, vm: &Vm, nested: bool) -> String {
    // Primitives and remaining kinds: reuse the structural renderer.
    let defs = &vm.unit().defs;
    let mut s = v.display(defs);
    if nested && matches!(v, Value::Str(_) | Value::Char(_)) {
        s = match v {
            Value::Str(x) => format!("{x:?}"),
            Value::Char(c) => format!("{c:?}"),
            _ => s,
        };
    }
    s
}

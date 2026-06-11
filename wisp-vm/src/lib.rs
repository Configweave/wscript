//! The wisp virtual machine: a register-based bytecode interpreter
//! (PRD §5.2).
//!
//! One `Vm` per thread — values are `Rc`-managed and `!Send` (PRD §4.3).
//! VM-level faults (out-of-bounds indexing, division by zero, borrow
//! conflicts at the host boundary) are trappable [`RuntimeError`]s returned
//! to the host; script code never observes a panic (PRD §3.5).

mod builtins;
mod ops;

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use wisp_core::bytecode::{CallTarget, CompiledUnit, FaultCode, Instr};
use wisp_core::host::{HostCallable, HostCtx, HostError};
use wisp_core::registry::Registry;
use wisp_core::span::Span;
use wisp_core::value::{Closure, DynObj, Key, Value};

/// A trappable runtime fault. Carries the source span of the faulting
/// instruction and a script-level stack trace.
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub span: Option<Span>,
    /// Function names, innermost first.
    pub trace: Vec<String>,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runtime error: {}", self.message)?;
        for name in &self.trace {
            write!(f, "\n  in {name}")?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

// ------------------------------------------------------------ print hook
//
// `print`/`println` write to stdout by default, but an embedder may need
// to capture script output (a TUI host whose stdout is a live screen, or
// a tool whose stdout carries machine-readable output). The hook is
// per-thread, matching the one-`Vm`-per-thread model.

thread_local! {
    static PRINT_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&str, bool)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Install (or clear, with `None`) the current thread's print hook. While
/// set, `print`/`println` deliver their text to the hook instead of
/// stdout; the `bool` argument is `true` for `println` (trailing newline).
pub fn set_print_hook(hook: Option<Box<dyn FnMut(&str, bool)>>) {
    PRINT_HOOK.with(|h| *h.borrow_mut() = hook);
}

pub(crate) fn print_text(s: &str, newline: bool) {
    let hooked = PRINT_HOOK.with(|h| {
        if let Some(hook) = h.borrow_mut().as_mut() {
            hook(s, newline);
            true
        } else {
            false
        }
    });
    if !hooked {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(s.as_bytes());
        if newline {
            let _ = out.write_all(b"\n");
        }
        let _ = out.flush();
    }
}

struct UnitState {
    unit: CompiledUnit,
    consts: Vec<Value>,
}

struct Frame {
    proto: u32,
    /// Absolute stack index of register 0.
    base: usize,
    pc: usize,
    /// Absolute stack slot receiving the return value (`usize::MAX` for
    /// entry frames).
    ret_slot: usize,
    closure: Option<Rc<Closure>>,
}

/// A wisp virtual machine. Not `Send`: script values are `Rc`-managed
/// (PRD §4.3) — spin one `Vm` per thread from a shared `Context`.
pub struct Vm {
    host_fns: Vec<Arc<dyn HostCallable>>,
    units: Vec<UnitState>,
    unit_ids: HashMap<u64, usize>,
    cur_unit: usize,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    /// Recursion guard.
    depth_limit: usize,
}

impl Vm {
    pub fn new(registry: &Registry) -> Vm {
        Vm {
            host_fns: registry.host_fns.iter().map(|e| e.imp.clone()).collect(),
            units: Vec::new(),
            unit_ids: HashMap::new(),
            cur_unit: 0,
            stack: Vec::new(),
            frames: Vec::new(),
            depth_limit: 10_000,
        }
    }

    /// Load (or find the cached copy of) a compiled unit.
    pub fn load(&mut self, unit: &CompiledUnit) -> usize {
        if let Some(&idx) = self.unit_ids.get(&unit.id) {
            return idx;
        }
        let consts = unit.consts.iter().map(Value::from_const).collect();
        let idx = self.units.len();
        self.units.push(UnitState {
            unit: unit.clone(),
            consts,
        });
        self.unit_ids.insert(unit.id, idx);
        idx
    }

    /// Call a function by proto index with already-converted values. The
    /// typed wrappers live in the `wisp` umbrella crate.
    pub fn call_proto(
        &mut self,
        unit: &CompiledUnit,
        proto: u32,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        let unit_idx = self.load(unit);
        self.cur_unit = unit_idx;
        let entry_depth = self.frames.len();
        let base = self.stack_top();
        self.push_frame(proto, base, usize::MAX, None, &args)?;
        let result = self.execute(entry_depth);
        if result.is_err() {
            // Unwind frames left behind by the fault.
            while self.frames.len() > entry_depth {
                let f = self.frames.pop().unwrap();
                self.stack.truncate(f.base);
            }
        }
        result
    }

    /// Call a function by exported name (signature checking happens in the
    /// `wisp` umbrella crate at the typed boundary).
    pub fn call_name(
        &mut self,
        unit: &CompiledUnit,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        let Some(&(proto, _)) = unit.exports.get(name) else {
            return Err(RuntimeError {
                message: format!("no function named `{name}` in the compiled script"),
                span: None,
                trace: vec![],
            });
        };
        self.call_proto(unit, proto, args)
    }

    fn stack_top(&self) -> usize {
        match self.frames.last() {
            Some(f) => {
                let n_regs = self.units[self.cur_unit].unit.protos[f.proto as usize].n_regs;
                f.base + n_regs as usize
            }
            None => 0,
        }
    }

    fn push_frame(
        &mut self,
        proto: u32,
        base: usize,
        ret_slot: usize,
        closure: Option<Rc<Closure>>,
        args: &[Value],
    ) -> Result<(), RuntimeError> {
        if self.frames.len() >= self.depth_limit {
            return Err(self.fault("stack overflow: too many nested calls"));
        }
        let n_regs = self.units[self.cur_unit].unit.protos[proto as usize].n_regs as usize;
        if self.stack.len() < base + n_regs {
            self.stack.resize(base + n_regs, Value::Unit);
        }
        for (i, a) in args.iter().enumerate() {
            self.stack[base + i] = a.clone();
        }
        self.frames.push(Frame {
            proto,
            base,
            pc: 0,
            ret_slot,
            closure,
        });
        Ok(())
    }

    /// Call a function value from inside a builtin (map/filter/custom
    /// impls): runs a nested dispatch loop until the frame returns.
    pub(crate) fn call_function(
        &mut self,
        f: &Value,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match f {
            Value::Closure(c) => {
                let entry_depth = self.frames.len();
                let base = self.stack_top();
                self.push_frame(c.proto, base, usize::MAX, Some(c.clone()), &args)?;
                self.execute(entry_depth)
            }
            other => Err(self.fault(format!("cannot call a {} value", other.kind_name()))),
        }
    }

    pub(crate) fn call_proto_nested(
        &mut self,
        proto: u32,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        let entry_depth = self.frames.len();
        let base = self.stack_top();
        self.push_frame(proto, base, usize::MAX, None, &args)?;
        self.execute(entry_depth)
    }

    pub(crate) fn fault(&self, message: impl Into<String>) -> RuntimeError {
        let span = self.frames.last().and_then(|f| {
            let proto = &self.units[self.cur_unit].unit.protos[f.proto as usize];
            proto.spans.get(f.pc.saturating_sub(1)).copied()
        });
        let trace = self
            .frames
            .iter()
            .rev()
            .take(20)
            .map(|f| {
                self.units[self.cur_unit].unit.protos[f.proto as usize]
                    .name
                    .clone()
            })
            .collect();
        RuntimeError {
            message: message.into(),
            span,
            trace,
        }
    }

    pub(crate) fn unit(&self) -> &CompiledUnit {
        &self.units[self.cur_unit].unit
    }

    // --------------------------------------------------------- dispatch

    fn execute(&mut self, entry_depth: usize) -> Result<Value, RuntimeError> {
        macro_rules! reg {
            ($base:expr, $r:expr) => {
                self.stack[$base + $r as usize]
            };
        }
        macro_rules! prim {
            ($base:expr, $r:expr, $variant:ident, $what:literal) => {
                match &reg!($base, $r) {
                    Value::$variant(x) => *x,
                    other => {
                        return Err(self.fault(format!(
                            concat!("type confusion: expected ", $what, ", found {}"),
                            other.kind_name()
                        )));
                    }
                }
            };
        }
        macro_rules! int {
            ($b:expr, $r:expr) => {
                prim!($b, $r, Int, "int")
            };
        }
        macro_rules! float {
            ($b:expr, $r:expr) => {
                prim!($b, $r, Float, "float")
            };
        }
        macro_rules! boolean {
            ($b:expr, $r:expr) => {
                prim!($b, $r, Bool, "bool")
            };
        }
        macro_rules! ch {
            ($b:expr, $r:expr) => {
                prim!($b, $r, Char, "char")
            };
        }
        macro_rules! string {
            ($base:expr, $r:expr) => {
                match &reg!($base, $r) {
                    Value::Str(s) => s.clone(),
                    other => {
                        return Err(self.fault(format!(
                            "type confusion: expected string, found {}",
                            other.kind_name()
                        )));
                    }
                }
            };
        }

        loop {
            let (instr, base) = {
                let frame = self.frames.last().unwrap();
                let proto = &self.units[self.cur_unit].unit.protos[frame.proto as usize];
                if frame.pc >= proto.code.len() {
                    (Instr::RetUnit, frame.base)
                } else {
                    (proto.code[frame.pc], frame.base)
                }
            };
            self.frames.last_mut().unwrap().pc += 1;

            match instr {
                Instr::Nop => {}
                Instr::LoadConst { dst, k } => {
                    reg!(base, dst) = self.units[self.cur_unit].consts[k as usize].clone();
                }
                Instr::LoadUnit { dst } => reg!(base, dst) = Value::Unit,
                Instr::LoadBool { dst, v } => reg!(base, dst) = Value::Bool(v),
                Instr::LoadInt { dst, v } => reg!(base, dst) = Value::Int(v as i64),
                Instr::Move { dst, src } => {
                    reg!(base, dst) = reg!(base, src).clone();
                }

                Instr::AddI { dst, a, b } => {
                    reg!(base, dst) = Value::Int(int!(base, a).wrapping_add(int!(base, b)));
                }
                Instr::SubI { dst, a, b } => {
                    reg!(base, dst) = Value::Int(int!(base, a).wrapping_sub(int!(base, b)));
                }
                Instr::MulI { dst, a, b } => {
                    reg!(base, dst) = Value::Int(int!(base, a).wrapping_mul(int!(base, b)));
                }
                Instr::DivI { dst, a, b } => {
                    let d = int!(base, b);
                    if d == 0 {
                        return Err(self.fault("division by zero"));
                    }
                    reg!(base, dst) = Value::Int(int!(base, a).wrapping_div(d));
                }
                Instr::RemI { dst, a, b } => {
                    let d = int!(base, b);
                    if d == 0 {
                        return Err(self.fault("remainder by zero"));
                    }
                    reg!(base, dst) = Value::Int(int!(base, a).wrapping_rem(d));
                }
                Instr::NegI { dst, src } => {
                    reg!(base, dst) = Value::Int(int!(base, src).wrapping_neg());
                }

                Instr::AddF { dst, a, b } => {
                    reg!(base, dst) = Value::Float(float!(base, a) + float!(base, b));
                }
                Instr::SubF { dst, a, b } => {
                    reg!(base, dst) = Value::Float(float!(base, a) - float!(base, b));
                }
                Instr::MulF { dst, a, b } => {
                    reg!(base, dst) = Value::Float(float!(base, a) * float!(base, b));
                }
                Instr::DivF { dst, a, b } => {
                    reg!(base, dst) = Value::Float(float!(base, a) / float!(base, b));
                }
                Instr::RemF { dst, a, b } => {
                    reg!(base, dst) = Value::Float(float!(base, a) % float!(base, b));
                }
                Instr::NegF { dst, src } => {
                    reg!(base, dst) = Value::Float(-float!(base, src));
                }

                Instr::ConcatStr { dst, a, b } => {
                    let a = string!(base, a);
                    let b = string!(base, b);
                    let mut s = String::with_capacity(a.len() + b.len());
                    s.push_str(&a);
                    s.push_str(&b);
                    reg!(base, dst) = Value::Str(Rc::from(s.as_str()));
                }

                Instr::Not { dst, src } => {
                    reg!(base, dst) = Value::Bool(!boolean!(base, src));
                }

                Instr::EqI { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(int!(base, a) == int!(base, b));
                }
                Instr::EqF { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(float!(base, a) == float!(base, b));
                }
                Instr::EqBool { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(boolean!(base, a) == boolean!(base, b));
                }
                Instr::EqChar { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(ch!(base, a) == ch!(base, b));
                }
                Instr::EqStr { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(string!(base, a) == string!(base, b));
                }
                Instr::LtI { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(int!(base, a) < int!(base, b));
                }
                Instr::LeI { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(int!(base, a) <= int!(base, b));
                }
                Instr::LtF { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(float!(base, a) < float!(base, b));
                }
                Instr::LeF { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(float!(base, a) <= float!(base, b));
                }
                Instr::LtChar { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(ch!(base, a) < ch!(base, b));
                }
                Instr::LeChar { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(ch!(base, a) <= ch!(base, b));
                }
                Instr::LtStr { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(string!(base, a) < string!(base, b));
                }
                Instr::LeStr { dst, a, b } => {
                    reg!(base, dst) = Value::Bool(string!(base, a) <= string!(base, b));
                }

                Instr::Jump { off } => {
                    let frame = self.frames.last_mut().unwrap();
                    frame.pc = (frame.pc as i64 + off as i64) as usize;
                }
                Instr::JumpIfFalse { cond, off } => {
                    if !boolean!(base, cond) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.pc = (frame.pc as i64 + off as i64) as usize;
                    }
                }
                Instr::JumpIfTrue { cond, off } => {
                    if boolean!(base, cond) {
                        let frame = self.frames.last_mut().unwrap();
                        frame.pc = (frame.pc as i64 + off as i64) as usize;
                    }
                }

                Instr::Call {
                    dst,
                    base: abase,
                    nargs,
                    target,
                } => {
                    let args_at = base + abase as usize;
                    match target {
                        CallTarget::Proto(p) => {
                            self.push_call(p, args_at, nargs, base + dst as usize, None)?;
                        }
                        CallTarget::Host(h) => {
                            let args: Vec<Value> = (0..nargs as usize)
                                .map(|i| self.stack[args_at + i].clone())
                                .collect();
                            let imp = self.host_fns[h as usize].clone();
                            let result = {
                                let mut ctx = VmHostCtx { vm: self };
                                imp.call(&mut ctx, args)
                            };
                            match result {
                                Ok(v) => reg!(base, dst) = v,
                                Err(e) => return Err(self.host_fault(e)),
                            }
                        }
                        CallTarget::Builtin(b) => {
                            let v = self.call_builtin(b, args_at, nargs)?;
                            reg!(base, dst) = v;
                        }
                    }
                }
                Instr::CallValue {
                    dst,
                    f,
                    base: abase,
                    nargs,
                } => {
                    let callee = reg!(base, f).clone();
                    match callee {
                        Value::Closure(c) => {
                            self.push_call(
                                c.proto,
                                base + abase as usize,
                                nargs,
                                base + dst as usize,
                                Some(c),
                            )?;
                        }
                        other => {
                            return Err(
                                self.fault(format!("cannot call a {} value", other.kind_name()))
                            );
                        }
                    }
                }
                Instr::CallVirtual {
                    dst,
                    base: abase,
                    nargs,
                    slot,
                } => {
                    let args_at = base + abase as usize;
                    let recv = self.stack[args_at].clone();
                    let Value::Dyn(d) = recv else {
                        return Err(self.fault(format!(
                            "type confusion: dynamic dispatch on a {} value",
                            recv.kind_name()
                        )));
                    };
                    let target = self.units[self.cur_unit].unit.vtables[d.vtable as usize].targets
                        [slot as usize];
                    let CallTarget::Proto(p) = target else {
                        return Err(self.fault("invalid vtable entry"));
                    };
                    // Unwrap the receiver for the concrete method.
                    self.stack[args_at] = d.inner.clone();
                    self.push_call(p, args_at, nargs, base + dst as usize, None)?;
                }
                Instr::Ret { src } => {
                    let v = reg!(base, src).clone();
                    let frame = self.frames.pop().unwrap();
                    self.stack.truncate(frame.base);
                    if self.frames.len() == entry_depth {
                        return Ok(v);
                    }
                    self.stack[frame.ret_slot] = v;
                }
                Instr::RetUnit => {
                    let frame = self.frames.pop().unwrap();
                    self.stack.truncate(frame.base);
                    if self.frames.len() == entry_depth {
                        return Ok(Value::Unit);
                    }
                    self.stack[frame.ret_slot] = Value::Unit;
                }

                Instr::NewStruct {
                    dst,
                    def,
                    base: abase,
                    n,
                } => {
                    let fields: Vec<Value> = (0..n as usize)
                        .map(|i| self.stack[base + abase as usize + i].clone())
                        .collect();
                    reg!(base, dst) = Value::new_struct(wisp_core::defs::DefId(def), fields);
                }
                Instr::GetField { dst, obj, idx } => {
                    let v = match &reg!(base, obj) {
                        Value::Struct(s) => s.fields.borrow().get(idx as usize).cloned(),
                        Value::Enum(e) => e.fields.borrow().get(idx as usize).cloned(),
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: field access on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    match v {
                        Some(v) => reg!(base, dst) = v,
                        None => return Err(self.fault("field index out of range")),
                    }
                }
                Instr::SetField { obj, idx, src } => {
                    let v = reg!(base, src).clone();
                    let fields = match &reg!(base, obj) {
                        Value::Struct(s) => &s.fields,
                        Value::Enum(e) => &e.fields,
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: field write on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    let mut fields = fields.borrow_mut();
                    if (idx as usize) < fields.len() {
                        fields[idx as usize] = v;
                    } else {
                        drop(fields);
                        return Err(self.fault("field index out of range"));
                    }
                }
                Instr::NewEnum {
                    dst,
                    def,
                    tag,
                    base: abase,
                    n,
                } => {
                    let fields: Vec<Value> = (0..n as usize)
                        .map(|i| self.stack[base + abase as usize + i].clone())
                        .collect();
                    reg!(base, dst) =
                        Value::new_enum(wisp_core::defs::DefId(def), tag as u32, fields);
                }
                Instr::GetTag { dst, obj } => {
                    let tag = match &reg!(base, obj) {
                        Value::Enum(e) => e.tag as i64,
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: tag read on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    reg!(base, dst) = Value::Int(tag);
                }

                Instr::NewList {
                    dst,
                    base: abase,
                    n,
                } => {
                    let items: Vec<Value> = (0..n as usize)
                        .map(|i| self.stack[base + abase as usize + i].clone())
                        .collect();
                    reg!(base, dst) = Value::new_list(items);
                }
                Instr::NewMap {
                    dst,
                    base: abase,
                    n,
                } => {
                    let mut map = std::collections::BTreeMap::new();
                    for i in 0..n as usize {
                        let k = self.stack[base + abase as usize + i * 2].clone();
                        let v = self.stack[base + abase as usize + i * 2 + 1].clone();
                        let Some(key) = Key::from_value(&k) else {
                            return Err(
                                self.fault(format!("invalid map key of type {}", k.kind_name()))
                            );
                        };
                        map.insert(key, v);
                    }
                    reg!(base, dst) = Value::new_map(map);
                }
                Instr::ListIndexGet { dst, list, idx } => {
                    let i = int!(base, idx);
                    let v = match &reg!(base, list) {
                        Value::List(items) => {
                            let items = items.borrow();
                            if i < 0 || i as usize >= items.len() {
                                let len = items.len();
                                drop(items);
                                return Err(
                                    self.fault(format!("list index {i} out of bounds (len {len})"))
                                );
                            }
                            items[i as usize].clone()
                        }
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: list index on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    reg!(base, dst) = v;
                }
                Instr::ListIndexSet { list, idx, src } => {
                    let i = int!(base, idx);
                    let v = reg!(base, src).clone();
                    match &reg!(base, list) {
                        Value::List(items) => {
                            let mut items = items.borrow_mut();
                            if i < 0 || i as usize >= items.len() {
                                let len = items.len();
                                drop(items);
                                return Err(
                                    self.fault(format!("list index {i} out of bounds (len {len})"))
                                );
                            }
                            items[i as usize] = v;
                        }
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: list index on {}",
                                other.kind_name()
                            )));
                        }
                    }
                }
                Instr::MapIndexGet { dst, map, key } => {
                    let k = reg!(base, key).clone();
                    let Some(key) = Key::from_value(&k) else {
                        return Err(
                            self.fault(format!("invalid map key of type {}", k.kind_name()))
                        );
                    };
                    let v = match &reg!(base, map) {
                        Value::Map(entries) => entries.borrow().get(&key).cloned(),
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: map index on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    match v {
                        Some(v) => reg!(base, dst) = v,
                        None => {
                            let ks = k.display(&self.units[self.cur_unit].unit.defs);
                            return Err(self.fault(format!(
                                "missing map key: {ks} (use .get() for an Option)"
                            )));
                        }
                    }
                }
                Instr::MapIndexSet { map, key, src } => {
                    let k = reg!(base, key).clone();
                    let v = reg!(base, src).clone();
                    let Some(key) = Key::from_value(&k) else {
                        return Err(
                            self.fault(format!("invalid map key of type {}", k.kind_name()))
                        );
                    };
                    match &reg!(base, map) {
                        Value::Map(entries) => {
                            entries.borrow_mut().insert(key, v);
                        }
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: map index on {}",
                                other.kind_name()
                            )));
                        }
                    }
                }

                Instr::NewCell { dst, src } => {
                    let v = reg!(base, src).clone();
                    reg!(base, dst) = Value::Cell(Rc::new(std::cell::RefCell::new(v)));
                }
                Instr::CellGet { dst, cell } => {
                    let v = match &reg!(base, cell) {
                        Value::Cell(c) => c.borrow().clone(),
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: cell read on {}",
                                other.kind_name()
                            )));
                        }
                    };
                    reg!(base, dst) = v;
                }
                Instr::CellSet { cell, src } => {
                    let v = reg!(base, src).clone();
                    match &reg!(base, cell) {
                        Value::Cell(c) => *c.borrow_mut() = v,
                        other => {
                            return Err(self.fault(format!(
                                "type confusion: cell write on {}",
                                other.kind_name()
                            )));
                        }
                    }
                }
                Instr::MakeClosure { dst, proto } => {
                    let cap_srcs = self.units[self.cur_unit].unit.protos[proto as usize]
                        .captures
                        .clone();
                    let mut captures = Vec::with_capacity(cap_srcs.len());
                    for src in cap_srcs {
                        match src {
                            wisp_core::bytecode::CaptureSrc::Reg(r) => match &reg!(base, r) {
                                Value::Cell(c) => captures.push(c.clone()),
                                other => {
                                    return Err(self.fault(format!(
                                        "type confusion: capture of non-cell {}",
                                        other.kind_name()
                                    )));
                                }
                            },
                            wisp_core::bytecode::CaptureSrc::Capture(i) => {
                                let frame = self.frames.last().unwrap();
                                match &frame.closure {
                                    Some(c) => captures.push(c.captures[i as usize].clone()),
                                    None => {
                                        return Err(
                                            self.fault("type confusion: capture outside closure")
                                        );
                                    }
                                }
                            }
                        }
                    }
                    reg!(base, dst) = Value::Closure(Rc::new(Closure { proto, captures }));
                }
                Instr::LoadCapture { dst, slot } => {
                    let frame = self.frames.last().unwrap();
                    let cell = match &frame.closure {
                        Some(c) => c.captures[slot as usize].clone(),
                        None => {
                            return Err(self.fault("type confusion: capture outside closure"));
                        }
                    };
                    reg!(base, dst) = Value::Cell(cell);
                }

                Instr::MakeDyn { dst, src, vt } => {
                    let inner = reg!(base, src).clone();
                    reg!(base, dst) = Value::Dyn(Rc::new(DynObj { vtable: vt, inner }));
                }

                Instr::Fault { code } => {
                    let msg = match code {
                        FaultCode::UnreachableMatch => {
                            "internal error: a match proven exhaustive fell through \
                             (this is a wisp bug — please report it)"
                        }
                    };
                    return Err(self.fault(msg));
                }
            }
        }
    }

    /// Push a script call frame, copying `nargs` args from `args_at`.
    fn push_call(
        &mut self,
        proto: u32,
        args_at: usize,
        nargs: u16,
        ret_slot: usize,
        closure: Option<Rc<Closure>>,
    ) -> Result<(), RuntimeError> {
        if self.frames.len() >= self.depth_limit {
            return Err(self.fault("stack overflow: too many nested calls"));
        }
        let new_base = self.stack_top();
        let n_regs = self.units[self.cur_unit].unit.protos[proto as usize].n_regs as usize;
        let needed = new_base + n_regs.max(nargs as usize);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::Unit);
        }
        for i in 0..nargs as usize {
            let v = self.stack[args_at + i].clone();
            self.stack[new_base + i] = v;
        }
        self.frames.push(Frame {
            proto,
            base: new_base,
            pc: 0,
            ret_slot,
            closure,
        });
        Ok(())
    }

    fn host_fault(&self, e: HostError) -> RuntimeError {
        let mut f = self.fault(e.message);
        f.trace.insert(0, "<host function>".into());
        f
    }
}

/// `HostCtx` implementation handed to host functions while they run.
struct VmHostCtx<'a> {
    vm: &'a mut Vm,
}

impl<'a> HostCtx for VmHostCtx<'a> {
    fn defs(&self) -> &wisp_core::defs::DefTable {
        &self.vm.units[self.vm.cur_unit].unit.defs
    }

    fn display_value(&self, v: &Value) -> String {
        // Structural rendering (custom Display impls are not consulted at
        // the host boundary — documented limitation).
        v.display(self.defs())
    }
}

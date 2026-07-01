//! wscript — an embeddable, statically typed scripting language for Rust
//! (see `docs/PRD.md`).
//!
//! ```no_run
//! use wscript::{Context, Module, Vm};
//!
//! let mut math = Module::new("mathx");
//! math.fn_("double", |x: i64| x * 2);
//!
//! let ctx = Context::new().module(math);
//! let unit = ctx
//!     .compile("use mathx\nfn main() -> int { mathx::double(21) }")
//!     .unwrap();
//! let mut vm = Vm::new(&ctx);
//! let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
//! assert_eq!(n, 42);
//! ```

mod interface;

use std::cell::{Ref, RefMut};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

pub use wscript_core::bytecode::CompiledUnit;
pub use wscript_core::defs::DefTable;
pub use wscript_core::diag::{Diagnostic, Severity, default_help as diag_default_help};
pub use wscript_core::host::{FromValue, HostError, IntoValue, ScriptOpaque, ScriptType};
pub use wscript_core::module::Module;
pub use wscript_core::registry::Registry;
pub use wscript_core::span::Span;
pub use wscript_core::types::{FnSig, Type};
pub use wscript_core::value::Value;
pub use wscript_macros::Script;
pub use wscript_vm::{DEFAULT_CALL_DEPTH_LIMIT, RuntimeError, TraceFrame};

// Used by `#[derive(Script)]` expansions; not public API.
#[doc(hidden)]
pub use wscript_core as core;

// Re-exports per PRD §2: the umbrella exposes compiler, vm and std.
pub use wscript_compiler as compiler;
#[cfg(feature = "std")]
pub use wscript_std as std_modules;
pub use wscript_vm as vm;

/// Everything that can go wrong embedding wscript.
#[derive(Debug)]
pub enum Error {
    /// The script failed to compile; all diagnostics included.
    Compile(Vec<Diagnostic>),
    /// A trappable VM fault (PRD §3.5).
    Runtime(RuntimeError),
    /// A conversion failure at the host boundary (PRD §6.3/6.5).
    Conversion(HostError),
    /// Host-side signature mismatch when calling a script function.
    Signature(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Compile(diags) => {
                let errors = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .count();
                write!(f, "compilation failed with {errors} error(s)")?;
                for d in diags.iter().take(5) {
                    write!(f, "\n  [{}] {}", d.code, d.message)?;
                }
                Ok(())
            }
            Error::Runtime(e) => write!(f, "{e}"),
            Error::Conversion(e) => write!(f, "conversion error: {e}"),
            Error::Signature(msg) => write!(f, "signature error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<RuntimeError> for Error {
    fn from(e: RuntimeError) -> Error {
        Error::Runtime(e)
    }
}

impl From<HostError> for Error {
    fn from(e: HostError) -> Error {
        Error::Conversion(e)
    }
}

/// A compilation context: host registrations plus the compiler entry
/// point. `Send + Sync` and cheaply cloneable — share one `Context` across
/// threads and spin a `Vm` per thread (PRD §4.3).
#[derive(Clone, Default)]
pub struct Context {
    registry: Arc<Registry>,
}

impl Context {
    pub fn new() -> Context {
        Context {
            registry: Arc::new(Registry::new()),
        }
    }

    /// Register a host module (builder style, PRD §2).
    pub fn module(mut self, module: Module) -> Context {
        let reg = Arc::make_mut(&mut self.registry);
        module.merge_into(reg);
        self
    }

    /// Register a `#[derive(Script)]` type that does not belong to any
    /// module (e.g. a type that only crosses via `ScriptFn` arguments, like
    /// Appendix B's `KeyEvent`). Types are ambient in the script's type
    /// namespace.
    pub fn register_type<T: ScriptType>(mut self) -> Context {
        let reg = Arc::make_mut(&mut self.registry);
        T::script_type(&mut reg.defs);
        self
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Build a context from an already-assembled registry (used by tools
    /// that merge `.wscripti` interface registrations).
    pub fn from_registry(registry: Registry) -> Context {
        Context {
            registry: Arc::new(registry),
        }
    }

    /// Render every registered module / function / type / const signature
    /// as a `.wscripti` interface file (PRD §9.1) — a textual, human-readable
    /// subset of wscript syntax (declarations only), consumed by the LSP and
    /// `wscript check` via `wscript.toml`. Think `.d.ts`.
    pub fn interface_text(&self) -> String {
        interface::render(&self.registry)
    }

    /// Write the interface to a file (PRD §9.1 / Appendix B).
    pub fn write_interface(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.interface_text())
    }

    /// Compile a script. **All** type errors — including misuse of
    /// host-registered APIs — surface here (PRD §2).
    pub fn compile(&self, source: &str) -> Result<CompiledUnit, Error> {
        match wscript_compiler::compile(source, &self.registry) {
            Ok(c) => Ok(c.unit),
            Err(diags) => Err(Error::Compile(diags)),
        }
    }

    /// Compile, returning warnings alongside the unit.
    pub fn compile_verbose(
        &self,
        source: &str,
    ) -> Result<(CompiledUnit, Vec<Diagnostic>), Vec<Diagnostic>> {
        wscript_compiler::compile(source, &self.registry).map(|c| (c.unit, c.warnings))
    }
}

/// Per-thread execution engine. `!Send` by construction — script values
/// are `Rc`-managed (PRD §4.3).
pub struct Vm {
    inner: wscript_vm::Vm,
}

impl Vm {
    pub fn new(ctx: &Context) -> Vm {
        Vm {
            inner: wscript_vm::Vm::new(ctx.registry()),
        }
    }

    /// Set the script call-depth limit (default
    /// [`wscript_vm::DEFAULT_CALL_DEPTH_LIMIT`]). Exceeding it faults
    /// with a trappable "stack overflow" runtime error — raise it for
    /// deeply recursive scripts, lower it to keep untrusted scripts on a
    /// tighter leash. Frames live on a heap-allocated register stack, so
    /// the limit counts calls, not bytes.
    pub fn set_call_depth_limit(&mut self, limit: usize) {
        self.inner.set_call_depth_limit(limit);
    }

    /// The current script call-depth limit.
    pub fn call_depth_limit(&self) -> usize {
        self.inner.call_depth_limit()
    }

    /// One-shot typed call (PRD §6.4): converts arguments, checks the
    /// script function's signature at the boundary, runs, converts the
    /// result.
    pub fn call_unit<A: IntoArgs, R: FromValue>(
        &mut self,
        unit: &CompiledUnit,
        name: &str,
        args: A,
    ) -> Result<R, Error> {
        let Some((proto, sig)) = unit.exports.get(name) else {
            return Err(Error::Signature(format!(
                "the script does not define a function named `{name}`"
            )));
        };
        let values = args.into_values(&unit.defs)?;
        if values.len() != sig.params.len() {
            return Err(Error::Signature(format!(
                "`{name}` takes {} argument(s), {} were supplied",
                sig.params.len(),
                values.len()
            )));
        }
        let result = self.inner.call_proto(unit, *proto, values)?;
        Ok(R::from_value(result, &unit.defs)?)
    }

    /// Alias for [`Vm::call_unit`] matching the PRD §6.4 spelling.
    pub fn call<A: IntoArgs, R: FromValue>(
        &mut self,
        unit: &CompiledUnit,
        name: &str,
        args: A,
    ) -> Result<R, Error> {
        self.call_unit(unit, name, args)
    }

    /// Untyped call (raw `Value`s in and out).
    pub fn call_values(
        &mut self,
        unit: &CompiledUnit,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, Error> {
        Ok(self.inner.call_name(unit, name, args)?)
    }
}

// ---------------------------------------------------------------- args

/// Tuples of `IntoValue` used as call arguments (PRD §6.4).
pub trait IntoArgs {
    fn into_values(self, defs: &DefTable) -> Result<Vec<Value>, HostError>;
    /// Script-level parameter types, for `ScriptFn` signature verification.
    fn arg_types(defs: &mut DefTable) -> Option<Vec<Type>>
    where
        Self: Sized,
    {
        let _ = defs;
        None
    }
}

impl IntoArgs for () {
    fn into_values(self, _defs: &DefTable) -> Result<Vec<Value>, HostError> {
        Ok(vec![])
    }
    fn arg_types(_defs: &mut DefTable) -> Option<Vec<Type>> {
        Some(vec![])
    }
}

impl IntoArgs for Vec<Value> {
    fn into_values(self, _defs: &DefTable) -> Result<Vec<Value>, HostError> {
        Ok(self)
    }
}

macro_rules! impl_into_args {
    ($($name:ident : $idx:tt),+) => {
        impl<$($name: IntoValue),+> IntoArgs for ($($name,)+) {
            fn into_values(self, defs: &DefTable) -> Result<Vec<Value>, HostError> {
                Ok(vec![$(self.$idx.into_value(defs)?),+])
            }
            fn arg_types(_defs: &mut DefTable) -> Option<Vec<Type>> {
                None
            }
        }
    };
}

impl_into_args!(A: 0);
impl_into_args!(A: 0, B: 1);
impl_into_args!(A: 0, B: 1, C: 2);
impl_into_args!(A: 0, B: 1, C: 2, D: 3);
impl_into_args!(A: 0, B: 1, C: 2, D: 3, E: 4);
impl_into_args!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
impl_into_args!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
impl_into_args!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);

/// Argument tuples whose script types are statically known — used by
/// [`ScriptFn`] to verify the script signature once at lookup.
pub trait TypedArgs: IntoArgs {
    fn types(defs: &mut DefTable) -> Vec<Type>;
}

impl TypedArgs for () {
    fn types(_defs: &mut DefTable) -> Vec<Type> {
        vec![]
    }
}

macro_rules! impl_typed_args {
    ($($name:ident : $idx:tt),+) => {
        impl<$($name: IntoValue + ScriptType),+> TypedArgs for ($($name,)+) {
            fn types(defs: &mut DefTable) -> Vec<Type> {
                vec![$(<$name as ScriptType>::script_type(defs)),+]
            }
        }
    };
}

impl_typed_args!(A: 0);
impl_typed_args!(A: 0, B: 1);
impl_typed_args!(A: 0, B: 1, C: 2);
impl_typed_args!(A: 0, B: 1, C: 2, D: 3);
impl_typed_args!(A: 0, B: 1, C: 2, D: 3, E: 4);
impl_typed_args!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);

// ------------------------------------------------------------- ScriptFn

/// A typed handle to a script function (PRD §6.4): the signature is
/// verified once at lookup; calls thereafter are cheap.
///
/// ```ignore
/// let on_key: ScriptFn<(char,), bool> = fn_handle(&unit, "on_key")?;
/// let quit = on_key.call(&mut vm, ('q',))?;
/// ```
pub struct ScriptFn<A, R> {
    unit: Arc<CompiledUnit>,
    proto: u32,
    _pd: PhantomData<fn(A) -> R>,
}

impl<A: TypedArgs, R: FromValue + ScriptType> ScriptFn<A, R> {
    pub fn call(&self, vm: &mut Vm, args: A) -> Result<R, Error> {
        let values = args.into_values(&self.unit.defs)?;
        let result = vm.inner.call_proto(&self.unit, self.proto, values)?;
        Ok(R::from_value(result, &self.unit.defs)?)
    }
}

/// Extension methods on compiled units.
pub trait UnitExt {
    /// Look up a typed function handle, verifying the script signature
    /// against `A`/`R` once (PRD §6.4).
    fn fn_handle<A: TypedArgs, R: FromValue + ScriptType>(
        &self,
        name: &str,
    ) -> Result<ScriptFn<A, R>, Error>;
}

impl UnitExt for CompiledUnit {
    fn fn_handle<A: TypedArgs, R: FromValue + ScriptType>(
        &self,
        name: &str,
    ) -> Result<ScriptFn<A, R>, Error> {
        let Some((proto, sig)) = self.exports.get(name) else {
            return Err(Error::Signature(format!(
                "the script does not define a function named `{name}`"
            )));
        };
        // Compute the host-side expectation against a scratch def table so
        // lookups never mutate the compiled unit.
        let mut defs = self.defs.clone();
        let expect_params = A::types(&mut defs);
        let expect_ret = R::script_type(&mut defs);
        if sig.params != expect_params || (sig.ret != expect_ret && expect_ret != Type::Unit) {
            let script_sig = render_sig(sig, &defs);
            let host_sig = render_sig(&FnSig::new(expect_params, expect_ret), &defs);
            return Err(Error::Signature(format!(
                "`{name}` has script signature {script_sig}, but the host requested \
                 {host_sig}"
            )));
        }
        Ok(ScriptFn {
            unit: Arc::new(self.clone()),
            proto: *proto,
            _pd: PhantomData,
        })
    }
}

fn render_sig(sig: &FnSig, defs: &DefTable) -> String {
    let params: Vec<String> = sig.params.iter().map(|p| p.display(defs)).collect();
    format!("fn({}) -> {}", params.join(", "), sig.ret.display(defs))
}

// --------------------------------------------------------------- Shared

/// Host-side handle to a live script value (PRD §6.5). Both sides observe
/// mutation; aliasing violations surface as `Err`, never panics.
///
/// - For **data** types (`#[derive(Script)]`): [`Shared::get`] converts a
///   snapshot out, [`Shared::set`] writes fields through the live value.
/// - For **opaque** types: [`Shared::borrow`] / [`Shared::borrow_mut`]
///   give direct access to the underlying Rust value.
pub struct Shared<T> {
    value: Value,
    defs: Arc<DefTable>,
    _pd: PhantomData<T>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared {
            value: self.value.clone(),
            defs: self.defs.clone(),
            _pd: PhantomData,
        }
    }
}

impl<T> Shared<T> {
    /// Wrap a raw script value (e.g. obtained from `call_values`).
    pub fn from_value(value: Value, unit: &CompiledUnit) -> Shared<T> {
        Shared {
            value,
            defs: Arc::new(unit.defs.clone()),
            _pd: PhantomData,
        }
    }

    /// The underlying script value (pass it back into calls).
    pub fn value(&self) -> &Value {
        &self.value
    }
}

impl<T: FromValue> Shared<T> {
    /// Convert the current state out (deep for data types).
    pub fn get(&self) -> Result<T, Error> {
        Ok(T::from_value(self.value.clone(), &self.defs)?)
    }
}

impl<T: IntoValue> Shared<T> {
    /// Replace the live value's contents so every script alias observes
    /// the update. For struct values the fields are written in place.
    pub fn set(&self, new: T) -> Result<(), Error> {
        let converted = new.into_value(&self.defs)?;
        match (&self.value, &converted) {
            (Value::Struct(dst), Value::Struct(src)) if dst.def == src.def => {
                let new_fields = src.fields.borrow().clone();
                let mut fields = dst
                    .fields
                    .try_borrow_mut()
                    .map_err(|_| HostError::msg("aliasing violation: value is borrowed"))?;
                *fields = new_fields;
                Ok(())
            }
            _ => Err(Error::Conversion(HostError::msg(
                "Shared::set replaces struct contents in place; for other types build a \
                 new value and pass it into the VM",
            ))),
        }
    }
}

impl<T: ScriptOpaque> Shared<T> {
    /// Borrow the live host value. Errors if the value is currently
    /// mutably borrowed (PRD §6.5).
    pub fn borrow(&self) -> Result<Ref<'_, T>, Error> {
        match &self.value {
            Value::Opaque(cell) => {
                let guard = cell.cell.try_borrow().map_err(|_| {
                    HostError::msg("aliasing violation: opaque value is mutably borrowed")
                })?;
                if guard.downcast_ref::<T>().is_none() {
                    return Err(Error::Conversion(HostError::msg(
                        "opaque handle holds a different Rust type",
                    )));
                }
                Ok(Ref::map(guard, |b| b.downcast_ref::<T>().unwrap()))
            }
            other => Err(Error::Conversion(wscript_core::host::type_mismatch(
                "opaque handle",
                other,
            ))),
        }
    }

    pub fn borrow_mut(&self) -> Result<RefMut<'_, T>, Error> {
        match &self.value {
            Value::Opaque(cell) => {
                let guard = cell
                    .cell
                    .try_borrow_mut()
                    .map_err(|_| HostError::msg("aliasing violation: opaque value is borrowed"))?;
                if guard.downcast_ref::<T>().is_none() {
                    return Err(Error::Conversion(HostError::msg(
                        "opaque handle holds a different Rust type",
                    )));
                }
                Ok(RefMut::map(guard, |b| b.downcast_mut::<T>().unwrap()))
            }
            other => Err(Error::Conversion(wscript_core::host::type_mismatch(
                "opaque handle",
                other,
            ))),
        }
    }

    /// Move a host value into the VM's world and keep a shared handle to
    /// it (PRD §6.5 "a host value inserted into the VM").
    pub fn insert(value: T, unit: &CompiledUnit) -> Result<Shared<T>, Error>
    where
        T: IntoValue,
    {
        let defs = Arc::new(unit.defs.clone());
        let v = value.into_value(&defs)?;
        Ok(Shared {
            value: v,
            defs,
            _pd: PhantomData,
        })
    }
}

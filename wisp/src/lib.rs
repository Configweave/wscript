//! wisp — an embeddable, statically typed scripting language for Rust
//! (see `docs/PRD.md`).
//!
//! ```no_run
//! use wisp::{Context, Vm};
//!
//! let ctx = Context::new();
//! let unit = ctx.compile("fn main() -> int { 40 + 2 }").unwrap();
//! let mut vm = Vm::new(&ctx);
//! let n: i64 = vm.call_unit(&unit, "main", ()).unwrap();
//! assert_eq!(n, 42);
//! ```

use std::fmt;
use std::sync::Arc;

pub use wisp_core::bytecode::CompiledUnit;
pub use wisp_core::diag::{Diagnostic, Severity};
pub use wisp_core::host::{FromValue, HostError, IntoValue};
pub use wisp_core::registry::{ModuleDef, Registry};
pub use wisp_core::span::Span;
pub use wisp_core::types::{FnSig, Type};
pub use wisp_core::value::Value;
pub use wisp_macros::Script;
pub use wisp_vm::RuntimeError;

/// Everything that can go wrong embedding wisp.
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
/// point. `Send + Sync` — share one `Context` across threads and spin a
/// `Vm` per thread (PRD §4.3).
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

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Compile a script. **All** type errors — including misuse of
    /// host-registered APIs — surface here (PRD §2).
    pub fn compile(&self, source: &str) -> Result<CompiledUnit, Error> {
        match wisp_compiler::compile(source, &self.registry) {
            Ok(c) => Ok(c.unit),
            Err(diags) => Err(Error::Compile(diags)),
        }
    }

    /// Compile, returning warnings alongside the unit.
    pub fn compile_verbose(
        &self,
        source: &str,
    ) -> Result<(CompiledUnit, Vec<Diagnostic>), Vec<Diagnostic>> {
        wisp_compiler::compile(source, &self.registry).map(|c| (c.unit, c.warnings))
    }
}

/// Per-thread execution engine. `!Send` by construction — script values
/// are `Rc`-managed (PRD §4.3).
pub struct Vm {
    inner: wisp_vm::Vm,
}

impl Vm {
    pub fn new(ctx: &Context) -> Vm {
        Vm {
            inner: wisp_vm::Vm::new(ctx.registry()),
        }
    }

    /// One-shot typed call: converts arguments, checks the script
    /// function's signature shape (arity), runs, converts the result.
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
        let values = args.into_values()?;
        if values.len() != sig.params.len() {
            return Err(Error::Signature(format!(
                "`{name}` takes {} argument(s), {} were supplied",
                sig.params.len(),
                values.len()
            )));
        }
        let result = self.inner.call_proto(unit, *proto, values)?;
        Ok(R::from_value(result)?)
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

/// Tuples of `IntoValue` used as call arguments (PRD §6.4).
pub trait IntoArgs {
    fn into_values(self) -> Result<Vec<Value>, HostError>;
}

impl IntoArgs for () {
    fn into_values(self) -> Result<Vec<Value>, HostError> {
        Ok(vec![])
    }
}

impl IntoArgs for Vec<Value> {
    fn into_values(self) -> Result<Vec<Value>, HostError> {
        Ok(self)
    }
}

macro_rules! impl_into_args {
    ($($name:ident : $idx:tt),+) => {
        impl<$($name: IntoValue),+> IntoArgs for ($($name,)+) {
            fn into_values(self) -> Result<Vec<Value>, HostError> {
                Ok(vec![$(self.$idx.into_value()?),+])
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

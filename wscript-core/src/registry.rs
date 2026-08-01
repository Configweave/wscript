//! The registration registry: everything a `Context` knows before compiling
//! a script — host modules, function signatures, constants, registered
//! types and their methods. The type checker reads signatures from here;
//! the VM reads implementations (PRD §2's key invariant).

use std::collections::HashMap;
use std::sync::Arc;

use crate::bytecode::Const;
use crate::defs::{DefId, DefTable};
use crate::host::HostCallable;
use crate::types::{FnSig, Type};

#[derive(Clone)]
pub struct HostFnEntry {
    pub sig: FnSig,
    pub imp: Arc<dyn HostCallable>,
}

impl std::fmt::Debug for HostFnEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HostFnEntry({:?})", self.sig)
    }
}

/// A method on a host-registered type (`m.ty::<Pane>().method(...)`).
/// The receiver is not part of `sig`.
#[derive(Debug, Clone)]
pub struct HostMethod {
    pub name: String,
    pub sig: FnSig,
    pub host_idx: u32,
    pub doc: Option<String>,
    /// Declared parameter names, receiver excluded — see [`ModuleFn::params`].
    pub params: Vec<String>,
}

/// A function registered under a module.
///
/// Parameter names live here rather than in [`FnSig`] because `FnSig` is
/// part of type identity (it derives `Eq`/`Hash` and is embedded in
/// `Type::Fn`): `fn(int) -> int` must not become a different type for
/// being written with a different parameter name. Names are documentation
/// — the interface emitter and the LSP show them, the checker ignores them.
#[derive(Debug, Clone)]
pub struct ModuleFn {
    pub name: String,
    pub sig: FnSig,
    /// Index into [`Registry::host_fns`].
    pub host_idx: u32,
    pub doc: Option<String>,
    /// Declared parameter names, positionally matching `sig.params`.
    /// Empty when the host declared none — consumers then fall back to
    /// positional placeholders rather than inventing a name.
    pub params: Vec<String>,
}

impl ModuleFn {
    /// Declared parameter names, or `None` when the host declared none.
    pub fn param_names(&self) -> Option<&[String]> {
        declared(&self.params, &self.sig)
    }
}

impl HostMethod {
    /// Declared parameter names (receiver excluded), or `None`.
    pub fn param_names(&self) -> Option<&[String]> {
        declared(&self.params, &self.sig)
    }
}

/// The placeholder standing in for parameter `i` where the host declared
/// no name. Deliberately synthetic: `a0` reads as "nothing was declared",
/// where a plausible-looking invented name would read as fact.
pub fn positional_param_name(i: usize) -> String {
    format!("a{i}")
}

/// Names are only usable if there is one per parameter; a mismatch is a
/// host registration bug (asserted in `Module::merge_into`) and degrades
/// here to "undeclared" rather than to a misaligned parameter list.
fn declared<'a>(params: &'a [String], sig: &FnSig) -> Option<&'a [String]> {
    (!params.is_empty() && params.len() == sig.params.len()).then_some(params)
}

#[derive(Debug, Clone, Default)]
pub struct ModuleDef {
    pub name: String,
    pub fns: Vec<ModuleFn>,
    pub consts: Vec<(String, Type, Const)>,
    /// Types registered under this module (also importable via `use`).
    pub types: Vec<DefId>,
    pub doc: Option<String>,
}

/// All host registrations visible to a compilation. Shared (immutably)
/// between the checker and every VM spun from the owning `Context`.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    /// Builtins + host-registered defs. Script compilation clones this and
    /// appends script defs.
    pub defs: DefTable,
    pub modules: Vec<ModuleDef>,
    pub host_fns: Vec<HostFnEntry>,
    /// Methods of host-registered (usually opaque) types.
    pub methods: HashMap<DefId, Vec<HostMethod>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry {
            defs: DefTable::with_builtins(),
            modules: Vec::new(),
            host_fns: Vec::new(),
            methods: HashMap::new(),
        }
    }

    pub fn module(&self, name: &str) -> Option<&ModuleDef> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn push_host_fn(&mut self, entry: HostFnEntry) -> u32 {
        let idx = self.host_fns.len() as u32;
        self.host_fns.push(entry);
        idx
    }
}

// A `Context` must be shareable across threads (PRD §4.3).
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Registry>();
};

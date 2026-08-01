//! [`Session`] — one module that knows how a wscript project is compiled.
//!
//! [`Context`] answers "what has the host registered?". A session answers
//! the larger question a *tool* has to answer: registrations **plus** how
//! `use` imports are resolved, kept together so they cannot be paired
//! wrongly.
//!
//! That pairing is the whole point. Before this existed, every caller
//! rebuilt it — and they drifted. The language server constructed an
//! import resolver with no source roots while the CLI passed the
//! manifest's, so the same project typechecked in `wscript check` and
//! reported `E0200 unknown module` in the editor. A session makes that
//! particular bug unrepresentable: you configure the resolver where you
//! configure the registry, once.
//!
//! ```no_run
//! use wscript::{Module, Session, VmConfig};
//!
//! let mut math = Module::new("mathx");
//! math.fn_("double", |x: i64| x * 2);
//!
//! let session = Session::builder().module(math).build();
//! let compiled = session.compile("main.wscript", "fn main() -> int { 42 }")?;
//! session.run(&compiled, VmConfig::default());
//! # Ok::<(), wscript::CompileFailure>(())
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use wscript_compiler::{Analysis, CompileFailure, Compiled, SourceResolver};

use crate::{
    Context, Error, FsResolver, Module, Registry, RuntimeError, ScriptType, Value, Vm, VmConfig,
};

/// A compilation context plus its import resolver: everything needed to
/// turn an entry file into an [`Analysis`] or a [`Compiled`] unit.
///
/// Cheap to clone — both halves are behind an `Arc`, so a tool that needs
/// to compile off the lock (a language server holding its state in a
/// mutex) clones the session rather than rebuilding it.
#[derive(Clone)]
pub struct Session {
    ctx: Context,
    resolver: Arc<dyn SourceResolver + Send>,
}

impl Session {
    pub fn builder() -> SessionBuilder {
        SessionBuilder {
            registry: Registry::new(),
            resolver: None,
        }
    }

    /// The host registrations, for callers that still need a [`Context`]
    /// (constructing a [`Vm`], rendering an interface file).
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    pub fn registry(&self) -> &Registry {
        self.ctx.registry()
    }

    pub fn resolver(&self) -> &dyn SourceResolver {
        &*self.resolver
    }

    /// Parse and check `source` as if it lived at `path`, following its
    /// imports. Never fails: the AST and check tables come back however
    /// partial, alongside every diagnostic — this is what an editor wants.
    pub fn analyze(&self, path: &str, source: &str) -> Analysis {
        wscript_compiler::analyze_entry(path, source, &*self.resolver, self.registry())
    }

    /// Compile `source` as the entry file at `path`. The whole import
    /// graph becomes one unit; `path` is what diagnostics name and what
    /// relative imports resolve against, so it need not exist on disk
    /// (`<repl>` is a legitimate entry path).
    pub fn compile(&self, path: &str, source: &str) -> Result<Compiled, CompileFailure> {
        wscript_compiler::compile_entry(path, source, &*self.resolver, self.registry())
    }

    /// Call `main` on a compiled unit and translate however it ended into
    /// a [`RunOutcome`].
    ///
    /// Nothing is printed and nothing is rendered: diagnostics belong to
    /// the caller, which is the only party that knows whether it is
    /// writing to a terminal, a test buffer or an editor. `compiled`
    /// carries the sources and source map a renderer needs.
    pub fn run(&self, compiled: &Compiled, config: VmConfig) -> RunOutcome {
        if !compiled.unit.exports.contains_key("main") {
            return RunOutcome::NoMain;
        }
        let mut vm = Vm::with_config(&self.ctx, config);
        match vm.call_values(&compiled.unit, "main", vec![]) {
            // Exit code from main's return: int, or unit → 0 (PRD §8).
            Ok(Value::Int(code)) => RunOutcome::Exited((code & 0xff) as u8),
            Ok(_) => RunOutcome::Exited(0),
            // process::exit — a requested exit, not a fault to render.
            Err(Error::Runtime(e)) if e.exit_code.is_some() => {
                RunOutcome::Exited((e.exit_code.unwrap() & 0xff) as u8)
            }
            Err(Error::Runtime(e)) => RunOutcome::Faulted(e),
            Err(e) => RunOutcome::Failed(e),
        }
    }
}

/// How a call to `main` ended.
#[derive(Debug)]
pub enum RunOutcome {
    /// Ran to completion with this process exit code.
    Exited(u8),
    /// The unit compiled but declares no `fn main()`.
    NoMain,
    /// A trappable runtime fault — render it against `compiled.sources`.
    Faulted(RuntimeError),
    /// A failure at the host boundary (conversion, signature).
    Failed(Error),
}

/// Builder for a [`Session`]. Registry first, then how imports resolve.
pub struct SessionBuilder {
    registry: Registry,
    resolver: Option<Arc<dyn SourceResolver + Send>>,
}

impl SessionBuilder {
    /// Register a host module (PRD §2).
    pub fn module(mut self, module: Module) -> SessionBuilder {
        module.merge_into(&mut self.registry);
        self
    }

    pub fn modules(mut self, modules: impl IntoIterator<Item = Module>) -> SessionBuilder {
        for module in modules {
            module.merge_into(&mut self.registry);
        }
        self
    }

    /// Register a `#[derive(Script)]` type that belongs to no module.
    pub fn register_type<T: ScriptType>(mut self) -> SessionBuilder {
        T::script_type(&mut self.registry.defs);
        self
    }

    /// Use an already-assembled registry, discarding anything registered
    /// so far. This is the `.wscripti` path: `wscript check` loads a
    /// manifest's interfaces into a fresh registry and hands it over
    /// whole, precisely so the CLI's own stdlib cannot shadow the
    /// embedder's modules (ADR-0002).
    pub fn registry(mut self, registry: Registry) -> SessionBuilder {
        self.registry = registry;
        self
    }

    /// Resolve `use` imports through a custom resolver — an in-memory map
    /// for tests, a virtual filesystem, a package store.
    pub fn resolver(mut self, resolver: impl SourceResolver + Send + 'static) -> Self {
        self.resolver = Some(Arc::new(resolver));
        self
    }

    /// Search these directories for `use name` imports, after the
    /// importing file's own directory — `wscript.toml`'s `src_roots`.
    /// Shorthand for [`SessionBuilder::resolver`] with an [`FsResolver`],
    /// and the reason the roots are hard to forget: they are set where
    /// the session is built, not where it is used.
    pub fn src_roots(self, roots: Vec<PathBuf>) -> SessionBuilder {
        self.resolver(FsResolver { roots })
    }

    /// Build the session. Without an explicit resolver, imports resolve
    /// on the filesystem relative to the importing file and nowhere else.
    pub fn build(self) -> Session {
        Session {
            ctx: Context::from_registry(self.registry),
            resolver: self.resolver.unwrap_or_else(|| Arc::new(FsResolver::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wscript_compiler::{ImportSpec, ResolvedSource};

    /// An in-memory resolver — the second adapter that makes the resolver
    /// seam real rather than hypothetical.
    struct Files(Vec<(&'static str, &'static str)>);

    impl SourceResolver for Files {
        fn resolve(&self, _from: &str, spec: ImportSpec) -> Result<ResolvedSource, String> {
            let want = match spec {
                ImportSpec::Name(n) => n,
                ImportSpec::Path(p) => p,
            };
            self.0
                .iter()
                .find(|(name, _)| *name == want)
                .map(|(name, src)| ResolvedSource {
                    key: name.to_string(),
                    path: name.to_string(),
                    src: src.to_string(),
                })
                .ok_or_else(|| format!("no such module `{want}`"))
        }
    }

    fn session() -> Session {
        Session::builder()
            .resolver(Files(vec![(
                "helper",
                "fn double(x: int) -> int { x * 2 }",
            )]))
            .build()
    }

    #[test]
    fn compiles_and_runs_an_entry_file() {
        let s = Session::builder().build();
        let c = s.compile("main.wscript", "fn main() -> int { 7 }").unwrap();
        assert!(matches!(
            s.run(&c, VmConfig::default()),
            RunOutcome::Exited(7)
        ));
    }

    /// The session's resolver is what an import goes through — the caller
    /// does not get to supply a different one.
    #[test]
    fn imports_resolve_through_the_session_resolver() {
        let c = session()
            .compile(
                "main.wscript",
                "use helper\nfn main() -> int { helper::double(21) }",
            )
            .unwrap();
        assert_eq!(c.sources.len(), 2, "entry plus the imported file");
    }

    #[test]
    fn a_session_without_that_resolver_cannot_find_the_import() {
        let Err(err) = Session::builder()
            .build()
            .compile("main.wscript", "use helper\nfn main() {}")
        else {
            panic!("`use helper` must not resolve without a resolver that knows it");
        };
        assert!(
            err.diags.iter().any(|d| d.code == "E0200"),
            "{:?}",
            err.diags
        );
    }

    /// Analysis follows imports too, and returns tables even when the
    /// program does not compile.
    #[test]
    fn analyze_returns_tables_for_a_broken_program() {
        let a = session().analyze("main.wscript", "fn main() -> int { \"nope\" }");
        assert!(
            a.check
                .diags
                .iter()
                .any(|d| d.severity == crate::Severity::Error),
            "the return-type mismatch is reported"
        );
        assert!(!a.parse.file.items.is_empty(), "AST still available");
    }

    #[test]
    fn exit_code_is_masked_to_a_byte() {
        let s = Session::builder().build();
        let c = s
            .compile("main.wscript", "fn main() -> int { 300 }")
            .unwrap();
        assert!(matches!(
            s.run(&c, VmConfig::default()),
            RunOutcome::Exited(44)
        ));
    }

    #[test]
    fn a_unit_returning_main_exits_zero() {
        let s = Session::builder().build();
        let c = s.compile("main.wscript", "fn main() { }").unwrap();
        assert!(matches!(
            s.run(&c, VmConfig::default()),
            RunOutcome::Exited(0)
        ));
    }

    #[test]
    fn no_main_is_reported_rather_than_faulting() {
        let s = Session::builder().build();
        let c = s
            .compile("lib.wscript", "fn helper() -> int { 1 }")
            .unwrap();
        assert!(matches!(s.run(&c, VmConfig::default()), RunOutcome::NoMain));
    }

    #[test]
    fn a_registered_module_is_callable() {
        let mut m = Module::new("mathx");
        m.fn_("double", |x: i64| x * 2);
        let s = Session::builder().module(m).build();
        let c = s
            .compile(
                "main.wscript",
                "use mathx\nfn main() -> int { mathx::double(21) }",
            )
            .unwrap();
        assert!(matches!(
            s.run(&c, VmConfig::default()),
            RunOutcome::Exited(42)
        ));
    }

    /// `registry()` replaces rather than merges — that is what makes the
    /// disjoint check registry of ADR-0002 expressible.
    #[test]
    fn an_explicit_registry_discards_earlier_modules() {
        let mut m = Module::new("mathx");
        m.fn_("double", |x: i64| x * 2);
        let s = Session::builder()
            .module(m)
            .registry(Registry::new())
            .build();
        assert!(s.registry().modules.iter().all(|m| m.name != "mathx"));
    }

    #[test]
    fn a_session_clone_shares_the_resolver() {
        let s = session();
        let clone = s.clone();
        assert!(
            clone
                .compile("main.wscript", "use helper\nfn main() {}")
                .is_ok()
        );
    }
}

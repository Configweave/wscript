//! The wscript compiler: lexer → parser → type checker → bytecode emitter
//! (PRD §5.1).

pub mod ast;
pub mod check;
pub mod emit;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod wscripti;

use wscript_core::bytecode::CompiledUnit;
use wscript_core::diag::{Diagnostic, Severity};
use wscript_core::registry::Registry;

pub use parser::{ParseOutput, parse};

/// A successful compilation (possibly with warnings).
pub struct Compiled {
    pub unit: CompiledUnit,
    pub warnings: Vec<Diagnostic>,
}

/// The parser and checker recurse (bounded — see the parser's
/// `MAX_NESTING_BUDGET` and the checker's `MAX_EXPR_DEPTH`), and debug
/// frames are big enough that deeply nested scripts need real headroom.
/// Callers can sit on small stacks (tokio gives the LSP's threads 2 MiB),
/// so the pipeline runs on a scoped thread with a dedicated stack.
const PIPELINE_STACK: usize = 32 * 1024 * 1024;

fn on_pipeline_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("wscript-compile".into())
            .stack_size(PIPELINE_STACK)
            .spawn_scoped(scope, f)
            .expect("failed to spawn compile thread");
        match handle.join() {
            Ok(v) => v,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

/// Compile a single script against the registered host context. All type
/// errors — including misuse of host-registered APIs — surface here
/// (PRD §1).
pub fn compile(source: &str, registry: &Registry) -> Result<Compiled, Vec<Diagnostic>> {
    on_pipeline_stack(|| {
        let parsed = parse(source);
        let mut checked = check::check(&parsed.file, registry);
        let mut diags = parsed.diags;
        diags.append(&mut checked.diags);
        diags.sort_by_key(|d| (d.span.lo, d.span.hi));
        if diags.iter().any(|d| d.severity == Severity::Error) {
            return Err(diags);
        }
        let unit = emit::emit(&parsed.file, &checked);
        Ok(Compiled {
            unit,
            warnings: diags,
        })
    })
}

/// Parse + check without emitting — the LSP's entry point: always returns
/// the (possibly partial) AST, the check tables and every diagnostic.
pub struct Analysis {
    pub parse: ParseOutput,
    pub check: check::CheckResult,
}

pub fn analyze(source: &str, registry: &Registry) -> Analysis {
    on_pipeline_stack(|| {
        let parse = parser::parse(source);
        let check = check::check(&parse.file, registry);
        Analysis { parse, check }
    })
}

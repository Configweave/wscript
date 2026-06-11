//! The wisp compiler: lexer → parser → type checker → bytecode emitter
//! (PRD §5.1).

pub mod ast;
pub mod check;
pub mod emit;
pub mod lexer;
pub mod parser;
pub mod token;

use wisp_core::bytecode::CompiledUnit;
use wisp_core::diag::{Diagnostic, Severity};
use wisp_core::registry::Registry;

pub use parser::{ParseOutput, parse};

/// A successful compilation (possibly with warnings).
pub struct Compiled {
    pub unit: CompiledUnit,
    pub warnings: Vec<Diagnostic>,
}

/// Compile a single script against the registered host context. All type
/// errors — including misuse of host-registered APIs — surface here
/// (PRD §1).
pub fn compile(source: &str, registry: &Registry) -> Result<Compiled, Vec<Diagnostic>> {
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
}

/// Parse + check without emitting — the LSP's entry point: always returns
/// the (possibly partial) AST, the check tables and every diagnostic.
pub struct Analysis {
    pub parse: ParseOutput,
    pub check: check::CheckResult,
}

pub fn analyze(source: &str, registry: &Registry) -> Analysis {
    let parse = parser::parse(source);
    let check = check::check(&parse.file, registry);
    Analysis { parse, check }
}

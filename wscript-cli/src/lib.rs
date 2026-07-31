//! The `wscript` CLI as a library: `run`, `check`, `repl`, `lsp` (PRD §8).
//!
//! The binary is argv parsing over this; everything else lives here so it
//! can be tested in-process. Before the split, `manifest`, `diag_render`,
//! the LSP helpers and the REPL were unreachable from any test — the crate
//! had a `[[bin]]` and no `[lib]`.

pub mod diag_render;
pub mod lsp;
pub mod manifest;
pub mod repl;

use std::io::Write;
use std::process::ExitCode;

use diag_render::Renderer;
use wscript::{Context, Error, PrintHook, Value, Vm, VmConfig};

/// The CLI enables the full stdlib by default (PRD §7/§8).
pub fn default_context(script_args: Vec<String>) -> Context {
    let mut ctx = Context::new();
    for module in wscript_std::all_modules(script_args) {
        ctx = ctx.module(module);
    }
    ctx
}

pub fn read_source(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read `{path}`: {e}");
        ExitCode::FAILURE
    })
}

/// Script imports resolve next to the entry file, then under the
/// manifest's `src_roots`.
pub fn file_resolver(script_path: &str) -> wscript::FsResolver {
    let mut resolver = wscript::FsResolver::new();
    if let Some(m) = manifest::find(std::path::Path::new(script_path)) {
        resolver.roots = m.src_roots;
    }
    resolver
}

/// How a script run ended.
///
/// Diagnostics and faults are rendered to the caller's [`Renderer`] as
/// they happen; this reports *what* happened so a caller can map it to an
/// exit code, or a test can assert on it.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Ran to completion with this process exit code.
    Exited(u8),
    /// The script (or one of its imports) failed to compile.
    CompileFailed,
    /// A trappable runtime fault.
    Faulted,
    /// Compiled, but has no `fn main()`.
    NoMain,
    /// Anything else at the host boundary (unreadable file, conversion,
    /// signature).
    Failed,
}

/// Compile and run `path`, delivering script output to `out` and
/// diagnostics to `err`.
///
/// Both sinks are parameters so a test can run a script and assert on
/// either stream without spawning a process.
pub fn run_script(
    path: &str,
    script_args: Vec<String>,
    out: PrintHook,
    err: &mut Renderer,
) -> Outcome {
    let Ok(source) = std::fs::read_to_string(path) else {
        let _ = writeln!(err, "error: cannot read `{path}`");
        return Outcome::Failed;
    };
    let ctx = default_context(script_args);
    let resolver = file_resolver(path);
    let compiled = match ctx.compile_entry(path, &source, &resolver) {
        Ok(c) => c,
        Err(failure) => {
            err.render_multi(&failure.sources, &failure.source_map, &failure.diags);
            return Outcome::CompileFailed;
        }
    };
    let (unit, warnings, sources) = (compiled.unit, compiled.warnings, compiled.sources);
    err.render_multi(&sources, &unit.source_map, &warnings);
    if !unit.exports.contains_key("main") {
        let _ = writeln!(err, "error: `{path}` has no `fn main()`");
        return Outcome::NoMain;
    }
    let mut vm = Vm::with_config(
        &ctx,
        VmConfig {
            out,
            ..VmConfig::default()
        },
    );
    match vm.call_values(&unit, "main", vec![]) {
        // Exit code from main's return: int, or unit → 0 (PRD §8).
        Ok(Value::Int(code)) => Outcome::Exited((code & 0xff) as u8),
        Ok(_) => Outcome::Exited(0),
        // process::exit — a requested exit, not an error to render.
        Err(Error::Runtime(e)) if e.exit_code.is_some() => {
            Outcome::Exited((e.exit_code.unwrap() & 0xff) as u8)
        }
        Err(Error::Runtime(e)) => {
            err.render_runtime_multi(&sources, &unit.source_map, &e);
            Outcome::Faulted
        }
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            Outcome::Failed
        }
    }
}

pub fn cmd_run(path: &str, script_args: Vec<String>) -> ExitCode {
    let mut err = Renderer::stderr();
    match run_script(path, script_args, wscript::stdout_sink(), &mut err) {
        Outcome::Exited(code) => ExitCode::from(code),
        _ => ExitCode::FAILURE,
    }
}

pub fn cmd_check(path: &str) -> ExitCode {
    let source = match read_source(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    // `wscript check` honors wscript.toml's .wscripti interfaces (PRD §8/§9.1).
    // A manifest describes the *complete* host context the script runs
    // under, so when one is present the CLI's default stdlib stays out of
    // the registry — otherwise a same-named CLI module would shadow the
    // embedder's interface and mis-check real scripts. See ADR-0002.
    let ctx = match manifest::find(std::path::Path::new(path)) {
        Some(m) => {
            let mut reg = wscript::Registry::new();
            manifest::load_interfaces(&m, &mut reg);
            wscript::Context::from_registry(reg)
        }
        None => default_context(Vec::new()),
    };
    let resolver = file_resolver(path);
    let mut err = Renderer::stderr();
    match ctx.compile_entry(path, &source, &resolver) {
        Ok(c) => {
            err.render_multi(&c.sources, &c.unit.source_map, &c.warnings);
            ExitCode::SUCCESS
        }
        Err(failure) => {
            err.render_multi(&failure.sources, &failure.source_map, &failure.diags);
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
usage:
\x20 wscript run <file> [args...]   compile and execute a script
\x20 wscript check <file>           compile only, print diagnostics
\x20 wscript repl                   interactive session
\x20 wscript lsp                    start the language server (stdio)";

/// Dispatch a parsed argv (without the program name).
pub fn run(args: &[String]) -> ExitCode {
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("run") => {
            let Some(file) = iter.next() else {
                eprintln!("usage: wscript run <file> [args...]");
                return ExitCode::from(2);
            };
            cmd_run(file, iter.cloned().collect())
        }
        Some("check") => {
            let Some(file) = iter.next() else {
                eprintln!("usage: wscript check <file>");
                return ExitCode::from(2);
            };
            cmd_check(file)
        }
        Some("repl") => repl::run(default_context(Vec::new())),
        Some("lsp") => lsp::run(default_context(Vec::new())),
        Some("--version") | Some("-V") => {
            println!("wscript {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "wscript {} — an embeddable, statically typed scripting language\n\n{USAGE}",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::from(2)
        }
    }
}

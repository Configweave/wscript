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

use std::process::ExitCode;

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

/// How a script run ended, before anything is rendered.
///
/// Separating the outcome from its rendering is what lets a test assert on
/// a run without a subprocess. Rendering still goes to stderr; giving
/// `diag_render` a sink is #18.
pub enum Outcome {
    /// Ran to completion with this process exit code.
    Exited(u8),
    /// The script (or its imports) failed to compile.
    CompileFailed(wscript::CompileFailure),
    /// A trappable runtime fault, with the sources needed to render it.
    Faulted {
        sources: Vec<(String, String)>,
        source_map: wscript::SourceMap,
        error: wscript::RuntimeError,
    },
    /// Compiled, but has no `fn main()`.
    NoMain,
    /// Anything else at the host boundary (conversion, signature).
    Failed(String),
}

/// Compile and run `path`, delivering script output to `out`.
///
/// Warnings are rendered to stderr as they are found; everything else is
/// returned for the caller to render or assert on.
pub fn run_script(path: &str, script_args: Vec<String>, out: PrintHook) -> Outcome {
    let Ok(source) = std::fs::read_to_string(path) else {
        return Outcome::Failed(format!("cannot read `{path}`"));
    };
    let ctx = default_context(script_args);
    let resolver = file_resolver(path);
    let compiled = match ctx.compile_entry(path, &source, &resolver) {
        Ok(c) => c,
        Err(failure) => return Outcome::CompileFailed(failure),
    };
    let (unit, warnings, sources) = (compiled.unit, compiled.warnings, compiled.sources);
    diag_render::render_multi(&sources, &unit.source_map, &warnings);
    if !unit.exports.contains_key("main") {
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
        Err(Error::Runtime(e)) => Outcome::Faulted {
            sources,
            source_map: unit.source_map,
            error: e,
        },
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

pub fn cmd_run(path: &str, script_args: Vec<String>) -> ExitCode {
    match run_script(path, script_args, wscript::stdout_sink()) {
        Outcome::Exited(code) => ExitCode::from(code),
        Outcome::CompileFailed(failure) => {
            diag_render::render_multi(&failure.sources, &failure.source_map, &failure.diags);
            ExitCode::FAILURE
        }
        Outcome::Faulted {
            sources,
            source_map,
            error,
        } => {
            diag_render::render_runtime_multi(&sources, &source_map, &error);
            ExitCode::FAILURE
        }
        Outcome::NoMain => {
            eprintln!("error: `{path}` has no `fn main()`");
            ExitCode::FAILURE
        }
        Outcome::Failed(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
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
    match ctx.compile_entry(path, &source, &resolver) {
        Ok(c) => {
            diag_render::render_multi(&c.sources, &c.unit.source_map, &c.warnings);
            ExitCode::SUCCESS
        }
        Err(failure) => {
            diag_render::render_multi(&failure.sources, &failure.source_map, &failure.diags);
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

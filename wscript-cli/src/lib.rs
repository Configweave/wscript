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
use std::path::Path;
use std::process::ExitCode;

use diag_render::Renderer;
use manifest::{Mode, project_for};
use wscript::{PrintHook, RunOutcome, VmConfig};

pub fn read_source(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read `{path}`: {e}");
        ExitCode::FAILURE
    })
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
    let session = project_for(Path::new(path), Mode::Run { script_args }).session;
    let compiled = match session.compile(path, &source) {
        Ok(c) => c,
        Err(failure) => {
            err.render_multi(&failure.sources, &failure.source_map, &failure.diags);
            return Outcome::CompileFailed;
        }
    };
    // Warnings before execution, so they precede the script's own output.
    err.render_multi(
        &compiled.sources,
        &compiled.unit.source_map,
        &compiled.warnings,
    );
    match session.run(
        &compiled,
        VmConfig {
            out,
            ..VmConfig::default()
        },
    ) {
        RunOutcome::Exited(code) => Outcome::Exited(code),
        RunOutcome::NoMain => {
            let _ = writeln!(err, "error: `{path}` has no `fn main()`");
            Outcome::NoMain
        }
        RunOutcome::Faulted(e) => {
            err.render_runtime_multi(&compiled.sources, &compiled.unit.source_map, &e);
            Outcome::Faulted
        }
        RunOutcome::Failed(e) => {
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
    // `wscript check` honors wscript.toml's .wscripti interfaces — that
    // is what `Mode::Check` means (PRD §8/§9.1, ADR-0002).
    let session = project_for(Path::new(path), Mode::Check).session;
    let mut err = Renderer::stderr();
    match session.compile(path, &source) {
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
        // The repl and the lsp have no entry file, so their project is
        // whatever manifest sits above the working directory.
        Some("repl") => repl::run(project_for(
            Path::new("."),
            Mode::Run {
                script_args: iter.cloned().collect(),
            },
        )),
        Some("lsp") => lsp::run(project_for(Path::new("."), Mode::Check)),
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

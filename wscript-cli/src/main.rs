//! The `wscript` binary: `run`, `check`, `repl`, `lsp` (PRD §8).

mod diag_render;
mod lsp;
mod manifest;
mod repl;

use std::process::ExitCode;

use wscript::{Context, Error, Value, Vm};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.iter();
    let cmd = iter.next().map(String::as_str);
    match cmd {
        Some("run") => {
            let Some(file) = iter.next() else {
                eprintln!("usage: wscript run <file> [args...]");
                return ExitCode::from(2);
            };
            let script_args: Vec<String> = iter.cloned().collect();
            cmd_run(file, script_args)
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
                "wscript {} — an embeddable, statically typed scripting language\n\
                 \n\
                 usage:\n\
                 \x20 wscript run <file> [args...]   compile and execute a script\n\
                 \x20 wscript check <file>           compile only, print diagnostics\n\
                 \x20 wscript repl                   interactive session\n\
                 \x20 wscript lsp                    start the language server (stdio)",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::from(2)
        }
    }
}

/// The CLI enables the full stdlib by default (PRD §7/§8).
fn default_context(script_args: Vec<String>) -> Context {
    let mut ctx = Context::new();
    for module in wscript_std::all_modules(script_args) {
        ctx = ctx.module(module);
    }
    ctx
}

fn read_source(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read `{path}`: {e}");
        ExitCode::FAILURE
    })
}

/// Script imports resolve next to the entry file, then under the
/// manifest's `src_roots`.
fn file_resolver(script_path: &str) -> wscript::FsResolver {
    let mut resolver = wscript::FsResolver::new();
    if let Some(m) = manifest::find(std::path::Path::new(script_path)) {
        resolver.roots = m.src_roots;
    }
    resolver
}

fn cmd_run(path: &str, script_args: Vec<String>) -> ExitCode {
    let source = match read_source(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let ctx = default_context(script_args);
    let resolver = file_resolver(path);
    let compiled = match ctx.compile_entry(path, &source, &resolver) {
        Ok(c) => c,
        Err(failure) => {
            diag_render::render_multi(&failure.sources, &failure.source_map, &failure.diags);
            return ExitCode::FAILURE;
        }
    };
    let (unit, warnings, sources) = (compiled.unit, compiled.warnings, compiled.sources);
    diag_render::render_multi(&sources, &unit.source_map, &warnings);
    if !unit.exports.contains_key("main") {
        eprintln!("error: `{path}` has no `fn main()`");
        return ExitCode::FAILURE;
    }
    let mut vm = Vm::new(&ctx);
    match vm.call_values(&unit, "main", vec![]) {
        // Exit code from main's return: int, or unit → 0 (PRD §8).
        Ok(Value::Int(code)) => ExitCode::from((code & 0xff) as u8),
        Ok(_) => ExitCode::SUCCESS,
        // process::exit — a requested exit, not an error to render.
        Err(Error::Runtime(e)) if e.exit_code.is_some() => {
            ExitCode::from((e.exit_code.unwrap() & 0xff) as u8)
        }
        Err(Error::Runtime(e)) => {
            diag_render::render_runtime_multi(&sources, &unit.source_map, &e);
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_check(path: &str) -> ExitCode {
    let source = match read_source(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    // `wscript check` honors wscript.toml's .wscripti interfaces (PRD §8/§9.1).
    // A manifest describes the *complete* host context the script runs
    // under (interface dumps include everything the host registers), so
    // when one is present the CLI's default stdlib stays out of the
    // registry — otherwise a same-named CLI module (e.g. `fs`) would
    // shadow the embedder's interface and mis-check real scripts. List
    // the stdlib's own .wscripti in the manifest to check against it.
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

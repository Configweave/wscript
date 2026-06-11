//! The `wisp` binary: `run`, `check`, `repl`, `lsp` (PRD §8).

mod diag_render;
mod lsp;
mod manifest;
mod repl;

use std::process::ExitCode;

use wisp::{Context, Error, Value, Vm};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.iter();
    let cmd = iter.next().map(String::as_str);
    match cmd {
        Some("run") => {
            let Some(file) = iter.next() else {
                eprintln!("usage: wisp run <file> [args...]");
                return ExitCode::from(2);
            };
            let script_args: Vec<String> = iter.cloned().collect();
            cmd_run(file, script_args)
        }
        Some("check") => {
            let Some(file) = iter.next() else {
                eprintln!("usage: wisp check <file>");
                return ExitCode::from(2);
            };
            cmd_check(file)
        }
        Some("repl") => repl::run(default_context(Vec::new())),
        Some("lsp") => lsp::run(default_context(Vec::new())),
        Some("--version") | Some("-V") => {
            println!("wisp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "wisp {} — an embeddable, statically typed scripting language\n\
                 \n\
                 usage:\n\
                 \x20 wisp run <file> [args...]   compile and execute a script\n\
                 \x20 wisp check <file>           compile only, print diagnostics\n\
                 \x20 wisp repl                   interactive session\n\
                 \x20 wisp lsp                    start the language server (stdio)",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::from(2)
        }
    }
}

/// The CLI enables the full stdlib by default (PRD §7/§8).
fn default_context(script_args: Vec<String>) -> Context {
    let mut ctx = Context::new();
    for module in wisp_std::all_modules(script_args) {
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

fn cmd_run(path: &str, script_args: Vec<String>) -> ExitCode {
    let source = match read_source(path) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let ctx = default_context(script_args);
    let (unit, warnings) = match ctx.compile_verbose(&source) {
        Ok(ok) => ok,
        Err(diags) => {
            diag_render::render(path, &source, &diags);
            return ExitCode::FAILURE;
        }
    };
    diag_render::render(path, &source, &warnings);
    if !unit.exports.contains_key("main") {
        eprintln!("error: `{path}` has no `fn main()`");
        return ExitCode::FAILURE;
    }
    let mut vm = Vm::new(&ctx);
    match vm.call_values(&unit, "main", vec![]) {
        // Exit code from main's return: int, or unit → 0 (PRD §8).
        Ok(Value::Int(code)) => ExitCode::from((code & 0xff) as u8),
        Ok(_) => ExitCode::SUCCESS,
        Err(Error::Runtime(e)) => {
            diag_render::render_runtime(path, &source, &e);
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
    let mut ctx = default_context(Vec::new());
    // `wisp check` honors wisp.toml's .wispi interfaces (PRD §8/§9.1).
    if let Some(m) = manifest::find(std::path::Path::new(path)) {
        let mut reg = ctx.registry().clone();
        manifest::load_interfaces(&m, &mut reg);
        ctx = wisp::Context::from_registry(reg);
    }
    match ctx.compile_verbose(&source) {
        Ok((_unit, warnings)) => {
            diag_render::render(path, &source, &warnings);
            ExitCode::SUCCESS
        }
        Err(diags) => {
            diag_render::render(path, &source, &diags);
            ExitCode::FAILURE
        }
    }
}

//! `wisp repl` (PRD §8): line-oriented REPL with persistent top-level
//! `let` bindings, implemented by incremental re-compilation (best-effort,
//! per the PRD).
//!
//! How it works: item declarations (`fn`, `struct`, `use`, …) accumulate
//! as a source prefix. A statement is compiled twice — once wrapped in a
//! unit function to *infer* its type, then (when it produces a value)
//! wrapped in a function returning that type so the value can be captured
//! into the persistent environment and printed. Bindings are passed back
//! into later lines as function parameters.
//!
//! Known limitation (documented): values containing *functions* (and `dyn`
//! values) do not survive across lines — proto indices are per-compilation.

use std::process::ExitCode;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use wisp::{Context, DefTable, Value};
use wisp_compiler::ast::{Item, Stmt};

use crate::diag_render;

pub fn run(ctx: Context) -> ExitCode {
    println!(
        "wisp {} — type expressions or declarations; :help for commands",
        env!("CARGO_PKG_VERSION")
    );
    let Ok(mut editor) = DefaultEditor::new() else {
        eprintln!("error: cannot initialize the line editor");
        return ExitCode::FAILURE;
    };
    let defs = ctx.registry().defs.clone();
    let mut repl = Repl {
        ctx,
        items: String::new(),
        bindings: Vec::new(),
        defs,
    };
    loop {
        let line = match read_input(&mut editor) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed {
            ":quit" | ":q" | "exit" => break,
            ":help" => {
                println!(
                    ":help          show this help\n\
                     :clear         forget all declarations and bindings\n\
                     :env           list current bindings\n\
                     :quit          leave the repl\n\
                     \n\
                     Anything else is wisp: declarations (fn/struct/enum/trait/impl/use)\n\
                     persist, `let` bindings persist, expressions print their value."
                );
                continue;
            }
            ":clear" => {
                repl.items.clear();
                repl.bindings.clear();
                println!("cleared");
                continue;
            }
            ":env" => {
                for (name, ty, value) in &repl.bindings {
                    println!("{name}: {ty} = {}", value.display(&repl.defs));
                }
                continue;
            }
            _ => {}
        }
        let _ = editor.add_history_entry(trimmed);
        repl.handle(trimmed);
    }
    ExitCode::SUCCESS
}

/// Read one logical input, continuing across lines while delimiters are
/// open (naive brace counting that skips string/char literals).
fn read_input(editor: &mut DefaultEditor) -> Result<Option<String>, ReadlineError> {
    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() { "wisp> " } else { "  ... " };
        match editor.readline(prompt) {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');
                if open_delims(&buffer) == 0 {
                    return Ok(Some(buffer));
                }
            }
            Err(ReadlineError::Interrupted) => return Ok(Some(String::new())),
            Err(ReadlineError::Eof) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
}

fn open_delims(src: &str) -> i32 {
    let mut depth = 0;
    let mut chars = src.chars().peekable();
    let mut in_str = false;
    let mut in_char = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' if in_str || in_char => {
                chars.next();
            }
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '(' | '[' | '{' if !in_str && !in_char => depth += 1,
            ')' | ']' | '}' if !in_str && !in_char => depth -= 1,
            '/' if !in_str && !in_char && chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    depth.max(0)
}

struct Repl {
    ctx: Context,
    items: String,
    bindings: Vec<(String, String, Value)>,
    /// Def table from the most recent successful compile — script-declared
    /// types live here (the registry alone only has host defs).
    defs: DefTable,
}

impl Repl {
    fn render(&self, value: &Value) -> String {
        value.display(&self.defs)
    }
}

impl Repl {
    fn handle(&mut self, input: &str) {
        if starts_item(input) {
            self.handle_item(input);
        } else {
            self.handle_stmt(input);
        }
    }

    fn handle_item(&mut self, input: &str) {
        let candidate = if self.items.is_empty() {
            input.to_string()
        } else {
            format!("{}\n{input}", self.items)
        };
        let probe = format!("{candidate}\nfn __probe() {{}}\n");
        match self.ctx.compile_verbose(&probe) {
            Ok((_, warnings)) => {
                diag_render::render("<repl>", &probe, &warnings);
                self.items = candidate;
            }
            Err(diags) => diag_render::render("<repl>", &probe, &diags),
        }
    }

    fn params(&self) -> String {
        self.bindings
            .iter()
            .map(|(name, ty, _)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn args(&self) -> Vec<Value> {
        self.bindings.iter().map(|(_, _, v)| v.clone()).collect()
    }

    fn handle_stmt(&mut self, input: &str) {
        // Pass 1: wrap in a unit fn (statement-terminated) to infer types.
        let src1 = format!(
            "{}\nfn __line({}) {{\n{input};\n}}\n",
            self.items,
            self.params()
        );
        let analysis = wisp_compiler::analyze(&src1, self.ctx.registry());
        let errors: Vec<wisp::Diagnostic> = {
            let mut all = analysis.parse.diags.clone();
            all.extend(analysis.check.diags.clone());
            all
        };
        if errors
            .iter()
            .any(|d| d.severity == wisp::Severity::Error)
        {
            diag_render::render("<repl>", &src1, &errors);
            return;
        }

        // What does the last statement produce?
        let last = analysis
            .parse
            .file
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                Item::Fn(f) if f.name.name == "__line" => f.body.stmts.last(),
                _ => None,
            });
        enum Kind {
            Binding(String, String),
            Show(String),
            Run,
        }
        let kind = match last {
            Some(Stmt::Let { name, id, .. }) => {
                let ty = analysis.check.types.get(id).cloned();
                match ty {
                    Some(t) if nameable(&t) => {
                        Kind::Binding(name.name.clone(), t.display(&analysis.check.defs))
                    }
                    _ => Kind::Run,
                }
            }
            Some(Stmt::Expr { expr, .. }) => {
                let ty = analysis.check.types.get(&expr.id).cloned();
                match ty {
                    Some(t)
                        if !matches!(
                            t,
                            wisp::Type::Unit | wisp::Type::Never | wisp::Type::Error
                        ) && nameable(&t) =>
                    {
                        Kind::Show(t.display(&analysis.check.defs))
                    }
                    _ => Kind::Run,
                }
            }
            _ => Kind::Run,
        };

        match kind {
            Kind::Run => {
                self.execute(&src1, None);
            }
            Kind::Show(ty) => {
                let src2 = format!(
                    "{}\nfn __line({}) -> {ty} {{\n{input}\n}}\n",
                    self.items,
                    self.params()
                );
                if let Some(value) = self.execute(&src2, None) {
                    println!("{}", self.render(&value));
                }
            }
            Kind::Binding(name, ty) => {
                let src2 = format!(
                    "{}\nfn __line({}) -> {ty} {{\n{input}\n{name}\n}}\n",
                    self.items,
                    self.params()
                );
                if let Some(value) = self.execute(&src2, None) {
                    println!("{name}: {ty} = {}", self.render(&value));
                    self.bindings.retain(|(n, ..)| *n != name);
                    self.bindings.push((name, ty, value));
                }
            }
        }
    }

    fn execute(&mut self, src: &str, _expect: Option<&str>) -> Option<Value> {
        let unit = match self.ctx.compile_verbose(src) {
            Ok((unit, warnings)) => {
                diag_render::render("<repl>", src, &warnings);
                self.defs = unit.defs.clone();
                unit
            }
            Err(diags) => {
                diag_render::render("<repl>", src, &diags);
                return None;
            }
        };
        let mut vm = wisp::Vm::new(&self.ctx);
        match vm.call_values(&unit, "__line", self.args()) {
            Ok(value) => Some(value),
            Err(wisp::Error::Runtime(e)) => {
                diag_render::render_runtime("<repl>", src, &e);
                None
            }
            Err(e) => {
                eprintln!("error: {e}");
                None
            }
        }
    }
}

fn starts_item(input: &str) -> bool {
    let first = input.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "fn" | "struct" | "enum" | "trait" | "impl" | "use"
    ) || input.trim_start().starts_with("#[")
}

/// Types whose surface syntax can round-trip through a generated
/// signature. Function and dyn values cannot persist across compilations
/// (proto/vtable indices are per-unit).
fn nameable(t: &wisp::Type) -> bool {
    use wisp::Type;
    match t {
        Type::Int | Type::Float | Type::Bool | Type::Char | Type::Unit | Type::Str => true,
        Type::List(e) | Type::Option(e) | Type::Weak(e) => nameable(e),
        Type::Map(k, v) => nameable(k) && nameable(v),
        Type::Result(a, b) => nameable(a) && nameable(b),
        Type::Named(_) => true,
        _ => false,
    }
}

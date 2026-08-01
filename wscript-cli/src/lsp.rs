//! `wscript lsp` — the language server (PRD §9), built on tower-lsp-server
//! over stdio. The four v1 features, in priority order: diagnostics, hover,
//! go-to-definition, completions. That list is a ceiling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp_server::jsonrpc::Result as LspResult;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use wscript::Session;
use wscript_compiler::ast;
use wscript_compiler::wscripti::WscriptiIndex;

use crate::manifest::{Mode, Project, project_for};

pub fn run(project: Project) -> std::process::ExitCode {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(|client| Backend {
            client,
            state: Mutex::new(State {
                project,
                docs: HashMap::new(),
            }),
        });
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    std::process::ExitCode::SUCCESS
}

struct State {
    /// The project the editor has open. Built from the working directory
    /// at startup, then rebuilt from the workspace root at `initialize` —
    /// the same `Mode::Check` project `wscript check` compiles with, so
    /// the two cannot resolve different imports.
    project: Project,
    docs: HashMap<Uri, String>,
}

impl State {
    /// The session to compile with. Cloned rather than borrowed so
    /// analysis runs off the state lock.
    fn session(&self) -> Session {
        self.project.session.clone()
    }
}

struct Backend {
    client: Client,
    state: Mutex<State>,
}

// --------------------------------------------------------- text helpers

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += c.len_utf16() as u32;
        }
    }
    Position { line, character }
}

fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, c) in text.char_indices() {
        if line == pos.line && character >= pos.character {
            return i;
        }
        if line > pos.line {
            return i;
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += c.len_utf16() as u32;
        }
    }
    text.len()
}

fn span_to_range(text: &str, span: wscript::Span) -> Range {
    Range {
        start: offset_to_position(text, span.lo as usize),
        end: offset_to_position(text, span.hi as usize),
    }
}

// ------------------------------------------------------ AST span index

/// Expressions by position, for hover, goto-definition and completion.
struct ExprIndex {
    /// (span, node id) for every expression, parents before children.
    spans: Vec<(wscript::Span, ast::NodeId)>,
    /// The call an expression is the callee of. A call's resolution hangs
    /// off the call, not off its callee, so pointing at `atan2` in
    /// `math::atan2(1.0, 2.0)` lands on the callee path and would
    /// otherwise find nothing.
    call_of_callee: HashMap<ast::NodeId, ast::NodeId>,
}

/// Collect the expression index, for position lookups.
///
/// Explicit worklist, not recursion: operator/postfix chains give the AST
/// unbounded depth even when the parser's nesting limit holds, and this
/// runs on a tokio stack. A node is recorded when popped, its children
/// later — parents always precede children in the output (the `find`
/// tie-break below relies on that).
///
/// This deliberately does *not* use `ast::Visit`, which is recursive.
/// `Session::analyze` runs the pipeline on a 32 MiB scoped thread, but
/// this index is built afterwards on tokio's 2 MiB stack, and it runs on
/// documents that failed to check — so a deep-but-parseable AST would
/// reach it. The exhaustive match below keeps the compile-time guarantee
/// that `Visit` provides; only the traversal strategy differs.
fn expr_index(file: &ast::SourceFile) -> ExprIndex {
    enum Work<'a> {
        E(&'a ast::Expr),
        B(&'a ast::Block),
    }
    let mut out = ExprIndex {
        spans: Vec::new(),
        call_of_callee: HashMap::new(),
    };
    let mut stack: Vec<Work> = Vec::new();
    for item in &file.items {
        match item {
            ast::Item::Fn(f) => stack.push(Work::B(&f.body)),
            ast::Item::Impl(im) => stack.extend(im.fns.iter().map(|f| Work::B(&f.body))),
            _ => {}
        }
    }
    while let Some(work) = stack.pop() {
        let e = match work {
            Work::E(e) => e,
            Work::B(b) => {
                for stmt in &b.stmts {
                    match stmt {
                        ast::Stmt::Let { init, .. } => stack.push(Work::E(init)),
                        ast::Stmt::LetElse {
                            init, else_block, ..
                        } => {
                            stack.push(Work::E(init));
                            stack.push(Work::B(else_block));
                        }
                        ast::Stmt::Expr { expr, .. } => stack.push(Work::E(expr)),
                    }
                }
                continue;
            }
        };
        out.spans.push((e.span, e.id));
        use ast::ExprKind::*;
        match &e.kind {
            Unary { expr, .. } | Try(expr) => stack.push(Work::E(expr)),
            StrInterp(parts) => {
                for p in parts {
                    if let wscript_compiler::ast::InterpPart::Hole(h) = p {
                        stack.push(Work::E(h));
                    }
                }
            }
            Binary { lhs, rhs, .. } => {
                stack.push(Work::E(lhs));
                stack.push(Work::E(rhs));
            }
            Assign { target, value, .. } => {
                stack.push(Work::E(target));
                stack.push(Work::E(value));
            }
            Call { callee, args } => {
                out.call_of_callee.insert(callee.id, e.id);
                stack.push(Work::E(callee));
                stack.extend(args.iter().map(Work::E));
            }
            MethodCall { recv, args, .. } => {
                stack.push(Work::E(recv));
                stack.extend(args.iter().map(Work::E));
            }
            Field { obj, .. } => stack.push(Work::E(obj)),
            Index { obj, idx } => {
                stack.push(Work::E(obj));
                stack.push(Work::E(idx));
            }
            StructLit { fields, .. } => stack.extend(fields.iter().map(|(_, v)| Work::E(v))),
            ListLit(items) => stack.extend(items.iter().map(Work::E)),
            MapLit(entries) => {
                for (k, v) in entries {
                    stack.push(Work::E(k));
                    stack.push(Work::E(v));
                }
            }
            If { cond, then, else_ } => {
                stack.push(Work::E(cond));
                stack.push(Work::B(then));
                if let Some(e) = else_ {
                    stack.push(Work::E(e));
                }
            }
            IfLet {
                scrutinee,
                then,
                else_,
                ..
            } => {
                stack.push(Work::E(scrutinee));
                stack.push(Work::B(then));
                if let Some(e) = else_ {
                    stack.push(Work::E(e));
                }
            }
            Match { scrutinee, arms } => {
                stack.push(Work::E(scrutinee));
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        stack.push(Work::E(g));
                    }
                    stack.push(Work::E(&arm.body));
                }
            }
            While { cond, body } => {
                stack.push(Work::E(cond));
                stack.push(Work::B(body));
            }
            Loop { body } => stack.push(Work::B(body)),
            For { iter, body, .. } => {
                stack.push(Work::E(iter));
                stack.push(Work::B(body));
            }
            Range { lo, hi, .. } => {
                stack.push(Work::E(lo));
                stack.push(Work::E(hi));
            }
            Return(v) => {
                if let Some(v) = v {
                    stack.push(Work::E(v));
                }
            }
            Block(b) => stack.push(Work::B(b)),
            Closure { body, .. } => stack.push(Work::E(body)),
            // Leaves, listed rather than caught by `_`: this walk cannot
            // share `ast::walk_expr` (see the note above), so an exhaustive
            // match is what makes a new expression form a compile error
            // here instead of a subtree that silently stops being indexed.
            IntLit(_)
            | FloatLit(_)
            | QuantityLit { .. }
            | BoolLit(_)
            | CharLit(_)
            | StrLit(_)
            | UnitLit
            | Path(_)
            | Break
            | Continue
            | Error => {}
        }
    }
    out
}

impl ExprIndex {
    /// Smallest expression containing `offset`. Children are walked after
    /// their parents, so on span ties the reversed scan prefers the
    /// innermost node (error-recovery wrappers share their child's span).
    fn node_at(&self, offset: usize) -> Option<ast::NodeId> {
        self.spans
            .iter()
            .rev()
            .filter(|(span, _)| span.lo as usize <= offset && offset < span.hi as usize)
            .min_by_key(|(span, _)| span.hi - span.lo)
            .map(|(_, id)| *id)
    }

    /// Expression ending exactly at `offset` (for `.` completions).
    fn node_ending_at(&self, offset: usize) -> Option<ast::NodeId> {
        self.spans
            .iter()
            .rev()
            .filter(|(span, _)| span.hi as usize == offset)
            .min_by_key(|(span, _)| span.hi - span.lo)
            .map(|(_, id)| *id)
    }

    /// The nodes a cursor at `offset` can carry a host registration: the
    /// expression itself, and — because a call's resolution hangs off the
    /// call rather than its callee — the call it is the callee of.
    ///
    /// Nothing wider: an enclosing call reached any other way is one the
    /// cursor is not on. `v` in `v.get(k)` is the method call's receiver,
    /// and hovering it should say what `v` is, not what `get` takes.
    fn host_nodes_at(&self, offset: usize) -> Vec<ast::NodeId> {
        let Some(node) = self.node_at(offset) else {
            return Vec::new();
        };
        let mut nodes = vec![node];
        nodes.extend(self.call_of_callee.get(&node).copied());
        nodes
    }
}

// ---------------------------------------------------- builtin methods

const STR_METHODS: &[&str] = &[
    "len",
    "bytes_len",
    "is_empty",
    "split",
    "trim",
    "trim_start",
    "trim_end",
    "to_upper",
    "to_lower",
    "starts_with",
    "ends_with",
    "contains",
    "find",
    "replace",
    "repeat",
    "pad_left",
    "pad_right",
    "chars",
    "slice",
    "parse_int",
    "parse_float",
];
const LIST_METHODS: &[&str] = &[
    "len", "is_empty", "push", "pop", "get", "set", "insert", "remove", "clear", "contains",
    "index_of", "reverse", "sort", "join", "map", "filter", "fold", "first", "last", "slice",
    "concat", "clone",
];
const MAP_METHODS: &[&str] = &[
    "len",
    "is_empty",
    "insert",
    "remove",
    "get",
    "contains_key",
    "keys",
    "values",
    "clear",
    "clone",
];
const OPTION_METHODS: &[&str] = &["is_some", "is_none", "unwrap", "unwrap_or", "expect"];
const RESULT_METHODS: &[&str] = &[
    "is_ok",
    "is_err",
    "unwrap",
    "unwrap_or",
    "unwrap_err",
    "expect",
];
const KEYWORDS: &[&str] = &[
    "let", "fn", "struct", "enum", "trait", "impl", "for", "in", "while", "loop", "if", "else",
    "match", "return", "break", "continue", "use", "true", "false", "dyn", "self",
];
const PRELUDE: &[&str] = &[
    "print", "println", "str", "fmt", "same", "weak", "int", "float",
];

// --------------------------------------------------------------- server

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // Load wscript.toml interfaces from the workspace root (PRD §9.1).
        #[allow(deprecated)]
        let root = params
            .root_uri
            .as_ref()
            .and_then(|u| u.to_file_path())
            .map(|p| p.into_owned());
        if let Some(root) = root {
            // Exactly what `wscript check` builds for a file under this
            // root: the declared interfaces as the host (ADR-0002) *and*
            // the manifest's `src_roots`. Getting the second half from
            // the same call is the point — the server used to load the
            // interfaces here and then resolve imports with no roots at
            // all, so the editor reported E0200 on `use` statements the
            // CLI resolved happily.
            let mut state = self.state.lock().unwrap();
            state.project = project_for(&root, Mode::Check);
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), ":".into()]),
                    ..CompletionOptions::default()
                }),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "wscript-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        {
            let mut state = self.state.lock().unwrap();
            state.docs.insert(uri.clone(), text.clone());
        }
        self.publish_diagnostics(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let text = change.text;
        {
            let mut state = self.state.lock().unwrap();
            state.docs.insert(uri.clone(), text.clone());
        }
        self.publish_diagnostics(uri, text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut state = self.state.lock().unwrap();
        state.docs.remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let (text, session) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.session())
        };
        let registry = session.registry();
        let analysis = session.analyze(&entry_path(&uri), &text);
        let index = expr_index(&analysis.parse.file);
        let offset = position_to_offset(&text, pos);
        let Some(node) = index.node_at(offset) else {
            return Ok(None);
        };
        let mut lines = Vec::new();
        if let Some(ty) = analysis.check.types.get(&node) {
            lines.push(format!(
                "```wscript\n{}\n```",
                ty.display(&analysis.check.defs)
            ));
            // For a unit value, the family's table is the thing you want
            // to see: which suffixes exist and what they convert to.
            if let wscript::core::types::Type::Named(id) = ty
                && let Some(u) = analysis.check.defs.as_unit(*id)
            {
                let units: Vec<String> = u
                    .units
                    .iter()
                    .map(|(n, f)| format!("`{n}` = {}", f.display()))
                    .collect();
                lines.push(format!(
                    "unit family, stored in `{}` — {}",
                    u.base_name(),
                    units.join(", ")
                ));
            }
        }
        // Host call info: signature + docs (PRD §9 feature 2).
        if let Some(info) = host_target(registry, &analysis, &index.host_nodes_at(offset)) {
            lines.push(format!(
                "`{}{}`",
                info.qualified_name(),
                render_sig_named(&info.sig, info.params.as_deref(), &analysis.check.defs)
            ));
            if let Some(doc) = info.doc {
                lines.push(doc);
            }
        }
        if lines.is_empty() {
            return Ok(None);
        }
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: lines.join("\n\n"),
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let (text, session, wscripti) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.session(), state.project.interfaces.clone())
        };
        let registry = session.registry();
        let analysis = session.analyze(&entry_path(&uri), &text);
        let index = expr_index(&analysis.parse.file);
        let offset = position_to_offset(&text, pos);
        let Some(node) = index.node_at(offset) else {
            return Ok(None);
        };
        // Script-local symbols.
        if let Some(span) = analysis.check.def_spans.get(&node) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range: span_to_range(&text, *span),
            })));
        }
        // Host symbols jump to the .wscripti entry (PRD §9 feature 3).
        let target = host_target(registry, &analysis, &index.host_nodes_at(offset))
            .and_then(|info| lookup_wscripti(&wscripti, |i| info.wscripti_span(i)));
        if let Some((path, span)) = target
            && let Ok(file_text) = std::fs::read_to_string(&path)
            && let Some(file_uri) = Uri::from_file_path(&path)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: file_uri,
                range: span_to_range(&file_text, span),
            })));
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let (text, session) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.session())
        };
        let registry = session.registry();
        let offset = position_to_offset(&text, pos);
        let before = &text[..offset.min(text.len())];

        let mut items: Vec<CompletionItem> = Vec::new();
        let push = |items: &mut Vec<CompletionItem>,
                    label: &str,
                    kind: CompletionItemKind,
                    detail: Option<String>| {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(kind),
                detail,
                ..CompletionItem::default()
            });
        };

        if let Some(rest) = before.strip_suffix("::") {
            // Module members or enum variants after `::` (PRD §9 feature 4).
            let seg = trailing_ident(rest);
            let analysis = session.analyze(&entry_path(&uri), &text);
            if let Some(module) = registry.modules.iter().find(|m| m.name == seg) {
                for f in &module.fns {
                    push(
                        &mut items,
                        &f.name,
                        CompletionItemKind::FUNCTION,
                        Some(format!(
                            "{}{}",
                            render_sig_named(&f.sig, f.param_names(), &analysis.check.defs),
                            f.doc
                                .as_deref()
                                .map(|d| format!(" — {d}"))
                                .unwrap_or_default()
                        )),
                    );
                }
                for (name, ty, _) in &module.consts {
                    push(
                        &mut items,
                        name,
                        CompletionItemKind::CONSTANT,
                        Some(ty.display(&analysis.check.defs)),
                    );
                }
            }
            // Enum variants.
            for def in &analysis.check.defs.defs {
                if let wscript::core::defs::DefKind::Enum(e) = def
                    && e.name == seg
                {
                    for v in &e.variants {
                        push(&mut items, &v.name, CompletionItemKind::ENUM_MEMBER, None);
                    }
                }
            }
        } else if let Some(rest) = before.strip_suffix(".") {
            // Methods after `.` — type the receiver via analysis.
            let analysis = session.analyze(&entry_path(&uri), &text);
            let index = expr_index(&analysis.parse.file);
            let recv = index.node_ending_at(rest.trim_end().len());
            let ty = recv.and_then(|n| analysis.check.types.get(&n)).cloned();
            match ty {
                Some(wscript::Type::Str) => {
                    for m in STR_METHODS {
                        push(&mut items, m, CompletionItemKind::METHOD, None);
                    }
                }
                Some(wscript::Type::List(_)) => {
                    for m in LIST_METHODS {
                        push(&mut items, m, CompletionItemKind::METHOD, None);
                    }
                }
                Some(wscript::Type::Map(..)) => {
                    for m in MAP_METHODS {
                        push(&mut items, m, CompletionItemKind::METHOD, None);
                    }
                }
                Some(wscript::Type::Option(_)) => {
                    for m in OPTION_METHODS {
                        push(&mut items, m, CompletionItemKind::METHOD, None);
                    }
                }
                Some(wscript::Type::Result(..)) => {
                    for m in RESULT_METHODS {
                        push(&mut items, m, CompletionItemKind::METHOD, None);
                    }
                }
                Some(wscript::Type::Weak(_)) => {
                    push(&mut items, "upgrade", CompletionItemKind::METHOD, None);
                }
                Some(wscript::Type::Named(def)) => {
                    if let Some(methods) = analysis.check.methods_by_type.get(&def) {
                        for (name, sig) in methods {
                            push(
                                &mut items,
                                name,
                                CompletionItemKind::METHOD,
                                Some(render_sig(sig, &analysis.check.defs)),
                            );
                        }
                    }
                    if let Some(methods) = registry.methods.get(&def) {
                        for m in methods {
                            push(
                                &mut items,
                                &m.name,
                                CompletionItemKind::METHOD,
                                Some(render_sig_named(
                                    &m.sig,
                                    m.param_names(),
                                    &analysis.check.defs,
                                )),
                            );
                        }
                    }
                    // Struct fields.
                    if let Some(s) = analysis.check.defs.as_struct(def)
                        && !s.opaque
                    {
                        for (fname, fty) in &s.fields {
                            push(
                                &mut items,
                                fname,
                                CompletionItemKind::FIELD,
                                Some(fty.display(&analysis.check.defs)),
                            );
                        }
                    }
                    // Units of a family: `d.` offers `ms`, `s`, `min`, …
                    if let Some(u) = analysis.check.defs.as_unit(def) {
                        let base = u.base.display(&analysis.check.defs);
                        for (uname, factor) in &u.units {
                            push(
                                &mut items,
                                uname,
                                CompletionItemKind::UNIT,
                                Some(format!("-> {base} (1 {uname} = {})", factor.display())),
                            );
                        }
                    }
                }
                Some(wscript::Type::Dyn(tr)) => {
                    if let Some(td) = analysis.check.defs.as_trait(tr) {
                        for (name, sig) in &td.methods {
                            push(
                                &mut items,
                                name,
                                CompletionItemKind::METHOD,
                                Some(render_sig(sig, &analysis.check.defs)),
                            );
                        }
                    }
                }
                _ => {}
            }
        } else {
            // Keywords, prelude, in-scope items, modules, types.
            for k in KEYWORDS {
                push(&mut items, k, CompletionItemKind::KEYWORD, None);
            }
            for p in PRELUDE {
                push(&mut items, p, CompletionItemKind::FUNCTION, None);
            }
            let analysis = session.analyze(&entry_path(&uri), &text);
            for (name, (_, sig)) in &analysis.check.exports {
                push(
                    &mut items,
                    name,
                    CompletionItemKind::FUNCTION,
                    Some(render_sig(sig, &analysis.check.defs)),
                );
            }
            for module in &registry.modules {
                push(
                    &mut items,
                    &module.name,
                    CompletionItemKind::MODULE,
                    module.doc.clone(),
                );
            }
            for def in &analysis.check.defs.defs {
                match def {
                    wscript::core::defs::DefKind::Struct(s) => {
                        push(&mut items, &s.name, CompletionItemKind::STRUCT, None);
                    }
                    wscript::core::defs::DefKind::Enum(e) => {
                        push(&mut items, &e.name, CompletionItemKind::ENUM, None);
                    }
                    wscript::core::defs::DefKind::Trait(t) => {
                        push(&mut items, &t.name, CompletionItemKind::INTERFACE, None);
                    }
                    wscript::core::defs::DefKind::Unit(u) => {
                        push(
                            &mut items,
                            &u.name,
                            CompletionItemKind::UNIT,
                            Some(format!(
                                "unit family, base `{}` ({})",
                                u.base_name(),
                                u.base.display(&analysis.check.defs)
                            )),
                        );
                    }
                }
            }
        }
        Ok(Some(CompletionResponse::Array(items)))
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Uri, text: String) {
        let session = {
            let state = self.state.lock().unwrap();
            state.session()
        };
        // Imports resolve through the project's session, so a multi-file
        // script doesn't light up with false unknown-module errors.
        // Diagnostics landing in IMPORTED files are dropped here — they
        // belong to that file's own `textDocument/publishDiagnostics`,
        // which the editor gets when that file is opened.
        let analysis = session.analyze(&entry_path(&uri), &text);
        let entry_len = text.len() as u32;
        let mut all = analysis.parse.diags;
        all.extend(analysis.check.diags);
        all.retain(|d| d.span.lo <= entry_len);
        let diags: Vec<Diagnostic> = all
            .into_iter()
            .map(|d| Diagnostic {
                range: span_to_range(&text, d.span),
                severity: Some(match d.severity {
                    wscript::Severity::Error => DiagnosticSeverity::ERROR,
                    wscript::Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                code: Some(NumberOrString::String(d.code.to_string())),
                source: Some("wscript".into()),
                message: match d.help_text() {
                    Some(help) => format!("{}\nhelp: {help}", d.message),
                    None => d.message.clone(),
                },
                ..Diagnostic::default()
            })
            .collect();
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

/// The entry path an open document is analyzed under. Imports resolve
/// relative to it, which is what gives every LSP feature multi-file
/// support; an unsaved or non-file URI falls back to a placeholder.
fn entry_path(uri: &Uri) -> String {
    uri.to_file_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script".to_string())
}

fn trailing_ident(text: &str) -> &str {
    let end = text.len();
    let start = text
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    &text[start..end]
}

fn render_sig(sig: &wscript::FnSig, defs: &wscript::DefTable) -> String {
    render_sig_named(sig, None, defs)
}

/// A signature for display. Declared parameter names are shown
/// (`(y: float, x: float)` — the point of declaring them); where the host
/// declared none the types stand alone rather than gaining an invented name.
fn render_sig_named(
    sig: &wscript::FnSig,
    names: Option<&[String]>,
    defs: &wscript::DefTable,
) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| match names.and_then(|names| names.get(i)) {
            Some(name) => format!("{name}: {}", p.display(defs)),
            None => p.display(defs),
        })
        .collect();
    if sig.ret == wscript::Type::Unit {
        format!("({})", params.join(", "))
    } else {
        format!("({}) -> {}", params.join(", "), sig.ret.display(defs))
    }
}

/// What the editor can say about one host registration.
struct HostInfo {
    kind: HostKind,
    /// Module for a function, type for a method.
    owner: String,
    name: String,
    sig: wscript::FnSig,
    /// Declared parameter names; `None` when the host declared none.
    params: Option<Vec<String>>,
    doc: Option<String>,
}

#[derive(Clone, Copy)]
enum HostKind {
    Fn,
    Method,
}

/// What follows from which kind of registration this is: how owner and
/// name are written together, and which of the interface's two indexes
/// records it. One `match` produces both, so they cannot disagree about
/// what a kind means.
struct Addressing {
    separator: &'static str,
    wscripti_index: fn(&WscriptiIndex) -> &HashMap<(String, String), wscript::Span>,
}

impl HostInfo {
    fn addressing(&self) -> Addressing {
        match self.kind {
            HostKind::Fn => Addressing {
                separator: "::",
                wscripti_index: |i| &i.module_items,
            },
            HostKind::Method => Addressing {
                separator: ".",
                wscripti_index: |i| &i.methods,
            },
        }
    }

    /// `math::atan2` for a module function, `Value.get` for a method.
    fn qualified_name(&self) -> String {
        format!("{}{}{}", self.owner, self.addressing().separator, self.name)
    }

    /// Where the `.wscripti` interface declares this registration.
    fn wscripti_span(&self, index: &WscriptiIndex) -> Option<wscript::Span> {
        (self.addressing().wscripti_index)(index)
            .get(&(self.owner.clone(), self.name.clone()))
            .copied()
    }
}

/// The host registration `nodes` refers to, if any — the first that
/// resolves to one, `nodes` being ordered innermost first (see
/// [`ExprIndex::host_nodes_at`]). Hover and goto-definition want the same
/// answer, and both want exactly one.
fn host_target(
    reg: &wscript::Registry,
    analysis: &wscript::Analysis,
    nodes: &[ast::NodeId],
) -> Option<HostInfo> {
    nodes.iter().find_map(
        |node| match (analysis.check.call(*node), analysis.check.method(*node)) {
            (Some(wscript_compiler::check::CallKind::Host(idx)), _) => host_fn_info(reg, *idx),
            (_, Some(wscript_compiler::check::MethodRes::Host(idx))) => host_method_info(reg, *idx),
            _ => None,
        },
    )
}

fn host_fn_info(reg: &wscript::Registry, idx: u32) -> Option<HostInfo> {
    for module in &reg.modules {
        for f in &module.fns {
            if f.host_idx == idx {
                return Some(HostInfo {
                    kind: HostKind::Fn,
                    owner: module.name.clone(),
                    name: f.name.clone(),
                    sig: f.sig.clone(),
                    params: f.param_names().map(<[String]>::to_vec),
                    doc: f.doc.clone(),
                });
            }
        }
    }
    None
}

fn host_method_info(reg: &wscript::Registry, idx: u32) -> Option<HostInfo> {
    for (def, methods) in &reg.methods {
        for m in methods {
            if m.host_idx == idx {
                return Some(HostInfo {
                    kind: HostKind::Method,
                    owner: reg.defs.name_of(*def).to_string(),
                    name: m.name.clone(),
                    sig: m.sig.clone(),
                    params: m.param_names().map(<[String]>::to_vec),
                    doc: m.doc.clone(),
                });
            }
        }
    }
    None
}

fn lookup_wscripti<F>(
    indexes: &[(PathBuf, WscriptiIndex)],
    f: F,
) -> Option<(PathBuf, wscript::Span)>
where
    F: Fn(&WscriptiIndex) -> Option<wscript::Span>,
{
    for (path, index) in indexes {
        if let Some(span) = f(index) {
            return Some((path.clone(), span));
        }
    }
    None
}

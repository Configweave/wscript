//! `wscript lsp` — the language server (PRD §9), built on tower-lsp over
//! stdio. The four v1 features, in priority order: diagnostics, hover,
//! go-to-definition, completions. That list is a ceiling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use wscript_compiler::ast;
use wscript_compiler::wscripti::WscriptiIndex;

use crate::manifest;

pub fn run(ctx: wscript::Context) -> std::process::ExitCode {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(|client| Backend {
            client,
            state: Mutex::new(State {
                base: ctx,
                registry: None,
                wscripti_indexes: Vec::new(),
                docs: HashMap::new(),
            }),
        });
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    std::process::ExitCode::SUCCESS
}

struct State {
    /// stdlib-only context (fallback when no wscript.toml is found).
    base: wscript::Context,
    /// Registry incl. wscript.toml interfaces (built at initialize).
    registry: Option<wscript::Registry>,
    wscripti_indexes: Vec<(PathBuf, WscriptiIndex)>,
    docs: HashMap<Url, String>,
}

impl State {
    fn registry(&self) -> wscript::Registry {
        self.registry
            .clone()
            .unwrap_or_else(|| self.base.registry().clone())
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

/// Collect (span, node id) for every expression, for position lookups.
fn expr_index(file: &ast::SourceFile) -> Vec<(wscript::Span, ast::NodeId)> {
    let mut out = Vec::new();
    fn walk_expr(e: &ast::Expr, out: &mut Vec<(wscript::Span, ast::NodeId)>) {
        out.push((e.span, e.id));
        use ast::ExprKind::*;
        match &e.kind {
            Unary { expr, .. } | Try(expr) => walk_expr(expr, out),
            Binary { lhs, rhs, .. } => {
                walk_expr(lhs, out);
                walk_expr(rhs, out);
            }
            Assign { target, value } => {
                walk_expr(target, out);
                walk_expr(value, out);
            }
            Call { callee, args } => {
                walk_expr(callee, out);
                args.iter().for_each(|a| walk_expr(a, out));
            }
            MethodCall { recv, args, .. } => {
                walk_expr(recv, out);
                args.iter().for_each(|a| walk_expr(a, out));
            }
            Field { obj, .. } => walk_expr(obj, out),
            Index { obj, idx } => {
                walk_expr(obj, out);
                walk_expr(idx, out);
            }
            StructLit { fields, .. } => fields.iter().for_each(|(_, v)| walk_expr(v, out)),
            ListLit(items) => items.iter().for_each(|i| walk_expr(i, out)),
            MapLit(entries) => entries.iter().for_each(|(k, v)| {
                walk_expr(k, out);
                walk_expr(v, out);
            }),
            If { cond, then, else_ } => {
                walk_expr(cond, out);
                walk_block(then, out);
                if let Some(e) = else_ {
                    walk_expr(e, out);
                }
            }
            IfLet {
                scrutinee,
                then,
                else_,
                ..
            } => {
                walk_expr(scrutinee, out);
                walk_block(then, out);
                if let Some(e) = else_ {
                    walk_expr(e, out);
                }
            }
            Match { scrutinee, arms } => {
                walk_expr(scrutinee, out);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        walk_expr(g, out);
                    }
                    walk_expr(&arm.body, out);
                }
            }
            While { cond, body } => {
                walk_expr(cond, out);
                walk_block(body, out);
            }
            Loop { body } => walk_block(body, out),
            For { iter, body, .. } => {
                walk_expr(iter, out);
                walk_block(body, out);
            }
            Range { lo, hi, .. } => {
                walk_expr(lo, out);
                walk_expr(hi, out);
            }
            Return(Some(v)) => walk_expr(v, out),
            Block(b) => walk_block(b, out),
            Closure { body, .. } => walk_expr(body, out),
            _ => {}
        }
    }
    fn walk_block(b: &ast::Block, out: &mut Vec<(wscript::Span, ast::NodeId)>) {
        for stmt in &b.stmts {
            match stmt {
                ast::Stmt::Let { init, .. } => walk_expr(init, out),
                ast::Stmt::LetElse {
                    init, else_block, ..
                } => {
                    walk_expr(init, out);
                    walk_block(else_block, out);
                }
                ast::Stmt::Expr { expr, .. } => walk_expr(expr, out),
            }
        }
    }
    for item in &file.items {
        match item {
            ast::Item::Fn(f) => walk_block(&f.body, &mut out),
            ast::Item::Impl(im) => im.fns.iter().for_each(|f| walk_block(&f.body, &mut out)),
            _ => {}
        }
    }
    out
}

/// Smallest expression containing `offset`. Children are walked after
/// their parents, so on span ties the reversed scan prefers the innermost
/// node (error-recovery wrappers share their child's span).
fn node_at(index: &[(wscript::Span, ast::NodeId)], offset: usize) -> Option<ast::NodeId> {
    index
        .iter()
        .rev()
        .filter(|(span, _)| span.lo as usize <= offset && offset < span.hi as usize)
        .min_by_key(|(span, _)| span.hi - span.lo)
        .map(|(_, id)| *id)
}

/// Expression ending exactly at `offset` (for `.` completions).
fn node_ending_at(index: &[(wscript::Span, ast::NodeId)], offset: usize) -> Option<ast::NodeId> {
    index
        .iter()
        .rev()
        .filter(|(span, _)| span.hi as usize == offset)
        .min_by_key(|(span, _)| span.hi - span.lo)
        .map(|(_, id)| *id)
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

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // Load wscript.toml interfaces from the workspace root (PRD §9.1).
        #[allow(deprecated)]
        let root = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok());
        if let Some(root) = root
            && let Some(m) = manifest::find(&root)
        {
            let mut state = self.state.lock().unwrap();
            // A manifest describes the complete host context (see
            // cmd_check): use exactly the declared interfaces rather
            // than overlaying them on the CLI stdlib, which would
            // shadow same-named embedder modules.
            let mut reg = wscript::Registry::new();
            let indexes = manifest::load_interfaces(&m, &mut reg);
            state.registry = Some(reg);
            state.wscripti_indexes = indexes;
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
        let (text, registry) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.registry())
        };
        let analysis = wscript_compiler::analyze(&text, &registry);
        let index = expr_index(&analysis.parse.file);
        let offset = position_to_offset(&text, pos);
        let Some(node) = node_at(&index, offset) else {
            return Ok(None);
        };
        let mut lines = Vec::new();
        if let Some(ty) = analysis.check.types.get(&node) {
            lines.push(format!(
                "```wscript\n{}\n```",
                ty.display(&analysis.check.defs)
            ));
        }
        // Host call info: signature + docs (PRD §9 feature 2).
        if let Some(wscript_compiler::check::CallKind::Host(idx)) = analysis.check.calls.get(&node)
            && let Some((module, name, sig, doc)) = host_fn_info(&registry, *idx)
        {
            lines.push(format!(
                "`{module}::{name}{}`",
                render_sig(&sig, &analysis.check.defs)
            ));
            if let Some(doc) = doc {
                lines.push(doc);
            }
        }
        if let Some(wscript_compiler::check::MethodRes::Host(idx)) =
            analysis.check.methods.get(&node)
            && let Some((ty_name, name, sig, doc)) = host_method_info(&registry, *idx)
        {
            lines.push(format!(
                "`{ty_name}.{name}{}`",
                render_sig(&sig, &analysis.check.defs)
            ));
            if let Some(doc) = doc {
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
        let (text, registry, wscripti) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.registry(), state.wscripti_indexes.clone())
        };
        let analysis = wscript_compiler::analyze(&text, &registry);
        let index = expr_index(&analysis.parse.file);
        let offset = position_to_offset(&text, pos);
        let Some(node) = node_at(&index, offset) else {
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
        let target = match (
            analysis.check.calls.get(&node),
            analysis.check.methods.get(&node),
        ) {
            (Some(wscript_compiler::check::CallKind::Host(idx)), _) => {
                host_fn_info(&registry, *idx).and_then(|(m, n, ..)| {
                    lookup_wscripti(&wscripti, |i| {
                        i.module_items.get(&(m.clone(), n.clone())).copied()
                    })
                })
            }
            (_, Some(wscript_compiler::check::MethodRes::Host(idx))) => {
                host_method_info(&registry, *idx).and_then(|(t, n, ..)| {
                    lookup_wscripti(&wscripti, |i| {
                        i.methods.get(&(t.clone(), n.clone())).copied()
                    })
                })
            }
            _ => None,
        };
        if let Some((path, span)) = target
            && let Ok(file_text) = std::fs::read_to_string(&path)
            && let Ok(file_uri) = Url::from_file_path(&path)
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
        let (text, registry) = {
            let state = self.state.lock().unwrap();
            let Some(text) = state.docs.get(&uri).cloned() else {
                return Ok(None);
            };
            (text, state.registry())
        };
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
            let analysis = wscript_compiler::analyze(&text, &registry);
            if let Some(module) = registry.modules.iter().find(|m| m.name == seg) {
                for (name, sig, _, doc) in &module.fns {
                    push(
                        &mut items,
                        name,
                        CompletionItemKind::FUNCTION,
                        Some(format!(
                            "{}{}",
                            render_sig(sig, &analysis.check.defs),
                            doc.as_deref()
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
            let analysis = wscript_compiler::analyze(&text, &registry);
            let index = expr_index(&analysis.parse.file);
            let recv = node_ending_at(&index, rest.trim_end().len());
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
                                Some(render_sig(&m.sig, &analysis.check.defs)),
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
            let analysis = wscript_compiler::analyze(&text, &registry);
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
                }
            }
        }
        Ok(Some(CompletionResponse::Array(items)))
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, text: String) {
        let registry = {
            let state = self.state.lock().unwrap();
            state.registry()
        };
        let analysis = wscript_compiler::analyze(&text, &registry);
        let mut all = analysis.parse.diags;
        all.extend(analysis.check.diags);
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
                message: match d
                    .help
                    .clone()
                    .or_else(|| wscript::diag_default_help(d.code).map(String::from))
                {
                    Some(help) => format!("{}\nhelp: {help}", d.message),
                    None => d.message.clone(),
                },
                ..Diagnostic::default()
            })
            .collect();
        self.client.publish_diagnostics(uri, diags, None).await;
    }
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
    let params: Vec<String> = sig.params.iter().map(|p| p.display(defs)).collect();
    if sig.ret == wscript::Type::Unit {
        format!("({})", params.join(", "))
    } else {
        format!("({}) -> {}", params.join(", "), sig.ret.display(defs))
    }
}

type HostInfo = (String, String, wscript::FnSig, Option<String>);

fn host_fn_info(reg: &wscript::Registry, idx: u32) -> Option<HostInfo> {
    for module in &reg.modules {
        for (name, sig, i, doc) in &module.fns {
            if *i == idx {
                return Some((module.name.clone(), name.clone(), sig.clone(), doc.clone()));
            }
        }
    }
    None
}

fn host_method_info(reg: &wscript::Registry, idx: u32) -> Option<HostInfo> {
    for (def, methods) in &reg.methods {
        for m in methods {
            if m.host_idx == idx {
                return Some((
                    reg.defs.name_of(*def).to_string(),
                    m.name.clone(),
                    m.sig.clone(),
                    m.doc.clone(),
                ));
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

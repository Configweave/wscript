//! `wscript lsp` — the language server (PRD §9), built on tower-lsp-server
//! over stdio. The four v1 features, in priority order: diagnostics, hover,
//! go-to-definition, completions. That list is a ceiling.
//!
//! Everything the editor knows about a position comes from the compiler's
//! `check::Index` (issue #17). This module translates: source offsets to
//! LSP positions, and the compiler's answers to LSP payloads. It derives
//! nothing about the language itself — when it did, its copies of the
//! builtin method tables went stale and fourteen methods stopped
//! completing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp_server::jsonrpc::Result as LspResult;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use wscript::Session;
use wscript::core::registry::{HostKind, HostRef};
use wscript_compiler::check::{CompletionKind, render_sig};
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

fn position_to_offset(text: &str, pos: Position) -> u32 {
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, c) in text.char_indices() {
        if line == pos.line && character >= pos.character {
            return i as u32;
        }
        if line > pos.line {
            return i as u32;
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += c.len_utf16() as u32;
        }
    }
    text.len() as u32
}

fn span_to_range(text: &str, span: wscript::Span) -> Range {
    Range {
        start: offset_to_position(text, span.lo as usize),
        end: offset_to_position(text, span.hi as usize),
    }
}

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
        let analysis = session.analyze(&entry_path(&uri), &text);
        let editor = analysis.editor(session.registry());
        let defs = editor.defs();
        let Some(symbol) = editor.symbol_at(position_to_offset(&text, pos)) else {
            return Ok(None);
        };
        let mut lines = Vec::new();
        if let Some(ty) = &symbol.ty {
            lines.push(format!("```wscript\n{}\n```", ty.display(defs)));
            // For a unit value, the family's table is the thing you want
            // to see: which suffixes exist and what they convert to.
            if let wscript::Type::Named(id) = ty
                && let Some(u) = defs.as_unit(*id)
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
        if let Some(host) = symbol.host {
            lines.push(format!(
                "`{}{}`",
                host.qualified_name(),
                render_sig(&host.decl.sig, host.decl.param_names(), defs)
            ));
            if let Some(doc) = &host.decl.doc {
                lines.push(doc.clone());
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
        let analysis = session.analyze(&entry_path(&uri), &text);
        let editor = analysis.editor(session.registry());
        let Some(symbol) = editor.symbol_at(position_to_offset(&text, pos)) else {
            return Ok(None);
        };
        // Script-local symbols.
        if let Some(span) = symbol.def_span {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range: span_to_range(&text, span),
            })));
        }
        // Host symbols jump to the .wscripti entry (PRD §9 feature 3).
        let target = symbol
            .host
            .and_then(|host| lookup_wscripti(&wscripti, |i| wscripti_span(&host, i)));
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
        let analysis = session.analyze(&entry_path(&uri), &text);
        let editor = analysis.editor(session.registry());
        let items: Vec<CompletionItem> = editor
            .completions_at(position_to_offset(&text, pos))
            .into_iter()
            .map(|c| CompletionItem {
                label: c.label,
                kind: Some(completion_item_kind(c.kind)),
                detail: c.detail,
                ..CompletionItem::default()
            })
            .collect();
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

/// wscript's own name for a completion, in the protocol's vocabulary.
fn completion_item_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Constant => CompletionItemKind::CONSTANT,
        CompletionKind::Method => CompletionItemKind::METHOD,
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::Module => CompletionItemKind::MODULE,
        CompletionKind::Struct => CompletionItemKind::STRUCT,
        CompletionKind::Enum => CompletionItemKind::ENUM,
        CompletionKind::EnumMember => CompletionItemKind::ENUM_MEMBER,
        CompletionKind::Trait => CompletionItemKind::INTERFACE,
        CompletionKind::UnitFamily | CompletionKind::UnitMember => CompletionItemKind::UNIT,
    }
}

/// Where the `.wscripti` interface declares a host registration. The
/// interface keeps two indexes because a registration is addressed two
/// ways, and [`HostKind`] is the same distinction — one match, so they
/// cannot disagree about which is which.
fn wscripti_span(host: &HostRef, index: &WscriptiIndex) -> Option<wscript::Span> {
    let declarations = match host.kind {
        HostKind::Fn => &index.module_items,
        HostKind::Method => &index.methods,
    };
    declarations
        .get(&(host.owner.to_string(), host.decl.name.clone()))
        .copied()
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

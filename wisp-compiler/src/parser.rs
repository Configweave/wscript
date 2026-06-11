//! Hand-written recursive-descent parser with error recovery (PRD §5.1).
//!
//! On broken input the parser reports diagnostics and produces a partial
//! AST (with `Error` nodes) rather than bailing — the LSP depends on this.
//!
//! Statement termination: a `Newline` token ends a statement. Continuation
//! happens when the construct cannot end yet — inside `(`/`[` the lexer
//! suppresses newlines entirely; after a binary operator or `,` the parser
//! simply skips newlines before the operand; and two lookahead cases let a
//! *following* line continue the previous one: a line starting with `.`
//! (method chains) and an `else` after `}`.

use wisp_core::diag::Diagnostic;
use wisp_core::span::Span;

use crate::ast::*;
use crate::lexer;
use crate::token::{Token, TokenKind};

pub struct ParseOutput {
    pub file: SourceFile,
    pub diags: Vec<Diagnostic>,
}

pub fn parse(src: &str) -> ParseOutput {
    let lexed = lexer::lex(src);
    let mut parser = Parser {
        tokens: lexed.tokens,
        pos: 0,
        diags: lexed.diags,
        next_id: 0,
        no_struct_lit: false,
    };
    let file = parser.source_file();
    ParseOutput {
        file,
        diags: parser.diags,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    next_id: NodeId,
    /// Set while parsing `if`/`while`/`for`/`match` headers, where `{`
    /// starts the body rather than a struct literal.
    no_struct_lit: bool,
}

impl Parser {
    // ------------------------------------------------------------ cursor

    fn id(&mut self) -> NodeId {
        self.next_id += 1;
        self.next_id
    }

    fn tok(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn kind(&self) -> &TokenKind {
        &self.tok().kind
    }

    fn span(&self) -> Span {
        self.tok().span
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1).min(self.tokens.len() - 1)].span
    }

    fn nth_kind(&self, n: usize) -> &TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn bump(&mut self) -> Token {
        let t = self.tok().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.kind() == kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), TokenKind::Eof)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.kind(), TokenKind::Newline | TokenKind::DocComment(_)) {
            self.bump();
        }
    }

    /// Skip newlines/semicolons, collecting `///` doc comments for the
    /// next declaration.
    fn collect_docs(&mut self) -> Option<String> {
        let mut docs: Vec<String> = Vec::new();
        loop {
            match self.kind() {
                TokenKind::Newline | TokenKind::Semi => {
                    self.bump();
                }
                TokenKind::DocComment(text) => {
                    docs.push(text.clone());
                    self.bump();
                }
                _ => break,
            }
        }
        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    /// Peek at the next token kind, looking through newlines.
    fn peek_through_newlines(&self) -> &TokenKind {
        let mut n = 0;
        while matches!(self.nth_kind(n), TokenKind::Newline | TokenKind::DocComment(_)) {
            n += 1;
        }
        self.nth_kind(n)
    }

    fn error(&mut self, code: &'static str, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(code, span, msg));
    }

    fn error_help(
        &mut self,
        code: &'static str,
        span: Span,
        msg: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diags
            .push(Diagnostic::error(code, span, msg).with_help(help));
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> Option<Span> {
        if self.at(kind) {
            Some(self.bump().span)
        } else {
            let found = self.kind().describe();
            let span = self.span();
            self.error(
                "E0100",
                span,
                format!("expected {what}, found {found}"),
            );
            None
        }
    }

    fn expect_ident(&mut self, what: &str) -> Option<Ident> {
        match self.kind() {
            TokenKind::Ident(name) => {
                let name = name.clone();
                let span = self.bump().span;
                Some(Ident { name, span })
            }
            _ => {
                let found = self.kind().describe();
                let span = self.span();
                self.error("E0100", span, format!("expected {what}, found {found}"));
                None
            }
        }
    }

    // ------------------------------------------------------------- items

    fn source_file(&mut self) -> SourceFile {
        let mut items = Vec::new();
        loop {
            let doc = self.collect_docs();
            if self.at_eof() {
                break;
            }
            match self.item(doc, true) {
                Some(item) => items.push(item),
                None => self.sync_to_item(),
            }
        }
        SourceFile { items }
    }

    /// Skip tokens until something that can plausibly start an item.
    fn sync_to_item(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.kind() {
                TokenKind::Eof => break,
                TokenKind::LBrace | TokenKind::HashBrace => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::RBrace => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::KwFn
                | TokenKind::KwStruct
                | TokenKind::KwEnum
                | TokenKind::KwTrait
                | TokenKind::KwImpl
                | TokenKind::KwUse
                | TokenKind::Hash
                    if depth == 0 =>
                {
                    break;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn item(&mut self, doc: Option<String>, allow_mod: bool) -> Option<Item> {
        let (derives, opaque) = self.attributes();
        match self.kind() {
            TokenKind::KwMod if allow_mod => self.mod_decl(doc).map(Item::Mod),
            TokenKind::KwConst => self.const_decl(doc).map(Item::Const),
            TokenKind::KwUse => {
                if !derives.is_empty() {
                    let span = self.span();
                    self.error("E0101", span, "attributes are not allowed on `use`");
                }
                self.use_decl().map(Item::Use)
            }
            TokenKind::KwFn => {
                if !derives.is_empty() {
                    let span = self.span();
                    self.error_help(
                        "E0101",
                        span,
                        "`#[derive(...)]` is not allowed on functions",
                        "derives apply to `struct` and `enum` declarations",
                    );
                }
                self.fn_decl(false, doc).map(Item::Fn)
            }
            TokenKind::KwStruct => self.struct_decl(derives, opaque, doc).map(Item::Struct),
            TokenKind::KwEnum => self.enum_decl(derives, doc).map(Item::Enum),
            TokenKind::KwTrait => self.trait_decl().map(Item::Trait),
            TokenKind::KwImpl => self.impl_decl().map(Item::Impl),
            _ => {
                let found = self.kind().describe();
                let span = self.span();
                self.error_help(
                    "E0102",
                    span,
                    format!("expected an item, found {found}"),
                    "top-level code lives in functions; script execution starts at `fn main()`",
                );
                None
            }
        }
    }

    /// `#[derive(A, B)]` (scripts) and `#[opaque]` (interface files).
    fn attributes(&mut self) -> (Vec<Ident>, bool) {
        let mut derives = Vec::new();
        let mut opaque = false;
        while self.at(&TokenKind::Hash) {
            let hash_span = self.bump().span;
            if self.expect(&TokenKind::LBracket, "`[` after `#`").is_none() {
                break;
            }
            let name = match self.expect_ident("attribute name") {
                Some(n) => n,
                None => break,
            };
            if name.name == "opaque" {
                opaque = true;
                self.expect(&TokenKind::RBracket, "`]` to close the attribute");
                self.skip_newlines();
                continue;
            }
            if name.name != "derive" {
                self.error_help(
                    "E0103",
                    hash_span.to(name.span),
                    format!("unknown attribute `{}`", name.name),
                    "the only attributes supported are `#[derive(...)]` and `#[opaque]` \
                     (interface files)",
                );
            }
            if self.eat(&TokenKind::LParen) {
                loop {
                    if self.eat(&TokenKind::RParen) {
                        break;
                    }
                    match self.expect_ident("trait name in derive list") {
                        Some(t) => derives.push(t),
                        None => break,
                    }
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(&TokenKind::RParen, "`)` to close the derive list");
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RBracket, "`]` to close the attribute");
            self.skip_newlines();
        }
        (derives, opaque)
    }

    fn use_decl(&mut self) -> Option<UseDecl> {
        let kw = self.bump().span;
        let module = self.expect_ident("module name after `use`")?;
        let mut item = None;
        if self.eat(&TokenKind::ColonColon) {
            item = Some(self.expect_ident("item name after `::`")?);
        }
        let span = kw.to(self.prev_span());
        self.terminate_stmt();
        Some(UseDecl { module, item, span })
    }

    /// `allow_self`: parsing inside an impl/trait block.
    fn fn_decl(&mut self, allow_self: bool, doc: Option<String>) -> Option<FnDecl> {
        let kw = self.bump().span; // `fn`
        let name = self.expect_ident("function name after `fn`")?;
        self.expect(&TokenKind::LParen, "`(` to start the parameter list")?;
        let params = self.params(allow_self);
        let mut ret = None;
        if self.eat(&TokenKind::Arrow) {
            ret = Some(self.type_expr());
        }
        let sig_span = kw.to(self.prev_span());
        // Bodyless declarations are the `.wispi` interface form (PRD §9.1);
        // the checker rejects them in scripts.
        if self.peek_through_newlines() != &TokenKind::LBrace {
            let id = self.id();
            return Some(FnDecl {
                name,
                params,
                ret,
                body: Block {
                    stmts: vec![],
                    span: sig_span,
                    id,
                },
                has_body: false,
                doc,
                span: sig_span,
                sig_span,
            });
        }
        self.skip_newlines();
        let body = self.block()?;
        let span = kw.to(body.span);
        Some(FnDecl {
            name,
            params,
            ret,
            body,
            has_body: true,
            doc,
            span,
            sig_span,
        })
    }

    fn mod_decl(&mut self, doc: Option<String>) -> Option<ModDecl> {
        let kw = self.bump().span; // `mod`
        let name = self.expect_ident("module name after `mod`")?;
        self.skip_newlines();
        self.expect(&TokenKind::LBrace, "`{` to start the module block")?;
        let mut items = Vec::new();
        loop {
            let doc = self.collect_docs();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                self.error("E0100", kw, "unclosed module block: missing `}`");
                break;
            }
            match self.item(doc, false) {
                Some(item) => items.push(item),
                None => self.sync_to_item(),
            }
        }
        let span = kw.to(self.prev_span());
        Some(ModDecl {
            name,
            items,
            doc,
            span,
        })
    }

    fn const_decl(&mut self, doc: Option<String>) -> Option<ConstDecl> {
        let kw = self.bump().span; // `const`
        let name = self.expect_ident("constant name after `const`")?;
        self.expect(&TokenKind::Colon, "`:` after the constant name")?;
        let ty = self.type_expr();
        let span = kw.to(ty.span);
        self.terminate_stmt();
        Some(ConstDecl {
            name,
            ty,
            doc,
            span,
        })
    }

    fn params(&mut self, allow_self: bool) -> Vec<Param> {
        let mut params = Vec::new();
        let mut first = true;
        loop {
            if self.eat(&TokenKind::RParen) {
                break;
            }
            if !first && !self.eat(&TokenKind::Comma) {
                self.expect(&TokenKind::RParen, "`,` or `)` in parameter list");
                self.recover_to(&[TokenKind::RParen, TokenKind::LBrace, TokenKind::Newline]);
                self.eat(&TokenKind::RParen);
                break;
            }
            if self.eat(&TokenKind::RParen) {
                break; // trailing comma
            }
            if self.at(&TokenKind::KwSelf) {
                let span = self.bump().span;
                if !allow_self {
                    self.error_help(
                        "E0104",
                        span,
                        "`self` parameter outside an `impl` or `trait` block",
                        "`self` is only valid as the first parameter of a method",
                    );
                } else if !first {
                    self.error("E0104", span, "`self` must be the first parameter");
                }
                params.push(Param {
                    name: Ident {
                        name: "self".into(),
                        span,
                    },
                    ty: None,
                    is_self: true,
                    span,
                });
                first = false;
                continue;
            }
            let name = match self.expect_ident("parameter name") {
                Some(n) => n,
                None => {
                    self.recover_to(&[
                        TokenKind::Comma,
                        TokenKind::RParen,
                        TokenKind::LBrace,
                        TokenKind::Newline,
                    ]);
                    first = false;
                    continue;
                }
            };
            let ty = if self.eat(&TokenKind::Colon) {
                Some(self.type_expr())
            } else {
                let span = name.span;
                self.error_help(
                    "E0105",
                    span,
                    format!("parameter `{}` is missing a type annotation", name.name),
                    "annotations are required on function parameters: `name: type` (PRD §3.3)",
                );
                None
            };
            let span = name.span.to(self.prev_span());
            params.push(Param {
                name,
                ty,
                is_self: false,
                span,
            });
            first = false;
        }
        params
    }

    fn struct_decl(
        &mut self,
        derives: Vec<Ident>,
        opaque: bool,
        doc: Option<String>,
    ) -> Option<StructDecl> {
        let kw = self.bump().span;
        let name = self.expect_ident("struct name")?;
        self.expect(&TokenKind::LBrace, "`{` to start the field list")?;
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed struct declaration");
                break;
            }
            let fname = match self.expect_ident("field name") {
                Some(n) => n,
                None => {
                    self.recover_to(&[TokenKind::Comma, TokenKind::Newline, TokenKind::RBrace]);
                    continue;
                }
            };
            self.expect(&TokenKind::Colon, "`:` after field name");
            let ty = self.type_expr();
            let span = fname.span.to(ty.span);
            fields.push(FieldDecl {
                name: fname,
                ty,
                span,
            });
            self.skip_newlines();
            if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                self.expect(&TokenKind::RBrace, "`,` or `}` after field");
                self.recover_to(&[TokenKind::Comma, TokenKind::Newline, TokenKind::RBrace]);
                self.eat(&TokenKind::Comma);
            }
        }
        let span = kw.to(self.prev_span());
        Some(StructDecl {
            name,
            fields,
            derives,
            opaque,
            doc,
            span,
        })
    }

    fn enum_decl(&mut self, derives: Vec<Ident>, doc: Option<String>) -> Option<EnumDecl> {
        let kw = self.bump().span;
        let name = self.expect_ident("enum name")?;
        self.expect(&TokenKind::LBrace, "`{` to start the variant list")?;
        let mut variants = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed enum declaration");
                break;
            }
            let vname = match self.expect_ident("variant name") {
                Some(n) => n,
                None => {
                    self.recover_to(&[TokenKind::Comma, TokenKind::Newline, TokenKind::RBrace]);
                    continue;
                }
            };
            let body = if self.eat(&TokenKind::LParen) {
                // tuple variant
                let mut tys = Vec::new();
                loop {
                    if self.eat(&TokenKind::RParen) {
                        break;
                    }
                    tys.push(self.type_expr());
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(&TokenKind::RParen, "`,` or `)` in variant payload");
                        break;
                    }
                }
                VariantBody::Tuple(tys)
            } else if self.eat(&TokenKind::LBrace) {
                // struct variant
                let mut fields = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    let fname = match self.expect_ident("field name") {
                        Some(n) => n,
                        None => {
                            self.recover_to(&[
                                TokenKind::Comma,
                                TokenKind::Newline,
                                TokenKind::RBrace,
                            ]);
                            continue;
                        }
                    };
                    self.expect(&TokenKind::Colon, "`:` after field name");
                    let ty = self.type_expr();
                    let span = fname.span.to(ty.span);
                    fields.push(FieldDecl {
                        name: fname,
                        ty,
                        span,
                    });
                    self.skip_newlines();
                    if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                        self.expect(&TokenKind::RBrace, "`,` or `}` after field");
                        break;
                    }
                }
                VariantBody::Struct(fields)
            } else {
                VariantBody::Unit
            };
            let span = vname.span.to(self.prev_span());
            variants.push(VariantDecl {
                name: vname,
                body,
                span,
            });
            self.skip_newlines();
            if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                self.expect(&TokenKind::RBrace, "`,` or `}` after variant");
                self.recover_to(&[TokenKind::Comma, TokenKind::Newline, TokenKind::RBrace]);
                self.eat(&TokenKind::Comma);
            }
        }
        let span = kw.to(self.prev_span());
        Some(EnumDecl {
            name,
            variants,
            derives,
            doc,
            span,
        })
    }

    fn trait_decl(&mut self) -> Option<TraitDecl> {
        let kw = self.bump().span;
        let name = self.expect_ident("trait name")?;
        self.expect(&TokenKind::LBrace, "`{` to start the trait body")?;
        let mut methods = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed trait declaration");
                break;
            }
            if !self.at(&TokenKind::KwFn) {
                let found = self.kind().describe();
                let span = self.span();
                self.error(
                    "E0100",
                    span,
                    format!("expected `fn` method signature in trait body, found {found}"),
                );
                self.recover_to(&[TokenKind::KwFn, TokenKind::RBrace]);
                continue;
            }
            let kw_fn = self.bump().span;
            let mname = match self.expect_ident("method name") {
                Some(n) => n,
                None => continue,
            };
            self.expect(&TokenKind::LParen, "`(` to start the parameter list");
            let mut params = self.params(true);
            if params.first().is_none_or(|p| !p.is_self) {
                self.error_help(
                    "E0106",
                    mname.span,
                    "trait methods must take `self` as their first parameter",
                    "write `fn name(self, ...)`",
                );
            } else {
                params.remove(0);
            }
            let ret = if self.eat(&TokenKind::Arrow) {
                Some(self.type_expr())
            } else {
                None
            };
            let span = kw_fn.to(self.prev_span());
            if self.peek_through_newlines() == &TokenKind::LBrace {
                self.skip_newlines();
                let body_span = self.span();
                self.error_help(
                    "E0107",
                    body_span,
                    "default method bodies are not supported in v1",
                    "declare the signature only; implement it in `impl Trait for Type` blocks",
                );
                // Skip the body for recovery.
                let _ = self.block();
            }
            methods.push(TraitMethodDecl {
                name: mname,
                params,
                ret,
                span,
            });
        }
        let span = kw.to(self.prev_span());
        Some(TraitDecl {
            name,
            methods,
            span,
        })
    }

    fn impl_decl(&mut self) -> Option<ImplDecl> {
        let kw = self.bump().span;
        let first = self.expect_ident("type or trait name after `impl`")?;
        let (trait_name, ty_name) = if self.eat(&TokenKind::KwFor) {
            let ty = self.expect_ident("type name after `for`")?;
            (Some(first), ty)
        } else {
            (None, first)
        };
        self.skip_newlines();
        self.expect(&TokenKind::LBrace, "`{` to start the impl body")?;
        let mut fns = Vec::new();
        loop {
            let doc = self.collect_docs();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed impl block");
                break;
            }
            if !self.at(&TokenKind::KwFn) {
                let found = self.kind().describe();
                let span = self.span();
                self.error(
                    "E0100",
                    span,
                    format!("expected `fn` in impl body, found {found}"),
                );
                self.recover_to(&[TokenKind::KwFn, TokenKind::RBrace]);
                continue;
            }
            if let Some(f) = self.fn_decl(true, doc) {
                fns.push(f);
            } else {
                self.recover_to(&[TokenKind::KwFn, TokenKind::RBrace]);
            }
        }
        let span = kw.to(self.prev_span());
        Some(ImplDecl {
            trait_name,
            ty_name,
            fns,
            span,
        })
    }

    /// Skip tokens until one of `stops` (or EOF). Does not consume the stop.
    fn recover_to(&mut self, stops: &[TokenKind]) {
        while !self.at_eof() && !stops.iter().any(|s| self.at(s)) {
            self.bump();
        }
    }

    // ------------------------------------------------------------- types

    fn type_expr(&mut self) -> TypeExpr {
        let start = self.span();
        match self.kind().clone() {
            TokenKind::LParen => {
                self.bump();
                if self.eat(&TokenKind::RParen) {
                    return TypeExpr {
                        kind: TypeExprKind::Unit,
                        span: start.to(self.prev_span()),
                    };
                }
                let inner = self.type_expr();
                self.expect(&TokenKind::RParen, "`)` to close the type");
                inner
            }
            TokenKind::KwFn => {
                self.bump();
                self.expect(&TokenKind::LParen, "`(` in function type");
                let mut params = Vec::new();
                loop {
                    if self.eat(&TokenKind::RParen) {
                        break;
                    }
                    params.push(self.type_expr());
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(&TokenKind::RParen, "`,` or `)` in function type");
                        break;
                    }
                }
                let ret = if self.eat(&TokenKind::Arrow) {
                    Some(Box::new(self.type_expr()))
                } else {
                    None
                };
                TypeExpr {
                    kind: TypeExprKind::Fn(params, ret),
                    span: start.to(self.prev_span()),
                }
            }
            TokenKind::KwDyn => {
                self.bump();
                match self.expect_ident("trait name after `dyn`") {
                    Some(name) => TypeExpr {
                        kind: TypeExprKind::Dyn(name),
                        span: start.to(self.prev_span()),
                    },
                    None => TypeExpr {
                        kind: TypeExprKind::Error,
                        span: start,
                    },
                }
            }
            TokenKind::Ident(name) => {
                let ident = Ident {
                    name,
                    span: self.bump().span,
                };
                if self.eat(&TokenKind::LBracket) {
                    let mut args = Vec::new();
                    loop {
                        if self.eat(&TokenKind::RBracket) {
                            break;
                        }
                        args.push(self.type_expr());
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(&TokenKind::RBracket, "`,` or `]` in type arguments");
                            break;
                        }
                    }
                    TypeExpr {
                        kind: TypeExprKind::App(ident, args),
                        span: start.to(self.prev_span()),
                    }
                } else {
                    TypeExpr {
                        span: ident.span,
                        kind: TypeExprKind::Name(ident),
                    }
                }
            }
            other => {
                let span = self.span();
                self.error(
                    "E0108",
                    span,
                    format!("expected a type, found {}", other.describe()),
                );
                TypeExpr {
                    kind: TypeExprKind::Error,
                    span,
                }
            }
        }
    }

    // -------------------------------------------------------- statements

    fn block(&mut self) -> Option<Block> {
        let open = self.expect(&TokenKind::LBrace, "`{` to start a block")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            while self.eat(&TokenKind::Semi) {
                self.skip_newlines();
            }
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                self.error("E0100", open, "unclosed block: missing `}`");
                break;
            }
            let stmt = self.stmt();
            stmts.push(stmt);
        }
        Some(Block {
            stmts,
            span: open.to(self.prev_span()),
            id: self.id(),
        })
    }

    fn stmt(&mut self) -> Stmt {
        if self.at(&TokenKind::KwLet) {
            return self.let_stmt();
        }
        let expr = self.expr();
        if matches!(expr.kind, ExprKind::Error) {
            // Recovery: resync to a statement boundary.
            self.recover_to(&[TokenKind::Newline, TokenKind::Semi, TokenKind::RBrace]);
        }
        let terminated = self.terminate_stmt();
        Stmt::Expr { expr, terminated }
    }

    /// Consume a statement terminator; report if the statement is followed
    /// by something else on the same line. Returns true when an explicit
    /// `;` was used (which discards the value even in tail position).
    fn terminate_stmt(&mut self) -> bool {
        match self.kind() {
            TokenKind::Semi => {
                self.bump();
                true
            }
            TokenKind::Newline => {
                self.bump();
                false
            }
            TokenKind::RBrace | TokenKind::Eof => false,
            other => {
                let found = other.describe();
                let span = self.span();
                self.error_help(
                    "E0109",
                    span,
                    format!("expected end of statement, found {found}"),
                    "statements end at a newline; use `;` to put several on one line",
                );
                self.recover_to(&[TokenKind::Newline, TokenKind::Semi, TokenKind::RBrace]);
                self.eat(&TokenKind::Newline);
                false
            }
        }
    }

    fn let_stmt(&mut self) -> Stmt {
        let kw = self.bump().span; // `let`
        // Simple binding: `let name [: ty] = init`. Anything else is a
        // pattern and requires `else` (let-else, PRD §3.4).
        let simple = matches!(self.kind(), TokenKind::Ident(_))
            && matches!(
                self.nth_kind(1),
                TokenKind::Colon | TokenKind::Eq
            );
        if simple {
            let name = self.expect_ident("binding name").unwrap();
            let ty = if self.eat(&TokenKind::Colon) {
                Some(self.type_expr())
            } else {
                None
            };
            self.expect(&TokenKind::Eq, "`=` in `let`");
            let init = self.expr();
            // `let x = e else { ... }` with a plain binding is suspicious
            // but grammatical — the checker rejects irrefutable let-else.
            let span = kw.to(init.span);
            let id = self.id();
            self.terminate_stmt();
            return Stmt::Let {
                name,
                ty,
                init,
                span,
                id,
            };
        }
        let pat = self.pattern();
        self.expect(&TokenKind::Eq, "`=` in `let`");
        let init = self.expr_no_struct_lit();
        self.skip_newlines();
        if !self.at(&TokenKind::KwElse) {
            let span = kw.to(init.span);
            self.error_help(
                "E0110",
                span,
                "destructuring `let` requires an `else` block in v1",
                "write `let pat = expr else { ... }`; the else block must diverge \
                 (return, break, or continue)",
            );
            let id = self.id();
            return Stmt::LetElse {
                pat,
                init,
                else_block: Block {
                    stmts: vec![],
                    span,
                    id: self.id(),
                },
                span,
                id,
            };
        }
        self.bump(); // `else`
        self.skip_newlines();
        let else_block = self.block().unwrap_or_else(|| Block {
            stmts: vec![],
            span: self.span(),
            id: self.id(),
        });
        let span = kw.to(else_block.span);
        let id = self.id();
        self.terminate_stmt();
        Stmt::LetElse {
            pat,
            init,
            else_block,
            span,
            id,
        }
    }

    // -------------------------------------------------------- expressions

    fn expr(&mut self) -> Expr {
        self.assign_expr()
    }

    fn expr_no_struct_lit(&mut self) -> Expr {
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let e = self.assign_expr();
        self.no_struct_lit = saved;
        e
    }

    fn mk(&mut self, kind: ExprKind, span: Span) -> Expr {
        Expr {
            kind,
            span,
            id: self.id(),
        }
    }

    fn assign_expr(&mut self) -> Expr {
        let lhs = self.range_expr();
        if self.at(&TokenKind::Eq) {
            self.bump();
            self.skip_newlines();
            let value = self.assign_expr();
            let span = lhs.span.to(value.span);
            return self.mk(
                ExprKind::Assign {
                    target: Box::new(lhs),
                    value: Box::new(value),
                },
                span,
            );
        }
        lhs
    }

    fn range_expr(&mut self) -> Expr {
        let lo = self.or_expr();
        let inclusive = match self.kind() {
            TokenKind::DotDot => false,
            TokenKind::DotDotEq => true,
            _ => return lo,
        };
        self.bump();
        self.skip_newlines();
        let hi = self.or_expr();
        let span = lo.span.to(hi.span);
        self.mk(
            ExprKind::Range {
                lo: Box::new(lo),
                hi: Box::new(hi),
                inclusive,
            },
            span,
        )
    }

    fn or_expr(&mut self) -> Expr {
        let mut lhs = self.and_expr();
        while self.at(&TokenKind::OrOr) {
            self.bump();
            self.skip_newlines();
            let rhs = self.and_expr();
            let span = lhs.span.to(rhs.span);
            lhs = self.mk(
                ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        lhs
    }

    fn and_expr(&mut self) -> Expr {
        let mut lhs = self.cmp_expr();
        while self.at(&TokenKind::AndAnd) {
            self.bump();
            self.skip_newlines();
            let rhs = self.cmp_expr();
            let span = lhs.span.to(rhs.span);
            lhs = self.mk(
                ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        lhs
    }

    fn cmp_expr(&mut self) -> Expr {
        let mut lhs = self.add_expr();
        loop {
            let op = match self.kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => return lhs,
            };
            self.bump();
            self.skip_newlines();
            let rhs = self.add_expr();
            let span = lhs.span.to(rhs.span);
            lhs = self.mk(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
    }

    fn add_expr(&mut self) -> Expr {
        let mut lhs = self.mul_expr();
        loop {
            let op = match self.kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => return lhs,
            };
            self.bump();
            self.skip_newlines();
            let rhs = self.mul_expr();
            let span = lhs.span.to(rhs.span);
            lhs = self.mk(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
    }

    fn mul_expr(&mut self) -> Expr {
        let mut lhs = self.unary_expr();
        loop {
            let op = match self.kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => return lhs,
            };
            self.bump();
            self.skip_newlines();
            let rhs = self.unary_expr();
            let span = lhs.span.to(rhs.span);
            lhs = self.mk(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
    }

    fn unary_expr(&mut self) -> Expr {
        let op = match self.kind() {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            let start = self.bump().span;
            let expr = self.unary_expr();
            let span = start.to(expr.span);
            return self.mk(
                ExprKind::Unary {
                    op,
                    expr: Box::new(expr),
                },
                span,
            );
        }
        self.postfix_expr()
    }

    fn postfix_expr(&mut self) -> Expr {
        let mut expr = self.primary_expr();
        loop {
            match self.kind() {
                TokenKind::LParen => {
                    self.bump();
                    let args = self.call_args();
                    let span = expr.span.to(self.prev_span());
                    expr = self.mk(
                        ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                        span,
                    );
                }
                TokenKind::Dot => {
                    self.bump();
                    let name = match self.expect_ident("method or field name after `.`") {
                        Some(n) => n,
                        None => {
                            // Keep the receiver in the tree — the LSP needs
                            // its type for `.` completions mid-typing.
                            let span = expr.span;
                            return self.mk(
                                ExprKind::Field {
                                    obj: Box::new(expr),
                                    name: Ident {
                                        name: String::new(),
                                        span,
                                    },
                                },
                                span,
                            );
                        }
                    };
                    if self.eat(&TokenKind::LParen) {
                        let args = self.call_args();
                        let span = expr.span.to(self.prev_span());
                        expr = self.mk(
                            ExprKind::MethodCall {
                                recv: Box::new(expr),
                                name,
                                args,
                            },
                            span,
                        );
                    } else {
                        let span = expr.span.to(name.span);
                        expr = self.mk(
                            ExprKind::Field {
                                obj: Box::new(expr),
                                name,
                            },
                            span,
                        );
                    }
                }
                TokenKind::LBracket => {
                    self.bump();
                    let idx = self.expr();
                    self.expect(&TokenKind::RBracket, "`]` to close the index");
                    let span = expr.span.to(self.prev_span());
                    expr = self.mk(
                        ExprKind::Index {
                            obj: Box::new(expr),
                            idx: Box::new(idx),
                        },
                        span,
                    );
                }
                TokenKind::Question => {
                    let q = self.bump().span;
                    let span = expr.span.to(q);
                    expr = self.mk(ExprKind::Try(Box::new(expr)), span);
                }
                // Leading-dot continuation: `expr\n    .method()` — the
                // next line cannot start a statement with `.`, so it is
                // unambiguously a continuation (documented in the grammar).
                TokenKind::Newline => {
                    if self.peek_through_newlines() == &TokenKind::Dot {
                        self.skip_newlines();
                        continue;
                    }
                    break;
                }
                _ => break,
            }
        }
        expr
    }

    fn call_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        loop {
            if self.eat(&TokenKind::RParen) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed call: missing `)`");
                break;
            }
            // Struct literals are fine inside call parens.
            let saved = self.no_struct_lit;
            self.no_struct_lit = false;
            args.push(self.expr());
            self.no_struct_lit = saved;
            if !self.eat(&TokenKind::Comma) {
                self.expect(&TokenKind::RParen, "`,` or `)` in arguments");
                break;
            }
        }
        args
    }

    fn primary_expr(&mut self) -> Expr {
        let start = self.span();
        match self.kind().clone() {
            TokenKind::Int(n) => {
                self.bump();
                self.mk(ExprKind::IntLit(n), start)
            }
            TokenKind::Float(f) => {
                self.bump();
                self.mk(ExprKind::FloatLit(f), start)
            }
            TokenKind::Str(s) => {
                self.bump();
                self.mk(ExprKind::StrLit(s), start)
            }
            TokenKind::Char(c) => {
                self.bump();
                self.mk(ExprKind::CharLit(c), start)
            }
            TokenKind::KwTrue => {
                self.bump();
                self.mk(ExprKind::BoolLit(true), start)
            }
            TokenKind::KwFalse => {
                self.bump();
                self.mk(ExprKind::BoolLit(false), start)
            }
            TokenKind::KwSelf => {
                self.bump();
                self.mk(
                    ExprKind::Path(vec![Ident {
                        name: "self".into(),
                        span: start,
                    }]),
                    start,
                )
            }
            TokenKind::Ident(_) => self.path_or_struct_lit(),
            TokenKind::LParen => {
                self.bump();
                if self.eat(&TokenKind::RParen) {
                    let span = start.to(self.prev_span());
                    return self.mk(ExprKind::UnitLit, span);
                }
                let saved = self.no_struct_lit;
                self.no_struct_lit = false;
                let inner = self.expr();
                self.no_struct_lit = saved;
                self.expect(&TokenKind::RParen, "`)` to close the parenthesized expression");
                inner
            }
            TokenKind::LBracket => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    if self.eat(&TokenKind::RBracket) {
                        break;
                    }
                    if self.at_eof() {
                        let span = self.span();
                        self.error("E0100", span, "unclosed list literal: missing `]`");
                        break;
                    }
                    let saved = self.no_struct_lit;
                    self.no_struct_lit = false;
                    items.push(self.expr());
                    self.no_struct_lit = saved;
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(&TokenKind::RBracket, "`,` or `]` in list literal");
                        break;
                    }
                }
                let span = start.to(self.prev_span());
                self.mk(ExprKind::ListLit(items), span)
            }
            TokenKind::HashBrace => {
                self.bump();
                let mut entries = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    if self.at_eof() {
                        let span = self.span();
                        self.error("E0100", span, "unclosed map literal: missing `}`");
                        break;
                    }
                    let saved = self.no_struct_lit;
                    self.no_struct_lit = false;
                    let key = self.expr();
                    self.expect(&TokenKind::Colon, "`:` between map key and value");
                    let value = self.expr();
                    self.no_struct_lit = saved;
                    entries.push((key, value));
                    self.skip_newlines();
                    if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                        self.expect(&TokenKind::RBrace, "`,` or `}` in map literal");
                        break;
                    }
                }
                let span = start.to(self.prev_span());
                self.mk(ExprKind::MapLit(entries), span)
            }
            TokenKind::KwIf => self.if_expr(),
            TokenKind::KwMatch => self.match_expr(),
            TokenKind::KwWhile => {
                self.bump();
                let cond = self.expr_no_struct_lit();
                self.skip_newlines();
                let body = self.block_or_error();
                let span = start.to(body.span);
                self.mk(
                    ExprKind::While {
                        cond: Box::new(cond),
                        body,
                    },
                    span,
                )
            }
            TokenKind::KwLoop => {
                self.bump();
                self.skip_newlines();
                let body = self.block_or_error();
                let span = start.to(body.span);
                self.mk(ExprKind::Loop { body }, span)
            }
            TokenKind::KwFor => {
                self.bump();
                let var = self.expect_ident("loop variable after `for`").unwrap_or(Ident {
                    name: "_".into(),
                    span: start,
                });
                self.expect(&TokenKind::KwIn, "`in` in `for` loop");
                let iter = self.expr_no_struct_lit();
                self.skip_newlines();
                let body = self.block_or_error();
                let span = start.to(body.span);
                self.mk(
                    ExprKind::For {
                        var,
                        iter: Box::new(iter),
                        body,
                    },
                    span,
                )
            }
            TokenKind::LBrace => {
                let block = self.block_or_error();
                let span = block.span;
                self.mk(ExprKind::Block(block), span)
            }
            TokenKind::KwBreak => {
                self.bump();
                self.mk(ExprKind::Break, start)
            }
            TokenKind::KwContinue => {
                self.bump();
                self.mk(ExprKind::Continue, start)
            }
            TokenKind::KwReturn => {
                self.bump();
                let value = if matches!(
                    self.kind(),
                    TokenKind::Newline
                        | TokenKind::Semi
                        | TokenKind::RBrace
                        | TokenKind::RParen
                        | TokenKind::Comma
                        | TokenKind::Eof
                ) {
                    None
                } else {
                    Some(Box::new(self.expr()))
                };
                let span = match &value {
                    Some(v) => start.to(v.span),
                    None => start,
                };
                self.mk(ExprKind::Return(value), span)
            }
            TokenKind::Pipe | TokenKind::OrOr => self.closure_expr(),
            other => {
                self.error(
                    "E0111",
                    start,
                    format!("expected an expression, found {}", other.describe()),
                );
                // Consume the offending token so parsing always advances
                // (statement-level recovery resyncs the rest).
                if !matches!(
                    other,
                    TokenKind::RBrace | TokenKind::Newline | TokenKind::Eof
                ) {
                    self.bump();
                }
                self.mk(ExprKind::Error, start)
            }
        }
    }

    fn block_or_error(&mut self) -> Block {
        self.block().unwrap_or_else(|| Block {
            stmts: vec![],
            span: self.span(),
            id: self.id(),
        })
    }

    fn path_or_struct_lit(&mut self) -> Expr {
        let start = self.span();
        let mut segments = vec![self.expect_ident("identifier").unwrap()];
        while self.at(&TokenKind::ColonColon) {
            self.bump();
            match self.expect_ident("identifier after `::`") {
                Some(seg) => segments.push(seg),
                None => break,
            }
        }
        // Struct literal? Only when `{` follows and we're not in a
        // condition/scrutinee header position.
        if self.at(&TokenKind::LBrace) && !self.no_struct_lit {
            self.bump();
            let mut fields = Vec::new();
            loop {
                self.skip_newlines();
                if self.eat(&TokenKind::RBrace) {
                    break;
                }
                if self.at_eof() {
                    self.error("E0100", start, "unclosed struct literal: missing `}`");
                    break;
                }
                let fname = match self.expect_ident("field name") {
                    Some(n) => n,
                    None => {
                        self.recover_to(&[
                            TokenKind::Comma,
                            TokenKind::Newline,
                            TokenKind::RBrace,
                        ]);
                        continue;
                    }
                };
                let value = if self.eat(&TokenKind::Colon) {
                    self.expr()
                } else {
                    // Field shorthand: `Point { x, y }`.
                    let id = self.id();
                    Expr {
                        kind: ExprKind::Path(vec![fname.clone()]),
                        span: fname.span,
                        id,
                    }
                };
                fields.push((fname, value));
                self.skip_newlines();
                if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                    self.expect(&TokenKind::RBrace, "`,` or `}` in struct literal");
                    break;
                }
            }
            let span = start.to(self.prev_span());
            return self.mk(
                ExprKind::StructLit {
                    path: segments,
                    fields,
                },
                span,
            );
        }
        let span = start.to(self.prev_span());
        self.mk(ExprKind::Path(segments), span)
    }

    fn if_expr(&mut self) -> Expr {
        let start = self.bump().span; // `if`
        // `if let pat = expr { ... }`
        if self.at(&TokenKind::KwLet) {
            self.bump();
            let pat = self.pattern();
            self.expect(&TokenKind::Eq, "`=` in `if let`");
            let scrutinee = self.expr_no_struct_lit();
            self.skip_newlines();
            let then = self.block_or_error();
            let else_ = self.else_tail();
            let span = start.to(self.prev_span());
            return self.mk(
                ExprKind::IfLet {
                    pat,
                    scrutinee: Box::new(scrutinee),
                    then,
                    else_,
                },
                span,
            );
        }
        let cond = self.expr_no_struct_lit();
        self.skip_newlines();
        let then = self.block_or_error();
        let else_ = self.else_tail();
        let span = start.to(self.prev_span());
        self.mk(
            ExprKind::If {
                cond: Box::new(cond),
                then,
                else_,
            },
            span,
        )
    }

    /// Parse an optional `else` clause, looking through newlines (an `else`
    /// on the next line continues the `if` — documented continuation rule).
    fn else_tail(&mut self) -> Option<Box<Expr>> {
        if self.peek_through_newlines() != &TokenKind::KwElse {
            return None;
        }
        self.skip_newlines();
        self.bump(); // `else`
        self.skip_newlines();
        if self.at(&TokenKind::KwIf) {
            return Some(Box::new(self.if_expr()));
        }
        let block = self.block_or_error();
        let span = block.span;
        Some(Box::new(self.mk(ExprKind::Block(block), span)))
    }

    fn match_expr(&mut self) -> Expr {
        let start = self.bump().span; // `match`
        let scrutinee = self.expr_no_struct_lit();
        self.skip_newlines();
        self.expect(&TokenKind::LBrace, "`{` to start the match arms");
        let mut arms = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                self.error("E0100", start, "unclosed match expression: missing `}`");
                break;
            }
            let arm_start = self.span();
            let pat = self.pattern();
            let guard = if self.eat(&TokenKind::KwIf) {
                Some(self.expr_no_struct_lit())
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow, "`=>` after the match pattern");
            self.skip_newlines();
            let body = self.expr();
            let span = arm_start.to(body.span);
            arms.push(MatchArm {
                pat,
                guard,
                body,
                span,
            });
            // Arms are separated by `,` and/or newline.
            let separated =
                self.eat(&TokenKind::Comma) || matches!(self.kind(), TokenKind::Newline);
            self.skip_newlines();
            if !separated && !self.at(&TokenKind::RBrace) {
                let found = self.kind().describe();
                let span = self.span();
                self.error(
                    "E0100",
                    span,
                    format!("expected `,` or `}}` after match arm, found {found}"),
                );
                self.recover_to(&[TokenKind::Comma, TokenKind::Newline, TokenKind::RBrace]);
                self.eat(&TokenKind::Comma);
            }
        }
        let span = start.to(self.prev_span());
        self.mk(
            ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span,
        )
    }

    fn closure_expr(&mut self) -> Expr {
        let start = self.span();
        let mut params = Vec::new();
        if self.eat(&TokenKind::OrOr) {
            // `||` — empty parameter list.
        } else {
            self.bump(); // `|`
            loop {
                if self.eat(&TokenKind::Pipe) {
                    break;
                }
                let name = match self.expect_ident("closure parameter") {
                    Some(n) => n,
                    None => {
                        self.recover_to(&[TokenKind::Pipe, TokenKind::Comma, TokenKind::Newline]);
                        if !self.eat(&TokenKind::Comma) {
                            self.eat(&TokenKind::Pipe);
                            break;
                        }
                        continue;
                    }
                };
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.type_expr())
                } else {
                    None
                };
                params.push((name, ty));
                if !self.eat(&TokenKind::Comma) {
                    self.expect(&TokenKind::Pipe, "`,` or `|` in closure parameters");
                    break;
                }
            }
        }
        let ret = if self.eat(&TokenKind::Arrow) {
            Some(self.type_expr())
        } else {
            None
        };
        if ret.is_some() && self.peek_through_newlines() != &TokenKind::LBrace {
            let span = self.span();
            self.error_help(
                "E0112",
                span,
                "a closure with a declared return type needs a block body",
                "write `|x| -> int { ... }`",
            );
        }
        let body = self.expr();
        let span = start.to(body.span);
        self.mk(
            ExprKind::Closure {
                params,
                ret,
                body: Box::new(body),
            },
            span,
        )
    }

    // ------------------------------------------------------------ patterns

    fn pattern(&mut self) -> Pattern {
        let first = self.pattern_single();
        if !self.at(&TokenKind::Pipe) {
            return first;
        }
        let mut alts = vec![first];
        while self.eat(&TokenKind::Pipe) {
            self.skip_newlines();
            alts.push(self.pattern_single());
        }
        let span = alts[0].span.to(alts[alts.len() - 1].span);
        Pattern {
            span,
            id: self.id(),
            kind: PatternKind::Or(alts),
        }
    }

    fn pattern_single(&mut self) -> Pattern {
        let start = self.span();
        let kind = match self.kind().clone() {
            TokenKind::Underscore => {
                self.bump();
                PatternKind::Wildcard
            }
            TokenKind::Int(n) => {
                self.bump();
                PatternKind::IntLit(n)
            }
            TokenKind::Minus => {
                self.bump();
                match self.kind().clone() {
                    TokenKind::Int(n) => {
                        self.bump();
                        PatternKind::IntLit(-n)
                    }
                    other => {
                        let span = self.span();
                        self.error(
                            "E0113",
                            span,
                            format!("expected integer after `-` in pattern, found {}", other.describe()),
                        );
                        PatternKind::Error
                    }
                }
            }
            TokenKind::KwTrue => {
                self.bump();
                PatternKind::BoolLit(true)
            }
            TokenKind::KwFalse => {
                self.bump();
                PatternKind::BoolLit(false)
            }
            TokenKind::Char(c) => {
                self.bump();
                PatternKind::CharLit(c)
            }
            TokenKind::Str(s) => {
                self.bump();
                PatternKind::StrLit(s)
            }
            TokenKind::Ident(_) => {
                let mut segments = vec![self.expect_ident("identifier").unwrap()];
                while self.at(&TokenKind::ColonColon) {
                    self.bump();
                    match self.expect_ident("identifier after `::`") {
                        Some(seg) => segments.push(seg),
                        None => break,
                    }
                }
                if self.eat(&TokenKind::LParen) {
                    // tuple variant pattern
                    let mut pats = Vec::new();
                    loop {
                        if self.eat(&TokenKind::RParen) {
                            break;
                        }
                        pats.push(self.pattern());
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(&TokenKind::RParen, "`,` or `)` in pattern");
                            break;
                        }
                    }
                    PatternKind::Variant {
                        path: segments,
                        args: VariantPatArgs::Tuple(pats),
                    }
                } else if self.at(&TokenKind::LBrace) {
                    self.bump();
                    let (fields, has_rest) = self.struct_pattern_fields();
                    // `Name { ... }` — struct or struct-variant pattern;
                    // the checker disambiguates by what `Name` resolves to.
                    if segments.len() >= 2 {
                        PatternKind::Variant {
                            path: segments,
                            args: VariantPatArgs::Struct { fields, has_rest },
                        }
                    } else {
                        PatternKind::Struct {
                            path: segments,
                            fields,
                            has_rest,
                        }
                    }
                } else if segments.len() > 1 {
                    PatternKind::Variant {
                        path: segments,
                        args: VariantPatArgs::Unit,
                    }
                } else {
                    // Single identifier: binding, or a bare unit-variant
                    // name (`None`) — resolved by the checker.
                    PatternKind::Binding(segments.pop().unwrap())
                }
            }
            other => {
                self.error(
                    "E0113",
                    start,
                    format!("expected a pattern, found {}", other.describe()),
                );
                if !matches!(
                    other,
                    TokenKind::RBrace | TokenKind::Newline | TokenKind::Eof | TokenKind::FatArrow
                ) {
                    self.bump();
                }
                PatternKind::Error
            }
        };
        Pattern {
            kind,
            span: start.to(self.prev_span()),
            id: self.id(),
        }
    }

    fn struct_pattern_fields(&mut self) -> (Vec<(Ident, Pattern)>, bool) {
        let mut fields = Vec::new();
        let mut has_rest = false;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_eof() {
                let span = self.span();
                self.error("E0100", span, "unclosed struct pattern: missing `}`");
                break;
            }
            if self.eat(&TokenKind::DotDot) {
                has_rest = true;
                self.skip_newlines();
                self.expect(&TokenKind::RBrace, "`}` after `..` in pattern");
                break;
            }
            let fname = match self.expect_ident("field name in pattern") {
                Some(n) => n,
                None => {
                    self.recover_to(&[TokenKind::Comma, TokenKind::RBrace, TokenKind::Newline]);
                    if !self.eat(&TokenKind::Comma) {
                        self.eat(&TokenKind::RBrace);
                        break;
                    }
                    continue;
                }
            };
            let pat = if self.eat(&TokenKind::Colon) {
                self.pattern()
            } else {
                // Shorthand `Point { x }` desugars to `x: x` (a binding).
                Pattern {
                    kind: PatternKind::Binding(fname.clone()),
                    span: fname.span,
                    id: self.id(),
                }
            };
            fields.push((fname, pat));
            self.skip_newlines();
            if !self.eat(&TokenKind::Comma) && !self.at(&TokenKind::RBrace) {
                self.expect(&TokenKind::RBrace, "`,` or `}` in struct pattern");
                break;
            }
        }
        (fields, has_rest)
    }
}

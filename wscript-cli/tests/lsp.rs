//! LSP smoke test (PRD §10 M6): drive `wscript lsp` over stdio with raw
//! JSON-RPC and exercise all four v1 features — diagnostics, hover,
//! goto-definition, completions. (Editor-equivalent: any generic LSP
//! client; no extension required.)

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use wscript::Type;
use wscript_compiler::check::builtin_methods;

struct Lsp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Lsp {
    fn start() -> Lsp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wscript"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn wscript lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Lsp {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, body: &str) {
        let msg = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.stdin.write_all(msg.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: &str) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#
        ));
        id
    }

    fn notify(&mut self, method: &str, params: &str) {
        self.send(&format!(
            r#"{{"jsonrpc":"2.0","method":"{method}","params":{params}}}"#
        ));
    }

    /// Read messages until one contains `needle`; panics after 50 messages.
    fn read_until(&mut self, needle: &str) -> String {
        for _ in 0..50 {
            let msg = self.read_message();
            if msg.contains(needle) {
                return msg;
            }
        }
        panic!("never saw `{needle}` in LSP output");
    }

    fn read_message(&mut self) -> String {
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).expect("read header");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0u8; content_length];
        self.stdout.read_exact(&mut buf).expect("read body");
        String::from_utf8(buf).expect("utf8 body")
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

const DOC: &str = "fn add(a: int, b: int) -> int {\n    a + b\n}\n\nfn main() {\n    let total = add(1, 2)\n    println(total)\n    let broken: string = total\n}\n";

#[test]
fn lsp_four_features() {
    let mut lsp = Lsp::start();

    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    let init = lsp.read_until(&format!("\"id\":{id}"));
    assert!(init.contains("wscript-lsp"), "{init}");
    lsp.notify("initialized", "{}");

    // 1. Diagnostics: the type error on `broken` must be published.
    let doc_json = serde_jsonish(DOC);
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///test.wscript","languageId":"wscript","version":1,"text":{doc_json}}}}}"#
        ),
    );
    let diags = lsp.read_until("publishDiagnostics");
    assert!(diags.contains("E0220"), "expected type error: {diags}");

    // 2. Hover over `total` in println(total) — line 6, col 12.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///test.wscript"},"position":{"line":6,"character":13}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(hover.contains("int"), "hover: {hover}");

    // 3. Goto-definition on the `add` callee — line 5, col 16.
    let id = lsp.request(
        "textDocument/definition",
        r#"{"textDocument":{"uri":"file:///test.wscript"},"position":{"line":5,"character":17}}"#,
    );
    let def = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        def.contains(r#""line":0"#),
        "definition should land on line 0: {def}"
    );

    // 4. Completions after `total.` …on a fresh doc state.
    let id = lsp.request(
        "textDocument/completion",
        r#"{"textDocument":{"uri":"file:///test.wscript"},"position":{"line":6,"character":4}}"#,
    );
    let completions = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        completions.contains("println"),
        "completions: {completions}"
    );
    assert!(completions.contains("match"), "keywords: {completions}");

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// Hover over a host call shows the parameter names the host declared
/// (issue #22). `atan2` is the case that matters: `(y, x)` and `(x, y)`
/// are both `(float, float)`, so positional placeholders leave the editor
/// unable to tell you which way round the arguments go.
#[test]
fn lsp_hover_shows_declared_host_parameter_names() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    let doc = "use math\nfn main() -> float {\n    math::atan2(1.0, 2.0)\n}\n";
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///hover.wscript","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(doc)
        ),
    );
    lsp.read_until("publishDiagnostics");

    // On the `atan2` callee — line 2, character 12.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///hover.wscript"},"position":{"line":2,"character":12}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        hover.contains("math::atan2(y: float, x: float) -> float"),
        "hover should name the parameters: {hover}"
    );

    // Methods carry names the same way.
    let doc = "use json\nfn main() {\n    let v = json::parse(\"{}\").unwrap()\n    let got = v.get(\"a\")\n}\n";
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///method.wscript","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(doc)
        ),
    );
    lsp.read_until("publishDiagnostics");
    // On the `get` callee — line 3, character 17.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///method.wscript"},"position":{"line":3,"character":17}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        hover.contains("Value.get(key: string)"),
        "method hover should name the parameters: {hover}"
    );

    // On the receiver `v` — line 3, character 14. A receiver is not the
    // call, so hovering it says what `v` is and nothing about `get`.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///method.wscript"},"position":{"line":3,"character":14}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        hover.contains("Value"),
        "the receiver's own type is what hover has to say: {hover}"
    );
    assert!(
        !hover.contains("Value.get"),
        "hovering the receiver should not report the method called on it: {hover}"
    );

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// A deep AST must survive the editor's stacks.
///
/// Operator chains stack AST depth without costing the parser recursion,
/// so a document that parses cleanly can still be hundreds of levels deep
/// — and `check::Index` is built by a recursive walk of that tree. It is
/// built inside `analyze_entry`, on the 32 MiB pipeline thread, where
/// every other recursive pass runs; building it lazily on first query
/// would put the walk on tokio's 2 MiB stack instead. Exercised through
/// the server for that reason: a unit test runs on the test harness's
/// stack and would pass either way.
#[test]
fn lsp_survives_a_deeply_chained_document() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    // Well inside the parser's chain budget, so this parses and checks:
    // the AST really is ~400 deep by the time the index walks it.
    let chain = std::iter::repeat_n("1", 400)
        .collect::<Vec<_>>()
        .join(" + ");
    let doc = format!("fn main() -> int {{\n    {chain}\n}}\n");
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///deep.wscript","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(&doc)
        ),
    );
    let diags = lsp.read_until("publishDiagnostics");
    assert!(
        diags.contains(r#""diagnostics":[]"#),
        "the chain must parse and check, or the walk never gets deep: {diags}"
    );

    // …and the server is still answering afterwards.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///deep.wscript"},"position":{"line":1,"character":8}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(hover.contains("int"), "hover: {hover}");

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// Every link of a call chain starts at the same offset, so hover must
/// pick the one the cursor is on rather than reporting all of them.
#[test]
fn lsp_hover_on_a_chain_reports_one_call() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    let doc = "use json\nfn main() {\n    let x = json::parse(\"{}\").unwrap().get(\"a\")\n}\n";
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///chain.wscript","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(doc)
        ),
    );
    lsp.read_until("publishDiagnostics");

    // On `json`, the head of the chain — line 2, character 13.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///chain.wscript"},"position":{"line":2,"character":13}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        hover.contains("json::parse(string)"),
        "hover should report the call under the cursor: {hover}"
    );
    assert!(
        !hover.contains("Value.get"),
        "…and not a method further along the chain: {hover}"
    );

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

#[test]
fn lsp_method_completions() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    let doc = "fn main() {\n    let name = \"wscript\"\n    name.\n}\n";
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///m.wscript","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(doc)
        ),
    );
    lsp.read_until("publishDiagnostics");

    // Cursor right after `name.` (line 2, character 9).
    let id = lsp.request(
        "textDocument/completion",
        r#"{"textDocument":{"uri":"file:///m.wscript"},"position":{"line":2,"character":9}}"#,
    );
    let completions = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        completions.contains("to_upper") && completions.contains("parse_int"),
        "string methods expected: {completions}"
    );

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// Completion offers every builtin method the checker knows, not a copy
/// of the list.
///
/// This is the regression the copy caused (issue #17): `d4b0214 feat: list
/// and map combinators` added fourteen methods to `check/methods.rs` and
/// never touched the editor's hardcoded tables, so `any`, `zip_with`,
/// `Map::each` and friends typechecked but never completed. Asserting
/// against `builtin_methods` rather than a literal list is the point —
/// adding a method to the checker now either shows up here or fails this
/// test; it cannot quietly go missing.
#[test]
fn lsp_completes_every_builtin_method_the_checker_knows() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    // (document, receiver type, cursor line/character just past the `.`)
    let cases: [(&str, Type, u32, u32); 2] = [
        (
            "fn main() {\n    let xs = [1, 2, 3]\n    xs.\n}\n",
            Type::List(Box::new(Type::Int)),
            2,
            7,
        ),
        (
            "fn main() {\n    let m = #{\"a\": 1}\n    m.\n}\n",
            Type::Map(Box::new(Type::Str), Box::new(Type::Int)),
            2,
            6,
        ),
    ];

    for (i, (doc, recv, line, character)) in cases.into_iter().enumerate() {
        let uri = format!("file:///builtin{i}.wscript");
        lsp.notify(
            "textDocument/didOpen",
            &format!(
                r#"{{"textDocument":{{"uri":"{uri}","languageId":"wscript","version":1,"text":{}}}}}"#,
                serde_jsonish(doc)
            ),
        );
        lsp.read_until("publishDiagnostics");

        let id = lsp.request(
            "textDocument/completion",
            &format!(
                r#"{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":{line},"character":{character}}}}}"#
            ),
        );
        let completions = lsp.read_until(&format!("\"id\":{id}"));
        let expected = builtin_methods(&recv);
        assert!(!expected.is_empty(), "no builtin methods for {recv:?}");
        for (name, _) in expected {
            assert!(
                completions.contains(&format!(r#""label":"{name}""#)),
                "`{name}` is a builtin method of {recv:?} but was not offered: {completions}"
            );
        }
    }

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// The editor must resolve the same imports `wscript check` does.
///
/// Opened against the fixture project's `wscript.toml`, `main.wscript`
/// imports a script from `src_roots` and calls a host function declared
/// in an `.wscripti`. Before #13 the server loaded the interfaces but
/// built its import resolver with no source roots, so this document lit
/// up with `E0200 unknown module `helper`` in the editor while
/// `wscript check` on the very same file exited 0.
#[test]
fn lsp_honors_the_manifest_src_roots() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project");
    let entry = root.join("main.wscript");
    let text = std::fs::read_to_string(&entry).expect("fixture readable");
    let root_uri = format!("file://{}", root.display());
    let entry_uri = format!("file://{}", entry.display());

    let mut lsp = Lsp::start();
    let id = lsp.request(
        "initialize",
        &format!(r#"{{"capabilities":{{}},"rootUri":"{root_uri}"}}"#),
    );
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"{entry_uri}","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(&text)
        ),
    );
    let diags = lsp.read_until("publishDiagnostics");
    assert!(
        !diags.contains("E0200"),
        "`use helper` must resolve through the manifest's src_roots: {diags}"
    );
    assert!(
        diags.contains(r#""diagnostics":[]"#),
        "the fixture is clean in the editor, as it is in `wscript check`: {diags}"
    );

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// `module::` completes to that module's members, in an expression and in
/// a `use` — the two places a qualified name is written.
#[test]
fn lsp_completes_module_members_after_colon_colon() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    // (document, cursor line/character just past the `::`)
    let cases: [(&str, u32, u32); 2] = [
        ("use math\nfn main() {\n    math::\n}\n", 2, 10),
        ("use math::\n", 0, 10),
    ];
    for (i, (doc, line, character)) in cases.into_iter().enumerate() {
        let uri = format!("file:///qualified{i}.wscript");
        lsp.notify(
            "textDocument/didOpen",
            &format!(
                r#"{{"textDocument":{{"uri":"{uri}","languageId":"wscript","version":1,"text":{}}}}}"#,
                serde_jsonish(doc)
            ),
        );
        lsp.read_until("publishDiagnostics");

        let id = lsp.request(
            "textDocument/completion",
            &format!(
                r#"{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":{line},"character":{character}}}}}"#
            ),
        );
        let completions = lsp.read_until(&format!("\"id\":{id}"));
        assert!(
            completions.contains(r#""label":"atan2""#),
            "`math::` should offer the module's functions: {completions}"
        );
        assert!(
            !completions.contains(r#""label":"match""#),
            "nothing unqualified belongs in a qualified position: {completions}"
        );
    }

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// A host symbol declared only in a `.wscripti` still resolves: hover
/// names it and goto-definition jumps to the interface (PRD §9 feature 3).
///
/// This is the second registration path — the interface loader, not
/// `Module::merge_into` — reaching the registry's reverse index, which is
/// what replaced the language server's linear scan over every module.
#[test]
fn lsp_resolves_a_host_symbol_declared_in_an_interface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project");
    let entry = root.join("main.wscript");
    let text = std::fs::read_to_string(&entry).expect("fixture readable");
    let root_uri = format!("file://{}", root.display());
    let entry_uri = format!("file://{}", entry.display());

    let mut lsp = Lsp::start();
    let id = lsp.request(
        "initialize",
        &format!(r#"{{"capabilities":{{}},"rootUri":"{root_uri}"}}"#),
    );
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"{entry_uri}","languageId":"wscript","version":1,"text":{}}}}}"#,
            serde_jsonish(&text)
        ),
    );
    lsp.read_until("publishDiagnostics");

    // On `greet` in `println(host::greet("world"))` — line 4, character 20.
    let position = r#""position":{"line":4,"character":20}"#;
    let id = lsp.request(
        "textDocument/hover",
        &format!(r#"{{"textDocument":{{"uri":"{entry_uri}"}},{position}}}"#),
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        // `a0` in the interface is the placeholder for an undeclared
        // name, so the type stands alone here (issue #22).
        hover.contains("host::greet(string) -> string"),
        "hover should name the interface declaration: {hover}"
    );

    let id = lsp.request(
        "textDocument/definition",
        &format!(r#"{{"textDocument":{{"uri":"{entry_uri}"}},{position}}}"#),
    );
    let def = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        def.contains("api.wscripti") && def.contains(r#""line":1"#),
        "definition should land on the interface's `fn greet`: {def}"
    );

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

/// Every registration in the CLI's own registry — the whole stdlib, the
/// biggest one that exists — is nameable from its host index.
///
/// `Registry::host_ref` is what replaced the language server's linear scan
/// over every module, and an index no declaration claimed resolves to
/// nothing, which reads in the editor as a host call with no hover at all.
#[test]
fn every_stdlib_registration_is_nameable_from_its_host_index() {
    let session = wscript::Session::builder()
        .modules(wscript_std::all_modules(Vec::new()))
        .build();
    let registry = session.registry();
    assert!(
        registry.undeclared_host_fns().is_empty(),
        "registrations with no declaration: {:?}",
        registry.undeclared_host_fns()
    );
    for idx in 0..registry.host_fns.len() as u32 {
        let host = registry
            .host_ref(idx)
            .unwrap_or_else(|| panic!("host index {idx} names nothing"));
        assert_eq!(host.decl.host_idx, idx, "{}", host.qualified_name());
    }
}

/// Minimal JSON string encoder for the test documents.
fn serde_jsonish(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

//! LSP smoke test (PRD §10 M6): drive `wscript lsp` over stdio with raw
//! JSON-RPC and exercise all four v1 features — diagnostics, hover,
//! goto-definition, completions. (Editor-equivalent: any generic LSP
//! client; no extension required.)

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

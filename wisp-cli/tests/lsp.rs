//! LSP smoke test (PRD §10 M6): drive `wisp lsp` over stdio with raw
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_wisp"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn wisp lsp");
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
    assert!(init.contains("wisp-lsp"), "{init}");
    lsp.notify("initialized", "{}");

    // 1. Diagnostics: the type error on `broken` must be published.
    let doc_json = serde_jsonish(DOC);
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///test.wisp","languageId":"wisp","version":1,"text":{doc_json}}}}}"#
        ),
    );
    let diags = lsp.read_until("publishDiagnostics");
    assert!(diags.contains("E0220"), "expected type error: {diags}");

    // 2. Hover over `total` in println(total) — line 6, col 12.
    let id = lsp.request(
        "textDocument/hover",
        r#"{"textDocument":{"uri":"file:///test.wisp"},"position":{"line":6,"character":13}}"#,
    );
    let hover = lsp.read_until(&format!("\"id\":{id}"));
    assert!(hover.contains("int"), "hover: {hover}");

    // 3. Goto-definition on the `add` callee — line 5, col 16.
    let id = lsp.request(
        "textDocument/definition",
        r#"{"textDocument":{"uri":"file:///test.wisp"},"position":{"line":5,"character":17}}"#,
    );
    let def = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        def.contains(r#""line":0"#),
        "definition should land on line 0: {def}"
    );

    // 4. Completions after `total.` …on a fresh doc state.
    let id = lsp.request(
        "textDocument/completion",
        r#"{"textDocument":{"uri":"file:///test.wisp"},"position":{"line":6,"character":4}}"#,
    );
    let completions = lsp.read_until(&format!("\"id\":{id}"));
    assert!(completions.contains("println"), "completions: {completions}");
    assert!(completions.contains("match"), "keywords: {completions}");

    lsp.request("shutdown", "null");
    lsp.notify("exit", "null");
}

#[test]
fn lsp_method_completions() {
    let mut lsp = Lsp::start();
    let id = lsp.request("initialize", r#"{"capabilities":{}}"#);
    lsp.read_until(&format!("\"id\":{id}"));
    lsp.notify("initialized", "{}");

    let doc = "fn main() {\n    let name = \"wisp\"\n    name.\n}\n";
    lsp.notify(
        "textDocument/didOpen",
        &format!(
            r#"{{"textDocument":{{"uri":"file:///m.wisp","languageId":"wisp","version":1,"text":{}}}}}"#,
            serde_jsonish(doc)
        ),
    );
    lsp.read_until("publishDiagnostics");

    // Cursor right after `name.` (line 2, character 9).
    let id = lsp.request(
        "textDocument/completion",
        r#"{"textDocument":{"uri":"file:///m.wisp"},"position":{"line":2,"character":9}}"#,
    );
    let completions = lsp.read_until(&format!("\"id\":{id}"));
    assert!(
        completions.contains("to_upper") && completions.contains("parse_int"),
        "string methods expected: {completions}"
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

//! Diagnostic snapshots: for each fixture under `tests/fixtures/diags/`, the
//! full set of diagnostics compiling it produces — code, location, message and
//! help — asserted against a committed `.snap` file.
//!
//! This is the safety net that lets the checker be restructured: `diags.rs`
//! asserts codes only, so a refactor that changes a message, a help line or a
//! span is invisible to it. Regenerate with `just snap-regen`.

mod common;

use std::path::Path;

use wscript_core::diag::{Diagnostic, Severity};
use wscript_core::registry::Registry;

/// Render one fixture's diagnostics. Help text falls back to `default_help`,
/// matching what a renderer shows the user, so the snapshot captures the
/// message a person actually reads.
fn render(src: &str, diags: &[Diagnostic]) -> String {
    if diags.is_empty() {
        return "(no diagnostics)\n".to_string();
    }
    let mut out = String::new();
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "",
            Severity::Warning => " warning",
        };
        out.push_str(&format!(
            "{}{sev}  {}\n",
            d.code,
            common::span_str(src, d.span.lo, d.span.hi)
        ));
        out.push_str(&format!("  {}\n", d.message));
        for (span, label) in &d.labels {
            out.push_str(&format!(
                "  label {}: {label}\n",
                common::span_str(src, span.lo, span.hi)
            ));
        }
        let help = d
            .help
            .clone()
            .or_else(|| wscript_core::diag::default_help(d.code).map(str::to_string));
        if let Some(help) = help {
            out.push_str(&format!("  help: {help}\n"));
        }
        out.push('\n');
    }
    out
}

fn diagnostics_of(src: &str) -> Vec<Diagnostic> {
    match wscript_compiler::compile(src, &Registry::new()) {
        Ok(compiled) => compiled.warnings,
        Err(diags) => diags,
    }
}

#[test]
fn diagnostic_snapshots() {
    let root = Path::new("tests/fixtures/diags");
    let fixtures = common::fixtures(root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures under {} — did the directory move?",
        root.display()
    );

    let mut failures = Vec::new();
    for fixture in fixtures {
        let src = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("reading {}: {e}", fixture.display()));
        let actual = render(&src, &diagnostics_of(&src));
        if let Err(msg) = common::check_snapshot(&fixture, &actual) {
            failures.push(msg);
        }
    }
    common::report(failures);
}

/// Every fixture must actually produce the diagnostic its directory claims —
/// a fixture that silently stops failing is worse than no fixture, because the
/// snapshot still passes.
#[test]
fn every_fixture_produces_a_diagnostic() {
    let mut silent = Vec::new();
    for fixture in common::fixtures(Path::new("tests/fixtures/diags")) {
        let src = std::fs::read_to_string(&fixture).unwrap();
        if !diagnostics_of(&src)
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            silent.push(fixture.display().to_string());
        }
    }
    assert!(
        silent.is_empty(),
        "these fixtures no longer produce an error:\n  {}",
        silent.join("\n  ")
    );
}

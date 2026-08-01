//! Diagnostic snapshots: for each fixture under `tests/fixtures/diags/`, the
//! full set of diagnostics compiling it produces — code, location, message and
//! help — asserted against a committed `.snap` file.
//!
//! This is the safety net that lets the checker be restructured: `diags.rs`
//! asserts codes only, so a refactor that changes a message, a help line or a
//! span is invisible to it. Regenerate with `just snap-regen`.
//!
//! A fixture is a `.wscript` file, optionally beside a `.wscripti` of the same
//! stem: an interface file the host would have registered, loaded before the
//! script compiles. That is the only way to reach the diagnostics about host
//! modules — a script alone cannot make one exist.

mod common;

use std::path::{Path, PathBuf};

use wscript_core::diag::{Diagnostic, Severity};
use wscript_core::registry::Registry;

/// Which of a fixture's two files a span indexes into. They have separate
/// address spaces, so a rendered line has to say which one `3:5` means.
#[derive(Clone, Copy)]
enum Origin {
    Script,
    Interface,
}

impl Origin {
    fn prefix(self) -> &'static str {
        match self {
            Origin::Script => "",
            Origin::Interface => "interface ",
        }
    }
}

/// A diagnostic, and where to read its span.
struct Located {
    origin: Origin,
    src: String,
    diag: Diagnostic,
}

/// Render one fixture's diagnostics. Help text falls back to `default_help`,
/// matching what a renderer shows the user, so the snapshot captures the
/// message a person actually reads.
fn render(diags: &[Located]) -> String {
    if diags.is_empty() {
        return "(no diagnostics)\n".to_string();
    }
    let mut out = String::new();
    for Located { origin, src, diag } in diags {
        let sev = match diag.severity {
            Severity::Error => "",
            Severity::Warning => " warning",
        };
        let at = origin.prefix();
        out.push_str(&format!(
            "{}{sev}  {at}{}\n",
            diag.code,
            common::span_str(src, diag.span.lo, diag.span.hi)
        ));
        out.push_str(&format!("  {}\n", diag.message));
        for (span, label) in &diag.labels {
            out.push_str(&format!(
                "  label {at}{}: {label}\n",
                common::span_str(src, span.lo, span.hi)
            ));
        }
        if let Some(help) = diag.help_text() {
            out.push_str(&format!("  help: {help}\n"));
        }
        out.push('\n');
    }
    out
}

/// Compile a fixture the way its host would: register the interface beside it
/// (if any), then compile the script against the resulting registry.
fn diagnostics_of(fixture: &Path) -> Vec<Located> {
    let mut reg = Registry::new();
    let mut out = Vec::new();

    let interface_path = fixture.with_extension("wscripti");
    if let Ok(interface_src) = std::fs::read_to_string(&interface_path) {
        let (diags, _index) = wscript_compiler::wscripti::load(&interface_src, &mut reg);
        out.extend(diags.into_iter().map(|diag| Located {
            origin: Origin::Interface,
            src: interface_src.clone(),
            diag,
        }));
    }

    let src = std::fs::read_to_string(fixture)
        .unwrap_or_else(|e| panic!("reading {}: {e}", fixture.display()));
    let diags = match wscript_compiler::compile(&src, &reg) {
        Ok(compiled) => compiled.warnings,
        Err(diags) => diags,
    };
    out.extend(diags.into_iter().map(|diag| Located {
        origin: Origin::Script,
        src: src.clone(),
        diag,
    }));
    out
}

/// The whole corpus, compiled once: every fixture paired with what it makes
/// the compiler say. Four tests ask four questions of the same answer.
fn corpus() -> Vec<(PathBuf, Vec<Located>)> {
    let root = Path::new("tests/fixtures/diags");
    let fixtures = common::fixtures(root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures under {} — did the directory move?",
        root.display()
    );
    fixtures
        .into_iter()
        .map(|fixture| {
            let diags = diagnostics_of(&fixture);
            (fixture, diags)
        })
        .collect()
}

#[test]
fn diagnostic_snapshots() {
    let mut failures = Vec::new();
    for (fixture, diags) in corpus() {
        if let Err(msg) = common::check_snapshot(&fixture, &render(&diags)) {
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
    for (fixture, diags) in corpus() {
        if diags.is_empty() {
            silent.push(fixture.display().to_string());
        }
    }
    assert!(
        silent.is_empty(),
        "these fixtures no longer produce a diagnostic:\n  {}",
        silent.join("\n  ")
    );
}

/// M7's "every error explains itself", as a gate: whatever a fixture makes the
/// compiler say, the reader gets a `help:` line telling them what to do about
/// it. Site help or the code's fallback both count — the reader cannot tell
/// them apart, and should not have to.
#[test]
fn every_rendered_diagnostic_explains_itself() {
    let mut mute = Vec::new();
    for (fixture, diags) in corpus() {
        for Located { diag, .. } in diags {
            if diag.help_text().is_none() {
                mute.push(format!(
                    "{} [{}] {}",
                    fixture.display(),
                    diag.code,
                    diag.message
                ));
            }
        }
    }
    assert!(
        mute.is_empty(),
        "these diagnostics render without a `help:` line — give the emission \
         site help, or add fallback help for the code in wscript-core's \
         `CODES`:\n  {}",
        mute.join("\n  ")
    );
}

/// The corpus's own coverage, checked against the code registry: every code
/// wscript can emit is rendered by some fixture, unless the registry records
/// why it cannot be.
///
/// This is the gate that makes the corpus a corpus rather than a pile of
/// examples — without it, a new code ships untested and nobody notices.
#[test]
fn every_code_is_covered_by_a_fixture() {
    use wscript_core::diag::{CODES, Coverage};

    let mut seen: Vec<&str> = Vec::new();
    for (_fixture, diags) in corpus() {
        for Located { diag, .. } in diags {
            if !seen.contains(&diag.code) {
                seen.push(diag.code);
            }
        }
    }

    let mut missing = Vec::new();
    let mut stale_exemptions = Vec::new();
    for info in CODES {
        match (info.coverage, seen.contains(&info.code)) {
            (Coverage::Fixture, false) => missing.push(info.code),
            (Coverage::Exempt(_), true) => stale_exemptions.push(info.code),
            _ => {}
        }
    }

    assert!(
        missing.is_empty(),
        "no fixture under tests/fixtures/diags/ produces these codes — add one, \
         or record in wscript-core's `CODES` why none can:\n  {}",
        missing.join(" ")
    );
    assert!(
        stale_exemptions.is_empty(),
        "these codes are exempted in wscript-core's `CODES` but a fixture does \
         produce them — drop the exemption:\n  {}",
        stale_exemptions.join(" ")
    );
}

//! Pretty terminal rendering of diagnostics (ariadne), colors when TTY
//! (PRD §8).

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use wisp::{Diagnostic, RuntimeError, Severity};

fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

pub fn render(path: &str, source: &str, diags: &[Diagnostic]) {
    if diags.is_empty() {
        return;
    }
    let colored = is_tty();
    for d in diags {
        let (kind, color) = match d.severity {
            Severity::Error => (ReportKind::Error, Color::Red),
            Severity::Warning => (ReportKind::Warning, Color::Yellow),
        };
        let span = clamp_span(source, d.span.lo as usize, d.span.hi as usize);
        let mut report = Report::<(&str, std::ops::Range<usize>)>::build(kind, path, span.start)
            .with_code(d.code)
            .with_message(&d.message)
            .with_config(Config::default().with_color(colored))
            .with_label(
                Label::new((path, span))
                    .with_message(&d.message)
                    .with_color(color),
            );
        for (lspan, lmsg) in &d.labels {
            let lspan = clamp_span(source, lspan.lo as usize, lspan.hi as usize);
            report = report.with_label(
                Label::new((path, lspan))
                    .with_message(lmsg)
                    .with_color(Color::Blue),
            );
        }
        if let Some(help) = &d.help {
            report = report.with_help(help);
        }
        let _ = report
            .finish()
            .eprint((path, Source::from(source)));
    }
}

pub fn render_runtime(path: &str, source: &str, e: &RuntimeError) {
    let colored = is_tty();
    match e.span {
        Some(span) => {
            let span = clamp_span(source, span.lo as usize, span.hi as usize);
            let mut report = Report::<(&str, std::ops::Range<usize>)>::build(ReportKind::Error, path, span.start)
                .with_message(&e.message)
                .with_config(Config::default().with_color(colored))
                .with_label(
                    Label::new((path, span))
                        .with_message("fault raised here")
                        .with_color(Color::Red),
                );
            if !e.trace.is_empty() {
                report = report.with_note(format!("call stack: {}", e.trace.join(" ← ")));
            }
            let _ = report.finish().eprint((path, Source::from(source)));
        }
        None => {
            eprintln!("error: {e}");
        }
    }
}

/// Ariadne panics on out-of-range spans; clamp defensively.
fn clamp_span(source: &str, lo: usize, hi: usize) -> std::ops::Range<usize> {
    let len = source.len();
    let lo = lo.min(len);
    let hi = hi.clamp(lo, len);
    lo..hi
}

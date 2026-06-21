//! Pretty terminal rendering of diagnostics (ariadne), colors when TTY
//! (PRD §8).

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use wscript::{Diagnostic, RuntimeError, Severity};

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
        if let Some(help) = d
            .help
            .clone()
            .or_else(|| wscript::diag_default_help(d.code).map(String::from))
        {
            report = report.with_help(help);
        }
        let _ = report.finish().eprint((path, Source::from(source)));
    }
}

pub fn render_runtime(path: &str, source: &str, e: &RuntimeError) {
    let colored = is_tty();
    match e.span {
        Some(span) => {
            let cspan = clamp_span(source, span.lo as usize, span.hi as usize);
            let report = Report::<(&str, std::ops::Range<usize>)>::build(
                ReportKind::Error,
                path,
                cspan.start,
            )
            .with_message(&e.message)
            .with_config(Config::default().with_color(colored))
            .with_label(
                Label::new((path, cspan))
                    .with_message("fault raised here")
                    .with_color(Color::Red),
            );
            let _ = report.finish().eprint((path, Source::from(source)));
            render_trace(path, source, e);
        }
        None => {
            eprintln!("error: {e}");
            render_trace(path, source, e);
        }
    }
}

/// Print the script-level stack trace, one frame per line with its
/// `file:line:col`. Innermost frame first. Deep traces are elided in the
/// middle to keep output readable.
fn render_trace(path: &str, source: &str, e: &RuntimeError) {
    if e.trace.is_empty() {
        return;
    }

    /// How many head/tail frames to keep when eliding a deep trace.
    const EDGE: usize = 10;

    let loc = |frame: &wscript::TraceFrame| -> String {
        match frame.span {
            Some(span) => {
                let (line, col) = line_col(source, span.lo as usize);
                format!("{path}:{line}:{col}")
            }
            None => "<unknown location>".to_string(),
        }
    };

    // Decide which frames to show, and pad function names within that set.
    let n = e.trace.len();
    let shown: Vec<usize> = if n > EDGE * 2 + 1 {
        (0..EDGE).chain(n - EDGE..n).collect()
    } else {
        (0..n).collect()
    };
    let name_width = shown
        .iter()
        .map(|&i| e.trace[i].function.len())
        .max()
        .unwrap_or(0);

    eprintln!("stack trace (most recent call first):");
    if n > EDGE * 2 + 1 {
        for &i in &shown[..EDGE] {
            let f = &e.trace[i];
            eprintln!("  at {:<name_width$}  {}", f.function, loc(f));
        }
        eprintln!("  … {} frames omitted …", n - EDGE * 2);
        for &i in &shown[EDGE..] {
            let f = &e.trace[i];
            eprintln!("  at {:<name_width$}  {}", f.function, loc(f));
        }
    } else {
        for f in &e.trace {
            eprintln!("  at {:<name_width$}  {}", f.function, loc(f));
        }
    }
}

/// 1-based line and column (column counted in characters) for a byte
/// offset. Shares the newline-counting logic with the LSP position mapper.
pub(crate) fn line_col(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Ariadne panics on out-of-range spans; clamp defensively.
fn clamp_span(source: &str, lo: usize, hi: usize) -> std::ops::Range<usize> {
    let len = source.len();
    let lo = lo.min(len);
    let hi = hi.clamp(lo, len);
    lo..hi
}

//! Pretty terminal rendering of diagnostics (ariadne), colours when TTY
//! (PRD §8).
//!
//! Output goes to a sink held by the [`Renderer`] rather than straight to
//! stderr. That is what lets the `// error:` script tests assert on
//! rendered output in-process, and it gives the pure helpers below
//! (`line_col`, `clamp_span`, `file_idx`) a seam to be tested through.

use std::io::Write;

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use wscript::{Diagnostic, RuntimeError, Severity};

/// Renders diagnostics and faults to a sink.
///
/// Colour is a field rather than a call to `stderr().is_terminal()` inside
/// each render: a captured sink is not a terminal, and inferring it from a
/// global would put escape codes in every assertion.
pub struct Renderer<'w> {
    w: Box<dyn Write + 'w>,
    color: bool,
}

impl Renderer<'static> {
    /// The binary's renderer: stderr, coloured when it is a terminal.
    pub fn stderr() -> Renderer<'static> {
        use std::io::IsTerminal;
        Renderer {
            w: Box::new(std::io::stderr()),
            color: std::io::stderr().is_terminal(),
        }
    }
}

/// So callers can `writeln!(renderer, ...)` for the plain-text lines that
/// sit alongside rendered reports (stack traces, `error: ...`).
impl Write for Renderer<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.w.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.w.flush()
    }
}

impl<'w> Renderer<'w> {
    /// Render to `w` without colour — for tests and for hosts capturing
    /// diagnostics as text.
    pub fn plain(w: &'w mut dyn Write) -> Renderer<'w> {
        Renderer {
            w: Box::new(w),
            color: false,
        }
    }

    pub fn render(&mut self, path: &str, source: &str, diags: &[Diagnostic]) {
        if diags.is_empty() {
            return;
        }
        for d in diags {
            let (kind, color) = match d.severity {
                Severity::Error => (ReportKind::Error, Color::Red),
                Severity::Warning => (ReportKind::Warning, Color::Yellow),
            };
            let span = clamp_span(source, d.span.lo as usize, d.span.hi as usize);
            let mut report = Report::build(kind, (path, span.clone()))
                .with_code(d.code)
                .with_message(&d.message)
                .with_config(Config::default().with_color(self.color))
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
            if let Some(help) = d.help_text() {
                report = report.with_help(help);
            }
            let _ = report
                .finish()
                .write((path, Source::from(source)), &mut self.w);
        }
    }

    /// Multi-file variant of [`Renderer::render`]: each diagnostic renders
    /// against the file its span lands in (global span address space →
    /// `unit.source_map`).
    pub fn render_multi(
        &mut self,
        sources: &[(String, String)],
        map: &wscript::SourceMap,
        diags: &[Diagnostic],
    ) {
        for d in diags {
            let Some(fi) = file_idx(map, d.span.lo) else {
                self.render("<script>", "", std::slice::from_ref(d));
                continue;
            };
            let base = map.files[fi].base;
            let (path, source) = &sources[fi];
            let mut local = d.clone();
            local.span = span_shift_back(d.span, base);
            for (lspan, _) in &mut local.labels {
                *lspan = span_shift_back(*lspan, base);
            }
            let (path, source) = (path.clone(), source.clone());
            self.render(&path, &source, std::slice::from_ref(&local));
        }
    }

    /// Multi-file variant of [`Renderer::render_runtime`]: the fault site
    /// and every trace frame are mapped to their own file.
    pub fn render_runtime_multi(
        &mut self,
        sources: &[(String, String)],
        map: &wscript::SourceMap,
        e: &RuntimeError,
    ) {
        match e.span.and_then(|s| file_idx(map, s.lo).map(|fi| (s, fi))) {
            Some((span, fi)) => {
                let base = map.files[fi].base;
                let (path, source) = &sources[fi];
                let local = span_shift_back(span, base);
                let cspan = clamp_span(source, local.lo as usize, local.hi as usize);
                let report = Report::build(ReportKind::Error, (path.as_str(), cspan.clone()))
                    .with_message(&e.message)
                    .with_config(Config::default().with_color(self.color))
                    .with_label(
                        Label::new((path.as_str(), cspan))
                            .with_message("fault raised here")
                            .with_color(Color::Red),
                    );
                let _ = report
                    .finish()
                    .write((path.as_str(), Source::from(source.as_str())), &mut self.w);
            }
            None => {
                let _ = writeln!(self.w, "error: {e}");
            }
        }
        if e.trace.is_empty() {
            return;
        }
        let _ = writeln!(self.w, "stack trace (most recent call first):");
        let name_width = e.trace.iter().map(|f| f.function.len()).max().unwrap_or(0);
        for f in &e.trace {
            let loc = match f.span.and_then(|s| file_idx(map, s.lo).map(|fi| (s, fi))) {
                Some((span, fi)) => {
                    let base = map.files[fi].base;
                    let (path, source) = &sources[fi];
                    let (line, col) = line_col(source, (span.lo - base) as usize);
                    format!("{path}:{line}:{col}")
                }
                None => "<unknown location>".to_string(),
            };
            let _ = writeln!(self.w, "  at {:<name_width$}  {}", f.function, loc);
        }
    }

    pub fn render_runtime(&mut self, path: &str, source: &str, e: &RuntimeError) {
        match e.span {
            Some(span) => {
                let cspan = clamp_span(source, span.lo as usize, span.hi as usize);
                let report = Report::build(ReportKind::Error, (path, cspan.clone()))
                    .with_message(&e.message)
                    .with_config(Config::default().with_color(self.color))
                    .with_label(
                        Label::new((path, cspan))
                            .with_message("fault raised here")
                            .with_color(Color::Red),
                    );
                let _ = report
                    .finish()
                    .write((path, Source::from(source)), &mut self.w);
            }
            None => {
                let _ = writeln!(self.w, "error: {e}");
            }
        }
        self.render_trace(path, source, e);
    }

    /// Print the script-level stack trace, one frame per line with its
    /// `file:line:col`. Innermost frame first. Deep traces are elided in
    /// the middle to keep output readable.
    fn render_trace(&mut self, path: &str, source: &str, e: &RuntimeError) {
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

        // Decide which frames to show, and pad function names within that
        // set.
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

        let _ = writeln!(self.w, "stack trace (most recent call first):");
        if n > EDGE * 2 + 1 {
            for &i in &shown[..EDGE] {
                let f = &e.trace[i];
                let _ = writeln!(self.w, "  at {:<name_width$}  {}", f.function, loc(f));
            }
            let _ = writeln!(self.w, "  … {} frames omitted …", n - EDGE * 2);
            for &i in &shown[EDGE..] {
                let f = &e.trace[i];
                let _ = writeln!(self.w, "  at {:<name_width$}  {}", f.function, loc(f));
            }
        } else {
            for f in &e.trace {
                let _ = writeln!(self.w, "  at {:<name_width$}  {}", f.function, loc(f));
            }
        }
    }
}

/// Index of the file whose span range contains `offset`, in a multi-file
/// compilation's global span address space.
pub(crate) fn file_idx(map: &wscript::SourceMap, offset: u32) -> Option<usize> {
    let idx = map
        .files
        .partition_point(|f| f.base <= offset)
        .checked_sub(1)?;
    (offset <= map.files[idx].base + map.files[idx].len).then_some(idx)
}

/// Rebase a global span onto its own file.
pub(crate) fn span_shift_back(span: wscript::Span, base: u32) -> wscript::Span {
    wscript::Span::new(span.lo.saturating_sub(base), span.hi.saturating_sub(base))
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
pub(crate) fn clamp_span(source: &str, lo: usize, hi: usize) -> std::ops::Range<usize> {
    let len = source.len();
    let lo = lo.min(len);
    let hi = hi.clamp(lo, len);
    lo..hi
}

#[cfg(test)]
mod tests {
    use super::*;
    use wscript::{SourceFileInfo, SourceMap, Span};

    #[test]
    fn line_col_is_one_based_and_counts_characters() {
        let src = "ab\ncd\n";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 1), (1, 2));
        // The newline itself still reports on its own line.
        assert_eq!(line_col(src, 2), (1, 3));
        assert_eq!(line_col(src, 3), (2, 1));
        assert_eq!(line_col(src, 4), (2, 2));
    }

    /// Columns count characters, not bytes — otherwise a diagnostic after
    /// any non-ASCII text points to the wrong column.
    #[test]
    fn line_col_counts_multi_byte_characters_once() {
        let src = "héllo";
        assert_eq!(src.len(), 6, "é is two bytes");
        assert_eq!(line_col(src, 6), (1, 6), "five characters in");
    }

    #[test]
    fn line_col_past_the_end_saturates_at_the_last_position() {
        let src = "ab";
        assert_eq!(line_col(src, 999), (1, 3));
    }

    #[test]
    fn clamp_span_passes_through_a_valid_range() {
        assert_eq!(clamp_span("hello", 1, 4), 1..4);
    }

    /// The reason this function exists: ariadne panics on out-of-range
    /// spans, and a recovery-produced span can point past the source.
    #[test]
    fn clamp_span_bounds_an_overrunning_span() {
        assert_eq!(clamp_span("hello", 2, 99), 2..5);
        assert_eq!(clamp_span("hello", 99, 120), 5..5);
    }

    #[test]
    fn clamp_span_repairs_an_inverted_span() {
        assert_eq!(clamp_span("hello", 4, 1), 4..4, "hi is clamped up to lo");
    }

    fn map_of(files: &[(&str, u32, u32)]) -> SourceMap {
        SourceMap {
            files: files
                .iter()
                .map(|(path, base, len)| SourceFileInfo {
                    path: (*path).to_string(),
                    base: *base,
                    len: *len,
                })
                .collect(),
        }
    }

    #[test]
    fn file_idx_finds_the_only_file() {
        let map = SourceMap::single("a.wscript", 10);
        assert_eq!(file_idx(&map, 0), Some(0));
        assert_eq!(file_idx(&map, 10), Some(0));
        assert_eq!(file_idx(&map, 11), None, "past the end");
    }

    /// Files occupy disjoint ranges with a one-byte gap; a span must land
    /// in the file that owns it, not its neighbour.
    #[test]
    fn file_idx_picks_the_owning_file_across_boundaries() {
        let map = map_of(&[("a", 0, 10), ("b", 11, 5), ("c", 17, 3)]);
        assert_eq!(file_idx(&map, 0), Some(0));
        assert_eq!(file_idx(&map, 10), Some(0));
        assert_eq!(file_idx(&map, 11), Some(1));
        assert_eq!(file_idx(&map, 16), Some(1));
        assert_eq!(file_idx(&map, 17), Some(2));
        assert_eq!(file_idx(&map, 20), Some(2));
        assert_eq!(file_idx(&map, 21), None);
    }

    #[test]
    fn file_idx_on_an_empty_map_is_none() {
        assert_eq!(file_idx(&SourceMap::default(), 0), None);
    }

    #[test]
    fn span_shift_back_rebases_onto_the_file() {
        let s = span_shift_back(Span::new(14, 18), 11);
        assert_eq!((s.lo, s.hi), (3, 7));
    }

    #[test]
    fn span_shift_back_saturates_rather_than_underflowing() {
        let s = span_shift_back(Span::new(2, 4), 11);
        assert_eq!((s.lo, s.hi), (0, 0));
    }

    #[test]
    fn a_captured_renderer_emits_no_escape_codes() {
        let src = "fn main() { }";
        let diag = Diagnostic::error("E0001", Span::new(3, 7), "something went wrong")
            .with_help("try the other thing");
        let mut buf: Vec<u8> = Vec::new();
        Renderer::plain(&mut buf).render("t.wscript", src, std::slice::from_ref(&diag));

        let text = String::from_utf8(buf).expect("utf-8");
        assert!(text.contains("E0001"), "{text}");
        assert!(text.contains("something went wrong"), "{text}");
        assert!(text.contains("try the other thing"), "{text}");
        assert!(
            !text.contains('\u{1b}'),
            "captured output must be colour-free:\n{text}"
        );
    }

    /// A code with no site-specific help falls back to `default_help`, so
    /// the rendered text is what a user actually reads.
    #[test]
    fn help_falls_back_to_the_default_table() {
        let diag = Diagnostic::error("E0001", Span::new(0, 1), "unterminated block comment");
        let mut buf: Vec<u8> = Vec::new();
        Renderer::plain(&mut buf).render("t.wscript", "/*", std::slice::from_ref(&diag));
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("close the comment"), "{text}");
    }
}

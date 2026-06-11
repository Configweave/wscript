use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// A structured diagnostic (PRD §5.1): code, span, message, optional help.
///
/// Rendered prettily by the CLI and consumed raw by the LSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable diagnostic code, e.g. `E0003`.
    pub code: &'static str,
    pub severity: Severity,
    /// Primary span the diagnostic points at.
    pub span: Span,
    pub message: String,
    /// Extra labelled spans (secondary notes attached to source locations).
    pub labels: Vec<(Span, String)>,
    /// A "help:" suggestion shown under the diagnostic.
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code,
            severity: Severity::Error,
            span,
            message: message.into(),
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn warning(code: &'static str, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(code, span, message)
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Diagnostic {
        self.help = Some(help.into());
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push((span, message.into()));
        self
    }
}

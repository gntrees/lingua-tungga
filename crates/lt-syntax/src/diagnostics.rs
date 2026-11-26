//! Diagnostics shared by the lexer and the parser.

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub line: u32,
    pub col: u32,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span, line: u32, col: u32) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            line,
            col,
        }
    }

    /// Render as a `file:line:col` diagnostic with a source snippet and caret.
    pub fn render(&self, file: &str, src: &str) -> String {
        let line_text = src.lines().nth(self.line as usize - 1).unwrap_or("");
        let mut caret_pad = String::new();
        for _ in 1..self.col {
            caret_pad.push(' ');
        }
        let width = self.span.len().max(1);
        let caret = "^".repeat(width.min(line_text.chars().count().max(1)));
        format!(
            "error: {}\n  --> {}:{}:{}\n   |\n{:>3} | {}\n   | {}{}",
            self.message, file, self.line, self.col, self.line, line_text, caret_pad, caret
        )
    }
}

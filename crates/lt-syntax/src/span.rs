//! Byte spans, plus a human-readable line/column pair computed from offsets.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
}

/// 1-based line and column, counted in characters (not bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Compute the 1-based line/column of a byte offset.
pub fn line_col_at(src: &str, offset: usize) -> LineCol {
    let offset = offset.min(src.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (idx, ch) in src.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    LineCol { line, col }
}

//! Source ranges for Obsidian-style callouts, supplied by the Markdown parser.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Note,
    Abstract,
    Info,
    Todo,
    Tip,
    Success,
    Question,
    Warning,
    Failure,
    Danger,
    Bug,
    Example,
    Quote,
}

impl Kind {
    pub const ALL: [Self; 13] = [
        Self::Note,
        Self::Abstract,
        Self::Info,
        Self::Todo,
        Self::Tip,
        Self::Success,
        Self::Question,
        Self::Warning,
        Self::Failure,
        Self::Danger,
        Self::Bug,
        Self::Example,
        Self::Quote,
    ];

    pub fn parse(identifier: &str) -> Self {
        match identifier.to_ascii_lowercase().as_str() {
            "abstract" | "summary" | "tldr" => Self::Abstract,
            "info" => Self::Info,
            "todo" => Self::Todo,
            "tip" | "hint" | "important" => Self::Tip,
            "success" | "check" | "done" => Self::Success,
            "question" | "help" | "faq" => Self::Question,
            "warning" | "caution" | "attention" => Self::Warning,
            "failure" | "fail" | "missing" => Self::Failure,
            "danger" | "error" => Self::Danger,
            "bug" => Self::Bug,
            "example" => Self::Example,
            "quote" | "cite" => Self::Quote,
            _ => Self::Note,
        }
    }

    pub fn identifier(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Abstract => "abstract",
            Self::Info => "info",
            Self::Todo => "todo",
            Self::Tip => "tip",
            Self::Success => "success",
            Self::Question => "question",
            Self::Warning => "warning",
            Self::Failure => "failure",
            Self::Danger => "danger",
            Self::Bug => "bug",
            Self::Example => "example",
            Self::Quote => "quote",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Callout {
    pub range: Range<usize>,
    pub header: Range<usize>,
    pub marker: Range<usize>,
    pub body: Range<usize>,
    pub kind: Kind,
    pub identifier: String,
    pub title: String,
    /// None is not foldable; otherwise true means collapsed by default.
    pub folded: Option<bool>,
    snapshot: String,
}

impl Callout {
    /// Only call this for a BlockQuote range emitted by pulldown-cmark.
    pub fn from_range(source: &str, range: Range<usize>) -> Option<Self> {
        let quote = source.get(range.clone())?;
        let line_end = quote
            .find('\n')
            .map_or(range.end, |end| range.start + end + 1);
        let first = source[range.start..line_end].trim_end_matches(['\r', '\n']);
        let content = first.strip_prefix('>')?.trim_start_matches([' ', '\t']);
        let identifier = content.strip_prefix("[!")?.split_once(']')?.0;
        if identifier.is_empty()
            || !identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return None;
        }
        let marker_start = range.start + first.len() - content.len();
        let marker = marker_start..marker_start + identifier.len() + 3;
        let mut after = &source[marker.end..range.start + first.len()];
        let folded = match after.as_bytes().first() {
            Some(b'+') => {
                after = &after[1..];
                Some(false)
            }
            Some(b'-') => {
                after = &after[1..];
                Some(true)
            }
            _ => None,
        };
        let title = after.trim();
        let title = if title.is_empty() {
            let mut characters = identifier.chars();
            format!(
                "{}{}",
                characters.next()?.to_uppercase(),
                characters.as_str().to_lowercase()
            )
        } else {
            title.to_owned()
        };
        Some(Self {
            snapshot: quote.to_owned(),
            header: range.start..line_end,
            body: line_end..range.end,
            marker,
            kind: Kind::parse(identifier),
            identifier: identifier.to_ascii_lowercase(),
            title,
            folded,
            range,
        })
    }

    /// Reject stale UI state before replacing only the type marker.
    pub fn change_kind(&self, source: &mut String, kind: Kind) -> bool {
        if source.get(self.range.clone()) != Some(self.snapshot.as_str()) {
            return false;
        }
        source.replace_range(self.marker.clone(), &format!("[!{}]", kind.identifier()));
        true
    }
}

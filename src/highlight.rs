//! Portable highlight markup shared by Markdown parsing and editor commands.
pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Cyan,
    Green,
    Orange,
    Red,
    Yellow,
}

impl Color {
    pub const ALL: [Self; 5] = [
        Self::Cyan,
        Self::Green,
        Self::Orange,
        Self::Red,
        Self::Yellow,
    ];

    pub const fn command(self) -> &'static str {
        match self {
            Self::Cyan => "Highlight Cyan",
            Self::Green => "Highlight Green",
            Self::Orange => "Highlight Orange",
            Self::Red => "Highlight Red",
            Self::Yellow => "Highlight Yellow",
        }
    }

    pub fn rgba(self) -> Rgba {
        match self {
            Self::Cyan => [0, 191, 188, 65],
            Self::Green => [68, 207, 110, 65],
            Self::Orange => [255, 170, 64, 65],
            Self::Red => [251, 70, 76, 65],
            Self::Yellow => [255, 218, 0, 65],
        }
    }

    pub fn markers(self) -> (&'static str, &'static str) {
        let opening = match self {
            Self::Cyan => "<mark style=\"background: #00bfbc41;\">",
            Self::Green => "<mark style=\"background: #44cf6e41;\">",
            Self::Orange => "<mark style=\"background: #ffaa4041;\">",
            Self::Red => "<mark style=\"background: #fb464c41;\">",
            Self::Yellow => return ("==", "=="),
        };
        (opening, "</mark>")
    }
}

/// Read a mark's inert background through the shared inline-style parser.
pub fn mark_color(tag: &str) -> Option<Rgba> {
    if let Some(crate::inline_style::Tag::Open(crate::inline_style::Element::Mark, style)) =
        crate::inline_style::parse(tag)
    {
        style.background
    } else {
        None
    }
}

/// Core Obsidian 1.14.4 color markers. Keep portable Highlightr colors above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeColor {
    Blue,
    Green,
    Orange,
    Purple,
    Red,
    Yellow,
}

impl NativeColor {
    pub const ALL: [Self; 6] = [
        Self::Blue,
        Self::Green,
        Self::Orange,
        Self::Purple,
        Self::Red,
        Self::Yellow,
    ];

    pub const fn command(self) -> &'static str {
        match self {
            Self::Blue => "Toggle blue highlight",
            Self::Green => "Toggle green highlight",
            Self::Orange => "Toggle orange highlight",
            Self::Purple => "Toggle purple highlight",
            Self::Red => "Toggle red highlight",
            Self::Yellow => "Toggle yellow highlight",
        }
    }

    pub const fn opening(self) -> &'static str {
        match self {
            Self::Blue => "==🔵",
            Self::Green => "==🟢",
            Self::Orange => "==🟠",
            Self::Purple => "==🟣",
            Self::Red => "==🔴",
            Self::Yellow => "==🟡",
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::Blue => "<mark data-highlight=\"blue\">",
            Self::Green => "<mark data-highlight=\"green\">",
            Self::Orange => "<mark data-highlight=\"orange\">",
            Self::Purple => "<mark data-highlight=\"purple\">",
            Self::Red => "<mark data-highlight=\"red\">",
            Self::Yellow => "<mark data-highlight=\"yellow\">",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.tag() == tag)
    }

    pub fn prefix(content: &str) -> Option<(Self, usize)> {
        Self::ALL.into_iter().find_map(|color| {
            let marker = &color.opening()[2..];
            content.starts_with(marker).then_some((color, marker.len()))
        })
    }

    // Precomputed from actual theme-specific computed Oklch colors, not at paint time.
    pub const fn rgba(self, dark: bool) -> Rgba {
        match (self, dark) {
            (Self::Blue, true) => [2, 122, 255, 77],
            (Self::Blue, false) => [8, 109, 221, 77],
            (Self::Green, true) => [68, 207, 110, 77],
            (Self::Green, false) => [8, 185, 78, 77],
            (Self::Orange, true) => [233, 151, 63, 77],
            (Self::Orange, false) => [236, 117, 0, 77],
            (Self::Purple, true) => [168, 130, 255, 77],
            (Self::Purple, false) => [120, 82, 238, 77],
            (Self::Red, true) => [251, 70, 76, 77],
            (Self::Red, false) => [233, 49, 71, 77],
            (Self::Yellow, true) => [224, 222, 113, 77],
            (Self::Yellow, false) => [224, 172, 0, 77],
        }
    }
}

/// Toggle a selected span or the word at the caret, retaining Unicode character offsets.
pub fn toggle_native(
    text: &mut String,
    selection: std::ops::Range<usize>,
    color: NativeColor,
) -> std::ops::Range<usize> {
    let characters: Vec<_> = text.chars().collect();
    let offsets: Vec<_> = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([text.len()])
        .collect();
    let Some((&start, &end)) = offsets.get(selection.start).zip(offsets.get(selection.end)) else {
        return selection;
    };
    if start > end {
        return selection;
    }
    let parsed = crate::editor::Markdown::parse(text);
    if let Some((range, content, old)) = parsed.highlights.into_iter().find_map(|range| {
        let old = NativeColor::prefix(&text[range.start + 2..range.end - 2]);
        let content = range.start + 2 + old.map_or(0, |(_, size)| size)..range.end - 2;
        (content.start <= start && end <= content.end).then_some((
            range,
            content,
            old.map(|(old, _)| old),
        ))
    }) {
        let outer = text[..range.start].chars().count();
        let content_start = text[..content.start].chars().count();
        let before = &text[content.start..start];
        let selected = &text[start..end];
        let after = &text[end..content.end];
        let opening = old.map_or("==", NativeColor::opening);
        if selection.is_empty() {
            let next_opening = if old == Some(color) {
                ""
            } else {
                color.opening()
            };
            let closing = if next_opening.is_empty() { "" } else { "==" };
            let replacement = format!("{next_opening}{}{closing}", &text[content.clone()]);
            let caret = outer + next_opening.chars().count() + selection.start - content_start;
            text.replace_range(range, &replacement);
            return caret..caret;
        }
        let mut replacement = String::new();
        if !before.is_empty() {
            replacement.push_str(opening);
            replacement.push_str(before);
            replacement.push_str("==");
        }
        let next_opening = if old == Some(color) {
            ""
        } else {
            color.opening()
        };
        replacement.push_str(next_opening);
        let first = outer + replacement.chars().count();
        replacement.push_str(selected);
        let last = first + selected.chars().count();
        if !next_opening.is_empty() {
            replacement.push_str("==");
        }
        if !after.is_empty() {
            replacement.push_str(opening);
            replacement.push_str(after);
            replacement.push_str("==");
        }
        text.replace_range(range, &replacement);
        return first..last;
    }
    let mut word = selection.clone();
    if word.is_empty() {
        let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '\u{200c}');
        while word.start > 0 && is_word(characters[word.start - 1]) {
            word.start -= 1;
        }
        while word.end < characters.len() && is_word(characters[word.end]) {
            word.end += 1;
        }
    }
    let new = crate::editor::wrap(text, word, color.opening(), "==");
    if selection.is_empty() {
        let caret = selection.start + color.opening().chars().count();
        caret..caret
    } else {
        new
    }
}

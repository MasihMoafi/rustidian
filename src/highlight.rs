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

/// Read only a mark's hex background. No CSS or HTML is executed.
pub fn mark_color(tag: &str) -> Option<Rgba> {
    if tag == "<mark>" {
        return Some(Color::Yellow.rgba());
    }
    let attribute = tag.strip_prefix("<mark ")?.strip_suffix('>')?.trim();
    let value = attribute.strip_prefix("style=")?;
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let style = value.strip_prefix(quote)?.strip_suffix(quote)?.trim();
    let (property, value) = style.trim_end_matches(';').split_once(':')?;
    if !matches!(property.trim(), "background" | "background-color") {
        return None;
    }
    let hex = value.trim().strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
        return None;
    }
    let mut rgba = [0, 0, 0, 255];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        rgba[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(rgba)
}

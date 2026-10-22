//! A small, inert subset of inline HTML styling shared by both Markdown renderers.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    Mark,
    Span,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub color: Option<[u8; 4]>,
    pub background: Option<[u8; 4]>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<bool>,
}

impl Style {
    pub fn overlay(&mut self, other: Self) {
        self.color = other.color.or(self.color);
        self.background = other.background.or(self.background);
        self.bold = other.bold.or(self.bold);
        self.italic = other.italic.or(self.italic);
        self.strike = other.strike.or(self.strike);
        self.underline = other.underline.or(self.underline);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Open(Element, Style),
    Close(Element),
}

/// Read one complete mark/span tag. Attributes and CSS are never executed.
/// Native `data-highlight` markup remains owned by the theme-aware parser.
pub fn parse(tag: &str) -> Option<Tag> {
    let inner = tag.trim().strip_prefix('<')?.strip_suffix('>')?.trim();
    if let Some(closing) = inner.strip_prefix('/') {
        return element(closing.trim()).map(Tag::Close);
    }
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let element = element(&inner[..name_end])?;
    let mut attributes = inner[name_end..].trim_start();
    let mut style = Style {
        background: (element == Element::Mark).then(|| crate::highlight::Color::Yellow.rgba()),
        ..Style::default()
    };
    let mut has_style = false;
    while !attributes.is_empty() {
        let (attribute, value) = attributes.split_once('=')?;
        if !attribute.trim().eq_ignore_ascii_case("style") || has_style {
            return None;
        }
        has_style = true;
        let value = value.trim_start();
        let quote = value.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let value = &value[1..];
        let end = value.find(quote)?;
        apply_css(&mut style, &value[..end])?;
        let rest = &value[end + 1..];
        if rest.chars().next().is_some_and(|ch| !ch.is_whitespace()) {
            return None;
        }
        attributes = rest.trim_start();
    }
    Some(Tag::Open(element, style))
}

fn element(name: &str) -> Option<Element> {
    if name.eq_ignore_ascii_case("mark") {
        Some(Element::Mark)
    } else if name.eq_ignore_ascii_case("span") {
        Some(Element::Span)
    } else {
        None
    }
}

fn apply_css(style: &mut Style, css: &str) -> Option<()> {
    let compact: String = css
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if css.contains(['\\', '<', '>'])
        || [
            "url(",
            "expression(",
            "javascript:",
            "@import",
            "-moz-binding",
            "behavior:",
        ]
        .iter()
        .any(|pattern| compact.contains(*pattern))
    {
        return None;
    }
    for declaration in css.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (property, value) = declaration.split_once(':')?;
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        let value = value.strip_suffix("!important").unwrap_or(&value).trim();
        if property.is_empty() || value.is_empty() {
            return None;
        }
        let inherited = matches!(value, "inherit" | "initial" | "unset" | "revert");
        match property.as_str() {
            "color" => style.color = if inherited { None } else { Some(color(value)?) },
            "background" | "background-color" => {
                style.background = if inherited { None } else { Some(color(value)?) };
            }
            "font-weight" => {
                style.bold = if inherited {
                    None
                } else {
                    Some(match value {
                        "bold" | "bolder" => true,
                        "normal" | "lighter" => false,
                        _ => {
                            let weight: u16 = value.parse().ok()?;
                            if !(1..=1000).contains(&weight) {
                                return None;
                            }
                            weight >= 600
                        }
                    })
                };
            }
            "font-style" => {
                style.italic = if inherited {
                    None
                } else {
                    Some(match value {
                        "italic" | "oblique" => true,
                        "normal" => false,
                        _ => return None,
                    })
                };
            }
            "text-decoration" | "text-decoration-line" => {
                if inherited {
                    style.strike = None;
                    style.underline = None;
                    continue;
                }
                let mut underline = false;
                let mut strike = false;
                for token in value.split_whitespace() {
                    match token {
                        "underline" => underline = true,
                        "line-through" => strike = true,
                        "none" | "overline" => {}
                        _ => return None,
                    }
                }
                style.underline = Some(underline);
                style.strike = Some(strike);
            }
            // Layout declarations such as padding and border-radius have no native effect.
            _ => {}
        }
    }
    Some(())
}

/// Decode literal CSS hex, rgb and rgba colors without resolving CSS or URLs.
pub fn color(value: &str) -> Option<[u8; 4]> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("transparent") {
        return Some([0, 0, 0, 0]);
    }
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.is_ascii() || !matches!(hex.len(), 3 | 4 | 6 | 8) {
            return None;
        }
        let mut rgba = [0, 0, 0, 255];
        if hex.len() <= 4 {
            for (index, digit) in hex.chars().enumerate() {
                rgba[index] = digit.to_digit(16)? as u8 * 17;
            }
        } else {
            for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
                rgba[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
            }
        }
        return Some(rgba);
    }
    let (function, arguments) = value.split_once('(')?;
    let rgba = function.eq_ignore_ascii_case("rgba");
    if !rgba && !function.eq_ignore_ascii_case("rgb") {
        return None;
    }
    let arguments = arguments.strip_suffix(')')?;
    let (channels, alpha): (Vec<_>, _) = if arguments.contains(',') {
        let mut parts: Vec<_> = arguments.split(',').map(str::trim).collect();
        if parts.len() != if rgba { 4 } else { 3 } {
            return None;
        }
        let alpha = if rgba { parts.pop() } else { None };
        (parts, alpha)
    } else {
        let (channels, alpha) = arguments
            .split_once('/')
            .map_or((arguments, None), |(channels, alpha)| {
                (channels, Some(alpha))
            });
        (channels.split_whitespace().collect(), alpha)
    };
    if channels.len() != 3 {
        return None;
    }
    Some([
        component(channels[0], 255.0)?,
        component(channels[1], 255.0)?,
        component(channels[2], 255.0)?,
        alpha.map_or(Some(255), |alpha| component(alpha, 1.0))?,
    ])
}

fn component(value: &str, maximum: f64) -> Option<u8> {
    let value = value.trim();
    let (value, maximum) = value
        .strip_suffix('%')
        .map_or((value, maximum), |percent| (percent, 100.0));
    let value: f64 = value.parse().ok()?;
    if !value.is_finite() || !(0.0..=maximum).contains(&value) {
        return None;
    }
    Some((value / maximum * 255.0).round() as u8)
}

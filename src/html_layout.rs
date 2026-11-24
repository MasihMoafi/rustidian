//! The HTML that READMEs use for layout, read as GitHub shows it: `<img>` is an image,
//! `align="center"` (or `<center>`) centres what it wraps, and wrapper tags such as `<div>`,
//! `<p>`, `<picture>` or `<a>` are not text. Nothing is executed; unknown tags keep their text.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// One tag, its byte range within the scanned text, and its attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlTag {
    pub range: Range<usize>,
    pub name: String,
    pub closing: bool,
    pub attributes: Vec<(String, String)>,
}

impl HtmlTag {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Wrapper and layout tags: never shown as text.
    pub fn is_layout(&self) -> bool {
        matches!(
            self.name.as_str(),
            "div" | "p" | "center" | "picture" | "source" | "a" | "details" | "summary"
        )
    }

    pub fn is_image(&self) -> bool {
        self.name == "img" && !self.closing
    }

    pub fn alignment(&self) -> Option<Alignment> {
        match self.attribute("align").map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("center") => Some(Alignment::Center),
            Some(value) if value.eq_ignore_ascii_case("right") => Some(Alignment::Right),
            Some(value) if value.eq_ignore_ascii_case("left") => Some(Alignment::Left),
            _ if self.name == "center" => Some(Alignment::Center),
            _ => None,
        }
    }

    /// An image's size in pixels; a percentage or unit other than px is ignored.
    pub fn pixels(&self, name: &str) -> Option<f32> {
        let value = self.attribute(name)?.trim();
        let value = value.strip_suffix("px").unwrap_or(value);
        value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
    }

    pub fn length(&self, name: &str, available: f32) -> Option<f32> {
        let value = self.attribute(name)?.trim();
        if let Some(percent) = value.strip_suffix('%') {
            return percent
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(|value| available * value / 100.0);
        }
        self.pixels(name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Text(Range<usize>),
    Tag(HtmlTag),
    Comment(Range<usize>),
}

/// A balanced element in the original source. Offsets stay in UTF-8 bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub tag: HtmlTag,
    pub range: Range<usize>,
    pub body: Range<usize>,
}

/// Elements recognized by Markdown's HTML tokenizer, excluding code and properties.
pub fn elements(source: &str) -> Vec<Element> {
    use pulldown_cmark::{Event, Options, Parser};
    if !source.contains('<') {
        return Vec::new();
    }
    let body = crate::metadata::frontmatter(source)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let mut opened: Vec<HtmlTag> = Vec::new();
    let mut result = Vec::new();
    for (event, range) in Parser::new_ext(&source[body..], Options::ENABLE_MATH).into_offset_iter()
    {
        if !matches!(event, Event::Html(_) | Event::InlineHtml(_)) {
            continue;
        }
        let offset = body + range.start;
        for token in tokens(&source[offset..body + range.end]) {
            let Token::Tag(mut tag) = token else { continue };
            tag.range = offset + tag.range.start..offset + tag.range.end;
            if tag.closing {
                if let Some(index) = opened.iter().rposition(|open| open.name == tag.name) {
                    let open = opened.remove(index);
                    opened.truncate(index);
                    result.push(Element {
                        range: open.range.start..tag.range.end,
                        body: open.range.end..tag.range.start,
                        tag: open,
                    });
                }
            } else if matches!(
                tag.name.as_str(),
                "img" | "source" | "br" | "hr" | "input" | "wbr"
            ) || source[tag.range.clone()].trim_end().ends_with("/>")
            {
                result.push(Element {
                    range: tag.range.clone(),
                    body: tag.range.end..tag.range.end,
                    tag,
                });
            } else {
                opened.push(tag);
            }
        }
    }
    for tag in opened {
        // An unfinished media element still gets a card while it is being edited.
        if tag.name == "video" {
            result.push(Element {
                range: tag.range.clone(),
                body: tag.range.end..tag.range.end,
                tag,
            });
        }
    }
    result.sort_by_key(|element| (element.range.start, std::cmp::Reverse(element.range.end)));
    result
}

/// HTML requiring a native child layout rather than inline text shaping.
pub fn blocks(source: &str) -> Vec<Element> {
    outermost(elements(source).into_iter().filter(|element| {
        matches!(
            element.tag.name.as_str(),
            "table" | "picture" | "video" | "details"
        )
    }))
}

/// Live preview also renders complete HTML paragraphs and image links as children.
pub fn live_blocks(source: &str) -> Vec<Element> {
    outermost(elements(source).into_iter().filter(|element| {
        matches!(
            element.tag.name.as_str(),
            "table" | "picture" | "video" | "details" | "p" | "a"
        )
    }))
}

fn outermost(elements: impl Iterator<Item = Element>) -> Vec<Element> {
    let mut end = 0;
    elements
        .filter(|element| {
            if element.range.start < end {
                return false;
            }
            end = element.range.end;
            true
        })
        .collect()
}

/// Select the first supported source for the current appearance; fall back to `<img>`.
pub fn picture_image(source: &str, dark: bool) -> Option<HtmlTag> {
    let tags: Vec<_> = tokens(source)
        .into_iter()
        .filter_map(|token| match token {
            Token::Tag(tag) if !tag.closing => Some(tag),
            _ => None,
        })
        .collect();
    let mut image = tags.iter().find(|tag| tag.is_image())?.clone();
    for tag in tags.iter().filter(|tag| tag.name == "source") {
        if tag.attribute("type").is_some_and(|mime| {
            !matches!(
                mime,
                "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/svg+xml"
            )
        }) {
            continue;
        }
        let matches = match tag.attribute("media").map(str::trim) {
            None | Some("") => true,
            Some(media) => {
                let compact: String = media
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .flat_map(char::to_lowercase)
                    .collect();
                compact
                    == if dark {
                        "(prefers-color-scheme:dark)"
                    } else {
                        "(prefers-color-scheme:light)"
                    }
            }
        };
        if matches && let Some(srcset) = tag.attribute("srcset") {
            let candidate = if srcset.trim_start().starts_with("data:") {
                srcset
            } else {
                srcset.split(',').next()?
            };
            let src = candidate.split_whitespace().next()?;
            image.attributes.retain(|(name, _)| name != "src");
            image.attributes.push(("src".into(), src.into()));
            break;
        }
    }
    Some(image)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub tag: HtmlTag,
    pub source: String,
}

/// Cell bodies retain Markdown and nested HTML, so both views use the same renderer.
pub fn table_rows(source: &str) -> Vec<Vec<Cell>> {
    let all = elements(source);
    let rows = outermost(
        all.iter()
            .filter(|element| element.tag.name == "tr")
            .cloned(),
    );
    rows.into_iter()
        .map(|row| {
            // Keep the table's HTML context: reparsing an indented row body as
            // Markdown would classify its <td> lines as indented code.
            outermost(
                all.iter()
                    .filter(|element| {
                        row.body.start <= element.range.start
                            && element.range.end <= row.body.end
                            && matches!(element.tag.name.as_str(), "td" | "th")
                    })
                    .cloned(),
            )
            .into_iter()
            .map(|element| Cell {
                source: source[element.body].into(),
                tag: element.tag,
            })
            .collect()
        })
        .collect()
}

/// Split an HTML fragment into text, tags and comments.
pub fn tokens(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text_start = 0;
    let mut cursor = 0;
    while let Some(offset) = html[cursor..].find('<') {
        let start = cursor + offset;
        let found = if html[start..].starts_with("<!--") {
            html[start + 4..]
                .find("-->")
                .map(|end| (start + 4 + end + 3, None))
        } else if html[start..].starts_with("<!") || html[start..].starts_with("<?") {
            tag_end(html, start).map(|end| (end, None))
        } else {
            tag_end(html, start)
                .and_then(|end| parse_tag(&html[start..end], start).map(|tag| (end, Some(tag))))
        };
        let Some((end, tag)) = found else {
            cursor = start + 1;
            continue;
        };
        if text_start < start {
            tokens.push(Token::Text(text_start..start));
        }
        tokens.push(match tag {
            Some(tag) => Token::Tag(tag),
            None => Token::Comment(start..end),
        });
        text_start = end;
        cursor = end;
    }
    if text_start < html.len() {
        tokens.push(Token::Text(text_start..html.len()));
    }
    tokens
}

fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in html[start + 1..].char_indices() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(start + 1 + offset + 1),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

fn parse_tag(source: &str, start: usize) -> Option<HtmlTag> {
    let inner = source.strip_prefix('<')?.strip_suffix('>')?;
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, inner),
    };
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    let name_end = inner
        .find(|ch: char| ch.is_ascii_whitespace())
        .unwrap_or(inner.len());
    let name = &inner[..name_end];
    if !name.starts_with(|ch: char| ch.is_ascii_alphabetic())
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':'))
    {
        return None;
    }
    let mut attributes = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest
            .find(|ch: char| ch == '=' || ch.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let key = &rest[..key_end];
        if key.is_empty() {
            return None;
        }
        rest = rest[key_end..].trim_start();
        let value = if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            match after.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let end = after[1..].find(quote)? + 1;
                    rest = &after[end + 1..];
                    &after[1..end]
                }
                Some(_) => {
                    let end = after
                        .find(|ch: char| ch.is_ascii_whitespace())
                        .unwrap_or(after.len());
                    rest = &after[end..];
                    &after[..end]
                }
                None => return None,
            }
        } else {
            ""
        };
        attributes.push((key.to_ascii_lowercase(), decode(value)));
        rest = rest.trim_start();
    }
    Some(HtmlTag {
        range: start..start + source.len(),
        name: name.to_ascii_lowercase(),
        closing,
        attributes,
    })
}

/// Decode the character references READMEs use; others stay as written.
pub fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.into();
    }
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 32) else {
            output.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "middot" => Some('·'),
            "bull" => Some('•'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "copy" => Some('©'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                output.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                output.push('&');
                rest = &rest[1..];
            }
        }
    }
    output.push_str(rest);
    output
}

/// Whether the text has an `<img` tag in any letter case.
pub fn has_image_tag(text: &str) -> bool {
    text.as_bytes()
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"<img"))
}

/// Where a note's HTML lays itself out, in byte offsets of the note.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// `<img>` tags.
    pub images: Vec<HtmlTag>,
    /// Wrapper tags and comments, shown only while their line is being edited.
    pub hidden: Vec<Range<usize>>,
    /// Lines made only of wrapper tags and comments, each with its newline.
    pub tag_lines: Vec<Range<usize>>,
    /// Text inside `align="center"` or `<center>`.
    pub centered: Vec<Range<usize>>,
    pub right: Vec<Range<usize>>,
    /// Text inside `<b>`/`<strong>` and `<i>`/`<em>`; their tags are in `hidden`.
    pub bold: Vec<Range<usize>>,
    pub italic: Vec<Range<usize>>,
    pub scripts: Vec<(Range<usize>, i8)>,
    pub breaks: Vec<Range<usize>>,
}

/// Read the layout HTML of a Markdown note, skipping its properties and code.
pub fn scan(source: &str) -> Layout {
    use pulldown_cmark::{Event, Options, Parser};
    let mut layout = Layout::default();
    if !source.contains('<') {
        return layout;
    }
    let body = crate::metadata::frontmatter(source)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let mut open: Vec<(String, usize, Option<Alignment>)> = Vec::new();
    let mut emphasis: Vec<(String, usize)> = Vec::new();
    let parser = Parser::new_ext(
        &source[body..],
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_MATH,
    );
    for (event, range) in parser.into_offset_iter() {
        let block = match event {
            Event::Html(_) => true,
            Event::InlineHtml(_) => false,
            _ => continue,
        };
        let start = body + range.start;
        let fragment = &source[start..body + range.end];
        let mut line_start = start;
        let mut line_only_tags = true;
        let mut line_has_tag = false;
        for token in tokens(fragment) {
            match token {
                Token::Tag(mut tag) => {
                    tag.range = start + tag.range.start..start + tag.range.end;
                    if tag.is_image() {
                        line_only_tags = false;
                        layout.images.push(tag);
                    } else if tag.is_layout() {
                        line_has_tag = true;
                        layout.hidden.push(tag.range.clone());
                        if tag.closing {
                            if let Some(index) =
                                open.iter().rposition(|(name, _, _)| *name == tag.name)
                            {
                                let (_, opened, alignment) = open.remove(index);
                                open.truncate(index);
                                if alignment == Some(Alignment::Center) {
                                    layout.centered.push(opened..tag.range.end);
                                } else if alignment == Some(Alignment::Right) {
                                    layout.right.push(opened..tag.range.end);
                                }
                            }
                        } else if !matches!(tag.name.as_str(), "source") {
                            open.push((tag.name.clone(), tag.range.start, tag.alignment()));
                        }
                    } else if matches!(
                        tag.name.as_str(),
                        "b" | "strong" | "i" | "em" | "sub" | "sup"
                    ) {
                        layout.hidden.push(tag.range.clone());
                        if !tag.closing {
                            emphasis.push((tag.name.clone(), tag.range.end));
                        } else if let Some(index) =
                            emphasis.iter().rposition(|(name, _)| *name == tag.name)
                        {
                            let (name, opened) = emphasis.remove(index);
                            let inside = opened..tag.range.start;
                            if matches!(name.as_str(), "b" | "strong") {
                                layout.bold.push(inside);
                            } else if matches!(name.as_str(), "i" | "em") {
                                layout.italic.push(inside);
                            } else {
                                layout
                                    .scripts
                                    .push((inside, if name == "sup" { 1 } else { -1 }));
                            }
                        }
                    } else if tag.name == "br" {
                        layout.breaks.push(tag.range);
                        line_only_tags = false;
                    } else {
                        line_has_tag = true;
                        layout.hidden.push(tag.range);
                    }
                }
                Token::Comment(range) => {
                    line_has_tag = true;
                    layout.hidden.push(start + range.start..start + range.end);
                }
                Token::Text(range) => {
                    let text = &fragment[range.clone()];
                    let mut offset = range.start;
                    for piece in text.split_inclusive('\n') {
                        if !piece.trim().is_empty() {
                            line_only_tags = false;
                        }
                        offset += piece.len();
                        if piece.ends_with('\n') && block {
                            if line_only_tags && line_has_tag {
                                layout.tag_lines.push(line_start..start + offset);
                            }
                            line_start = start + offset;
                            line_only_tags = true;
                            line_has_tag = false;
                        }
                    }
                }
            }
        }
        if block && line_only_tags && line_has_tag && line_start < body + range.end {
            let end = body + range.end;
            let line_end = source[end..]
                .find('\n')
                .map_or(source.len(), |i| end + i + 1);
            if source[end..line_end].trim().is_empty() {
                layout.tag_lines.push(line_start..line_end);
            }
        }
    }
    // An unclosed centring element lasts to the end of the note, as in a browser.
    for (_, opened, alignment) in open {
        if alignment == Some(Alignment::Center) {
            layout.centered.push(opened..source.len());
        } else if alignment == Some(Alignment::Right) {
            layout.right.push(opened..source.len());
        }
    }
    layout.centered.sort_by_key(|range| range.start);
    layout.tag_lines.sort_by_key(|range| range.start);
    layout.tag_lines.dedup();
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    const README: &str = "---\nname: skills\n---\n\n<div align=\"center\">\n\n<img src=\"logo.png\" alt=\"Skills\" width=\"650\">\n\n<br>\n\n**Bold.**\n\n</div>\n\nAfter.\n";

    #[test]
    fn a_readme_header_centres_its_content_and_hides_its_wrappers() {
        let layout = scan(README);
        let image = &layout.images[0];
        assert_eq!(
            &README[image.range.clone()],
            "<img src=\"logo.png\" alt=\"Skills\" width=\"650\">"
        );
        assert_eq!(image.attribute("src"), Some("logo.png"));
        assert_eq!(image.pixels("width"), Some(650.0));
        let tagged: Vec<_> = layout
            .tag_lines
            .iter()
            .map(|range| &README[range.clone()])
            .collect();
        assert_eq!(tagged, ["<div align=\"center\">\n", "</div>\n"]);
        let centred = &README[layout.centered[0].clone()];
        assert!(
            centred.starts_with("<div")
                && centred.contains("**Bold.**")
                && centred.ends_with("</div>")
        );
        assert!(!centred.contains("After."), "centring stops at </div>");
    }

    #[test]
    fn bold_and_italic_tags_mark_their_text_and_hide_themselves() {
        let source = "<p align=\"center\"><b>The harness.</b></p>\n<p align=\"center\"><i>Be Elpis.</i></p>\n";
        let layout = scan(source);
        assert_eq!(&source[layout.bold[0].clone()], "The harness.");
        assert_eq!(&source[layout.italic[0].clone()], "Be Elpis.");
        let hidden: Vec<_> = layout
            .hidden
            .iter()
            .map(|range| &source[range.clone()])
            .collect();
        assert!(hidden.contains(&"<b>") && hidden.contains(&"</i>"));
    }

    #[test]
    fn code_stays_literal_and_unknown_elements_keep_their_text() {
        let source = "```html\n<div align=\"center\"><img src=\"x.png\"></div>\n```\n\n<custom-box>x</custom-box>\n";
        let layout = scan(source);
        assert!(layout.images.is_empty() && layout.centered.is_empty());
        let hidden: Vec<_> = layout
            .hidden
            .iter()
            .map(|range| &source[range.clone()])
            .collect();
        assert_eq!(hidden, ["<custom-box>", "</custom-box>"]);
        assert!(layout.tag_lines.is_empty());
    }

    #[test]
    fn a_one_line_centred_paragraph_hides_only_its_tags() {
        let source = "<p align=\"center\"><img src=\"a.gif\" alt=\"A\"></p>\n\nText.\n";
        let layout = scan(source);
        assert_eq!(layout.images.len(), 1);
        assert!(layout.tag_lines.is_empty(), "the line holds an image");
        assert_eq!(layout.hidden.len(), 2);
        assert_eq!(
            &source[layout.centered[0].clone()],
            "<p align=\"center\"><img src=\"a.gif\" alt=\"A\"></p>"
        );
    }

    #[test]
    fn tokens_read_quoted_attributes_and_entities() {
        let tokens = tokens("<a href='x?a=1&amp;b=2' title=\"a > b\">Go</a>");
        let Token::Tag(tag) = &tokens[0] else {
            panic!()
        };
        assert_eq!(tag.attribute("href"), Some("x?a=1&b=2"));
        assert_eq!(tag.attribute("title"), Some("a > b"));
        assert!(matches!(tokens[1], Token::Text(_)));
        assert!(matches!(&tokens[2], Token::Tag(tag) if tag.closing && tag.name == "a"));
        assert_eq!(decode("a &middot; b &#x263A; &bogus;"), "a · b ☺ &bogus;");
    }
}

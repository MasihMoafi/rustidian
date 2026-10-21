use rust_notes::{
    editor::{Markdown, surround, wrap},
    highlight::{Color, mark_color},
};

#[test]
fn highlight_ranges_keep_unicode_formatting_and_original_source() {
    let text = "Before ==فارسی **bold**== and ==second==.\r\n";
    let parsed = Markdown::parse(text);
    assert_eq!(parsed.highlights.len(), 2);
    assert_eq!(&text[parsed.highlights[0].clone()], "==فارسی **bold**==");
    assert!(
        parsed
            .spans
            .iter()
            .any(|span| span.style.highlight.is_some() && span.style.strong)
    );
    let mut edited = "before فارسی after".to_owned();
    let selected = surround(&mut edited, 7..12, "==");
    assert_eq!(edited, "before ==فارسی== after");
    assert_eq!(selected, 9..14);
}

#[test]
fn highlight_ignores_yaml_code_escapes_link_destinations_and_incomplete_pairs() {
    for text in [
        "---\nvalue: '==yaml=='\n---\n",
        "`==inline==`",
        "```md\n==fenced==\n```",
        "\\==escaped==",
        "===not a highlight===",
        "==open",
        "==one\n\ntwo==",
        "[Link](https://example.com/==literal==)",
    ] {
        assert!(Markdown::parse(text).highlights.is_empty(), "{text}");
    }
}

#[test]
fn five_colors_wrap_unicode_and_parse_without_changing_other_bytes() {
    for color in Color::ALL {
        let (opening, closing) = color.markers();
        let mut text = "before فارسی after\r\n".to_owned();
        let selection = wrap(&mut text, 7..12, opening, closing);
        assert_eq!(text, format!("before {opening}فارسی{closing} after\r\n"));
        let parsed = Markdown::parse(&text);
        let word = text.find("فارسی").unwrap();
        assert!(
            parsed.spans.iter().any(
                |span| span.range.contains(&word) && span.style.highlight == Some(color.rgba())
            )
        );
        let selection = wrap(&mut text, selection, opening, closing);
        assert_eq!(text, "before فارسی after\r\n");
        assert_eq!(selection, 7..12);
    }
}

#[test]
fn color_parser_rejects_executable_or_unknown_styles_and_ignores_code_and_yaml() {
    assert_eq!(
        mark_color("<mark style='background-color: #123456;'>"),
        Some([18, 52, 86, 255])
    );
    for tag in [
        "<mark onclick=\"alert(1)\">",
        "<mark style=\"background: url(x);\">",
        "<mark style=\"color: #123456;\">",
        "<mark style=\"background: #123456; color: red;\">",
    ] {
        assert_eq!(mark_color(tag), None);
    }
    let (opening, closing) = Color::Cyan.markers();
    for text in [
        format!("`{opening}code{closing}`"),
        format!("```md\n{opening}code{closing}\n```"),
        format!("---\nvalue: '{opening}yaml{closing}'\n---\n"),
    ] {
        assert!(
            Markdown::parse(&text)
                .spans
                .iter()
                .all(|span| span.style.highlight.is_none()),
            "{text}"
        );
    }
    let (inner, end) = Color::Red.markers();
    let text = format!("{opening}outer {inner}inner{end} outer{closing}");
    let parsed = Markdown::parse(&text);
    let inner_start = text.find("inner").unwrap();
    assert!(
        parsed
            .spans
            .iter()
            .any(|span| span.range.contains(&inner_start)
                && span.style.highlight == Some(Color::Red.rgba()))
    );
}

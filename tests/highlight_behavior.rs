use rustidian::{
    editor::{Markdown, wrap},
    highlight::Color,
    inline_style::{self, Element, Style, Tag},
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
fn mark_colors_default_to_yellow_nest_and_ignore_code_and_yaml() {
    assert_eq!(
        inline_style::parse("<mark style=\"color:#123456\">"),
        Some(Tag::Open(
            Element::Mark,
            Style {
                color: Some([18, 52, 86, 255]),
                background: Some(Color::Yellow.rgba()),
                ..Style::default()
            },
        ))
    );
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

#[test]
fn native_color_toggles_keep_unicode_selection_and_repeat_exactly() {
    use rustidian::highlight::{NativeColor, toggle_native};
    for color in NativeColor::ALL {
        let original = "Before فارسی TARGET after\r\n";
        let start = original[..original.find("TARGET").unwrap()].chars().count();
        let mut text = original.to_owned();
        let range = toggle_native(&mut text, start..start + 6, color);
        assert_eq!(
            text,
            original.replacen("TARGET", &format!("{}TARGET==", color.opening()), 1)
        );
        assert_eq!(
            text.chars()
                .skip(range.start)
                .take(range.end - range.start)
                .collect::<String>(),
            "TARGET"
        );
        let word = text.find("TARGET").unwrap();
        assert!(
            Markdown::parse(&text).spans.iter().any(
                |span| span.range.contains(&word) && span.style.native_highlight == Some(color)
            )
        );
        assert_eq!(toggle_native(&mut text, range, color), start..start + 6);
        assert_eq!(text, original);
    }
}

#[test]
fn native_caret_toggle_formats_the_word_and_retains_the_interior_caret() {
    use rustidian::highlight::{NativeColor, toggle_native};
    for caret in [3, 6] {
        let mut text = "TARGET".to_owned();
        let highlighted = toggle_native(&mut text, caret..caret, NativeColor::Blue);
        assert_eq!(text, "==🔵TARGET==");
        assert_eq!(highlighted, caret + 3..caret + 3);
        assert_eq!(
            toggle_native(&mut text, highlighted, NativeColor::Blue),
            caret..caret
        );
        assert_eq!(text, "TARGET");
    }
}

#[test]
fn native_color_changes_at_the_caret_do_not_insert_or_select_other_text() {
    use rustidian::highlight::{NativeColor, toggle_native};
    let mut text = "Before ==🔵TARGET==".to_owned();
    let caret = text[..text.find("T==").unwrap()].chars().count();
    assert_eq!(
        toggle_native(&mut text, caret..caret, NativeColor::Purple),
        caret..caret
    );
    assert_eq!(text, "Before ==🟣TARGET==");
}

#[test]
fn native_partial_color_change_splits_the_old_span_without_touching_its_neighbors() {
    use rustidian::highlight::{NativeColor, toggle_native};
    let mut text = "Before ==🔵TARGET== after\r\n".to_owned();
    let start = text[..text.find("RGE").unwrap()].chars().count();
    let selected = toggle_native(&mut text, start..start + 3, NativeColor::Purple);
    assert_eq!(text, "Before ==🔵TA====🟣RGE====🔵T== after\r\n");
    assert_eq!(
        text.chars()
            .skip(selected.start)
            .take(selected.end - selected.start)
            .collect::<String>(),
        "RGE"
    );
    let parsed = Markdown::parse(&text);
    assert_eq!(parsed.highlights.len(), 3);
    for (part, color) in [
        ("TA", NativeColor::Blue),
        ("RGE", NativeColor::Purple),
        ("T==", NativeColor::Blue),
    ] {
        let word = text.find(part).unwrap();
        assert!(
            parsed.spans.iter().any(
                |span| span.range.contains(&word) && span.style.native_highlight == Some(color)
            ),
            "{part}"
        );
    }
    assert!(
        !parsed
            .spans
            .iter()
            .any(|span| !span.style.hidden && text[span.range.clone()].contains(['🔵', '🟣']))
    );
}

#[test]
fn yellow_native_marker_is_distinct_from_the_generic_yellow_highlight() {
    use rustidian::highlight::{NativeColor, toggle_native};
    let mut text = "Before ==TARGET==".to_owned();
    let selected = toggle_native(&mut text, 9..15, NativeColor::Yellow);
    assert_eq!(text, "Before ==🟡TARGET==");
    assert_eq!(selected, 10..16);
    assert_eq!(
        toggle_native(&mut text, selected, NativeColor::Yellow),
        7..13
    );
    assert_eq!(text, "Before TARGET");
}

#[test]
fn native_markers_remain_literal_in_code_yaml_destinations_escapes_and_odd_delimiters() {
    for text in [
        "`==🔵inline==`",
        "```md\n==🟣fenced==\n```",
        "---\ncolor: '==🔴yaml=='\n---\n",
        "[Link](https://example.com/==🟢destination==)",
        "\\==🔵escaped==",
        "===🔵literal===",
    ] {
        let parsed = Markdown::parse(text);
        assert!(parsed.highlights.is_empty(), "{text}");
        assert!(
            parsed
                .spans
                .iter()
                .all(|span| span.style.native_highlight.is_none()),
            "{text}"
        );
    }
    let parsed = Markdown::parse("==🟡[TARGET](Other.md)==");
    assert!(parsed.spans.iter().any(|span| span.style.link
        && span.style.native_highlight == Some(rustidian::highlight::NativeColor::Yellow)));
}

#[test]
fn an_empty_native_mark_reverses_and_accepts_text_without_swallowing_the_next_mark() {
    use rustidian::highlight::{NativeColor, toggle_native};
    let original = "Before  ==🟣EXISTING== after";
    let mut text = original.to_owned();
    let cursor = toggle_native(&mut text, 7..7, NativeColor::Blue);
    assert_eq!(text, "Before ==🔵== ==🟣EXISTING== after");
    assert_eq!(cursor, 10..10);
    let parsed = Markdown::parse(&text);
    assert_eq!(parsed.highlights.len(), 2);
    assert_eq!(&text[parsed.highlights[0].clone()], "==🔵==");
    let existing = text.find("EXISTING").unwrap();
    assert!(
        parsed
            .spans
            .iter()
            .any(|span| span.range.contains(&existing)
                && span.style.native_highlight == Some(NativeColor::Purple))
    );
    assert_eq!(toggle_native(&mut text, cursor, NativeColor::Blue), 7..7);
    assert_eq!(text, original);
    let cursor = toggle_native(&mut text, 7..7, NativeColor::Blue);
    let byte = text.char_indices().nth(cursor.start).unwrap().0;
    text.insert_str(byte, "NEW");
    assert_eq!(text, "Before ==🔵NEW== ==🟣EXISTING== after");
}

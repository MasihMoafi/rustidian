use rust_notes::{editor::Markdown, inline_style::Style};

#[test]
fn document_table_and_callout_content_share_compound_inline_styles() {
    let source = include_str!("fixtures/inline-styles.md");
    let parsed = Markdown::parse(source);
    for (word, color) in [("DW", [43, 122, 75, 255]), ("4h", [184, 97, 8, 255])] {
        for (byte, _) in source.match_indices(word) {
            let span = parsed
                .spans
                .iter()
                .find(|span| span.range.contains(&byte))
                .unwrap();
            assert_eq!(span.style.html.color, Some(color), "{word} at {byte}");
            assert_eq!(span.style.html.bold, Some(true));
        }
    }
    for (byte, _) in source.match_indices(">56h</mark>") {
        let span = parsed
            .spans
            .iter()
            .find(|span| span.range.contains(&(byte + 1)))
            .unwrap();
        assert_eq!(span.style.highlight, Some([46, 204, 113, 102]));
    }
    assert!(
        parsed
            .highlights
            .iter()
            .any(|range| &source[(*range).clone()] == "==11h==")
    );
    let outside = source.find("Paragraph ").unwrap();
    assert_eq!(
        parsed
            .spans
            .iter()
            .find(|span| span.range.contains(&outside))
            .unwrap()
            .style
            .html,
        Style::default()
    );
}

#[test]
fn nested_inline_styles_restore_outer_formatting_and_hide_only_paired_tags() {
    let source = "<span style='color:#2b7a4b;font-weight:700'>outer <mark style='background:rgba(46,204,113,.4)'>inner</mark> outer</span> plain";
    let parsed = Markdown::parse(source);
    for word in ["outer", "inner"] {
        for (byte, _) in source.match_indices(word) {
            let style = parsed
                .spans
                .iter()
                .find(|span| span.range.contains(&byte))
                .unwrap()
                .style;
            assert_eq!(style.html.color, Some([43, 122, 75, 255]));
            assert_eq!(style.html.bold, Some(true));
            assert_eq!(
                style.highlight,
                (word == "inner").then_some([46, 204, 113, 102])
            );
        }
    }
    let plain = source.rfind("plain").unwrap();
    let style = parsed
        .spans
        .iter()
        .find(|span| span.range.contains(&plain))
        .unwrap()
        .style;
    assert_eq!(style.html, Style::default());
    assert!(style.highlight.is_none());
    for marker in ["<span", "<mark", "</mark>", "</span>"] {
        let byte = source.find(marker).unwrap();
        let style = parsed
            .spans
            .iter()
            .find(|span| span.range.contains(&byte))
            .unwrap()
            .style;
        assert!(style.hidden && style.marker, "{marker}");
    }
}

#[test]
fn literal_code_yaml_and_unbalanced_tags_keep_their_original_source() {
    for source in [
        "`<span style='color:#123456'>literal</span>`",
        "```md\n<mark style='background:rgba(46,204,113,.4)'>literal</mark>\n```",
        "---\nvalue: \"<span style='color:#123456'>literal</span>\"\n---\n",
        "<span style='color:#123456'>unclosed",
        "unopened</span>",
    ] {
        let original = source.to_owned();
        let parsed = Markdown::parse(source);
        assert!(
            parsed
                .spans
                .iter()
                .all(|span| span.style.html == Style::default()),
            "{source}"
        );
        assert_eq!(source, original);
    }
}

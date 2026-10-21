use rust_notes::editor::{Markdown, surround};

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
            .any(|span| span.style.highlight && span.style.strong)
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

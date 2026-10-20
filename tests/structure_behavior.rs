use rust_notes::structure::{find_anchor, outline};

#[test]
fn outline_and_anchor_navigation_handle_formatted_unicode_and_setext_headings() {
    let text = "# First\n\n你好 paragraph. ^greeting\n\n## **Connected** 你好\n\nSetext\n---\n\n```\n# Ignored\n^hidden\n```\n";
    let headings = outline(text);
    assert_eq!(
        headings
            .iter()
            .map(|h| h.title.as_str())
            .collect::<Vec<_>>(),
        ["First", "Connected 你好", "Setext"]
    );
    assert_eq!(headings[1].level, 2);
    let position = find_anchor(text, "connected 你好").unwrap();
    assert_eq!(
        text.chars().skip(position).take(2).collect::<String>(),
        "##"
    );
    assert_eq!(find_anchor(text, "^greeting"), Some(9));
    assert!(find_anchor(text, "Ignored").is_none());
    assert!(find_anchor(text, "^hidden").is_none());
    assert!(find_anchor(text, "missing").is_none());
}

use rust_notes::{editor::Markdown, folding::Sections};
use std::collections::HashSet;

#[test]
fn heading_folds_keep_nested_state_and_unicode_source_ranges() {
    let source =
        "# Note\n\nBefore\n\n## Parent π\n\nPARENT\n\n### Child\n\nسلام\n\n## Sibling\n\nSIBLING\n";
    let sections = Sections::new(source, &Markdown::parse(source));
    assert_eq!(sections.0.len(), 4);
    let mut collapsed = HashSet::from([sections.0[1].key.clone(), sections.0[2].key.clone()]);
    let hidden = sections.hidden(&collapsed);
    assert_eq!(hidden, vec![sections.0[1].body_chars.clone()]);
    assert!(source[sections.0[1].body.clone()].contains("سلام"));
    assert!(!source[sections.0[1].body.clone()].contains("SIBLING"));
    collapsed.remove(&sections.0[1].key);
    assert_eq!(
        sections.hidden(&collapsed),
        vec![sections.0[2].body_chars.clone()]
    );
    sections.expand_at(sections.0[2].body_chars.start + 1, &mut collapsed);
    assert!(sections.hidden(&collapsed).is_empty());
    collapsed.extend([sections.0[1].key.clone(), sections.0[2].key.clone()]);
    sections.expand_at(sections.0[2].heading_chars.start, &mut collapsed);
    assert!(!collapsed.contains(&sections.0[1].key));
    assert!(collapsed.contains(&sections.0[2].key));
    sections.reveal(sections.0[2].heading_chars.start, &mut collapsed);
    assert!(collapsed.is_empty());
    collapsed.insert(sections.0[3].key.clone());
    sections.expand_at(source.chars().count(), &mut collapsed);
    assert!(collapsed.is_empty());
}

#[test]
fn folding_excludes_yaml_code_and_nested_quote_headings_and_keeps_setext_underline() {
    let source = "---\nvalue: |\n  # Metadata\n---\n\n```md\n# Code\n```\n\n> ## Quote\n> body\n\nTitle π\r\n-------\r\n\r\nBody\r\n\r\n## Next\nTail";
    let sections = Sections::new(source, &Markdown::parse(source));
    assert_eq!(sections.0.len(), 2);
    assert_eq!(
        &source[sections.0[0].heading.clone()],
        "Title π\r\n-------\r\n"
    );
    assert!(!source[sections.0[0].body.clone()].contains("Next"));
    for section in sections.0 {
        assert_eq!(
            source[..section.heading.start].chars().count(),
            section.heading_chars.start
        );
        assert_eq!(
            source[..section.body.end].chars().count(),
            section.body_chars.end
        );
    }
}

#[test]
fn repeated_headings_have_distinct_keys_stable_after_body_changes() {
    let before = "## Same\nBody\n## Same\nTail";
    let after = "## Same\nBody changed π\n## Same\nTail";
    let first = Sections::new(before, &Markdown::parse(before));
    let second = Sections::new(after, &Markdown::parse(after));
    assert_ne!(first.0[0].key, first.0[1].key);
    assert_eq!(
        first
            .0
            .iter()
            .map(|section| &section.key)
            .collect::<Vec<_>>(),
        second
            .0
            .iter()
            .map(|section| &section.key)
            .collect::<Vec<_>>()
    );
    let adjacent = "## Empty\n## Next\n";
    let sections = Sections::new(adjacent, &Markdown::parse(adjacent));
    assert!(sections.0[0].body.is_empty());
}

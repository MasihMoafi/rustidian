use rust_notes::{callouts::Kind, editor::Markdown};

#[test]
fn callouts_use_parser_structure_and_preserve_nested_unicode_crlf_ranges() {
    let source = "Before π\r\n\r\n> [!WaRnInG]- **Careful**\r\n> Body فارسی\r\n>\r\n> > [!hint]+ Nested\r\n> > Inside\r\n\r\nAfter\r\n";
    let parsed = Markdown::parse(source);
    assert_eq!(parsed.callouts.len(), 2);
    let outer = &parsed.callouts[0];
    let inner = &parsed.callouts[1];
    assert_eq!(outer.kind, Kind::Warning);
    assert_eq!(outer.folded, Some(true));
    assert_eq!(outer.title, "**Careful**");
    assert_eq!(&source[outer.marker.clone()], "[!WaRnInG]");
    assert!(source[outer.body.clone()].contains("فارسی"));
    assert!(!source[outer.range.clone()].contains("After"));
    assert_eq!(inner.kind, Kind::Tip);
    assert_eq!(inner.folded, Some(false));
    assert!(outer.body.contains(&inner.range.start));
    assert!(source[inner.header.clone()].ends_with("\r\n"));
}

#[test]
fn literal_markers_code_metadata_and_later_quote_paragraphs_are_not_callouts() {
    let source = "---\nquoted: |\n  > [!tip] YAML\n---\n\n```md\n> [!note] Fenced\n```\n\n    > [!info] Indented\n\n> \\[!tip] Escaped\n\n> Ordinary\n>\n> [!warning] Later paragraph\n\n[!note] Outside\n\n> [!] Empty\n\n> [!bad type] Invalid\n";
    assert!(Markdown::parse(source).callouts.is_empty());
    let source = "> [!unknown-kind]\n\n> [!SUMMARY]";
    let parsed = Markdown::parse(source);
    assert_eq!(parsed.callouts.len(), 2);
    assert_eq!(parsed.callouts[0].kind, Kind::Note);
    assert_eq!(parsed.callouts[0].title, "Unknown-kind");
    assert_eq!(parsed.callouts[1].kind, Kind::Abstract);
    assert_eq!(parsed.callouts[1].title, "Summary");
    assert!(
        parsed
            .callouts
            .iter()
            .all(|callout| callout.body.is_empty())
    );
}

#[test]
fn type_change_preserves_container_fold_title_body_and_rejects_stale_source() {
    let original = "- > [!note]- Title π\r\n  > Body **bold** [[Home]]\r\n\r\nAfter\r\n";
    let parsed = Markdown::parse(original);
    assert_eq!(parsed.callouts.len(), 1);
    let callout = &parsed.callouts[0];
    let mut changed = original.to_owned();
    assert!(callout.change_kind(&mut changed, Kind::Warning));
    assert_eq!(changed, original.replacen("[!note]", "[!warning]", 1));
    assert!(!callout.change_kind(&mut changed, Kind::Info));
    let unchanged = changed.clone();
    assert!(!callout.change_kind(&mut changed, Kind::Bug));
    assert_eq!(changed, unchanged);
}

#[test]
fn canonical_types_and_every_documented_alias_have_expected_identity() {
    for kind in Kind::ALL {
        assert_eq!(Kind::parse(kind.identifier()), kind);
    }
    for (aliases, kind) in [
        ("summary tldr", Kind::Abstract),
        ("hint important", Kind::Tip),
        ("check done", Kind::Success),
        ("help faq", Kind::Question),
        ("caution attention", Kind::Warning),
        ("fail missing", Kind::Failure),
        ("error", Kind::Danger),
        ("cite", Kind::Quote),
    ] {
        for alias in aliases.split_whitespace() {
            assert_eq!(Kind::parse(&alias.to_uppercase()), kind);
        }
    }
    assert_eq!(Kind::parse("custom"), Kind::Note);
}

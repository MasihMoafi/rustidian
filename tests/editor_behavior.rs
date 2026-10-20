use rust_notes::editor::{Markdown, find_matches, surround};

#[test]
fn wiki_alias_visibility_uses_original_byte_offsets_for_escaped_table_delimiters() {
    let source = "[[Home\\|Home note]] and `[[Home\\|Literal]]`";
    let parsed = Markdown::parse(source);
    let visible: String = parsed
        .spans
        .iter()
        .filter(|span| !span.style.hidden)
        .map(|span| &source[span.range.clone()])
        .collect();
    assert_eq!(visible, "Home note and [[Home\\|Literal]]");
    assert!(
        parsed
            .spans
            .iter()
            .all(|span| source.get(span.range.clone()).is_some())
    );
}

#[test]
fn toggling_native_tasks_preserves_source_and_ignores_code_and_properties() {
    use rust_notes::editor::toggle_task;
    let source = "---\nexample: '- [ ] Property'\n---\n\n- [ ] فارسی task\n- [X] Done\n\n`- [ ] Inline`\n```md\n- [ ] Code\n```\n";
    let mut text = source.to_owned();
    assert!(toggle_task(&mut text, 0));
    assert!(text.contains("- [x] فارسی task"));
    assert!(toggle_task(&mut text, 1));
    assert!(text.contains("- [ ] Done"));
    assert!(!toggle_task(&mut text, 2));
    assert!(text.contains("example: '- [ ] Property'"));
    assert!(text.contains("`- [ ] Inline`"));
    assert!(text.contains("```md\n- [ ] Code\n```"));
    assert!(toggle_task(&mut text, 0));
    assert!(text.contains("- [ ] فارسی task"));
}
use rust_notes::editor::{complete_link, link_completion};

#[test]
fn link_under_cursor_uses_character_offsets_and_ignores_code() {
    use rust_notes::editor::link_at;
    assert_eq!(
        link_at("你好 [[Home|Go home]]", 9).as_deref(),
        Some("Home|Go home")
    );
    assert_eq!(
        link_at("Click [site](https://example.com)", 9).as_deref(),
        Some("https://example.com")
    );
    assert!(link_at("`[[Home]]`", 5).is_none());
    assert!(link_at("```\n[[Home]]\n```", 8).is_none());
    assert!(link_at("Plain text", 3).is_none());
}

#[test]
fn link_completion_handles_unicode_and_existing_closing_brackets() {
    let mut text = "你好 [[Fir]] and more".to_owned();
    let completion = link_completion(&text, 8).unwrap();
    assert_eq!(completion.query, "Fir");
    let caret = complete_link(&mut text, &completion, "Notes/First steps");
    assert_eq!(text, "你好 [[Notes/First steps]] and more");
    assert_eq!(text.chars().nth(caret), Some(' '));
    assert!(link_completion("`[[code", 7).is_none());
    assert!(link_completion("```\n[[code", 10).is_none());
    assert!(link_completion("[[done]]", 8).is_none());
    assert!(link_completion("plain text", 5).is_none());
}

#[test]
fn inline_preview_styles_prose_and_preserves_every_source_byte() {
    let source = "# Heading\n\n**Bold** and *soft* with [[Notes/你好|你好]] and `**literal**`.\n\n```rust\n**not bold**\n```\n";
    let parsed = Markdown::parse(source);
    assert_eq!(
        parsed
            .spans
            .iter()
            .map(|span| &source[span.range.clone()])
            .collect::<String>(),
        source
    );
    let style_at = |needle: &str| {
        parsed
            .spans
            .iter()
            .find(|span| span.range.contains(&source.find(needle).unwrap()))
            .unwrap()
            .style
    };
    assert_eq!(style_at("Heading").heading, 1);
    assert!(style_at("Bold").strong);
    assert!(style_at("soft").italic);
    assert!(!style_at("and *soft").strong);
    assert!(style_at("你好]]").link);
    assert!(style_at("literal").code);
    assert!(!style_at("literal").strong);
    assert!(style_at("not bold").code);
    assert!(!style_at("not bold").strong);
    assert!(parsed.spans.iter().any(|span| span.style.hidden));
}

#[test]
fn setext_headings_keep_the_first_word_and_show_a_plain_marker() {
    let source = "How are you?!\n---\n";
    let parsed = Markdown::parse(source);
    assert!(!parsed.spans[0].style.hidden);
    assert_eq!(parsed.spans[0].style.heading, 2);
    assert!(parsed.spans.iter().any(|span| !span.style.hidden
        && span.style.heading == 0
        && source[span.range.clone()].contains("---")));
}

#[test]
fn quote_prefixes_preserve_prose_and_literal_code_markers() {
    let source = "> Plain **warm** prose\n> > Nested\n>\n> ```text\n> > literal\n> ```\n\n```\n> outside\n```\n";
    let parsed = Markdown::parse(source);
    let prefixes: Vec<_> = parsed
        .quotes
        .iter()
        .map(|quote| (&source[quote.range.clone()], quote.depth))
        .collect();
    assert_eq!(
        prefixes,
        vec![
            ("> ", 1),
            ("> > ", 2),
            (">", 1),
            ("> ", 1),
            ("> ", 1),
            ("> ", 1),
        ]
    );
    let style_at = |offset| {
        parsed
            .spans
            .iter()
            .find(|span| span.range.contains(&offset))
            .unwrap()
            .style
    };
    assert!(style_at(source.find("warm").unwrap()).quote);
    assert!(!style_at(source.find("warm").unwrap()).quote_marker);
    let literal = source.find("> literal").unwrap();
    assert!(style_at(literal).code);
    assert!(!style_at(literal).quote_marker);
    assert!(!style_at(source.find("> outside").unwrap()).quote_marker);
    assert_eq!(
        parsed
            .spans
            .iter()
            .map(|span| &source[span.range.clone()])
            .collect::<String>(),
        source
    );
}

#[test]
fn fenced_code_decorations_keep_literal_text_and_handle_unclosed_blocks() {
    for (source, closed, content) in [
        (
            "```rust\n**literal** [[No link]] 你好\n```\n",
            true,
            "**literal** [[No link]] 你好\n",
        ),
        (
            "  ~~~~text\n``` is literal\n  ~~~~\n",
            true,
            "``` is literal\n",
        ),
        ("```text\n    ```\nstill code", false, "    ```\nstill code"),
        ("```text", false, ""),
        ("```text\r\ncode π\r\n```\r\n", true, "code π\r\n"),
    ] {
        let parsed = Markdown::parse(source);
        assert_eq!(parsed.code_blocks.len(), 1, "{source}");
        let block = &parsed.code_blocks[0];
        assert_eq!(block.closing.is_some(), closed, "{block:?}");
        assert_eq!(block.content(source), Some(content));
        assert!(block.content("").is_none());
        assert!(!block.opening.is_empty());
        assert!(
            parsed
                .spans
                .iter()
                .all(|span| !span.style.strong && !span.style.link)
        );
        assert_eq!(
            parsed
                .spans
                .iter()
                .map(|span| &source[span.range.clone()])
                .collect::<String>(),
            source
        );
    }
    assert!(
        Markdown::parse("`inline` and literal ``` ticks")
            .code_blocks
            .is_empty()
    );
    assert!(
        Markdown::parse("> ```text\n> quoted\n> ```\n")
            .code_blocks
            .is_empty()
    );
    let properties =
        "---\nsample: |\n  ```text\n  YAML value\n  ```\n---\n\n```rust\nlet x = 1;\n```\n";
    let parsed = Markdown::parse(properties);
    assert_eq!(parsed.code_blocks.len(), 1);
    assert_eq!(parsed.code_blocks[0].language, "rust");
}

#[test]
fn formatting_and_find_keep_unicode_and_reject_empty_search() {
    let mut text = "Hello 你好 and hello 👋".to_owned();
    let selected = surround(&mut text, 6..8, "**");
    assert_eq!(text, "Hello **你好** and hello 👋");
    assert_eq!(selected, 8..10);
    let selected = surround(&mut text, selected, "**");
    assert_eq!(text, "Hello 你好 and hello 👋");
    assert_eq!(selected, 6..8);
    assert_eq!(find_matches(&text, "hello"), vec![0..5, 13..18]);
    assert_eq!(find_matches(&text, "你好"), vec![6..8]);
    assert!(find_matches(&text, "").is_empty());
    assert!(find_matches(&text, "missing").is_empty());
}

#[test]
fn enter_continues_lists_tasks_quotes_and_exits_empty_items() {
    use rust_notes::editor::markdown_enter;
    for (source, expected) in [
        ("- First", "- First\n- "),
        ("  * Nested", "  * Nested\n  * "),
        ("9. Ninth", "9. Ninth\n10. "),
        ("- [x] Finished", "- [x] Finished\n- [ ] "),
        ("> - Item", "> - Item\n> - "),
        ("> Quote", "> Quote\n> "),
        ("- ", ""),
        ("  - [ ] ", "  "),
        ("> - ", "> "),
        ("> ", ""),
        ("- فارسی 🦀", "- فارسی 🦀\n- "),
    ] {
        let mut text = source.to_owned();
        let end = text.chars().count();
        let cursor = markdown_enter(&mut text, end..end).unwrap();
        assert_eq!(text, expected, "{source}");
        assert_eq!(cursor, text.chars().count()..text.chars().count());
    }
    for source in [
        "Normal paragraph",
        "    - Code item",
        "```md\n- Code item",
        "`- Inline code`",
    ] {
        let mut text = source.to_owned();
        let end = text.chars().count();
        assert!(markdown_enter(&mut text, end..end).is_none(), "{source}");
        assert_eq!(text, source);
    }
    let mut text = "- First and second".to_owned();
    assert_eq!(markdown_enter(&mut text, 7..12), Some(10..10));
    assert_eq!(text, "- First\n- second");
}

#[test]
fn live_heading_positions_exclude_yaml_and_setext_underlines() {
    let source = "---\ntitle: Metadata\n---\n\n# Body\n\nSetext\n======\n";
    let parsed = rust_notes::editor::Markdown::parse(source);
    assert_eq!(
        parsed.heading_starts,
        vec![
            source.find("# Body").unwrap(),
            source.find("Setext").unwrap()
        ]
    );
    assert!(
        parsed
            .heading_starts
            .iter()
            .all(|&start| start > source.find("title:").unwrap())
    );
}

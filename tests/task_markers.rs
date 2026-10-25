use rustidian::editor::{Markdown, toggle_task};

#[test]
fn live_task_ranges_preserve_nested_unicode_crlf_and_ignore_non_tasks() {
    let mut text = "---\r\nexample: '- [ ] YAML'\r\n---\r\nفارسی 🦀\r\n- [ ] First\r\n  - [X] Nested\r\n1. [ ] Ordered\r\n\r\n```md\r\n- [ ] Code\r\n```\r\n\r\n`- [ ] Inline`\r\n\\- [ ] Escaped\r\n".to_owned();
    let original = text.clone();
    let parsed = Markdown::parse(&text);
    assert_eq!(parsed.tasks.len(), 3);
    assert!(!parsed.tasks[0].checked);
    assert!(parsed.tasks[1].checked);
    for task in &parsed.tasks {
        assert_eq!(task.character, text[..task.prefix.start].chars().count());
        assert!(text[task.prefix.clone()].contains('['));
        assert!(parsed.spans.iter().any(|span| span.style.task_marker
            && span.range.start >= task.prefix.start
            && span.range.end <= task.prefix.end));
    }
    assert!(toggle_task(&mut text, 1));
    assert_eq!(text, original.replace("[X] Nested", "[ ] Nested"));
    let retained = text.clone();
    assert!(!toggle_task(&mut text, 3));
    assert_eq!(text, retained);
}

#[test]
fn completed_task_styling_does_not_reach_children_or_following_prose() {
    let text = "- [x] Completed\n  - [ ] Child\n\nOutside\n";
    let parsed = Markdown::parse(text);
    assert_eq!(parsed.tasks.len(), 2);
    for span in &parsed.spans {
        if span.style.task_checked {
            assert!(!text[span.range.clone()].contains("Child"));
            assert!(!text[span.range.clone()].contains("Outside"));
        }
    }
    assert!(
        parsed
            .spans
            .iter()
            .any(|span| span.style.task_checked && text[span.range.clone()].contains("Completed"))
    );
}

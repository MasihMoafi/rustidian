use rustidian::metadata::{analyze, frontmatter, remove_property, set_property};
use serde_yaml_ng::Value;

#[test]
fn properties_round_trip_without_touching_body_or_losing_types() {
    let mut text = "---\naliases: [Thinking, 你好]\ntags: [notes/deep, Rust]\ncount: 42\ndone: false\n---\n\n# Title\nBody remains **exact**.\n".to_owned();
    let body = text[text.find("\n\n# Title").unwrap()..].to_owned();
    set_property(&mut text, "done", Value::Bool(true)).unwrap();
    set_property(&mut text, "title", Value::String("Persian فارسی".into())).unwrap();
    assert!(text.ends_with(&body));
    let properties = frontmatter(&text).unwrap().unwrap();
    assert_eq!(
        properties.values[Value::String("done".into())],
        Value::Bool(true)
    );
    assert_eq!(
        properties.values[Value::String("count".into())].as_i64(),
        Some(42)
    );
    remove_property(&mut text, "title").unwrap();
    assert!(text.ends_with(&body));
    let metadata = analyze(&text);
    assert_eq!(metadata.aliases, ["Thinking", "你好"]);
    assert_eq!(metadata.tags, ["notes/deep", "Rust"]);
}

#[test]
fn tags_use_unicode_and_nested_names_and_ignore_code_urls_and_numeric_only_tags() {
    let text = "# Heading\n\n#rust #notes/deep #فارسی #🦀 #123 #123abc #RUST.\n`#inline` and https://example.com/#fragment\n```\n#fenced\n```\n";
    assert_eq!(
        analyze(text).tags,
        ["rust", "notes/deep", "فارسی", "🦀", "123abc"]
    );
}

#[test]
fn malformed_frontmatter_is_reported_and_never_rewritten() {
    let mut text = "---\nbroken: [unfinished\n---\nBody\n".to_owned();
    let original = text.clone();
    assert!(frontmatter(&text).is_err());
    assert!(set_property(&mut text, "title", Value::String("New".into())).is_err());
    assert_eq!(text, original);
    let mut plain = "# Plain\nBody\n".to_owned();
    set_property(
        &mut plain,
        "aliases",
        Value::Sequence(vec![Value::String("Alias".into())]),
    )
    .unwrap();
    assert!(plain.ends_with("# Plain\nBody\n"));
    assert_eq!(analyze(&plain).aliases, ["Alias"]);
}
#[test]
fn common_yaml_scalars_keep_text_booleans_numbers_and_dates_distinct() {
    let text = "---\nyes_word: yes\non_word: on\nflag: true\nleading_zero: 0123\ndate: 2026-10-20\n---\nBody\n";
    let properties = frontmatter(text).unwrap().unwrap();
    assert_eq!(
        properties.values[Value::String("yes_word".into())].as_str(),
        Some("yes")
    );
    assert_eq!(
        properties.values[Value::String("on_word".into())].as_str(),
        Some("on")
    );
    assert_eq!(
        properties.values[Value::String("flag".into())].as_bool(),
        Some(true)
    );
    assert_eq!(
        properties.values[Value::String("leading_zero".into())].as_f64(),
        Some(123.0)
    );
    assert_eq!(
        properties.values[Value::String("date".into())].as_str(),
        Some("2026-10-20")
    );
}

use rust_notes::{
    tab::{Tab, View},
    vault::Vault,
};
use std::fs;

#[test]
fn file_moves_keep_editing_identity_but_navigation_changes_it() {
    use rust_notes::vault::Document;
    use std::path::Path;
    let mut tab = Tab::new(Document::new("Source.md".into()));
    let id = tab.id();
    tab.document.path = "Folder/Source.md".into();
    tab.replace(Document::new("Folder/Source.md".into()));
    assert_eq!(tab.id(), id);
    assert_eq!(tab.editor_path(), Path::new("Source.md"));
    tab.open(Document::new("Another.md".into())).unwrap();
    assert_eq!(tab.id(), id);
    assert_eq!(tab.editor_path(), Path::new("Another.md"));
}

#[test]
fn plain_text_opens_in_source_mode_and_markdown_keeps_live_mode() {
    use rust_notes::vault::Document;
    let mut tab = Tab::new(Document::new("Read me.txt".into()));
    assert_eq!(tab.view, View::Source);
    tab.toggle_reading();
    tab.toggle_reading();
    assert_eq!(tab.view, View::Source);
    let mut markdown = Tab::new(Document::blank());
    assert_eq!(markdown.view, View::Live);
    markdown.open(Document::new("Another.TXT".into())).unwrap();
    assert_eq!(markdown.view, View::Source);
}

#[test]
fn navigation_reuses_the_tab_and_histories_and_modes_are_independent() {
    let root = tempfile::tempdir().unwrap();
    for name in ["Home", "First", "Second"] {
        fs::write(
            root.path().join(format!("{name}.md")),
            format!("# {name}\n"),
        )
        .unwrap();
    }
    let vault = Vault::open(root.path()).unwrap();
    let mut first = Tab::new(vault.read("Home.md".as_ref()).unwrap());
    first
        .open(vault.read("First.md".as_ref()).unwrap())
        .unwrap();
    assert_eq!(first.document.title(), "First");
    let mut second = Tab::new(vault.read("Second.md".as_ref()).unwrap());
    second.view = View::Read;
    assert_eq!(first.view, View::Live);
    assert!(first.history.can_back());
    assert!(!second.history.can_back());
    assert_eq!(first.history.back().unwrap().to_str(), Some("Home.md"));
    first.document.text.push_str("unsaved");
    assert!(
        first
            .open(vault.read("Second.md".as_ref()).unwrap())
            .is_err()
    );
    assert!(first.document.text.ends_with("unsaved"));
}
#[test]
fn reading_toggle_returns_to_the_previous_editing_mode() {
    use rust_notes::vault::Document;
    let mut tab = Tab::new(Document::blank());
    tab.set_view(View::Source);
    tab.toggle_reading();
    assert_eq!(tab.view, View::Read);
    tab.toggle_reading();
    assert_eq!(tab.view, View::Source);
    tab.set_view(View::Live);
    tab.toggle_reading();
    tab.toggle_reading();
    assert_eq!(tab.view, View::Live);
}

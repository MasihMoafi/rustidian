use rust_notes::{
    navigation::History,
    vault::{Document, Vault},
};
use std::path::Path;

#[test]
fn folder_moves_update_history_descendants_without_changing_position_or_similar_names() {
    let mut history = History::default();
    for path in ["Home.md", "Old/Nested/Body.md", "Oldish/Keep.md"] {
        history.visit(Path::new(path));
    }
    history.back();
    history.rename_path(Path::new("Old"), Path::new("New/Folder"));
    assert_eq!(history.back().as_deref(), Some(Path::new("Home.md")));
    assert_eq!(
        history.forward().as_deref(),
        Some(Path::new("New/Folder/Nested/Body.md"))
    );
    assert_eq!(
        history.forward().as_deref(),
        Some(Path::new("Oldish/Keep.md"))
    );
}
use tempfile::tempdir;

#[test]
fn arrows_follow_visited_notes_and_drop_forward_history_after_a_new_visit() {
    let mut history = History::default();
    assert!(history.back().is_none());
    assert!(history.forward().is_none());
    history.visit(Path::new("Home.md"));
    history.visit(Path::new("First.md"));
    history.visit(Path::new("Second.md"));
    assert_eq!(history.back().as_deref(), Some(Path::new("First.md")));
    assert_eq!(history.forward().as_deref(), Some(Path::new("Second.md")));
    history.back();
    history.visit(Path::new("Branch.md"));
    assert!(history.forward().is_none());
    history.visit(Path::new("Branch.md"));
    assert_eq!(history.back().as_deref(), Some(Path::new("First.md")));
    assert_eq!(history.back().as_deref(), Some(Path::new("Home.md")));
    assert!(history.back().is_none());
}

#[test]
fn a_new_tab_is_an_empty_workspace_and_does_not_create_a_note() {
    let folder = tempdir().unwrap();
    let mut vault = Vault::open(folder.path()).unwrap();
    let mut tab = Document::blank();
    assert_eq!(tab.title(), "New tab");
    assert!(!tab.dirty());
    assert!(tab.is_blank());
    assert!(vault.save(&mut tab).is_err());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

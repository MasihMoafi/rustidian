use rustidian::{
    tab::{Tab, View},
    vault::Vault,
};
use std::fs;

#[test]
fn closed_views_keep_identity_modes_and_navigation_across_a_fresh_read() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("First.md"), "# First\n").unwrap();
    fs::write(root.path().join("Second.md"), "# Second\r\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("First.md")).unwrap());
    tab.set_view(View::Source);
    tab.open(vault.read(Path::new("Second.md")).unwrap())
        .unwrap();
    tab.toggle_reading();
    let id = tab.id();
    let editor_path = tab.editor_path().to_owned();
    let closed = tab.into_closed().unwrap();
    assert_eq!(closed.path(), Path::new("Second.md"));
    let document = vault.read(closed.path()).unwrap();
    assert!(closed.matches_saved_text(&document));
    let mut restored = closed.restore(document);
    assert_eq!(restored.id(), id);
    assert_eq!(restored.editor_path(), editor_path);
    assert_eq!(restored.view, View::Read);
    restored.toggle_reading();
    assert_eq!(restored.view, View::Source);
    assert_eq!(restored.history.back(), Some("First.md".into()));
    assert_eq!(restored.history.forward(), Some("Second.md".into()));
    assert!(!restored.document.dirty());
    assert_eq!(restored.document.text, "# Second\r\n");
}

#[test]
fn closed_views_detect_new_disk_bytes_without_holding_the_old_document() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Changed.md"), "# Before\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Changed.md")).unwrap());
    tab.set_view(View::Source);
    let id = tab.id();
    let closed = tab.into_closed().unwrap();
    fs::write(root.path().join("Changed.md"), "# External after\n").unwrap();
    let document = vault.read(closed.path()).unwrap();
    assert!(!closed.matches_saved_text(&document));
    let restored = closed.restore(document);
    assert_eq!(restored.id(), id);
    assert_eq!(restored.view, View::Source);
    assert_eq!(restored.document.text, "# External after\n");
    assert!(!restored.document.dirty());
    assert_eq!(
        fs::read_to_string(root.path().join("Changed.md")).unwrap(),
        "# External after\n"
    );
}

#[test]
fn empty_views_do_not_enter_closed_file_history() {
    let blank = Tab::new(rustidian::vault::Document::blank());
    assert!(blank.into_closed().is_none());
}

#[test]
fn graph_navigation_preserves_the_buffer_and_refuses_to_discard_dirty_edits() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Home.md"), "# Home\r\n").unwrap();
    fs::write(root.path().join("Target.md"), "# Target\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Home.md")).unwrap());
    tab.set_view(View::Source);
    tab.document.text.push_str("Unsaved input\n");
    tab.set_view(View::Graph);
    tab.toggle_reading();
    assert_eq!(tab.view, View::Graph);
    assert_eq!(tab.title(), "Graph view");
    assert!(
        tab.open(vault.read(Path::new("Target.md")).unwrap())
            .is_err()
    );
    assert_eq!(tab.document.path, Path::new("Home.md"));
    assert_eq!(tab.document.text, "# Home\r\nUnsaved input\n");
    tab.show_document();
    assert_eq!(tab.view, View::Source);
    assert_eq!(tab.title(), "Home");
    assert!(tab.document.dirty());
    assert_eq!(
        fs::read(root.path().join("Home.md")).unwrap(),
        b"# Home\r\n"
    );
}

#[test]
fn refreshed_graph_tabs_stay_graphs_until_a_note_is_opened() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Home.md"), "# Home\n").unwrap();
    fs::write(root.path().join("Target.md"), "# Target\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Home.md")).unwrap());
    tab.set_view(View::Source);
    tab.set_view(View::Graph);
    fs::write(root.path().join("Home.md"), "# External edit\n").unwrap();
    tab.replace(vault.read(Path::new("Home.md")).unwrap());
    assert_eq!(tab.view, View::Graph);
    assert_eq!(tab.document.text, "# External edit\n");
    tab.open(vault.read(Path::new("Target.md")).unwrap())
        .unwrap();
    assert_eq!(tab.view, View::Source);
    assert_eq!(tab.document.path, Path::new("Target.md"));
    assert_eq!(tab.history.back(), Some("Home.md".into()));
}

#[test]
fn a_graph_without_a_file_can_be_closed_and_restored() {
    use rustidian::vault::Document;
    let mut tab = Tab::new(Document::blank());
    tab.set_view(View::Graph);
    let id = tab.id();
    let closed = tab.into_closed().unwrap();
    assert!(closed.path().as_os_str().is_empty());
    let mut restored = closed.restore(Document::blank());
    assert_eq!(restored.id(), id);
    assert_eq!(restored.view, View::Graph);
    assert_eq!(restored.title(), "Graph view");
    restored.show_document();
    assert!(restored.document.is_blank());
    assert_eq!(restored.view, View::Live);
}

#[test]
fn discarded_edits_require_history_reset_even_when_the_saved_bytes_are_unchanged() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Discarded.md"), "# Saved\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Discarded.md")).unwrap());
    tab.document.text.push_str("DISCARDED_INPUT");
    assert!(tab.document.dirty());
    let closed = tab.into_closed().unwrap();
    let document = vault.read(closed.path()).unwrap();
    assert!(!closed.matches_saved_text(&document));
    let restored = closed.restore(document);
    assert_eq!(restored.document.text, "# Saved\n");
    assert!(!restored.document.dirty());
}

#[test]
fn renamed_closed_views_keep_their_editor_identity_and_updated_history() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Folder")).unwrap();
    fs::create_dir(root.path().join("Renamed")).unwrap();
    fs::write(root.path().join("Home.md"), "# Home\n").unwrap();
    fs::write(root.path().join("Folder/Target.md"), "# Target\n").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Home.md")).unwrap());
    tab.open(vault.read(Path::new("Folder/Target.md")).unwrap())
        .unwrap();
    let id = tab.id();
    let editor_path = tab.editor_path().to_owned();
    let mut closed = tab.into_closed().unwrap();
    fs::rename(
        root.path().join("Folder/Target.md"),
        root.path().join("Renamed/Target.md"),
    )
    .unwrap();
    closed.rename_path(Path::new("Folder"), Path::new("Renamed"));
    assert_eq!(closed.path(), Path::new("Renamed/Target.md"));
    let document = vault.read(closed.path()).unwrap();
    assert!(closed.matches_saved_text(&document));
    let mut restored = closed.restore(document);
    assert_eq!(restored.id(), id);
    assert_eq!(restored.editor_path(), editor_path);
    assert_eq!(restored.history.back(), Some("Home.md".into()));
    assert_eq!(restored.history.forward(), Some("Renamed/Target.md".into()));
}

#[test]
fn file_moves_keep_editing_identity_but_navigation_changes_it() {
    use std::path::Path;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Source.md"), "# Source\n").unwrap();
    fs::write(root.path().join("Another.md"), "# Another\n").unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let mut tab = Tab::new(vault.read(Path::new("Source.md")).unwrap());
    let id = tab.id();
    let updates = vault
        .begin_move_file(Path::new("Source.md"), Path::new("Folder"), false)
        .unwrap()
        .unwrap();
    let moved = vault.finish_link_updates(updates);
    assert!(moved.warnings.is_empty());
    tab.document.path = moved.path.clone();
    tab.replace(vault.read(&moved.path).unwrap());
    assert_eq!(tab.id(), id);
    assert_eq!(tab.editor_path(), Path::new("Source.md"));
    assert!(!tab.document.dirty());
    tab.open(vault.read(Path::new("Another.md")).unwrap())
        .unwrap();
    assert_eq!(tab.id(), id);
    assert_eq!(tab.editor_path(), Path::new("Another.md"));
}

#[test]
fn plain_text_opens_in_source_mode_and_markdown_keeps_live_mode() {
    use rustidian::vault::Document;
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
    use rustidian::vault::Document;
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

#[test]
fn closing_others_preserves_selected_and_pinned_views_in_visual_order() {
    use rustidian::{tab::other_tab_indices, vault::Document};
    let mut tabs: Vec<_> = ["A", "B", "C", "D"]
        .into_iter()
        .map(|name| Tab::new(Document::new(format!("{name}.md").into())))
        .collect();
    tabs[0].pinned = true;
    assert_eq!(other_tab_indices(&tabs, tabs[2].id()), vec![1, 3]);
    assert_eq!(other_tab_indices(&tabs, tabs[0].id()), vec![1, 2, 3]);
}

#[test]
fn closing_others_has_no_targets_when_all_other_views_are_pinned() {
    use rustidian::{tab::other_tab_indices, vault::Document};
    let mut tabs = vec![
        Tab::new(Document::new("Pinned.md".into())),
        Tab::new(Document::blank()),
    ];
    tabs[0].pinned = true;
    assert!(other_tab_indices(&tabs, tabs[1].id()).is_empty());
    tabs[1].pinned = true;
    assert!(other_tab_indices(&tabs, tabs[0].id()).is_empty());
}

#[test]
fn stale_selected_view_never_targets_the_remaining_tabs() {
    use rustidian::{tab::other_tab_indices, vault::Document};
    let closed = Tab::new(Document::blank()).id();
    let tabs = vec![Tab::new(Document::new("Keep.md".into()))];
    assert!(other_tab_indices(&tabs, closed).is_empty());
    assert!(other_tab_indices(&[], closed).is_empty());
}

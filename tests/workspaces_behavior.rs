use rust_notes::{
    tab::View,
    vault::Vault,
    workspaces::{Layout, SavedTab, Workspaces},
};
use std::{fs, path::Path};

fn layout(path: &str) -> Layout {
    Layout {
        tabs: vec![SavedTab {
            path: path.into(),
            view: View::Read,
            editing_view: View::Source,
            pinned: false,
        }],
        active: 0,
        left: false,
        right: true,
        left_width: 320.0,
        right_width: 270.0,
        right_tab: 2,
    }
}

#[test]
fn opening_does_not_create_state_and_save_overwrites_in_creation_order() {
    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("state/layouts.json");
    let mut store = Workspaces::load(path.clone()).unwrap();
    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
    store.save("Writing", layout("Home.md"), 10).unwrap();
    store.save("Reading", layout("Other.md"), 20).unwrap();
    store.save("Writing", layout("Updated.md"), 30).unwrap();
    let reopened = Workspaces::load(path).unwrap();
    assert_eq!(reopened.entries().len(), 2);
    assert_eq!(reopened.entries()[0].name, "Writing");
    assert_eq!(reopened.entries()[0].modified_ms, 30);
    assert_eq!(
        reopened.entries()[0].layout.tabs[0].path,
        Path::new("Updated.md")
    );
    assert_eq!(reopened.entries()[1].name, "Reading");
    assert_eq!(reopened.active(), Some("Writing"));
}

#[test]
fn concurrent_writers_preserve_other_names_and_deletion_never_touches_notes() {
    let base = tempfile::tempdir().unwrap();
    let note = base.path().join("Keep.md");
    fs::write(&note, "# Keep\r\nسلام\n").unwrap();
    let path = base.path().join("state/layouts.json");
    let mut first = Workspaces::load(path.clone()).unwrap();
    let mut second = Workspaces::load(path.clone()).unwrap();
    first.save("Writing", layout("Keep.md"), 10).unwrap();
    second.save("Reading", layout("Keep.md"), 20).unwrap();
    first.activate("Writing").unwrap();
    second.remove("Reading").unwrap();
    let store = Workspaces::load(path).unwrap();
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.active(), Some("Writing"));
    assert_eq!(fs::read(&note).unwrap(), "# Keep\r\nسلام\n".as_bytes());
    assert!(second.activate("Gone").is_err());
}

#[test]
fn corrupt_or_unwritable_metadata_is_not_replaced() {
    let base = tempfile::tempdir().unwrap();
    let path = base.path().join("layouts.json");
    let mut store = Workspaces::load(path.clone()).unwrap();
    fs::write(&path, b"{unfinished").unwrap();
    assert!(store.save("Writing", layout("Home.md"), 1).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{unfinished");
    assert!(store.entries().is_empty());
    assert!(Workspaces::load(path).is_err());
    let blocked = base.path().join("not-a-folder");
    let mut store = Workspaces::load(blocked.join("layouts.json")).unwrap();
    fs::write(&blocked, "keep").unwrap();
    assert!(store.save("Writing", layout("Home.md"), 1).is_err());
    assert!(store.entries().is_empty());
    assert_eq!(fs::read(blocked).unwrap(), b"keep");
}

#[test]
fn restoration_preserves_order_modes_active_identity_and_saved_missing_entries() {
    let base = tempfile::tempdir().unwrap();
    fs::write(base.path().join("Home.md"), "home").unwrap();
    fs::write(base.path().join("Other.md"), "other").unwrap();
    let vault = Vault::open(base.path()).unwrap();
    let mut saved = layout("Missing.md");
    saved.tabs.push(SavedTab {
        path: "Home.md".into(),
        view: View::Read,
        editing_view: View::Source,
        pinned: true,
    });
    saved.tabs.push(SavedTab {
        path: "Other.md".into(),
        view: View::Live,
        editing_view: View::Live,
        pinned: false,
    });
    saved.active = 2;
    let restored = saved.restore(&vault).unwrap();
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.active, 1);
    assert_eq!(restored.tabs[0].document.path, Path::new("Home.md"));
    assert_eq!(restored.tabs[0].view, View::Read);
    assert_eq!(restored.tabs[0].editing_view(), View::Source);
    assert!(restored.tabs[0].pinned);
    assert!(!restored.tabs[1].pinned);
    let mut legacy = serde_json::to_value(&saved).unwrap();
    for tab in legacy["tabs"].as_array_mut().unwrap() {
        tab.as_object_mut().unwrap().remove("pinned");
    }
    let legacy: Layout = serde_json::from_value(legacy).unwrap();
    assert!(
        legacy
            .restore(&vault)
            .unwrap()
            .tabs
            .iter()
            .all(|tab| !tab.pinned)
    );
    assert_eq!(restored.tabs[1].document.text, "other");
    assert_eq!(saved.tabs.len(), 3);
    assert!(layout("Missing.md").restore(&vault).is_err());
    assert!(layout("../Keep.md").restore(&vault).is_err());
    assert!(layout("/tmp/Keep.md").restore(&vault).is_err());
    assert_eq!(layout("").restore(&vault).unwrap().tabs.len(), 1);
}

#[test]
fn names_layouts_and_separate_vault_locations_are_validated() {
    let base = tempfile::tempdir().unwrap();
    let mut store = Workspaces::load(base.path().join("layouts.json")).unwrap();
    assert!(store.save("   ", layout("Home.md"), 1).is_err());
    assert!(store.save("Bad\nname", layout("Home.md"), 1).is_err());
    let mut invalid = layout("Home.md");
    invalid.left_width = f32::NAN;
    assert!(store.save("Bad", invalid, 1).is_err());
    let mut invalid = layout("Home.md");
    invalid.active = 2;
    assert!(store.save("Bad", invalid, 1).is_err());
    store
        .save("یادداشت / Writing", layout("Home.md"), 1)
        .unwrap();
    assert_eq!(store.entries()[0].name, "یادداشت / Writing");
    assert_ne!(
        rust_notes::workspaces::location(Path::new("/vault/a")),
        rust_notes::workspaces::location(Path::new("/vault/b"))
    );
}

#[cfg(unix)]
#[test]
fn linked_files_and_folders_cannot_restore_external_notes() {
    use std::os::unix::fs::symlink;
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("Private.md"), "private").unwrap();
    symlink(
        outside.path().join("Private.md"),
        base.path().join("Link.md"),
    )
    .unwrap();
    symlink(outside.path(), base.path().join("Linked")).unwrap();
    let vault = Vault::open(base.path()).unwrap();
    assert!(layout("Link.md").restore(&vault).is_err());
    assert!(layout("Linked/Private.md").restore(&vault).is_err());
    assert_eq!(
        fs::read(outside.path().join("Private.md")).unwrap(),
        b"private"
    );
}

use rust_notes::{
    bookmarks::{Bookmark, Bookmarks},
    vault::Vault,
};
use std::{fs, path::Path};

#[test]
fn bookmarks_keep_order_titles_unicode_images_and_exact_file_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let source = "# فارسی 🦀\r\n\r\nOriginal [[Other]].\r\n";
    fs::write(temp.path().join("فارسی.md"), source).unwrap();
    fs::write(temp.path().join("Other.md"), "# Other").unwrap();
    let image = [0, 255, 128, 1];
    fs::write(temp.path().join("Image.png"), image).unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    let mut bookmarks = Bookmarks::default();
    assert!(bookmarks.add(&vault, Path::new("فارسی.md")).unwrap());
    assert!(!bookmarks.add(&vault, Path::new("فارسی.md")).unwrap());
    bookmarks.add(&vault, Path::new("Other.md")).unwrap();
    bookmarks.add(&vault, Path::new("Image.png")).unwrap();
    bookmarks.set_title(Path::new("فارسی.md"), "  عنوان 🦀  ");
    bookmarks.move_item(Path::new("Image.png"), -1);
    assert_eq!(bookmarks.items[0].label(), "عنوان 🦀");
    assert_eq!(bookmarks.items[1].path, Path::new("Image.png"));
    let encoded = serde_json::to_string(&bookmarks).unwrap();
    let restored: Bookmarks = serde_json::from_str(&encoded).unwrap();
    assert_eq!(bookmarks, restored);
    bookmarks.remove(Path::new("Other.md"));
    bookmarks.set_title(Path::new("فارسی.md"), "  ");
    assert_eq!(bookmarks.items[0].label(), "فارسی");
    assert_eq!(
        fs::read(temp.path().join("فارسی.md")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(fs::read(temp.path().join("Image.png")).unwrap(), image);
    assert!(temp.path().join("Other.md").exists());
}

#[test]
fn invalid_missing_blank_and_directory_bookmarks_are_refused_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("Note.md"), "# Note").unwrap();
    fs::create_dir(temp.path().join("Folder")).unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    let mut bookmarks = Bookmarks::default();
    bookmarks.add(&vault, Path::new("Note.md")).unwrap();
    let before = bookmarks.clone();
    for path in [
        "",
        "../Note.md",
        "/etc/hosts",
        ".obsidian/secret.md",
        "Missing.md",
        "Folder",
    ] {
        assert!(bookmarks.add(&vault, Path::new(path)).is_err(), "{path}");
        assert_eq!(bookmarks, before);
    }
    bookmarks.move_item(Path::new("Note.md"), -1);
    bookmarks.move_item(Path::new("Note.md"), 1);
    bookmarks.remove(Path::new("Missing.md"));
    assert_eq!(bookmarks, before);
}

#[test]
fn renames_follow_folder_boundaries_without_losing_custom_titles_or_missing_targets() {
    let mut bookmarks = Bookmarks {
        items: [
            "Folder/Note.md",
            "Folder/Nested/Image.png",
            "Folderish/Other.md",
            "Gone.md",
        ]
        .into_iter()
        .map(|path| Bookmark {
            path: path.into(),
            title: None,
        })
        .collect(),
    };
    bookmarks.set_title(Path::new("Folder/Note.md"), "Keep my title");
    bookmarks.rename(Path::new("Folder"), Path::new("Moved"));
    assert_eq!(bookmarks.items[0].path, Path::new("Moved/Note.md"));
    assert_eq!(bookmarks.items[0].label(), "Keep my title");
    assert_eq!(bookmarks.items[1].path, Path::new("Moved/Nested/Image.png"));
    assert_eq!(bookmarks.items[2].path, Path::new("Folderish/Other.md"));
    bookmarks.rename(Path::new("Moved/Note.md"), Path::new("Moved/New.md"));
    assert!(bookmarks.contains(Path::new("Moved/New.md")));
    assert!(bookmarks.contains(Path::new("Gone.md")));
}

#[test]
fn restored_bookmarks_remove_unsafe_and_duplicate_entries_but_retain_missing_files() {
    let mut bookmarks: Bookmarks = serde_json::from_str(
        r#"{"items":[
        {"path":"../Outside.md"},{"path":"/etc/hosts"},{"path":""},
        {"path":".obsidian/Private.md"},{"path":"Gone.md","title":"  Keep  "},
        {"path":"Gone.md"},{"path":"Note.md","title":" "}] }"#,
    )
    .unwrap();
    bookmarks.sanitize();
    assert_eq!(bookmarks.items.len(), 2);
    assert_eq!(bookmarks.items[0].label(), "Keep");
    assert_eq!(bookmarks.items[1].label(), "Note");
}

#[cfg(unix)]
#[test]
fn bookmarking_rechecks_linked_files_and_parents_after_indexing() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("Folder")).unwrap();
    fs::write(temp.path().join("Folder/Note.md"), "inside").unwrap();
    fs::write(outside.path().join("Secret.md"), "outside").unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    fs::rename(temp.path().join("Folder"), temp.path().join("Original")).unwrap();
    symlink(outside.path(), temp.path().join("Folder")).unwrap();
    symlink(
        outside.path().join("Secret.md"),
        temp.path().join("Link.md"),
    )
    .unwrap();
    let mut bookmarks = Bookmarks::default();
    assert!(
        bookmarks
            .add(&vault, Path::new("Folder/Secret.md"))
            .is_err()
    );
    assert!(bookmarks.add(&vault, Path::new("Link.md")).is_err());
    assert!(bookmarks.items.is_empty());
    assert_eq!(
        fs::read_to_string(outside.path().join("Secret.md")).unwrap(),
        "outside"
    );
}

use rust_notes::vault::Vault;
use rust_notes::{
    paths,
    search::FileMatcher,
    tab::{ImageScale, Tab, View},
};
use std::{fs, path::Path};

#[test]
fn following_an_image_opens_the_existing_file_and_keeps_note_creation_separate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Assets")).unwrap();
    fs::write(root.join("Home.md"), "![[Assets/Photo.png]]").unwrap();
    let image = b"opaque attachment bytes\x00\xff";
    fs::write(root.join("Assets/Photo.png"), image).unwrap();
    let mut vault = Vault::open(root).unwrap();
    let document = vault
        .open_link(Path::new("Home.md"), "Assets/Photo.png")
        .unwrap();
    assert_eq!(document.path, Path::new("Assets/Photo.png"));
    assert!(document.is_image());
    assert_eq!(fs::read(root.join("Assets/Photo.png")).unwrap(), image);
    assert_eq!(fs::read_dir(root.join("Assets")).unwrap().count(), 1);
    assert!(vault.read(Path::new("Assets/Photo.png")).is_err());
    let created = vault
        .open_link(Path::new("Home.md"), "Created note")
        .unwrap();
    assert_eq!(created.path, Path::new("Created note.md"));
    assert!(root.join("Created note.md").is_file());
}

#[test]
fn image_moves_keep_binary_bytes_and_support_bracketed_filenames_in_wiki_links() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Assets")).unwrap();
    fs::write(root.join("Home.md"), "![[Assets/Photo.png|64]]").unwrap();
    let image = b"image bytes\x00\xff";
    fs::write(root.join("Assets/Photo.png"), image).unwrap();
    let mut vault = Vault::open(root).unwrap();
    let plan = vault
        .plan_rename(
            Path::new("Assets/Photo.png"),
            Path::new("Assets/Renamed[1].png"),
        )
        .unwrap();
    assert!(vault.rename_note(plan).unwrap().warnings.is_empty());
    assert_eq!(fs::read(root.join("Assets/Renamed[1].png")).unwrap(), image);
    assert_eq!(
        fs::read_to_string(root.join("Home.md")).unwrap(),
        "![[Assets/Renamed[1].png|64]]"
    );
    assert!(
        vault
            .open_link(Path::new("Home.md"), "Assets/Renamed[1].png")
            .unwrap()
            .is_image()
    );
    assert_eq!(vault.backlinks(Path::new("Assets/Renamed[1].png")).len(), 1);
}

#[test]
fn image_documents_cannot_write_text_even_when_the_public_path_and_buffer_are_changed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Assets")).unwrap();
    fs::write(root.join("Home.md"), "original note").unwrap();
    fs::write(root.join("Assets/Photo.png"), b"original image").unwrap();
    let mut vault = Vault::open(root).unwrap();
    let mut document = vault.open_document(Path::new("Assets/Photo.png")).unwrap();
    document.path = "Home.md".into();
    document.text = "must not write".into();
    assert!(vault.save(&mut document).is_err());
    assert_eq!(
        fs::read_to_string(root.join("Home.md")).unwrap(),
        "original note"
    );
    assert_eq!(
        fs::read(root.join("Assets/Photo.png")).unwrap(),
        b"original image"
    );
    let mut note = vault.read(Path::new("Home.md")).unwrap();
    note.text.push_str(" edited");
    vault.save(&mut note).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("Home.md")).unwrap(),
        "original note edited"
    );
}

#[test]
fn missing_image_links_do_not_create_empty_markdown_replacements() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Assets")).unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    for target in ["Assets/Missing.png", "Assets/Missing%2Epng"] {
        assert!(vault.open_link(Path::new("Home.md"), target).is_err());
        assert_eq!(fs::read_dir(dir.path().join("Assets")).unwrap().count(), 0);
    }
}

#[test]
fn image_targets_are_unambiguous_and_escaped_names_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("Home.md"), "# Home").unwrap();
    fs::write(root.join("Photo.png.md"), "This is a note").unwrap();
    for name in [
        "Photo.png",
        "100%#|]].png",
        "Bracket[1].png",
        "mailto:art.png",
    ] {
        fs::write(root.join(name), b"image\x00\xff").unwrap();
    }
    let mut vault = Vault::open(root).unwrap();
    for name in [
        "Photo.png",
        "100%#|]].png",
        "Bracket[1].png",
        "mailto:art.png",
        "Photo.png.md",
    ] {
        let target = paths::wiki_destination(Path::new(name));
        let document = vault.open_link(Path::new("Home.md"), &target).unwrap();
        assert_eq!(document.path, Path::new(name), "{target}");
        assert_eq!(document.is_image(), !name.ends_with(".md"));
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 6);
}

#[test]
fn image_tabs_keep_read_only_modes_and_restore_note_editing_mode() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Home.md"), "# Home").unwrap();
    fs::write(dir.path().join("Photo.png"), b"image").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let note = || vault.open_document(Path::new("Home.md")).unwrap();
    let image = || vault.open_document(Path::new("Photo.png")).unwrap();
    let mut tab = Tab::new(note());
    tab.set_view(View::Source);
    tab.open(image()).unwrap();
    for view in [View::Source, View::Live, View::Split, View::Read] {
        tab.set_view(view);
        tab.toggle_reading();
        assert_eq!(tab.view, View::Read);
    }
    tab.image_scale = ImageScale::Actual;
    tab.open(note()).unwrap();
    assert_eq!(tab.view, View::Source);
    assert_eq!(tab.history.back(), Some(Path::new("Photo.png").to_owned()));
    tab.replace(image());
    assert_eq!(tab.view, View::Read);
    assert_eq!(tab.image_scale, ImageScale::Fit);
}

#[test]
fn quick_open_keeps_all_files_while_link_completion_excludes_unsafe_note_names() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "Home.md",
        "Bad#name.md",
        "Photo.png.md",
        "Photo.png",
        "100%#|]].png",
        "Viewer[1].png",
    ] {
        fs::write(dir.path().join(name), b"fixture").unwrap();
    }
    let vault = Vault::open(dir.path()).unwrap();
    let mut matcher = FileMatcher::from_vault(&vault);
    assert!(
        matcher
            .rank("Bad", 20)
            .contains(&Path::new("Bad#name.md").to_owned())
    );
    assert!(matcher.rank_links("Bad", 20).is_empty());
    assert_eq!(
        matcher.rank_links("Viewer", 20),
        vec![Path::new("Viewer[1].png").to_owned()]
    );
    for name in ["Photo.png.md", "Photo.png", "100%#|]].png"] {
        assert!(
            matcher
                .rank_links("", 20)
                .contains(&Path::new(name).to_owned())
        );
    }
}

#[test]
fn image_moves_refuse_extension_changes_collisions_and_stale_source_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("Home.md"), "![[Photo.png]]").unwrap();
    fs::write(root.join("Photo.png"), b"original").unwrap();
    fs::write(root.join("Taken.png"), b"existing").unwrap();
    let mut vault = Vault::open(root).unwrap();
    assert!(
        vault
            .plan_rename(Path::new("Photo.png"), Path::new("Photo.jpg"))
            .is_err()
    );
    assert!(
        vault
            .plan_rename(Path::new("Photo.png"), Path::new("Taken.png"))
            .is_err()
    );
    let stale = vault
        .plan_rename(Path::new("Photo.png"), Path::new("Moved.png"))
        .unwrap();
    fs::write(root.join("Photo.png"), b"external change").unwrap();
    assert!(vault.rename_note(stale).is_err());
    assert_eq!(
        fs::read(root.join("Photo.png")).unwrap(),
        b"external change"
    );
    assert!(!root.join("Moved.png").exists());
    let late = vault
        .plan_rename(Path::new("Photo.png"), Path::new("Moved.png"))
        .unwrap();
    fs::write(root.join("Moved.png"), b"late arrival").unwrap();
    assert!(vault.rename_note(late).is_err());
    assert_eq!(fs::read(root.join("Moved.png")).unwrap(), b"late arrival");
    assert_eq!(
        fs::read_to_string(root.join("Home.md")).unwrap(),
        "![[Photo.png]]"
    );
}

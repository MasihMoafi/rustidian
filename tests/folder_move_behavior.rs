use rust_notes::vault::Vault;
use std::{fs, path::Path};

#[test]
fn moving_a_folder_preserves_nested_files_and_retargets_resolved_links() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Original/Nested")).unwrap();
    fs::create_dir(root.join("Original/Empty")).unwrap();
    fs::create_dir(root.join("Destination")).unwrap();
    fs::write(root.join("Home.md"), "# Outside\n").unwrap();
    fs::write(root.join("Original/Parent.md"), "# Parent\n[Outside](../Home.md)\n![[Original/Nested/photo.png|64]]\n[[Original/Nested/Child#Part|child]]\n").unwrap();
    fs::write(
        root.join("Original/Nested/Child.md"),
        "# Child\n[Parent](../Parent.md)\n[Outside](../../Home.md)\n![Photo](photo.png)\n",
    )
    .unwrap();
    fs::write(
        root.join("Original/Nested/photo.png"),
        b"unchanged image bytes",
    )
    .unwrap();
    fs::write(root.join("Links.md"), "[[Original/Parent#Parent|alias]]\n[child](Original/Nested/Child.md#Part)\n![photo](Original/Nested/photo.png)\n[reference][r]\n\n[r]: Original/Parent.md \"Title\"\n\n`[[Original/Parent]]`\n").unwrap();
    let mut vault = Vault::open(root).unwrap();
    let plan = vault
        .plan_rename(Path::new("Original"), Path::new("Destination/Renamed"))
        .unwrap();
    let outcome = vault.rename_note(plan).unwrap();
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert!(!root.join("Original").exists());
    assert!(root.join("Destination/Renamed/Empty").is_dir());
    assert_eq!(
        fs::read(root.join("Destination/Renamed/Nested/photo.png")).unwrap(),
        b"unchanged image bytes"
    );
    assert_eq!(
        fs::read_to_string(root.join("Home.md")).unwrap(),
        "# Outside\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("Destination/Renamed/Parent.md")).unwrap(),
        "# Parent\n[Outside](../../Home.md)\n![[Destination/Renamed/Nested/photo.png|64]]\n[[Destination/Renamed/Nested/Child#Part|child]]\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("Destination/Renamed/Nested/Child.md")).unwrap(),
        "# Child\n[Parent](../Parent.md)\n[Outside](../../../Home.md)\n![Photo](photo.png)\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("Links.md")).unwrap(),
        "[[Destination/Renamed/Parent#Parent|alias]]\n[child](Destination/Renamed/Nested/Child.md#Part)\n![photo](Destination/Renamed/Nested/photo.png)\n[reference][r]\n\n[r]: Destination/Renamed/Parent.md \"Title\"\n\n`[[Original/Parent]]`\n"
    );
    assert!(
        vault
            .resolve(Path::new("Links.md"), "Destination/Renamed/Parent")
            .is_some()
    );
    assert!(vault.folders.contains(&"Destination/Renamed/Empty".into()));
    assert!(
        vault
            .resolve_attachment(
                Path::new("Links.md"),
                "Destination/Renamed/Nested/photo.png"
            )
            .is_some()
    );
}

#[test]
fn empty_folders_move_and_invalid_or_existing_destinations_preserve_every_entry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Empty/Nested")).unwrap();
    fs::create_dir(root.join("Occupied")).unwrap();
    fs::write(root.join("Occupied/Keep.md"), "keep").unwrap();
    let mut vault = Vault::open(root).unwrap();
    for target in [
        "Empty/Inside",
        "Occupied",
        "../Outside",
        ".hidden",
        "target",
        "",
    ] {
        assert!(
            vault
                .plan_rename(Path::new("Empty"), Path::new(target))
                .is_err(),
            "{target}"
        );
        assert!(root.join("Empty/Nested").is_dir());
        assert_eq!(
            fs::read_to_string(root.join("Occupied/Keep.md")).unwrap(),
            "keep"
        );
    }
    let plan = vault
        .plan_rename(Path::new("Empty"), Path::new("Renamed"))
        .unwrap();
    let outcome = vault.rename_note(plan).unwrap();
    assert_eq!(outcome.updated_notes, 0);
    assert!(outcome.warnings.is_empty());
    assert!(root.join("Renamed/Nested").is_dir());
    assert!(!root.join("Empty").exists());
}

#[test]
fn destination_names_that_would_break_rewritten_wiki_links_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Source")).unwrap();
    fs::write(root.join("Source/Body.md"), "body").unwrap();
    fs::write(root.join("Links.md"), "[[Source/Body|body]]").unwrap();
    let vault = Vault::open(root).unwrap();
    for name in [
        "Hash#name",
        "Pipe|name",
        "Closing]]name",
        "Line\nbreak",
        "Back\\slash",
    ] {
        assert!(
            vault
                .plan_rename(Path::new("Source"), Path::new(name))
                .is_err(),
            "{name}"
        );
        assert!(root.join("Source/Body.md").is_file());
        assert_eq!(
            fs::read_to_string(root.join("Links.md")).unwrap(),
            "[[Source/Body|body]]"
        );
    }
}

#[test]
fn changed_sources_links_tree_membership_and_late_collisions_abort_the_move() {
    for change in ["source", "links", "entries", "destination", "replacement"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("Source")).unwrap();
        fs::write(root.join("Source/Body.md"), "body").unwrap();
        fs::write(root.join("Links.md"), "[[Source/Body]]").unwrap();
        let mut vault = Vault::open(root).unwrap();
        let plan = vault
            .plan_rename(Path::new("Source"), Path::new("Moved"))
            .unwrap();
        match change {
            "source" => fs::write(root.join("Source/Body.md"), "external body").unwrap(),
            "links" => fs::write(root.join("Links.md"), "external links").unwrap(),
            "entries" => fs::write(root.join("Source/New.md"), "new entry").unwrap(),
            "destination" => {
                fs::create_dir(root.join("Moved")).unwrap();
                fs::write(root.join("Moved/Keep.md"), "keep").unwrap();
            }
            "replacement" => {
                fs::rename(root.join("Source"), root.join("Original")).unwrap();
                fs::create_dir(root.join("Source")).unwrap();
                fs::write(root.join("Source/Body.md"), "body").unwrap();
            }
            _ => unreachable!(),
        }
        assert!(vault.rename_note(plan).is_err(), "{change}");
        assert!(root.join("Source/Body.md").is_file(), "{change}");
        if change == "destination" {
            assert_eq!(
                fs::read_to_string(root.join("Moved/Keep.md")).unwrap(),
                "keep"
            );
        } else {
            assert!(!root.join("Moved").exists(), "{change}");
        }
        assert_eq!(
            fs::read_to_string(root.join("Source/Body.md")).unwrap(),
            if change == "source" {
                "external body"
            } else {
                "body"
            }
        );
        assert_eq!(
            fs::read_to_string(root.join("Links.md")).unwrap(),
            if change == "links" {
                "external links"
            } else {
                "[[Source/Body]]"
            }
        );
    }
}

#[cfg(unix)]
#[test]
fn linked_folders_are_refused_and_moved_notes_keep_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Source")).unwrap();
    fs::create_dir(root.join("Linked contents")).unwrap();
    fs::write(root.join("Source/Body.md"), "[Outside](../Home.md)").unwrap();
    fs::write(root.join("Home.md"), "outside").unwrap();
    symlink(root.join("Source"), root.join("Shortcut")).unwrap();
    symlink(root.join("Source"), root.join("Linked contents/Child")).unwrap();
    fs::set_permissions(
        root.join("Source/Body.md"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let mut vault = Vault::open(root).unwrap();
    assert!(
        vault
            .plan_rename(Path::new("Shortcut"), Path::new("Moved"))
            .is_err()
    );
    assert!(
        vault
            .plan_rename(Path::new("Linked contents"), Path::new("Moved"))
            .is_err()
    );
    assert!(
        vault
            .plan_rename(Path::new("Source"), Path::new("Shortcut/Inside"))
            .is_err()
    );
    let plan = vault
        .plan_rename(Path::new("Source"), Path::new("Moved"))
        .unwrap();
    vault.rename_note(plan).unwrap();
    assert_eq!(
        fs::metadata(root.join("Moved/Body.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_to_string(root.join("Moved/Body.md")).unwrap(),
        "[Outside](../Home.md)"
    );
}

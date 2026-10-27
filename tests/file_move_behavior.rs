use rustidian::vault::Vault;
use std::{fs, path::Path};

#[test]
fn move_keeps_file_bytes_and_defers_reference_changes_until_the_choice() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("From")).unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    let source = "# Source\r\n\r\nKeep this file.\r\n";
    let reference = "[[From/Source|Alias]]\r\n\r\n[Label](From/Source.md)\r\n";
    fs::write(root.path().join("From/Source.md"), source).unwrap();
    fs::write(root.path().join("Reference.md"), reference).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("From/Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.path(), Path::new("To/Source.md"));
    assert_eq!(updates.counts(), (2, 1));
    assert!(!root.path().join("From/Source.md").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("To/Source.md")).unwrap(),
        source
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        reference
    );
    let result = vault.finish_link_updates(updates);
    assert!(result.warnings.is_empty());
    assert_eq!(result.updated_notes, 1);
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        "[[To/Source|Alias]]\r\n\r\n[Label](To/Source.md)\r\n"
    );
    assert_eq!(
        vault
            .resolve(Path::new("Reference.md"), "To/Source|Alias")
            .as_deref(),
        Some(Path::new("To/Source.md"))
    );
}

#[test]
fn declining_link_updates_preserves_the_references_after_the_real_move() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("From")).unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("From/Source.md"), "Keep source\n").unwrap();
    let reference = "[[From/Source|Do not edit]]\n";
    fs::write(root.path().join("Reference.md"), reference).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("From/Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.counts(), (1, 1));
    drop(updates);
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        reference
    );
    assert_eq!(
        fs::read_to_string(root.path().join("To/Source.md")).unwrap(),
        "Keep source\n"
    );
    assert!(
        vault
            .resolve(Path::new("Reference.md"), "From/Source")
            .is_none()
    );
}

#[test]
fn still_valid_unique_names_need_no_rewrite_even_in_a_mixed_reference_file() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("From")).unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("From/Source.md"), "# Source\n").unwrap();
    let reference = "[[Source|Keep short]]\r\n[[From/Source|Update path]]\r\n[Short](Source.md)\r\n[Path](From/Source.md)\r\n";
    fs::write(root.path().join("Reference.md"), reference).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("From/Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.counts(), (2, 1));
    let outcome = vault.finish_link_updates(updates);
    assert!(outcome.warnings.is_empty());
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        "[[Source|Keep short]]\r\n[[To/Source|Update path]]\r\n[Short](Source.md)\r\n[Path](To/Source.md)\r\n"
    );
    let updates = vault
        .begin_move_file(Path::new("To/Source.md"), Path::new("From"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.counts(), (2, 1));
    vault.finish_link_updates(updates);
    fs::write(
        root.path().join("Reference.md"),
        "[[Source|Short]]\n[Short](Source.md)\n",
    )
    .unwrap();
    vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("From/Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.counts(), (0, 0));
    assert_eq!(vault.finish_link_updates(updates).updated_notes, 0);
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        "[[Source|Short]]\n[Short](Source.md)\n"
    );
}

#[test]
fn occupied_names_are_numbered_without_changing_the_normal_rename_contract() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("Source 4.MD"), "Keep moved bytes\r\n").unwrap();
    fs::write(root.path().join("To/Source 4.MD"), "KEEP_OCCUPIED\n").unwrap();
    fs::create_dir(root.path().join("To/Source 4 1.MD")).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(
        vault
            .plan_rename(Path::new("Source 4.MD"), Path::new("To/Source 4.MD"))
            .is_err()
    );
    let updates = vault
        .begin_move_file(Path::new("Source 4.MD"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.path(), Path::new("To/Source 4 2.MD"));
    assert_eq!(
        fs::read_to_string(root.path().join(updates.path())).unwrap(),
        "Keep moved bytes\r\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("To/Source 4.MD")).unwrap(),
        "KEEP_OCCUPIED\n"
    );
    assert!(root.path().join("To/Source 4 1.MD").is_dir());
}

#[test]
fn creating_nested_folders_is_explicit_and_same_folder_is_a_no_op() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Source.md"), "Keep source\n").unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(
        vault
            .begin_move_file(Path::new("Source.md"), Path::new("New/Nested"), false)
            .is_err()
    );
    assert!(!root.path().join("New").exists());
    assert!(
        vault
            .begin_move_file(Path::new("Source.md"), Path::new(""), false)
            .unwrap()
            .is_none()
    );
    let updates = vault
        .begin_move_file(Path::new("Source.md"), Path::new("New/Nested"), true)
        .unwrap()
        .unwrap();
    assert_eq!(updates.path(), Path::new("New/Nested/Source.md"));
    assert_eq!(
        fs::read_to_string(root.path().join(updates.path())).unwrap(),
        "Keep source\n"
    );
    assert!(root.path().join("New/Nested").is_dir());
    assert!(vault.folders.contains(&"New".into()));
    assert!(vault.folders.contains(&"New/Nested".into()));
    assert!(!root.path().join("Source.md").exists());
}

#[test]
fn a_link_changed_during_the_prompt_is_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("Source.md"), "Keep source\n").unwrap();
    // A collision changes the basename, so even a short link needs a rewrite.
    fs::write(root.path().join("To/Source.md"), "KEEP_COLLISION\n").unwrap();
    fs::write(root.path().join("Reference.md"), "[[Source]]\n").unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.counts(), (1, 1));
    fs::write(
        root.path().join("Reference.md"),
        "External edit must survive\n",
    )
    .unwrap();
    let outcome = vault.finish_link_updates(updates);
    assert_eq!(outcome.updated_notes, 0);
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        "External edit must survive\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("To/Source 1.md")).unwrap(),
        "Keep source\n"
    );
}

#[test]
fn images_keep_their_bytes_and_move_does_not_escape_through_linked_or_hidden_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    let bytes = [137, 80, 78, 71, 13, 10, 0, 255];
    fs::write(root.path().join("Picture.png"), bytes).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    for folder in ["../Outside", "/absolute", ".hidden", "__pycache__"] {
        assert!(
            vault
                .begin_move_file(Path::new("Picture.png"), Path::new(folder), true)
                .is_err()
        );
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("Linked")).unwrap();
        assert!(
            vault
                .begin_move_file(Path::new("Picture.png"), Path::new("Linked"), true)
                .is_err()
        );
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    }
    let updates = vault
        .begin_move_file(Path::new("Picture.png"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(fs::read(root.path().join(updates.path())).unwrap(), bytes);
    assert!(vault.attachments.contains(&updates.path().to_owned()));
    assert!(!root.path().join(".hidden").exists());
    assert!(!root.path().join("__pycache__").exists());
}

#[test]
fn deferred_updates_cannot_write_to_another_vault_with_identical_original_text() {
    let original = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for root in [original.path(), other.path()] {
        fs::create_dir(root.join("From")).unwrap();
        fs::create_dir(root.join("To")).unwrap();
        fs::write(root.join("From/Source.md"), "# Source\n").unwrap();
        fs::write(root.join("Reference.md"), "[[From/Source]]\n").unwrap();
    }
    let mut vault = Vault::open(original.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("From/Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    let mut wrong_vault = Vault::open(other.path()).unwrap();
    let outcome = wrong_vault.finish_link_updates(updates);
    assert_eq!(outcome.updated_notes, 0);
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(
        fs::read_to_string(other.path().join("Reference.md")).unwrap(),
        "[[From/Source]]\n"
    );
    assert!(other.path().join("From/Source.md").is_file());
    assert!(!other.path().join("To/Source.md").exists());
}

#[test]
fn simultaneous_moves_keep_both_sources_at_distinct_numbered_destinations() {
    let root = tempfile::tempdir().unwrap();
    for folder in ["A", "B", "To"] {
        fs::create_dir(root.path().join(folder)).unwrap();
    }
    fs::write(root.path().join("A/Source.md"), "# First\n").unwrap();
    fs::write(root.path().join("B/Source.md"), "# Second\n").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = ["A/Source.md", "B/Source.md"]
        .into_iter()
        .map(|source| {
            let path = root.path().to_owned();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut vault = Vault::open(&path).unwrap();
                barrier.wait();
                let updates = vault
                    .begin_move_file(Path::new(source), Path::new("To"), false)
                    .unwrap()
                    .unwrap();
                let destination = updates.path().to_owned();
                assert_eq!(updates.counts(), (0, 0));
                vault.finish_link_updates(updates);
                destination
            })
        })
        .collect();
    let destinations: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_ne!(destinations[0], destinations[1]);
    assert_eq!(
        fs::read_to_string(root.path().join(&destinations[0])).unwrap(),
        "# First\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join(&destinations[1])).unwrap(),
        "# Second\n"
    );
    assert!(!root.path().join("A/Source.md").exists());
    assert!(!root.path().join("B/Source.md").exists());
}

#[cfg(unix)]
#[test]
fn a_move_preserves_source_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("Source.md"), "Keep permissions\n").unwrap();
    fs::set_permissions(
        root.path().join("Source.md"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let updates = vault
        .begin_move_file(Path::new("Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(
        fs::metadata(root.path().join(updates.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    vault.finish_link_updates(updates);
}

#[cfg(unix)]
#[test]
fn occupied_symlinks_are_preserved_and_numbered_without_following_them() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("To")).unwrap();
    fs::write(root.path().join("Source.md"), "KEEP_SOURCE\r\n").unwrap();
    fs::write(root.path().join("Reference.md"), "[[Source]]\n").unwrap();
    fs::write(outside.path().join("External.md"), "KEEP_OUTSIDE\n").unwrap();
    let occupied = root.path().join("To/Source.md");
    let dangling = root.path().join("To/Source 1.md");
    std::os::unix::fs::symlink(outside.path().join("External.md"), &occupied).unwrap();
    std::os::unix::fs::symlink(outside.path().join("Missing.md"), &dangling).unwrap();
    fs::create_dir(root.path().join("To/Source 2.md")).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(
        vault
            .plan_rename(Path::new("Source.md"), Path::new("To/Source.md"))
            .is_err()
    );
    let updates = vault
        .begin_move_file(Path::new("Source.md"), Path::new("To"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.path(), Path::new("To/Source 3.md"));
    let result = vault.finish_link_updates(updates);
    assert!(result.warnings.is_empty());
    assert_eq!(
        fs::read(root.path().join("To/Source 3.md")).unwrap(),
        b"KEEP_SOURCE\r\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Reference.md")).unwrap(),
        "[[To/Source 3]]\n"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("External.md")).unwrap(),
        "KEEP_OUTSIDE\n"
    );
    assert_eq!(
        fs::read_link(occupied).unwrap(),
        outside.path().join("External.md")
    );
    assert_eq!(
        fs::read_link(dangling).unwrap(),
        outside.path().join("Missing.md")
    );
    assert!(!outside.path().join("Missing.md").exists());
    assert!(root.path().join("To/Source 2.md").is_dir());
    assert!(!root.path().join("Source.md").exists());
}

#[test]
fn extension_less_text_files_move_but_never_take_an_unopenable_numbered_name() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Build")).unwrap();
    fs::write(root.path().join("Makefile"), "all:\n\techo ok\n").unwrap();
    fs::write(root.path().join("Build/Makefile"), "occupied\n").unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let error = vault
        .begin_move_file(Path::new("Makefile"), Path::new("Build"), false)
        .err()
        .expect("a collision must be refused");
    assert!(error.contains("already exists"), "{error}");
    assert!(root.path().join("Makefile").is_file());
    fs::remove_file(root.path().join("Build/Makefile")).unwrap();
    let updates = vault
        .begin_move_file(Path::new("Makefile"), Path::new("Build"), false)
        .unwrap()
        .unwrap();
    assert_eq!(updates.path(), Path::new("Build/Makefile"));
    assert_eq!(
        fs::read_to_string(root.path().join("Build/Makefile")).unwrap(),
        "all:\n\techo ok\n"
    );
    assert!(!root.path().join("Makefile").exists());
}

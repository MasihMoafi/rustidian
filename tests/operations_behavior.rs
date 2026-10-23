use rust_notes::vault::Vault;
use std::{fs, path::Path};

#[test]
fn duplicate_preserves_note_bytes_skips_occupied_names_and_indexes_the_copy() {
    let root = tempfile::tempdir().unwrap();
    let original = "---\r\naliases: [Original]\r\n---\r\n# طرح\r\n[[Target]]\r\n";
    fs::write(root.path().join("طرح 2026.MD"), original).unwrap();
    fs::write(root.path().join("طرح 2026 1.MD"), "Keep collision\n").unwrap();
    fs::create_dir(root.path().join("طرح 2026 2.MD")).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let copied = vault.duplicate_file(Path::new("طرح 2026.MD")).unwrap();
    assert_eq!(copied, Path::new("طرح 2026 3.MD"));
    assert_eq!(
        fs::read_to_string(root.path().join(&copied)).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(root.path().join("طرح 2026.MD")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(root.path().join("طرح 2026 1.MD")).unwrap(),
        "Keep collision\n"
    );
    let note = vault.notes.iter().find(|note| note.path == copied).unwrap();
    assert_eq!(note.aliases, ["Original"]);
    assert_eq!(note.text, original);
    let twice = vault.duplicate_file(&copied).unwrap();
    assert_eq!(twice, Path::new("طرح 2026 3 1.MD"));
    assert_eq!(
        fs::read_to_string(root.path().join(twice)).unwrap(),
        original
    );
}

#[test]
fn duplicate_keeps_binary_images_out_of_the_text_index_and_supports_plain_text() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    let bytes = [137, 80, 78, 71, 13, 10, 0, 255, 1];
    fs::write(root.path().join("Assets/photo.PNG"), bytes).unwrap();
    fs::write(root.path().join("Memo.txt"), "Keep plain text\r\nسلام\r\n").unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let copied = vault.duplicate_file(Path::new("Assets/photo.PNG")).unwrap();
    assert_eq!(copied, Path::new("Assets/photo 1.PNG"));
    assert_eq!(fs::read(root.path().join(&copied)).unwrap(), bytes);
    assert!(vault.attachments.contains(&copied));
    assert!(vault.open_document(&copied).unwrap().is_image());
    assert!(!vault.notes.iter().any(|note| note.path == copied));
    let text = vault.duplicate_file(Path::new("Memo.txt")).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join(&text)).unwrap(),
        "Keep plain text\r\nسلام\r\n"
    );
    assert!(vault.notes.iter().any(|note| note.path == text));
}

#[test]
fn simultaneous_duplicates_use_distinct_atomic_destinations() {
    let root = tempfile::tempdir().unwrap();
    let original = "# Do not overwrite\r\nسلام\r\n";
    fs::write(root.path().join("Source.md"), original).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let path = root.path().to_owned();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut vault = Vault::open(&path).unwrap();
                barrier.wait();
                vault.duplicate_file(Path::new("Source.md")).unwrap()
            })
        })
        .collect();
    let paths: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_ne!(paths[0], paths[1]);
    for path in paths {
        assert_eq!(
            fs::read_to_string(root.path().join(path)).unwrap(),
            original
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join("Source.md")).unwrap(),
        original
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
}

#[test]
fn invalid_and_linked_duplicate_sources_cannot_write_outside_the_vault() {
    let root = tempfile::tempdir().unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    for path in [
        "Missing.md",
        "../Outside.md",
        "/absolute.md",
        "__pycache__/Source.md",
        ".hidden.md",
        "Unsupported.bin",
    ] {
        assert!(vault.duplicate_file(Path::new(path)).is_err(), "{path}");
    }
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("Secret.md"), "Keep outside\n").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("Secret.md"),
            root.path().join("Linked.md"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("Linked folder")).unwrap();
        assert!(vault.duplicate_file(Path::new("Linked.md")).is_err());
        assert!(
            vault
                .duplicate_file(Path::new("Linked folder/Secret.md"))
                .is_err()
        );
        fs::write(root.path().join("Source.md"), "Keep source\n").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("Secret.md"),
            root.path().join("Source 1.md"),
        )
        .unwrap();
        assert_eq!(
            vault.duplicate_file(Path::new("Source.md")).unwrap(),
            Path::new("Source 2.md")
        );
        assert_eq!(
            fs::read_to_string(outside.path().join("Secret.md")).unwrap(),
            "Keep outside\n"
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }
}

#[test]
fn table_wiki_aliases_resolve_and_moves_preserve_their_escaped_delimiters() {
    let dir = tempfile::tempdir().unwrap();
    let source = "| Note | Example |\n| --- | --- |\n| [[Home\\|Home note]] | `[[Home\\|Literal]]` |\n\n~~~md\n[[Home\\|In code]]\n~~~\n";
    fs::write(dir.path().join("Home.md"), "Home stays exact.\n").unwrap();
    fs::write(dir.path().join("Table.md"), source).unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    assert_eq!(rust_notes::vault::wiki_links(source), ["Home|Home note"]);
    assert_eq!(vault.backlinks(Path::new("Home.md")).len(), 1);
    let plan = vault
        .plan_rename(Path::new("Home.md"), Path::new("New Home.md"))
        .unwrap();
    vault.rename_note(plan).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("Table.md")).unwrap(),
        source.replacen("[[Home\\|Home note]]", "[[New Home\\|Home note]]", 1)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("New Home.md")).unwrap(),
        "Home stays exact.\n"
    );
    assert_eq!(vault.backlinks(Path::new("New Home.md")).len(), 1);
}

#[test]
fn moving_a_note_preserves_its_relative_outgoing_links() {
    let temp = fixture();
    fs::write(temp.path().join("Notes/Old.md"), "# Old\n[Home](../Home.md#Heading)\n[Ref](../Projects/Ref.md \"Title\")\n[[../Home|Home]]\n`[Home](../Home.md)`\n").unwrap();
    let mut vault = Vault::open(temp.path()).unwrap();
    let destination = Path::new("Else/Deep/New.md");
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), destination)
        .unwrap();
    vault.rename_note(plan).unwrap();
    let moved = vault.read(destination).unwrap();
    assert!(moved.text.contains("[Home](../../Home.md#Heading)"));
    assert!(
        moved
            .text
            .contains("[Ref](../../Projects/Ref.md \"Title\")")
    );
    assert!(moved.text.contains("[[Home|Home]]"));
    assert!(moved.text.contains("`[Home](../Home.md)`"));
    assert_eq!(
        vault.resolve(destination, "../../Home.md"),
        Some("Home.md".into())
    );
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Notes")).unwrap();
    fs::create_dir(root.path().join("Projects")).unwrap();
    fs::write(root.path().join("Home.md"), "# Home\n").unwrap();
    fs::write(root.path().join("Notes/Old.md"), "# Old\n[[Old]]\n").unwrap();
    fs::write(
        root.path().join("Projects/Ref.md"),
        "[[Notes/Old#Heading|Alias]] and [document](../Notes/Old.md#Heading).\n`[[Old]]`\n",
    )
    .unwrap();
    root
}

#[test]
fn rename_preserves_reference_labels_titles_and_angle_destinations() {
    let temp = fixture();
    fs::write(temp.path().join("Projects/Ref.md"), "[Inline](<../Notes/Old.md> \"Inline title\")\n[Reference][old]\n\n[old]: <../Notes/Old.md#Heading> \"Reference title\"\n\n`[old]: ../Notes/Old.md`\n").unwrap();
    let mut vault = Vault::open(temp.path()).unwrap();
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), Path::new("Deep/New Name.md"))
        .unwrap();
    vault.rename_note(plan).unwrap();
    let reference = fs::read_to_string(temp.path().join("Projects/Ref.md")).unwrap();
    assert!(
        reference.contains("[Inline](<../Deep/New%20Name.md> \"Inline title\")"),
        "{reference}"
    );
    assert!(reference.contains("[Reference][old]"));
    assert!(
        reference.contains("[old]: <../Deep/New%20Name.md#Heading> \"Reference title\""),
        "{reference}"
    );
    assert!(reference.contains("`[old]: ../Notes/Old.md`"));
}

#[test]
fn moving_a_note_preserves_relative_image_embeds() {
    let temp = fixture();
    fs::create_dir(temp.path().join("Assets")).unwrap();
    fs::write(temp.path().join("Assets/Image.png"), "image fixture").unwrap();
    fs::write(
        temp.path().join("Notes/Old.md"),
        "![Caption](../Assets/Image.png)\n![[../Assets/Image.png|180]]\n",
    )
    .unwrap();
    let mut vault = Vault::open(temp.path()).unwrap();
    let destination = Path::new("Deep/Nested/Moved.md");
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), destination)
        .unwrap();
    vault.rename_note(plan).unwrap();
    let moved = vault.read(destination).unwrap();
    assert!(moved.text.contains("![Caption](../../Assets/Image.png)"));
    assert!(moved.text.contains("![[Assets/Image.png|180]]"));
    assert_eq!(
        fs::read_to_string(temp.path().join("Assets/Image.png")).unwrap(),
        "image fixture"
    );
}

#[test]
fn rename_moves_without_clobbering_and_updates_real_links_only() {
    let root = fixture();
    let mut vault = Vault::open(root.path()).unwrap();
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), Path::new("Else/New Name.md"))
        .unwrap();
    assert!(
        !root.path().join("Else").exists(),
        "planning must be read-only"
    );
    let result = vault.rename_note(plan).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(!root.path().join("Notes/Old.md").exists());
    assert!(root.path().join("Else/New Name.md").is_file());
    let referrer = fs::read_to_string(root.path().join("Projects/Ref.md")).unwrap();
    assert!(referrer.contains("[[Else/New Name#Heading|Alias]]"));
    assert!(referrer.contains("(../Else/New%20Name.md#Heading)"));
    assert!(referrer.contains("`[[Old]]`"));
    assert!(
        fs::read_to_string(root.path().join("Else/New Name.md"))
            .unwrap()
            .contains("[[Else/New Name]]")
    );
    assert_eq!(
        vault.resolve(
            Path::new("Projects/Ref.md"),
            "../Else/New%20Name.md#Heading"
        ),
        Some("Else/New Name.md".into())
    );
}

#[test]
fn rename_preflight_rejects_collisions_external_edits_and_escapes() {
    let root = fixture();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(
        vault
            .plan_rename(Path::new("Notes/Old.md"), Path::new("Home.md"))
            .is_err()
    );
    assert!(
        vault
            .plan_rename(Path::new("Notes/Old.md"), Path::new("../Outside.md"))
            .is_err()
    );
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), Path::new("New.md"))
        .unwrap();
    fs::write(root.path().join("Projects/Ref.md"), "External version").unwrap();
    assert!(vault.rename_note(plan).is_err());
    assert!(root.path().join("Notes/Old.md").is_file());
    assert!(!root.path().join("New.md").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Projects/Ref.md")).unwrap(),
        "External version"
    );
    assert!(vault.trash_entry(Path::new("../Outside.md")).is_err());
}

#[test]
fn rename_never_replaces_a_destination_created_after_planning() {
    let root = fixture();
    let mut vault = Vault::open(root.path()).unwrap();
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), Path::new("New.md"))
        .unwrap();
    fs::write(root.path().join("New.md"), "Other editor created this").unwrap();
    assert!(vault.rename_note(plan).is_err());
    assert!(root.path().join("Notes/Old.md").is_file());
    assert_eq!(
        fs::read_to_string(root.path().join("New.md")).unwrap(),
        "Other editor created this"
    );
}

#[test]
fn rename_preserves_delimiters_inside_labels_and_titles() {
    let root = fixture();
    let text = r#"[label](Notes/Old.md "keep ](Notes/Old.md")

[ref]: Notes/Old.md "keep ]: Notes/Old.md"
[reference][ref]
[bracket \] label](Notes/Old.md "untouched")
[code `](` label](Notes/Old.md "code label")
[unpaired ` label](Notes/Old.md "a ` in the title")
[![image](cover.png)](Notes/Old.md "nested label")
`[literal](Notes/Old.md)`
"#;
    fs::write(root.path().join("Home.md"), text).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let plan = vault
        .plan_rename(Path::new("Notes/Old.md"), Path::new("Notes/New.md"))
        .unwrap();
    vault.rename_note(plan).unwrap();
    let updated = vault.read(Path::new("Home.md")).unwrap().text;
    assert_eq!(
        updated,
        text.replace("[label](Notes/Old.md", "[label](Notes/New.md")
            .replace("[ref]: Notes/Old.md", "[ref]: Notes/New.md")
            .replace("label](Notes/Old.md", "label](Notes/New.md")
            .replace(
                "[unpaired ` label](Notes/New.md",
                "[unpaired ` label](Notes/Old.md"
            )
            .replace("](Notes/Old.md \"nested", "](Notes/New.md \"nested")
    );
}

#[test]
fn empty_folders_are_indexed_created_and_refreshed_without_overwriting_entries() {
    let root = fixture();
    fs::create_dir(root.path().join("Empty")).unwrap();
    fs::create_dir(root.path().join(".hidden")).unwrap();
    fs::create_dir(root.path().join("node_modules")).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(vault.folders.contains(&Path::new("Empty").to_owned()));
    assert!(!vault.folders.contains(&Path::new(".hidden").to_owned()));
    assert!(
        !vault
            .folders
            .contains(&Path::new("node_modules").to_owned())
    );
    vault.create_folder(Path::new("Notes/New folder")).unwrap();
    assert!(root.path().join("Notes/New folder").is_dir());
    assert!(
        vault
            .folders
            .contains(&Path::new("Notes/New folder").to_owned())
    );
    for invalid in [
        "Notes/New folder",
        "Home.md",
        "../Outside",
        "/tmp/Outside",
        ".obsidian",
        "node_modules/New",
        "Missing/Child",
    ] {
        assert!(
            vault.create_folder(Path::new(invalid)).is_err(),
            "{invalid}"
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join("Home.md")).unwrap(),
        "# Home\n"
    );
    assert!(!root.path().join("Missing").exists());
    vault.refresh().unwrap();
    assert!(
        vault
            .folders
            .contains(&Path::new("Notes/New folder").to_owned())
    );
}

#[cfg(unix)]
#[test]
fn folder_creation_refuses_linked_parents_and_linked_targets() {
    use std::os::unix::fs::symlink;
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join("Linked")).unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    assert!(!vault.folders.contains(&Path::new("Linked").to_owned()));
    assert!(vault.create_folder(Path::new("Linked/Child")).is_err());
    assert!(vault.create_folder(Path::new("Linked")).is_err());
    assert!(!outside.path().join("Child").exists());
}

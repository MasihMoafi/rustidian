use rustidian::vault::{Document, Vault, headings, wiki_links};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

#[test]
fn immediate_notes_are_empty_indexed_and_never_replace_existing_files_or_directories() {
    let folder = tempdir().unwrap();
    fs::write(folder.path().join("Untitled.md"), "Keep original bytes\n").unwrap();
    fs::create_dir(folder.path().join("Untitled 1.md")).unwrap();
    let mut vault = Vault::open(folder.path()).unwrap();
    for name in ["Untitled 2.md", "Untitled 3.md"] {
        let document = vault.create_untitled().unwrap();
        assert_eq!(document.path, Path::new(name));
        assert!(document.text.is_empty() && !document.dirty());
        assert_eq!(fs::read(folder.path().join(name)).unwrap(), b"");
        assert!(vault.notes.iter().any(|note| note.path == document.path));
    }
    assert_eq!(
        fs::read_to_string(folder.path().join("Untitled.md")).unwrap(),
        "Keep original bytes\n"
    );
    assert!(folder.path().join("Untitled 1.md").is_dir());
    // A root that disappears while the app is open must report failure.
    let gone = tempdir().unwrap();
    let mut missing = Vault::open(gone.path()).unwrap();
    drop(gone);
    assert!(missing.create_untitled().is_err());
}

#[test]
fn plain_text_documents_open_and_save_without_bulk_indexing_or_overwriting_external_changes() {
    let folder = tempdir().unwrap();
    fs::write(folder.path().join("Home.md"), "# Home\n").unwrap();
    let names = [
        "Notes سلام.TXT",
        "main.rs",
        "script.py",
        "app.ts",
        "page.html",
        "config.json",
        "project.toml",
        "Makefile",
    ];
    for name in names {
        fs::write(folder.path().join(name), "// Literal text\r\nسلام\r\n").unwrap();
    }
    let mut vault = Vault::open(folder.path()).unwrap();
    for name in names {
        let filename = Path::new(name);
        let path = folder.path().join(filename);
        assert!(
            vault.attachments.iter().any(|entry| entry == filename),
            "{name} must appear in navigation"
        );
        let mut document = vault.open_document(filename).unwrap();
        assert!(document.is_plain_text());
        assert_eq!(document.text, "// Literal text\r\nسلام\r\n");
        document.text.push_str("Saved text\r\n");
        vault.save(&mut document).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            "// Literal text\r\nسلام\r\nSaved text\r\n".as_bytes()
        );
        assert!(!document.dirty());
        document.text.push_str("Unsaved text\n");
        fs::write(&path, "External change\n").unwrap();
        assert!(vault.save(&mut document).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "External change\n");
        assert!(document.dirty());
    }
    assert_eq!(
        vault.notes.len(),
        2,
        "Source code stays outside the note index; saved .txt files keep their existing indexing behavior"
    );
    assert_eq!(vault.notes[0].path, Path::new("Home.md"));
    assert!(
        vault
            .notes
            .iter()
            .any(|note| note.path == Path::new(names[0]))
    );
    fs::write(folder.path().join("binary.rs"), b"text\0binary").unwrap();
    assert!(vault.open_document(Path::new("binary.rs")).is_err());
    fs::write(folder.path().join("invalid.py"), [0xff, 0xfe]).unwrap();
    assert!(vault.open_document(Path::new("invalid.py")).is_err());
    assert_eq!(
        fs::read(folder.path().join("binary.rs")).unwrap(),
        b"text\0binary"
    );
    assert!(vault.read(Path::new("../escape.rs")).is_err());
}

#[test]
fn backlink_cache_tracks_saved_created_moved_and_externally_changed_notes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Home.md"), "[[Home]]").unwrap();
    fs::write(dir.path().join("A.md"), "[[Home]] [[Home]]\n`[[Home]]`").unwrap();
    fs::write(dir.path().join("Code.md"), "```md\n[[Home]]\n```").unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    assert_eq!(vault.backlinks(Path::new("Home.md")).len(), 1);
    assert!(vault.backlinks(Path::new("Missing.md")).is_empty());
    let mut a = vault.read(Path::new("A.md")).unwrap();
    a.text = "No links".into();
    vault.save(&mut a).unwrap();
    assert!(vault.backlinks(Path::new("Home.md")).is_empty());
    let mut new = Document::new("New.md".into());
    new.text = "[[Home]]".into();
    vault.save(&mut new).unwrap();
    assert_eq!(
        vault.backlinks(Path::new("Home.md"))[0].path,
        Path::new("New.md")
    );
    let plan = vault
        .plan_rename(Path::new("New.md"), Path::new("Deep/Moved.md"))
        .unwrap();
    vault.rename_note(plan).unwrap();
    assert_eq!(
        vault.backlinks(Path::new("Home.md"))[0].path,
        Path::new("Deep/Moved.md")
    );
    assert_eq!(
        vault.resolve(Path::new("A.md"), "Moved"),
        Some("Deep/Moved.md".into())
    );
    assert!(vault.resolve(Path::new("A.md"), "New").is_none());
    fs::write(dir.path().join("Deep/Moved.md"), "No links now").unwrap();
    vault.refresh().unwrap();
    assert!(vault.backlinks(Path::new("Home.md")).is_empty());
}

#[test]
fn discovers_markdown_without_indexing_dependencies_or_hidden_data() {
    let dir = tempdir().unwrap();
    for path in [
        "Notes",
        "node_modules",
        ".obsidian-data",
        ".git",
        "__pycache__",
        "target",
    ] {
        fs::create_dir(dir.path().join(path)).unwrap();
    }
    for path in [
        "Home.md",
        "Notes/سلام.MD",
        "node_modules/no.md",
        ".obsidian-data/no.md",
        "target/no.md",
        "__pycache__/no.md",
        ".git/no.md",
        ".gitignore.md",
    ] {
        fs::write(dir.path().join(path), "# note").unwrap();
    }
    let vault = Vault::open(dir.path()).unwrap();
    assert_eq!(vault.notes.len(), 2);
    assert_eq!(vault.folders, vec![PathBuf::from("Notes")]);
    assert!(
        vault
            .notes
            .iter()
            .any(|note| note.path == Path::new("Notes/سلام.MD"))
    );
}

#[test]
fn edits_and_reopens_unicode_without_changing_other_files() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Note.md"), "# Original\n").unwrap();
    fs::write(dir.path().join("Other.md"), "leave me alone").unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    let mut document = vault.read(Path::new("Note.md")).unwrap();
    document.text = "# سلام\nRust 🦀\n".into();
    assert!(document.dirty());
    vault.save(&mut document).unwrap();
    assert!(!document.dirty());
    assert_eq!(
        vault.read(Path::new("Note.md")).unwrap().text,
        document.text
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("Other.md")).unwrap(),
        "leave me alone"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn refuses_external_edits_deletion_and_new_note_collisions() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Note.md"), "original").unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    let mut document = vault.read(Path::new("Note.md")).unwrap();
    document.text = "my change".into();
    fs::write(dir.path().join("Note.md"), "someone else's change").unwrap();
    assert!(
        vault
            .save(&mut document)
            .unwrap_err()
            .contains("changed outside")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("Note.md")).unwrap(),
        "someone else's change"
    );
    assert!(document.dirty());
    fs::remove_file(dir.path().join("Note.md")).unwrap();
    assert!(vault.save(&mut document).is_err());
    assert!(!dir.path().join("Note.md").exists());
    let mut new = Document::new("Fresh.md".into());
    fs::write(dir.path().join("Fresh.md"), "taken").unwrap();
    assert!(vault.save(&mut new).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("Fresh.md")).unwrap(),
        "taken"
    );
}

#[test]
fn creates_nested_notes_and_rejects_paths_outside_the_vault() {
    let dir = tempdir().unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    let mut new = Document::new("Work/Today.md".into());
    new.text = "# Today\n".into();
    vault.save(&mut new).unwrap();
    assert_eq!(
        vault.read(Path::new("Work/Today.md")).unwrap().text,
        "# Today\n"
    );
    assert!(vault.read(Path::new("../escape.md")).is_err());
    assert!(
        vault
            .save(&mut Document::new("../escape.md".into()))
            .is_err()
    );
    assert!(
        vault
            .save(&mut Document::new("/tmp/escape.md".into()))
            .is_err()
    );
    assert!(
        vault
            .save(&mut Document::new("not-a-text-document.bin".into()))
            .is_err()
    );
}

#[test]
fn resolves_links_and_backlinks_without_guessing_ambiguous_names() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("Notes")).unwrap();
    fs::create_dir(dir.path().join("Other")).unwrap();
    fs::write(
        dir.path().join("Home.md"),
        "[[Notes/Idea|idea]] [[Unique#Heading]]",
    )
    .unwrap();
    fs::write(dir.path().join("Notes/Idea.md"), "[[Home]]").unwrap();
    fs::write(dir.path().join("Other/Idea.md"), "unrelated").unwrap();
    fs::write(dir.path().join("Unique.md"), "# Heading").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    assert_eq!(
        vault.resolve(Path::new("Home.md"), "Notes/Idea|idea"),
        Some("Notes/Idea.md".into())
    );
    assert_eq!(
        vault.resolve(Path::new("Home.md"), "Unique#Heading"),
        Some("Unique.md".into())
    );
    assert_eq!(vault.resolve(Path::new("Home.md"), "Idea"), None);
    assert_eq!(
        vault.resolve(Path::new("Notes/Idea.md"), "Idea"),
        Some("Notes/Idea.md".into())
    );
    assert_eq!(vault.backlinks(Path::new("Notes/Idea.md")).len(), 1);
    assert!(
        vault
            .search("unrelated")
            .iter()
            .any(|note| note.path == Path::new("Other/Idea.md"))
    );
    assert!(vault.search("absent").is_empty());
}

#[test]
fn markdown_metadata_ignores_fenced_code() {
    let source = "# Title\n[[Real|label]]\n```rust\n# Not a heading\n[[Fake]]\n```\n## Second\n";
    assert_eq!(wiki_links(source), vec!["Real|label"]);
    assert_eq!(
        headings(source),
        vec![(1, "Title".into()), (2, "Second".into())]
    );
}

#[test]
fn wiki_links_ignore_inline_code_and_escaped_openers() {
    assert_eq!(
        wiki_links("`[[Example]]` and [[Real]] and \\[[Escaped]]"),
        vec!["Real"]
    );
    assert_eq!(wiki_links("``a ` [[Example]]`` and [[Real]]"), vec!["Real"]);
    assert_eq!(
        wiki_links("an unmatched ` is literal: [[Real]]"),
        vec!["Real"]
    );
}

#[cfg(unix)]
#[test]
fn ignores_and_refuses_symlink_files_and_preserves_save_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("Secret.md"), "secret").unwrap();
    symlink(outside.path(), dir.path().join("Linked")).unwrap();
    symlink(
        outside.path().join("Secret.md"),
        dir.path().join("Secret.md"),
    )
    .unwrap();
    fs::write(dir.path().join("Own.md"), "own").unwrap();
    fs::set_permissions(dir.path().join("Own.md"), fs::Permissions::from_mode(0o640)).unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    assert_eq!(vault.notes.len(), 1);
    assert!(vault.read(Path::new("Secret.md")).is_err());
    assert!(
        vault
            .save(&mut Document::new("Linked/New.md".into()))
            .is_err()
    );
    let mut document = vault.read(Path::new("Own.md")).unwrap();
    document.text = "updated".into();
    vault.save(&mut document).unwrap();
    assert_eq!(
        fs::metadata(dir.path().join("Own.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("Secret.md")).unwrap(),
        "secret"
    );
}

#[test]
fn opening_a_vault_is_read_only_and_invalid_utf8_is_reported() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Good.md"), "hello").unwrap();
    fs::write(dir.path().join("Broken.md"), [0xff, 0xfe]).unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    assert_eq!(vault.notes.len(), 1);
    assert_eq!(vault.warnings.len(), 1);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert!(Vault::open(&dir.path().join("missing")).is_err());
}
#[test]
fn follows_or_creates_links_without_overwriting_or_guessing_ambiguous_names() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Home.md"), "# Existing\n").unwrap();
    let mut vault = rustidian::vault::Vault::open(root.path()).unwrap();
    let existing = vault.open_link("Home.md".as_ref(), "Home").unwrap();
    assert_eq!(existing.text, "# Existing\n");
    let created = vault
        .open_link("Home.md".as_ref(), "Notes/你好.v1|Read this")
        .unwrap();
    assert_eq!(created.path.to_str(), Some("Notes/你好.v1.md"));
    assert!(root.path().join("Notes/你好.v1.md").is_file());
    assert!(vault.open_link("Home.md".as_ref(), "../Outside").is_err());
    std::fs::create_dir(root.path().join("Other")).unwrap();
    std::fs::write(root.path().join("Other/你好.v1.md"), "# Other\n").unwrap();
    vault.refresh().unwrap();
    assert!(vault.open_link("Home.md".as_ref(), "你好.v1").is_err());
    assert!(!root.path().join("你好.v1.md").exists());
}

#[test]
fn markdown_backlinks_include_references_and_embeds_but_exclude_code_and_external_urls() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("Notes")).unwrap();
    fs::write(dir.path().join("Home page.md"), "[Self](Home%20page.md)").unwrap();
    fs::write(
        dir.path().join("Notes/Relative.md"),
        "[Home](../Home%20page.md#Title) [[Home page]]",
    )
    .unwrap();
    fs::write(
        dir.path().join("Reference.md"),
        "[Home][ref]\n\n[ref]: <Home page.md> \"Title\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("Embed.md"), "![Home](Home%20page.md)").unwrap();
    fs::write(dir.path().join("Excluded.md"), "`[Home](Home%20page.md)`\n\n```md\n[Home](Home%20page.md)\n```\n\n[External](https://example.org/Home%20page.md)\n[Missing](Missing.md)").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let paths: Vec<_> = vault
        .backlinks(Path::new("Home page.md"))
        .iter()
        .map(|note| note.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            Path::new("Embed.md"),
            Path::new("Notes/Relative.md"),
            Path::new("Reference.md")
        ]
    );
    assert!(vault.backlinks(Path::new("Missing.md")).is_empty());
}

#[test]
fn text_links_resolve_relative_encoded_paths_without_creating_markdown_copies() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("Notes")).unwrap();
    fs::write(
        dir.path().join("Notes/From.md"),
        "[Text](../Plain%20text.txt)",
    )
    .unwrap();
    fs::write(dir.path().join("Plain text.txt"), "Original text\r\n").unwrap();
    let mut vault = Vault::open(dir.path()).unwrap();
    assert_eq!(
        vault.notes.len(),
        1,
        "Unopened text files must not expand the note index"
    );
    let document = vault
        .open_link(Path::new("Notes/From.md"), "../Plain%20text.txt")
        .unwrap();
    assert_eq!(document.path, Path::new("Plain text.txt"));
    assert_eq!(document.text, "Original text\r\n");
    assert_eq!(vault.notes.len(), 1);
    assert!(!dir.path().join("Plain text.txt.md").exists());
    assert!(
        vault
            .open_link(Path::new("Notes/From.md"), "../../Outside.txt")
            .is_err()
    );
}

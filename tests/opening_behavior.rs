use rustidian::{
    commands::{self, Scope},
    vault::Vault,
    vaults::resolve_input,
};
use std::{fs, path::Path};

#[test]
fn existing_file_links_use_encoded_desktop_uris_without_creating_or_escaping_notes() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("Vault");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Home.md"), "# Home\n").unwrap();
    let outside = base.path().join("record #1.json");
    fs::write(&outside, "original evidence").unwrap();
    let mut vault = Vault::open(&root).unwrap();
    let uri = format!(
        "file://{}/record%20%231.json",
        base.path().canonicalize().unwrap().display()
    );
    assert_eq!(
        vault.external_file_uri(Path::new("Home.md"), "../record%20%231.json"),
        Some(uri)
    );
    for target in ["Home.md", "../Missing.json", ".."] {
        assert!(
            vault
                .external_file_uri(Path::new("Home.md"), target)
                .is_none()
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("Escaping.json")).unwrap();
        assert!(
            vault
                .external_file_uri(Path::new("Home.md"), "Escaping.json")
                .is_none()
        );
    }
    // The external opening route does not relax the vault's read/write boundary.
    assert!(
        vault
            .open_link(Path::new("Home.md"), "../record%20%231.json")
            .is_err()
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "original evidence");
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        if cfg!(unix) { 2 } else { 1 }
    );
}

#[test]
fn file_input_selects_the_requested_document_and_its_folder() {
    let folder = tempfile::tempdir().unwrap();
    for filename in ["Read me سلام.md", "Plain text.TXT"] {
        let path = folder.path().join(filename);
        fs::write(&path, "Original content").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        let (root, document) = resolve_input(&path, None, &[]).unwrap();
        assert_eq!(root, folder.path().canonicalize().unwrap());
        assert_eq!(document.as_deref(), Some(Path::new(filename)));
        assert_eq!(
            Vault::open(&root)
                .unwrap()
                .open_document(document.as_deref().unwrap())
                .unwrap()
                .text,
            "Original content"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "Original content");
    }
    assert!(resolve_input(&folder.path().join("Missing.md"), None, &[]).is_err());
    let unsupported = folder.path().join("Not a text document.pdf");
    fs::write(&unsupported, "%PDF-1.7").unwrap();
    assert!(resolve_input(&unsupported, None, &[]).is_err());
    let invalid = folder.path().join("Not UTF-8.md");
    fs::write(&invalid, [0xff, 0xfe]).unwrap();
    let (root, document) = resolve_input(&invalid, None, &[]).unwrap();
    assert!(
        Vault::open(&root)
            .unwrap()
            .open_document(document.as_deref().unwrap())
            .is_err()
    );
    assert_eq!(fs::read(invalid).unwrap(), [0xff, 0xfe]);
}

#[test]
fn files_inside_the_default_vault_keep_that_root() {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir(folder.path().join("Nested")).unwrap();
    let path = folder.path().join("Nested/Note.md");
    fs::write(&path, "A note").unwrap();
    let (root, document) = resolve_input(&path, Some(folder.path()), &[]).unwrap();
    assert_eq!(root, folder.path().canonicalize().unwrap());
    assert_eq!(document.as_deref(), Some(Path::new("Nested/Note.md")));
    let outside = tempfile::tempdir().unwrap();
    let (_, document) = resolve_input(&path, Some(outside.path()), &[]).unwrap();
    assert_eq!(document.as_deref(), Some(Path::new("Note.md")));
    let (root, document) = resolve_input(folder.path(), None, &[]).unwrap();
    assert_eq!(root, folder.path().canonicalize().unwrap());
    assert!(document.is_none());
}

#[test]
fn registered_vaults_keep_nested_files_and_do_not_match_neighbouring_names() {
    let base = tempfile::tempdir().unwrap();
    let outer = base.path().join("Vault");
    let inner = outer.join("Inner vault");
    let notes = inner.join("Notes");
    fs::create_dir_all(&notes).unwrap();
    let file = notes.join("Note.md");
    fs::write(&file, "[[Home]]").unwrap();
    fs::write(inner.join("Home.md"), "# Home\n").unwrap();
    #[cfg(unix)]
    let alias = base.path().join("Registered vault alias");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&inner, &alias).unwrap();
    #[cfg(not(unix))]
    let alias = inner.join(".");
    let roots = [outer.clone(), alias];
    let (root, document) = resolve_input(&file, Some(&outer), &roots).unwrap();
    assert_eq!(root, inner);
    assert_eq!(document.as_deref(), Some(Path::new("Notes/Note.md")));
    let vault = Vault::open(&root).unwrap();
    let note = vault.open_document(document.as_deref().unwrap()).unwrap();
    assert_eq!(vault.resolve(&note.path, "Home"), Some("Home.md".into()));
    assert_eq!(fs::read_to_string(&file).unwrap(), "[[Home]]");
    let neighbour = base.path().join("Vault-other/Notes");
    fs::create_dir_all(&neighbour).unwrap();
    let file = neighbour.join("Note.md");
    fs::write(&file, "Original").unwrap();
    let (root, document) = resolve_input(&file, Some(&outer), &roots).unwrap();
    assert_eq!(root, neighbour);
    assert_eq!(document.as_deref(), Some(Path::new("Note.md")));
    assert_eq!(fs::read_to_string(file).unwrap(), "Original");
}

#[test]
fn a_registered_vault_enclosing_the_default_vault_never_claims_outside_files() {
    // Masih's list holds ~/Desktop (92,095 folders) beside the default ~/Desktop/o; a
    // document under Desktop but outside o opens with its own folder, not all of Desktop.
    let base = tempfile::tempdir().unwrap();
    let desktop = base.path().join("Desktop");
    let default = desktop.join("o");
    let legal = desktop.join("tmp/legal");
    fs::create_dir_all(&default).unwrap();
    fs::create_dir_all(&legal).unwrap();
    let file = legal.join("brief.md");
    fs::write(&file, "# Brief\n").unwrap();
    let roots = [desktop.clone(), default.clone()];
    let (root, document) = resolve_input(&file, Some(&default), &roots).unwrap();
    assert_eq!(root, legal.canonicalize().unwrap());
    assert_eq!(document.as_deref(), Some(Path::new("brief.md")));
    let inside = default.join("Note.md");
    fs::write(&inside, "# Note\n").unwrap();
    let (root, document) = resolve_input(&inside, Some(&default), &roots).unwrap();
    assert_eq!(root, default.canonicalize().unwrap());
    assert_eq!(document.as_deref(), Some(Path::new("Note.md")));
}

#[test]
fn system_open_commands_are_available_without_a_vault_and_preserve_quick_switcher() {
    for name in [commands::OPEN_FILE, commands::OPEN_FOLDER] {
        assert_eq!(commands::ranked(name, Scope::Global)[0].name, name);
    }
    let quick = commands::ALL
        .iter()
        .find(|command| command.name == "Open a file")
        .unwrap();
    assert_eq!(quick.shortcut, "Ctrl+O");
    for shortcut in [commands::OPEN_FILE_SHORTCUT, commands::OPEN_FOLDER_SHORTCUT] {
        assert_eq!(
            commands::ALL
                .iter()
                .filter(|command| command.shortcut == shortcut)
                .count(),
            1
        );
        assert_ne!(quick.shortcut, shortcut);
    }
}

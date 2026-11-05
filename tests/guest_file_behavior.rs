//! A file of another vault opens beside the notes being read (4 Nov: a second
//! agent's file replaced the first one). It stays where it is.
use rustidian::vault::Vault;
use std::fs;

#[test]
fn a_guest_file_is_read_and_saved_where_it_is_and_never_enters_the_index() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("Agent note.md");
    fs::write(&file, "first\n").unwrap();
    let mut vault = Vault::open(home.path()).unwrap();
    let notes = vault.notes.len();

    let mut document = vault.open_guest(&file).unwrap();
    assert!(document.path.is_absolute() && vault.is_guest(&document.path));
    assert_eq!(document.text, "first\n");
    document.text = "second\n".into();
    vault.save(&mut document).unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "second\n");
    assert_eq!(vault.notes.len(), notes, "A guest file entered the index");
    assert!(vault.attachments.is_empty());
}

#[test]
fn a_guest_file_changed_outside_is_not_overwritten() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("Agent note.md");
    fs::write(&file, "first\n").unwrap();
    let mut vault = Vault::open(home.path()).unwrap();
    let mut document = vault.open_guest(&file).unwrap();
    fs::write(&file, "an agent's edit\n").unwrap();
    document.text = "mine\n".into();
    assert!(vault.save(&mut document).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), "an agent's edit\n");
}

#[test]
fn only_a_file_outside_the_vault_opens_as_a_guest_and_only_once_opened() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("Agent note.md");
    let stranger = elsewhere.path().join("Stranger.md");
    fs::write(&file, "first\n").unwrap();
    fs::write(&stranger, "never opened\n").unwrap();
    let mut vault = Vault::open(home.path()).unwrap();

    assert!(vault.open_guest(&home.path().join("Home.md")).is_err());
    assert!(
        vault.read(&stranger).is_err(),
        "An absolute path nobody opened must stay refused"
    );
    vault.open_guest(&file).unwrap();
    assert!(vault.read(&stranger).is_err());
    assert!(
        vault
            .open_guest(&elsewhere.path().join("Missing.md"))
            .is_err()
    );
    assert!(!vault.is_guest(&elsewhere.path().join("Missing.md")));
}

#[test]
fn a_link_to_a_note_outside_the_vault_names_that_file_not_a_browser_page() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let note = elsewhere.path().join("Other.md");
    fs::write(&note, "x\n").unwrap();
    let vault = Vault::open(home.path()).unwrap();
    let link = note.to_string_lossy().replace(' ', "%20");

    let found = vault.external_file(std::path::Path::new("Home.md"), &link);

    assert_eq!(found, Some(note.canonicalize().unwrap()));
    assert_eq!(
        vault.external_file(std::path::Path::new("Home.md"), "Home.md"),
        None
    );
}

#[test]
fn a_heading_is_found_by_the_slug_a_markdown_link_gives() {
    let text = "# Guide\n\n## Runtime Architecture\n\ntext\n\n## Q&A: what's new?\n";
    assert!(rustidian::structure::find_anchor(text, "runtime-architecture").is_some());
    assert!(rustidian::structure::find_anchor(text, "Runtime Architecture").is_some());
    assert!(rustidian::structure::find_anchor(text, "qa-whats-new").is_some());
    assert!(rustidian::structure::find_anchor(text, "missing-heading").is_none());
}

#[cfg(unix)]
#[test]
fn a_link_reached_through_a_symlinked_folder_still_names_the_file() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let real = tempfile::tempdir().unwrap();
    fs::write(real.path().join("Guide.md"), "# Guide\n").unwrap();
    let link = tempfile::tempdir().unwrap().keep().join("docs");
    std::os::unix::fs::symlink(real.path(), &link).unwrap();
    let vault = Vault::open(home.path()).unwrap();

    let found = vault.external_file(
        std::path::Path::new("Home.md"),
        &format!("{}/Guide.md", link.display()),
    );

    assert_eq!(
        found,
        Some(real.path().join("Guide.md").canonicalize().unwrap())
    );
}

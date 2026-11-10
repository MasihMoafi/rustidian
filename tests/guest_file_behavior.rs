//! A file of another vault opens beside the notes being read (4 Nov: a second
//! agent's file replaced the first one). It stays where it is.
use rustidian::vault::Vault;
use std::fs;

/// 9 Nov: searching a file's name found nothing, because the file was open in
/// a tab from outside the vault. Open outside files are searched too: by name
/// in any case, by their text, and by text typed but not saved; a file not
/// open in a tab stays out.
#[test]
fn search_finds_an_open_outside_file_by_name_and_by_its_text() {
    use rustidian::{query::Cache, vault::Document};
    use std::{path::PathBuf, sync::mpsc, time::Duration};

    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("Home.md"),
        "# Home\nsee the guide in guide.md\n",
    )
    .unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("GUIDE.md");
    fs::write(&file, "only here: zanzibar\n").unwrap();
    let closed = elsewhere.path().join("Closed.md");
    fs::write(&closed, "zanzibar too\n").unwrap();
    let mut vault = Vault::open(home.path()).unwrap();
    let mut open = vault.open_guest(&file).unwrap();
    vault.open_guest(&closed).unwrap();
    let mut cache = Cache::default();
    let mut search = |query: &str, open: &Document| {
        cache.invalidate();
        let (wake, done) = mpsc::sync_channel(1);
        cache.update(query, &vault, &[open], move || {
            let _ = wake.send(());
        });
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        cache.update(query, &vault, &[open], || {});
        assert!(cache.error.is_none(), "{query}: {:?}", cache.error);
        cache.paths.clone()
    };

    for name in ["guide.md", "GUIDE.MD", "Guide"] {
        assert_eq!(search(name, &open)[0], open.path, "{name}");
    }
    assert_eq!(search("zanzibar", &open), vec![open.path.clone()]);
    assert!(search("guide", &open).contains(&PathBuf::from("Home.md")));
    open.text = "typed, not saved: quokka\n".into();
    assert_eq!(search("quokka", &open), vec![open.path.clone()]);
}

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

/// 7 Nov: a file opened from outside (`op`) showed "Notes must stay inside the
/// vault" after every rescan, because the rescanned vault forgot its guests.
#[test]
fn a_rescan_keeps_the_guest_files_readable() {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("Home.md"), "# Home\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("PRIORITIES.md");
    fs::write(&file, "first\n").unwrap();
    let mut vault = Vault::open(home.path()).unwrap();
    let document = vault.open_guest(&file).unwrap();

    let mut rescanned = Vault::open(home.path()).unwrap();
    rescanned.keep_guests(&vault);
    assert_eq!(
        rescanned.open_document(&document.path).unwrap().text,
        "first\n"
    );
}

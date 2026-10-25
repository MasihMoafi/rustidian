use rustidian::vault::Vault;
use std::{fs, path::Path};

// One test per binary: it points XDG_DATA_HOME at a private Trash, so nothing
// reaches the real desktop Trash and no parallel test sees the variable.
#[test]
fn folders_and_files_move_to_trash_with_everything_inside() {
    // Under HOME, so the Trash is on the home filesystem (a RAM /tmp is not); a fresh CI runner has no ~/.cache yet.
    let cache = Path::new(&std::env::var_os("HOME").unwrap()).join(".cache");
    fs::create_dir_all(&cache).unwrap();
    let scratch = tempfile::tempdir_in(cache).unwrap();
    let data_home = scratch.path().join("data");
    fs::create_dir(&data_home).unwrap();
    // SAFETY: the only test in this binary, set before any thread reads the environment.
    unsafe { std::env::set_var("XDG_DATA_HOME", &data_home) };
    let root = scratch.path().join("vault");
    fs::create_dir_all(root.join("Folder/Sub")).unwrap();
    fs::write(root.join("Folder/A.md"), "# A\n").unwrap();
    fs::write(root.join("Folder/Sub/B.md"), "# B\n").unwrap();
    fs::write(root.join("Folder/Sub/picture.png"), [137, 80, 78, 71]).unwrap();
    fs::write(root.join("Other.md"), "[[A]]\n").unwrap();
    let mut vault = Vault::open(&root).unwrap();

    assert!(vault.trash_entry(Path::new("Missing")).is_err());
    assert!(vault.trash_entry(Path::new("../vault")).is_err());

    vault.trash_entry(Path::new("Folder")).unwrap();
    assert!(!root.join("Folder").exists());
    let trashed = data_home.join("Trash/files/Folder");
    assert_eq!(
        fs::read_to_string(trashed.join("Sub/B.md")).unwrap(),
        "# B\n"
    );
    assert!(trashed.join("Sub/picture.png").is_file());
    let notes: Vec<_> = vault.notes.iter().map(|note| note.path.clone()).collect();
    assert_eq!(notes, [Path::new("Other.md")]);
    assert!(
        vault
            .folders
            .iter()
            .all(|folder| !folder.starts_with("Folder"))
    );
    assert!(vault.attachments.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("Other.md")).unwrap(),
        "[[A]]\n"
    );

    vault.trash_entry(Path::new("Other.md")).unwrap();
    assert!(!root.join("Other.md").exists());
    assert!(data_home.join("Trash/files/Other.md").is_file());
    assert!(vault.notes.is_empty());
}

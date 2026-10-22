use rust_notes::search::FileMatcher;
use std::path::PathBuf;

#[test]
fn folder_matching_keeps_dotted_names_root_and_current_folder() {
    let root = tempfile::tempdir().unwrap();
    for folder in ["Version 1.2", "Other", "Nested/Folder"] {
        std::fs::create_dir_all(root.path().join(folder)).unwrap();
    }
    let vault = rust_notes::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::folders(&vault, std::path::Path::new("Other"));
    assert_eq!(matcher.rank("", 20)[0], PathBuf::from("Other"));
    assert_eq!(matcher.rank("1.2", 20), vec![PathBuf::from("Version 1.2")]);
    let slash = matcher.rank("/", 20);
    assert!(slash.contains(&PathBuf::new()));
    assert!(slash.contains(&PathBuf::from("Nested/Folder")));
    let all = matcher.rank("", 20);
    assert_eq!(
        all.iter()
            .filter(|path| **path == PathBuf::from("Other"))
            .count(),
        1
    );
    assert!(matcher.rank("missing", 20).is_empty());
}

#[test]
fn fuzzy_file_matching_ranks_unicode_and_abbreviations_without_false_empty_hits() {
    let paths: Vec<_> = [
        "Notes/Connected thinking.md",
        "Notes/First steps.md",
        "你好.md",
        "Projects/Native notebook.md",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    let mut matcher = FileMatcher::new(paths);
    assert_eq!(
        matcher.rank("fststp", 3),
        vec![PathBuf::from("Notes/First steps.md")]
    );
    assert_eq!(matcher.rank("你好", 3), vec![PathBuf::from("你好.md")]);
    assert!(matcher.rank("impossible_xyz", 3).is_empty());
    assert_eq!(matcher.rank("", 2).len(), 2);
}
#[test]
fn aliases_are_fuzzy_matched_without_duplicate_file_results() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Home.md"),
        "---\naliases: [Thinking room, Home]\n---\n# Home\n",
    )
    .unwrap();
    std::fs::write(root.path().join("Other.md"), "# Other\n").unwrap();
    let vault = rust_notes::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::from_notes(&vault.notes);
    assert_eq!(
        matcher.rank("thnkrm", 10),
        vec![std::path::PathBuf::from("Home.md")]
    );
    assert_eq!(matcher.rank("", 10).len(), 2);
    assert_eq!(matcher.rank("Home", 10).len(), 1);
}

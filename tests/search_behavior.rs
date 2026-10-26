use rustidian::search::FileMatcher;
use std::path::PathBuf;

#[test]
fn exact_source_filename_and_qualified_path_outrank_fuzzy_siblings() {
    let root = tempfile::tempdir().unwrap();
    let paths = [
        "Native language.ts",
        "Native language.tsx",
        "A/Native language.ts",
        "B/Native language.ts",
        "B/Native language.tsx",
        "AB/Native language.ts",
        "Nested/config.ts",
        "config.tsx",
    ];
    for path in paths {
        let file = root.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "const value = 1;\n").unwrap();
    }
    let vault = rustidian::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::from_vault(&vault);
    for query in ["Native language.ts", "Native language.tsx"] {
        assert_eq!(
            matcher.rank(query, 1),
            vec![PathBuf::from(query)],
            "An exact source filename must not select its fuzzy sibling"
        );
    }
    for query in ["A/Native language.ts", "B/Native language.ts"] {
        assert_eq!(
            matcher.rank(query, 1),
            vec![PathBuf::from(query)],
            "A qualified path must select its folder when basenames repeat"
        );
    }
    assert_eq!(
        matcher.rank("config.ts", 1),
        vec![PathBuf::from("Nested/config.ts")],
        "An exact filename in a folder must outrank a fuzzy root filename"
    );
    assert_eq!(matcher.rank("", 20).len(), paths.len());
    assert!(matcher.rank("Native language.ts", 0).is_empty());
}

#[test]
fn file_query_words_match_in_order_without_reusing_the_shared_prefix() {
    let mut matcher = FileMatcher::new(
        ["Closed A.md", "Closed B.md", "Closed C.md"]
            .into_iter()
            .map(PathBuf::from),
    );
    for query in ["Closed C", "closed c", "ClsdC"] {
        assert_eq!(matcher.rank(query, 10), vec![PathBuf::from("Closed C.md")]);
    }
    assert_eq!(
        matcher.rank("Closed B", 10),
        vec![PathBuf::from("Closed B.md")]
    );
    assert!(matcher.rank("Closed Closed", 10).is_empty());
    assert!(matcher.rank("C Closed", 10).is_empty());
    assert_eq!(matcher.rank("", 2).len(), 2);
    assert!(matcher.rank("Closed C", 0).is_empty());
}

#[test]
fn ordered_queries_keep_aliases_unicode_paths_and_deduplication() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Notes")).unwrap();
    std::fs::write(
        root.path().join("Notes/Thinking circle.md"),
        "---\naliases: [Thinking circle, یادداشت فارسی]\n---\n# Circle\n",
    )
    .unwrap();
    std::fs::write(root.path().join("Notes/Thinking cat.md"), "# Cat\n").unwrap();
    let vault = rustidian::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::from_notes(&vault.notes);
    for query in ["Thinking circle", "Notes/Thinking circle", "یادداشت فارسی"] {
        assert_eq!(
            matcher.rank(query, 10),
            vec![PathBuf::from("Notes/Thinking circle.md")]
        );
    }
    assert_eq!(matcher.rank("", 10).len(), 2);
    assert!(matcher.rank("circle Thinking", 10).is_empty());
}

#[test]
fn folder_matching_keeps_dotted_names_root_and_current_folder() {
    let root = tempfile::tempdir().unwrap();
    for folder in ["Version 1.2", "Other", "Nested/Folder"] {
        std::fs::create_dir_all(root.path().join(folder)).unwrap();
    }
    let vault = rustidian::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::folders(&vault, std::path::Path::new("Other"));
    assert_eq!(matcher.rank("", 20)[0], PathBuf::from("Other"));
    assert_eq!(matcher.rank("1.2", 20), vec![PathBuf::from("Version 1.2")]);
    let slash = matcher.rank("/", 20);
    assert!(slash.contains(&PathBuf::new()));
    assert!(slash.contains(&PathBuf::from("Nested/Folder")));
    let all = matcher.rank("", 20);
    assert_eq!(
        all.iter()
            .filter(|path| path.as_path() == std::path::Path::new("Other"))
            .count(),
        1
    );
    assert!(matcher.rank("missing", 20).is_empty());
    assert_eq!(matcher.rank_folders("/", 20)[0], std::path::Path::new(""));
    assert_eq!(
        matcher.rank_folders("Other", 20)[0],
        std::path::Path::new("Other")
    );
    assert_eq!(
        matcher.rank_folders("", 20)[0],
        std::path::Path::new("Other")
    );
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
    let vault = rustidian::vault::Vault::open(root.path()).unwrap();
    let mut matcher = FileMatcher::from_notes(&vault.notes);
    assert_eq!(
        matcher.rank("thnkrm", 10),
        vec![std::path::PathBuf::from("Home.md")]
    );
    assert_eq!(matcher.rank("", 10).len(), 2);
    assert_eq!(matcher.rank("Home", 10).len(), 1);
}

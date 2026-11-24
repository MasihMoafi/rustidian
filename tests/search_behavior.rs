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

/// 10 Nov: Ctrl+O "agents.md" showed an unrelated note, because the file was
/// AGENTS.md outside the vault. Files elsewhere are listed (hidden, dependency
/// folders and the vault itself left out) and an exact name wins in any case.
#[test]
fn ctrl_o_finds_a_file_outside_the_vault_by_its_exact_name_in_any_case() {
    let home = tempfile::tempdir().unwrap();
    let vault = home.path().join("notes");
    for (path, text) in [
        ("notes/Steering agents with CLAUDE.md.md", "in the vault"),
        ("p/AGENTS.md", "rules"),
        ("p/app/node_modules/pkg/AGENTS.md", "dependency"),
        ("p/.cache/AGENTS.md", "hidden"),
        ("p/cover.png", "not text"),
    ] {
        let file = home.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let found = rustidian::elsewhere::scan(home.path(), &vault);
    assert_eq!(found, vec![home.path().join("p/AGENTS.md")], "{found:?}");

    let mut matcher = FileMatcher::elsewhere(home.path(), found);
    for query in ["agents.md", "AGENTS.MD", "Agents.md"] {
        assert_eq!(
            matcher.rank(query, 5)[0],
            home.path().join("p/AGENTS.md"),
            "{query}"
        );
    }
    let mut vault_files = FileMatcher::new([PathBuf::from("Agents.md"), PathBuf::from("Other.md")]);
    assert_eq!(
        vault_files.rank("agents.md", 5)[0],
        PathBuf::from("Agents.md")
    );
}

#[test]
fn ctrl_o_lists_the_least_nested_file_of_a_name_first() {
    let home = tempfile::tempdir().unwrap();
    let names = [
        "p/app/docs/AGENTS.md",
        "p/rustidian/AGENTS.md",
        "p/AGENTS.md",
    ];
    for path in names {
        let file = home.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "rules").unwrap();
    }
    let found = rustidian::elsewhere::scan(home.path(), &home.path().join("vault"));
    let shallow_first: Vec<_> = names
        .iter()
        .rev()
        .map(|path| home.path().join(path))
        .collect();
    assert_eq!(found, shallow_first, "nearest folders are read first");
    let mut matcher = FileMatcher::elsewhere(home.path(), found.into_iter().rev().collect());
    assert_eq!(matcher.rank("agents.md", 3), shallow_first);
}

#[test]
fn ctrl_o_finds_folders_by_name_least_nested_first() {
    let home = tempfile::tempdir().unwrap();
    for folder in [
        "p/skills",
        "p/Matilda/skills",
        "p/.hidden/skills",
        "p/app/node_modules/skills",
    ] {
        std::fs::create_dir_all(home.path().join(folder)).unwrap();
    }
    std::fs::write(home.path().join("p/skills/README.md"), "skills").unwrap();
    let (files, folders) = rustidian::elsewhere::scan_all(home.path(), &home.path().join("vault"));
    assert_eq!(files, vec![home.path().join("p/skills/README.md")]);
    assert!(
        !folders
            .iter()
            .any(|folder| folder.to_string_lossy().contains("hidden")
                || folder.to_string_lossy().contains("node_modules")),
        "{folders:?}"
    );
    let mut paths = files;
    paths.extend(folders);
    let mut matcher = FileMatcher::elsewhere(home.path(), paths);
    assert_eq!(
        matcher.rank("skills", 2),
        vec![
            home.path().join("p/skills"),
            home.path().join("p/Matilda/skills")
        ]
    );
}

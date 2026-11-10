use rustidian::{query::Query, vault::Vault};
use std::fs;

#[test]
fn exact_and_full_title_matches_precede_body_matches() {
    use rustidian::query::Cache;
    use std::{path::PathBuf, sync::mpsc, time::Duration};

    let root = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("A body match.md", "Good code. Good code. Good code."),
        ("B Good.md", "Code belongs in this body."),
        ("Good Code.md", "The intended note."),
        ("Y Code that stays Good.md", "A full title match."),
        ("Z Good Code Practices.md", "Another full title match."),
        ("Unrelated.md", "Neither term appears here."),
    ] {
        fs::write(root.path().join(name), text).unwrap();
    }
    let vault = Vault::open(root.path()).unwrap();
    let mut cache = Cache::default();
    for source in ["good code", " GOOD   CODE ", "good code -path:Missing"] {
        let expected: Vec<_> = [
            "Good Code.md",
            "Y Code that stays Good.md",
            "Z Good Code Practices.md",
            "A body match.md",
            "B Good.md",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let (wake, done) = mpsc::sync_channel(1);
        cache.update(source, &vault, &[], move || {
            let _ = wake.send(());
        });
        done.recv_timeout(Duration::from_secs(3)).unwrap();
        cache.update(source, &vault, &[], || {});
        assert!(cache.error.is_none(), "{source}: {:?}", cache.error);
        assert_eq!(cache.paths, expected, "Sidebar search: {source}");
        let paths: Vec<_> = vault
            .query(source)
            .unwrap()
            .iter()
            .map(|note| note.path.clone())
            .collect();
        assert_eq!(paths, expected, "Vault query: {source}");
    }
    assert_eq!(
        vault.query("\"good code\"").unwrap()[0].path,
        PathBuf::from("Good Code.md")
    );
    assert_eq!(
        vault.query("content:\"good code\"").unwrap()[0].path,
        PathBuf::from("A body match.md")
    );
    assert_eq!(vault.query("").unwrap().len(), 6);
}

#[test]
fn search_composes_phrases_groups_negation_tags_paths_regex_and_properties() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Notes")).unwrap();
    fs::write(root.path().join("Notes/First.md"), "---\ntags: [notes/deep]\nstatus: Done\nscore: 5\n---\n# First\nConnected thinking is calm.\n你好 world\n").unwrap();
    fs::write(
        root.path().join("Other.md"),
        "# Other\nConnected but busy.\n",
    )
    .unwrap();
    let vault = Vault::open(root.path()).unwrap();
    for query in [
        "\"connected thinking\"",
        "connected -busy",
        "tag:#notes",
        "path:Notes file:First",
        "/你好\\s+world/",
        "[status: done]",
        "[score: > 3]",
        "(calm OR missing) -file:Other",
        "line:(你好 world)",
    ] {
        let query = Query::parse(query).unwrap();
        let matches: Vec<_> = vault
            .notes
            .iter()
            .filter(|note| query.matches(note))
            .map(|note| note.title())
            .collect();
        assert_eq!(matches, ["First"], "{}", query.source());
    }
    assert_eq!(
        Query::parse("connected OR nonexistent")
            .unwrap()
            .count(&vault.notes),
        2
    );
    assert_eq!(Query::parse("-[status]").unwrap().count(&vault.notes), 1);
    assert_eq!(Query::parse("tag:#nope").unwrap().count(&vault.notes), 0);
    assert!(Query::parse("(unclosed").is_err());
    assert!(Query::parse("\"unclosed").is_err());
    assert!(Query::parse("/[invalid/").is_err());
}

#[test]
fn regular_expressions_respect_fields_and_incomplete_or_is_an_error() {
    use std::path::PathBuf;
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("Notes")).unwrap();
    fs::write(
        temp.path().join("Notes/First.md"),
        "---\ntags: [work/deep]\n---\nOnly body text",
    )
    .unwrap();
    fs::write(temp.path().join("Second.md"), "First Notes work/deep").unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    for query in ["file:/^First$/", "path:/^Notes\\//", "tag:/^work\\//"] {
        let results = vault.query(query).unwrap();
        assert_eq!(results.len(), 1, "{query}");
        assert_eq!(results[0].path, PathBuf::from("Notes/First.md"), "{query}");
    }
    assert_eq!(
        vault.query("content:/First/").unwrap()[0].path,
        PathBuf::from("Second.md")
    );
    assert!(
        vault
            .query("file:/Second/ content:/Only/")
            .unwrap()
            .is_empty()
    );
    for query in [
        "OR First",
        "First OR",
        "First OR OR Second",
        "(First OR)",
        "()",
    ] {
        assert!(Query::parse(query).is_err(), "{query}");
    }
    assert_eq!(vault.query("").unwrap().len(), 2);
}

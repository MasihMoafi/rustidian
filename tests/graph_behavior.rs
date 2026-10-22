use rust_notes::{
    graph::{Graph, Group, Kind, Options, View},
    vault::Vault,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn fixture(files: &[(&str, &str)]) -> (TempDir, Vault) {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let vault = Vault::open(root.path()).unwrap();
    (root, vault)
}

fn reference() -> (TempDir, Vault) {
    fixture(&[
        (
            "Home.md",
            "# Home\n[[Alpha]]\n[[Beta]]\n[[Projects/Plan]]\n[[Missing target]]\n#start\n",
        ),
        ("Alpha.md", "# Alpha\n[[Gamma]]\n[[Beta]]\n#team/green\n"),
        ("Beta.md", "# Beta\n[[Delta]]\n#team/blue\n"),
        ("Gamma.md", "# Gamma\n[[Epsilon]]\n#team/green\n"),
        ("Delta.md", "# Delta\n"),
        ("Epsilon.md", "# Epsilon\n"),
        ("Isolated.md", "# Isolated\n"),
        ("Projects/Plan.md", "# Plan\n[[Alpha]]\n#project\n"),
    ])
}

fn paths(graph: &Graph, view: &View) -> Vec<PathBuf> {
    let mut paths: Vec<_> = view
        .nodes
        .iter()
        .map(|&i| graph.nodes[i].path.clone())
        .collect();
    paths.sort();
    paths
}

fn has_edge(graph: &Graph, view: &View, from: &str, to: &str) -> bool {
    view.edges.iter().any(|edge| {
        graph.nodes[edge.from].path == Path::new(from) && graph.nodes[edge.to].path == Path::new(to)
    })
}

#[test]
fn global_defaults_and_local_depths_match_the_reference_fixture() {
    let (_root, vault) = reference();
    let graph = Graph::from_vault(&vault);
    let global = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(global.nodes.len(), 9);
    assert_eq!(
        global.orphans,
        vec![graph.node(Path::new("Isolated.md")).unwrap()]
    );
    let mut options = Options::default();
    for (depth, count, extra) in [
        (1, 5, vec![]),
        (2, 7, vec!["Gamma.md", "Delta.md"]),
        (3, 8, vec!["Gamma.md", "Delta.md", "Epsilon.md"]),
    ] {
        options.depth = depth;
        let view = graph
            .visible(&vault, &options, Some(Path::new("Home.md")))
            .unwrap();
        let paths = paths(&graph, &view);
        assert_eq!(paths.len(), count, "depth {depth}");
        assert!(!paths.contains(&PathBuf::from("Isolated.md")));
        for name in extra {
            assert!(
                paths.contains(&PathBuf::from(name)),
                "depth {depth}: {name}"
            );
        }
        assert_eq!(paths.contains(&PathBuf::from("Epsilon.md")), depth >= 3);
    }
    options.depth = 0;
    assert_eq!(
        graph
            .visible(&vault, &options, Some(Path::new("Home.md")))
            .unwrap()
            .nodes
            .len(),
        5
    );
    options.depth = usize::MAX;
    assert_eq!(
        graph
            .visible(&vault, &options, Some(Path::new("Home.md")))
            .unwrap()
            .nodes
            .len(),
        8
    );
}

#[test]
fn local_direction_and_neighbor_switches_change_edges_without_inventing_nodes() {
    let (_root, vault) = reference();
    let graph = Graph::from_vault(&vault);
    let mut options = Options {
        outgoing: false,
        ..Options::default()
    };
    let root = Some(Path::new("Home.md"));
    let incoming = graph.visible(&vault, &options, root).unwrap();
    assert_eq!(paths(&graph, &incoming), [PathBuf::from("Home.md")]);
    assert!(incoming.edges.is_empty());
    options.outgoing = true;
    let star = graph.visible(&vault, &options, root).unwrap();
    assert_eq!(star.edges.len(), 4);
    assert!(!has_edge(&graph, &star, "Alpha.md", "Beta.md"));
    assert!(!has_edge(&graph, &star, "Projects/Plan.md", "Alpha.md"));
    options.neighbor_links = true;
    let neighbors = graph.visible(&vault, &options, root).unwrap();
    assert_eq!(paths(&graph, &neighbors), paths(&graph, &star));
    assert_eq!(neighbors.edges.len(), 6);
    assert!(has_edge(&graph, &neighbors, "Alpha.md", "Beta.md"));
    assert!(has_edge(&graph, &neighbors, "Projects/Plan.md", "Alpha.md"));
    options.incoming = false;
    options.outgoing = false;
    options.orphans = false;
    let alone = graph.visible(&vault, &options, root).unwrap();
    assert_eq!(paths(&graph, &alone), [PathBuf::from("Home.md")]);
    assert!(alone.edges.is_empty());
}

#[test]
fn incoming_and_outgoing_are_independent_at_a_note_with_both_directions() {
    let (_root, vault) = reference();
    let graph = Graph::from_vault(&vault);
    let root = Some(Path::new("Alpha.md"));
    let incoming = graph
        .visible(
            &vault,
            &Options {
                outgoing: false,
                ..Options::default()
            },
            root,
        )
        .unwrap();
    assert_eq!(
        paths(&graph, &incoming),
        ["Alpha.md", "Home.md", "Projects/Plan.md"].map(PathBuf::from)
    );
    let outgoing = graph
        .visible(
            &vault,
            &Options {
                incoming: false,
                ..Options::default()
            },
            root,
        )
        .unwrap();
    assert_eq!(
        paths(&graph, &outgoing),
        ["Alpha.md", "Beta.md", "Gamma.md"].map(PathBuf::from)
    );
}

#[test]
fn existing_only_and_orphans_apply_to_the_visible_global_graph() {
    let (_root, vault) = reference();
    let graph = Graph::from_vault(&vault);
    let options = Options {
        existing_only: true,
        orphans: false,
        ..Options::default()
    };
    let view = graph.visible(&vault, &options, None).unwrap();
    assert_eq!(view.nodes.len(), 7);
    assert!(
        view.nodes
            .iter()
            .all(|&i| graph.nodes[i].kind == Kind::Note)
    );
    assert!(!paths(&graph, &view).contains(&PathBuf::from("Isolated.md")));
    assert!(view.orphans.is_empty());
    let isolated = graph
        .visible(&vault, &options, Some(Path::new("Isolated.md")))
        .unwrap();
    assert_eq!(paths(&graph, &isolated), [PathBuf::from("Isolated.md")]);
}

#[test]
fn repeated_wiki_markdown_and_reference_links_deduplicate_but_keep_direction() {
    let (_root, vault) = fixture(&[
        (
            "A.md",
            "[[B]] [[B|caption]] [B](B.md#Heading) [reference][b]\n\n[b]: B.md\n\n[[A]] [self](#Heading)\n",
        ),
        ("B.md", "[[A]]\n"),
    ]);
    let graph = Graph::from_vault(&vault);
    let view = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(view.nodes.len(), 2);
    assert_eq!(view.edges.len(), 2);
    assert!(has_edge(&graph, &view, "A.md", "B.md"));
    assert!(has_edge(&graph, &view, "B.md", "A.md"));
    assert!(!has_edge(&graph, &view, "A.md", "A.md"));
}

#[test]
fn canonical_destinations_use_the_shared_sibling_root_alias_and_escape_rules() {
    let (_root, vault) = fixture(&[
        (
            "A/Start.md",
            "[[Shared]] [[B/Shared|display alias]] [[B/Shared\\|table alias]] [space](../B/Spaced%20Name.md#Part) [paren](../B/Parens\\(note\\).md) [[Dotted.Name]]\n",
        ),
        ("Root.md", "[[Shared]] [[B/Spaced%20Name]]\n"),
        ("A/Shared.md", "# A shared\n"),
        ("B/Shared.md", "# B shared\n"),
        ("B/Spaced Name.md", "# Space\n"),
        ("B/Parens(note).md", "# Parentheses\n"),
        ("Dotted.Name.md", "# Existing dotted name\n"),
    ]);
    let graph = Graph::from_vault(&vault);
    let view = graph.visible(&vault, &Options::default(), None).unwrap();
    for destination in [
        "A/Shared.md",
        "B/Shared.md",
        "B/Spaced Name.md",
        "B/Parens(note).md",
        "Dotted.Name.md",
    ] {
        assert!(
            has_edge(&graph, &view, "A/Start.md", destination),
            "{destination}"
        );
    }
    assert!(!has_edge(&graph, &view, "Root.md", "A/Shared.md"));
    assert!(!has_edge(&graph, &view, "Root.md", "B/Shared.md"));
    assert!(has_edge(&graph, &view, "Root.md", "B/Spaced Name.md"));
    let ambiguous = graph.node(Path::new("Shared.md")).unwrap();
    assert_eq!(graph.nodes[ambiguous].kind, Kind::Unresolved);
}

#[test]
fn literal_external_and_malformed_links_cannot_create_graph_edges() {
    let (_root, vault) = fixture(&[
        (
            "Home.md",
            "---\ncaption: '[[Yaml]]'\n---\n\n[[Good]]\n\n`[[Inline]]`\n\n    [[Indented]]\n\n```md\n[[Fenced]]\n```\n\n<!-- [[Comment]] -->\n\n\\[[Escaped]]\n[external](https://example.com/[[Url-part]])\n[[https://example.com]]\n[[mailto:a@example.com]]\n[mail](mailto:a@example.com)\n[unsafe](javascript:alert(1))\n![remote](https://example.com/image.png)\n[[ ]]\n[[unfinished\n[broken](No.md\n",
        ),
        ("Good.md", "# Good\n"),
    ]);
    let graph = Graph::from_vault(&vault);
    let view = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(
        paths(&graph, &view),
        ["Good.md", "Home.md"].map(PathBuf::from)
    );
    assert_eq!(view.edges.len(), 1);
    assert!(has_edge(&graph, &view, "Home.md", "Good.md"));
}

#[test]
fn filter_and_ordered_groups_reuse_query_properties_tags_and_aliases() {
    let (_root, vault) = fixture(&[
        (
            "Projects/Plan.md",
            "---\nstatus: active\ntags: [project/deep]\naliases: [Roadmap]\n---\n[[Other]]\n[[Missing]]\n",
        ),
        ("Other.md", "# Other\n"),
    ]);
    let graph = Graph::from_vault(&vault);
    let mut options = Options {
        filter: "[status:active] tag:#project Roadmap".into(),
        groups: vec![
            Group {
                query: "path:Projects".into(),
                color: [224, 82, 82, 255],
            },
            Group {
                query: "[status:active]".into(),
                color: [1, 2, 3, 255],
            },
        ],
        ..Options::default()
    };
    let view = graph.visible(&vault, &options, None).unwrap();
    assert_eq!(paths(&graph, &view), [PathBuf::from("Projects/Plan.md")]);
    assert!(view.edges.is_empty());
    assert_eq!(view.groups.values().copied().collect::<Vec<_>>(), [0]);
    options.filter = "path:Missing".into();
    let unresolved = graph.visible(&vault, &options, None).unwrap();
    assert_eq!(paths(&graph, &unresolved), [PathBuf::from("Missing.md")]);
    assert_eq!(graph.nodes[unresolved.nodes[0]].kind, Kind::Unresolved);
    assert!(unresolved.groups.is_empty());
    options.filter = "path:DOES_NOT_EXIST".into();
    assert!(
        graph
            .visible(&vault, &options, None)
            .unwrap()
            .nodes
            .is_empty()
    );
    options.filter = "(".into();
    assert!(graph.visible(&vault, &options, None).is_err());
    options.filter.clear();
    options.groups[0].query = "(".into();
    assert!(
        graph
            .visible(&vault, &options, None)
            .unwrap_err()
            .contains("Group 1")
    );
}

#[test]
fn optional_tags_and_attachments_have_distinct_nodes_and_directed_edges() {
    let (_root, vault) = fixture(&[
        (
            "Home.md",
            "[[Target]] #team/green\n![[Pictures/picture one.png]]\n![same](Pictures/picture%20one.png)\n[unsupported](Reference.pdf)\n",
        ),
        ("Target.md", "# Target\n"),
        ("Pictures/picture one.png", "image bytes unchanged"),
        ("Reference.pdf", "unindexed attachment bytes unchanged"),
    ]);
    let graph = Graph::from_vault(&vault);
    let default = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(default.nodes.len(), 2);
    let options = Options {
        tags: true,
        attachments: true,
        ..Options::default()
    };
    let view = graph.visible(&vault, &options, None).unwrap();
    assert_eq!(view.nodes.len(), 4);
    assert!(has_edge(&graph, &view, "Home.md", "#team/green"));
    assert!(has_edge(
        &graph,
        &view,
        "Home.md",
        "Pictures/picture one.png"
    ));
    assert_eq!(view.edges.len(), 3);
    let filtered = graph
        .visible(
            &vault,
            &Options {
                filter: "path:Pictures".into(),
                ..options.clone()
            },
            None,
        )
        .unwrap();
    assert_eq!(
        paths(&graph, &filtered),
        [PathBuf::from("Pictures/picture one.png")]
    );
    let tagged = graph
        .visible(
            &vault,
            &Options {
                filter: "tag:#team".into(),
                ..options
            },
            None,
        )
        .unwrap();
    assert_eq!(
        paths(&graph, &tagged),
        ["#team/green", "Home.md"].map(PathBuf::from)
    );
}

#[test]
fn ignored_targets_and_unsafe_missing_paths_do_not_leak_into_the_graph() {
    let (_root, vault) = fixture(&[
        (
            "Home.md",
            "[[Good]] [[target/Generated]] [[.obsidian/Hidden]] [[../../Outside]] [[Missing]]\n",
        ),
        ("Good.md", "# Good\n"),
        ("target/Generated.md", "[[Leaked]]\n"),
        (".obsidian/Hidden.md", "[[Leaked]]\n"),
    ]);
    let graph = Graph::from_vault(&vault);
    let view = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(
        paths(&graph, &view),
        ["Good.md", "Home.md", "Missing.md"].map(PathBuf::from)
    );
    assert_eq!(view.edges.len(), 2);
}

#[cfg(unix)]
#[test]
fn linked_files_never_import_outside_contents_or_become_existing_nodes() {
    let (root, _) = fixture(&[("Home.md", "[[Linked]]\n")]);
    let outside = tempfile::tempdir().unwrap();
    let outside_note = outside.path().join("Outside.md");
    let bytes = b"[[Outside-only target]]\n#outside-only-tag\n";
    fs::write(&outside_note, bytes).unwrap();
    std::os::unix::fs::symlink(&outside_note, root.path().join("Linked.md")).unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let graph = Graph::from_vault(&vault);
    let view = graph.visible(&vault, &Options::default(), None).unwrap();
    assert_eq!(
        paths(&graph, &view),
        ["Home.md", "Linked.md"].map(PathBuf::from)
    );
    let linked = graph.node(Path::new("Linked.md")).unwrap();
    assert_eq!(graph.nodes[linked].kind, Kind::Unresolved);
    assert!(graph.nodes.iter().all(|node| node.kind != Kind::Tag));
    assert_eq!(fs::read(&outside_note).unwrap(), bytes);
    let existing = graph
        .visible(
            &vault,
            &Options {
                existing_only: true,
                ..Options::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(paths(&graph, &existing), [PathBuf::from("Home.md")]);
}

#[test]
fn empty_vaults_unknown_roots_and_filtered_bridges_have_no_placeholder_nodes() {
    let (_root, empty) = fixture(&[]);
    let graph = Graph::from_vault(&empty);
    assert!(
        graph
            .visible(&empty, &Options::default(), None)
            .unwrap()
            .nodes
            .is_empty()
    );
    let (_root, vault) = reference();
    let graph = Graph::from_vault(&vault);
    assert!(
        graph
            .visible(&vault, &Options::default(), Some(Path::new("Unknown.md")))
            .unwrap()
            .nodes
            .is_empty()
    );
    let view = graph
        .visible(
            &vault,
            &Options {
                filter: "-file:Alpha".into(),
                depth: 5,
                ..Options::default()
            },
            Some(Path::new("Home.md")),
        )
        .unwrap();
    let paths = paths(&graph, &view);
    for absent in ["Alpha.md", "Gamma.md", "Epsilon.md"] {
        assert!(!paths.contains(&PathBuf::from(absent)), "{absent}");
    }
    assert!(paths.contains(&PathBuf::from("Delta.md")));
}

#[test]
fn graph_construction_selection_and_failures_never_write_original_files() {
    let (root, vault) = reference();
    let snapshot = || -> BTreeMap<PathBuf, Vec<u8>> {
        walkdir::WalkDir::new(root.path())
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                (
                    entry.path().strip_prefix(root.path()).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    };
    let before = snapshot();
    let graph = Graph::from_vault(&vault);
    let repeated = Graph::from_vault(&vault);
    assert_eq!(graph, repeated);
    graph.visible(&vault, &Options::default(), None).unwrap();
    graph
        .visible(
            &vault,
            &Options {
                depth: 5,
                ..Options::default()
            },
            Some(Path::new("Home.md")),
        )
        .unwrap();
    assert!(
        graph
            .visible(
                &vault,
                &Options {
                    filter: "(".into(),
                    ..Options::default()
                },
                None
            )
            .is_err()
    );
    assert_eq!(snapshot(), before);
    assert!(!root.path().join("Missing target.md").exists());
}

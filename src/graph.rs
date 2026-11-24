//! Read-only graph snapshots built from the shared vault resolver and query language.
use crate::{
    query::Query,
    vault::{Note, Vault, is_ignored_path, is_markdown},
};
use pulldown_cmark::{Event, Parser, Tag};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
};

mod relationships;
pub use relationships::{Facets, Focus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Note,
    Unresolved,
    Attachment,
    Tag,
    Author,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    pub label: String,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub facets: Facets,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub query: String,
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub filter: String,
    pub focus: Focus,
    pub groups: Vec<Group>,
    pub tags: bool,
    pub attachments: bool,
    pub existing_only: bool,
    pub orphans: bool,
    pub depth: usize,
    pub incoming: bool,
    pub outgoing: bool,
    pub neighbor_links: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            filter: String::new(),
            focus: Focus::default(),
            groups: Vec::new(),
            tags: false,
            attachments: false,
            existing_only: false,
            orphans: true,
            depth: 1,
            incoming: true,
            outgoing: true,
            neighbor_links: false,
        }
    }
}

/// Indices refer to the original snapshot; filtering never renumbers a node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub nodes: Vec<usize>,
    pub edges: Vec<Edge>,
    pub groups: BTreeMap<usize, usize>,
    pub orphans: Vec<usize>,
}

// Virtual entities have separate namespaces from files, even when their labels match.
type Key = (u8, PathBuf);

impl Graph {
    pub fn from_vault(vault: &Vault) -> Self {
        let mut nodes: BTreeMap<Key, Node> = BTreeMap::new();
        let mut edges = BTreeSet::new();
        let mut facets = relationships::Index::new(vault);
        for note in &vault.notes {
            if !is_ignored_path(&note.path) {
                insert(&mut nodes, note.path.clone(), Kind::Note);
            }
        }
        for path in &vault.attachments {
            if !is_ignored_path(path) {
                insert(&mut nodes, path.clone(), Kind::Attachment);
            }
        }
        for note in &vault.notes {
            if is_ignored_path(&note.path) {
                continue;
            }
            let from = (0, note.path.clone());
            let properties = crate::metadata::frontmatter(&note.text).ok().flatten();
            for target in links(
                properties
                    .as_ref()
                    .map_or(note.text.as_str(), |meta| &note.text[meta.range.end..]),
            ) {
                if external(&target) {
                    continue;
                }
                let destination = vault.resolve_destination(&note.path, &target);
                let (path, kind) = if let Some(path) = destination {
                    let kind = if is_markdown(&path) {
                        Kind::Note
                    } else {
                        Kind::Attachment
                    };
                    (path, kind)
                } else if let Some(path) = unresolved(&target) {
                    (path, Kind::Unresolved)
                } else {
                    continue;
                };
                if is_ignored_path(&path) || path == note.path {
                    continue;
                }
                let to = insert(&mut nodes, path, kind);
                edges.insert((from.clone(), to));
            }
            for tag in &note.tags {
                let path =
                    PathBuf::from(format!("#{}", tag.trim_start_matches('#').to_lowercase()));
                let to = insert(&mut nodes, path, Kind::Tag);
                edges.insert((from.clone(), to));
            }
            if let Some(properties) = properties {
                facets.add(note, properties);
            }
        }
        let (mut facets, author_links) = facets.finish(&mut nodes);
        let indices: BTreeMap<_, _> = nodes
            .keys()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        facets.authored = author_links
            .into_iter()
            .map(|(from, to)| {
                (
                    Edge {
                        from: indices[&from],
                        to: indices[&to],
                    },
                    to.1,
                )
            })
            .collect();
        Self {
            nodes: nodes.into_values().collect(),
            edges: edges
                .into_iter()
                .map(|(from, to)| Edge {
                    from: indices[&from],
                    to: indices[&to],
                })
                .collect(),
            facets,
        }
    }

    pub fn node(&self, path: &Path) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| !matches!(node.kind, Kind::Tag | Kind::Author) && node.path == path)
    }

    /// Use the same vault snapshot that produced this graph. Queries are evaluated
    /// before traversal; a hidden node cannot act as an invisible local bridge.
    pub fn visible(
        &self,
        vault: &Vault,
        options: &Options,
        root: Option<&Path>,
    ) -> Result<View, String> {
        let query = Query::parse(&options.filter)?;
        let groups = options
            .groups
            .iter()
            .enumerate()
            .map(|(i, group)| {
                Query::parse(&group.query).map_err(|error| format!("Group {}: {error}", i + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let notes: HashMap<_, _> = vault
            .notes
            .iter()
            .map(|note| (note.path.as_path(), note))
            .collect();
        let mut allowed: Vec<_> = self
            .nodes
            .iter()
            .map(|node| {
                node.kind != Kind::Author
                    && (options.tags || node.kind != Kind::Tag)
                    && (options.attachments || node.kind != Kind::Attachment)
                    && (!options.existing_only || node.kind != Kind::Unresolved)
                    && matches(&query, node, &notes)
                    && self.facets.matches(node, &options.focus)
            })
            .collect();
        let authored = self.facets.edges(&mut allowed, &options.focus);
        let mut eligible: Vec<_> = self
            .edges
            .iter()
            .copied()
            .filter(|edge| allowed[edge.from] && allowed[edge.to])
            .collect();
        if !authored.is_empty() {
            eligible.extend(authored);
            eligible.sort_unstable();
            eligible.dedup();
        }
        let (mut selected, edges) = if let Some(path) = root {
            let Some(root) = self.node(path).filter(|&i| allowed[i]) else {
                return Ok(View::default());
            };
            let mut selected = BTreeSet::from([root]);
            let mut frontier = HashSet::from([root]);
            let mut traversed = BTreeSet::new();
            for _ in 0..options.depth.clamp(1, 5) {
                let mut next = HashSet::new();
                for edge in &eligible {
                    if options.outgoing && frontier.contains(&edge.from) {
                        next.insert(edge.to);
                        traversed.insert(*edge);
                    }
                    if options.incoming && frontier.contains(&edge.to) {
                        next.insert(edge.from);
                        traversed.insert(*edge);
                    }
                }
                next.retain(|node| !selected.contains(node));
                selected.extend(next.iter().copied());
                frontier = next;
                if frontier.is_empty() {
                    break;
                }
            }
            if options.neighbor_links {
                traversed.extend(
                    eligible
                        .iter()
                        .filter(|edge| selected.contains(&edge.from) && selected.contains(&edge.to))
                        .copied(),
                );
            }
            (selected, traversed.into_iter().collect::<Vec<_>>())
        } else {
            (
                allowed
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &allowed)| allowed.then_some(i))
                    .collect(),
                eligible,
            )
        };
        let connected: HashSet<_> = edges.iter().flat_map(|edge| [edge.from, edge.to]).collect();
        if root.is_none() && !options.orphans {
            selected.retain(|node| connected.contains(node));
        }
        let mut view = View {
            nodes: selected.iter().copied().collect(),
            orphans: selected
                .iter()
                .filter(|node| !connected.contains(*node))
                .copied()
                .collect(),
            edges,
            ..View::default()
        };
        for &index in &view.nodes {
            if let Some(group) = groups
                .iter()
                .position(|query| matches(query, &self.nodes[index], &notes))
            {
                view.groups.insert(index, group);
            }
        }
        Ok(view)
    }
}

fn insert(nodes: &mut BTreeMap<Key, Node>, path: PathBuf, kind: Kind) -> Key {
    let key = (u8::from(kind == Kind::Tag), path.clone());
    nodes.entry(key.clone()).or_insert_with(|| Node {
        label: if kind == Kind::Tag {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            crate::vault::file_title(&path)
        },
        path,
        kind,
    });
    key
}

fn links(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut blocked = Vec::new();
    for (event, range) in Parser::new(body).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                links.push(dest_url.into_string());
                blocked.push(range);
            }
            Event::Start(Tag::CodeBlock(_))
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_) => blocked.push(range),
            _ => {}
        }
    }
    blocked.sort_by_key(|range| range.start);
    let mut literal = 0;
    let mut literal_end = 0;
    for (range, target) in crate::vault::wiki_spans(body) {
        while let Some(literal_range) = blocked
            .get(literal)
            .filter(|blocked| blocked.start < range.end)
        {
            literal_end = literal_end.max(literal_range.end);
            literal += 1;
        }
        if literal_end <= range.start {
            links.push(target);
        }
    }
    links
}

fn external(target: &str) -> bool {
    let target = target.trim();
    target.starts_with("//")
        || target.split_once(':').is_some_and(|(scheme, _)| {
            scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && scheme
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'+' | b'-' | b'.'))
        })
}

fn unresolved(target: &str) -> Option<PathBuf> {
    let target = target.split('|').next()?.split('#').next()?.trim();
    let target = crate::paths::decode(target)?;
    let path = crate::paths::normalise(Path::new(&target))?;
    if path.as_os_str().is_empty()
        || external(&target)
        || target
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '[' | ']' | '<' | '>'))
        // Unindexed attachments cannot be resolved by the current vault index.
        // A missing dotted note needs an explicit .md suffix to distinguish it.
        || (path.extension().is_some() && !is_markdown(&path))
    {
        return None;
    }
    let path = crate::paths::markdown_name(&path);
    crate::paths::wiki_path_supported(&path).then_some(path)
}

fn matches(query: &Query, node: &Node, notes: &HashMap<&Path, &Note>) -> bool {
    if query.source().trim().is_empty() {
        return true;
    }
    if node.kind == Kind::Note
        && let Some(note) = notes.get(node.path.as_path())
    {
        return query.matches(note);
    }
    let text = if node.kind == Kind::Tag {
        node.path.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let mut note = Note::new(node.path.clone(), text);
    if node.kind == Kind::Tag {
        note.tags = vec![
            node.path
                .to_string_lossy()
                .trim_start_matches('#')
                .to_owned(),
        ];
    }
    query.matches(&note)
}

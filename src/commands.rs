//! The same available commands and shortcuts feed the palette and settings.
use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Global,
    Tab,
    File,
    Document,
    Editor,
}

impl Scope {
    fn includes(self, required: Self) -> bool {
        self >= required
    }
}

pub const ADD_UNIQUE_LINK: &str = "Unique note creator: Add unique internal link";
pub const DUPLICATE_FILE: &str = "Make a copy of the current file";
pub const MOVE_FILE: &str = "Move current file to another folder";
pub const UNDO_CLOSE_TAB: &str = "Undo close tab";
pub const CLOSE_OTHER_TABS: &str = "Close all other tabs";
pub const OPEN_FILE: &str = "Open file…";
pub const OPEN_FOLDER: &str = "Open folder…";
pub const OPEN_FILE_SHORTCUT: &str = "Ctrl+Shift+O";
pub const OPEN_FOLDER_SHORTCUT: &str = "Ctrl+Alt+O";
pub const OPEN_AGENT: &str = "Agent: Open agent tab";
pub const OPEN_GRAPH: &str = "Graph view: Open graph view";
pub const OPEN_LOCAL_GRAPH: &str = "Graph view: Open local graph";
pub const INCREASE_FONT_SIZE: &str = "Increase font size";
pub const DECREASE_FONT_SIZE: &str = "Decrease font size";

pub struct Command {
    pub name: &'static str,
    pub shortcut: &'static str,
    pub scope: Scope,
}

impl AsRef<str> for Command {
    fn as_ref(&self) -> &str {
        self.name
    }
}

pub const ALL: &[Command] = &[
    Command {
        name: OPEN_FILE,
        shortcut: OPEN_FILE_SHORTCUT,
        scope: Scope::Global,
    },
    Command {
        name: OPEN_FOLDER,
        shortcut: OPEN_FOLDER_SHORTCUT,
        scope: Scope::Global,
    },
    Command {
        name: OPEN_AGENT,
        shortcut: "Ctrl+Shift+A",
        scope: Scope::Global,
    },
    Command {
        name: OPEN_GRAPH,
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: OPEN_LOCAL_GRAPH,
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: INCREASE_FONT_SIZE,
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: DECREASE_FONT_SIZE,
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: CLOSE_OTHER_TABS,
        shortcut: "",
        scope: Scope::Tab,
    },
    Command {
        name: UNDO_CLOSE_TAB,
        shortcut: "Ctrl+Shift+T",
        scope: Scope::Global,
    },
    Command {
        name: MOVE_FILE,
        shortcut: "",
        scope: Scope::File,
    },
    Command {
        name: DUPLICATE_FILE,
        shortcut: "",
        scope: Scope::File,
    },
    Command {
        name: "Toggle pin",
        shortcut: "",
        scope: Scope::Tab,
    },
    Command {
        name: "New tab",
        shortcut: "Ctrl+T",
        scope: Scope::Global,
    },
    Command {
        name: "Close current tab",
        shortcut: "Ctrl+W",
        scope: Scope::Tab,
    },
    Command {
        name: "Go to next tab",
        shortcut: "Ctrl+Tab",
        scope: Scope::Tab,
    },
    Command {
        name: "Go to previous tab",
        shortcut: "Ctrl+Shift+Tab",
        scope: Scope::Tab,
    },
    Command {
        name: ADD_UNIQUE_LINK,
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: "Unique note creator: Create new unique note",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Random note: Open random note",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "File recovery: Open local history",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Workspaces: Manage workspace layouts",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Workspaces: Load workspace layout",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Workspaces: Save layout",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Workspaces: Save and load another layout",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "New note",
        shortcut: "Ctrl+N",
        scope: Scope::Global,
    },
    Command {
        name: "New folder",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Daily notes: Open today's daily note",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Templates: Insert template",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Templates: Insert current date",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Templates: Insert current time",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Open a file",
        shortcut: "Ctrl+O",
        scope: Scope::Global,
    },
    Command {
        name: "Open vault",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Open another vault",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Create new vault",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Save note",
        shortcut: "Ctrl+S",
        scope: Scope::Document,
    },
    Command {
        name: "Save a copy",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Reading view",
        shortcut: "Ctrl+E",
        scope: Scope::Document,
    },
    Command {
        name: "Live preview",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Source editing",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Split source and preview",
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Add property",
        shortcut: "Ctrl+;",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::NativeColor::Blue.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: crate::highlight::NativeColor::Green.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: crate::highlight::NativeColor::Orange.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: crate::highlight::NativeColor::Purple.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: crate::highlight::NativeColor::Red.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: crate::highlight::NativeColor::Yellow.command(),
        shortcut: "",
        scope: Scope::Editor,
    },
    Command {
        name: "Highlight selection",
        shortcut: "Ctrl+Shift+H",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::Color::Cyan.command(),
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::Color::Green.command(),
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::Color::Orange.command(),
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::Color::Red.command(),
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: crate::highlight::Color::Yellow.command(),
        shortcut: "",
        scope: Scope::Document,
    },
    Command {
        name: "Rename or move file",
        shortcut: "F2",
        scope: Scope::Global,
    },
    Command {
        name: "Move file to system Trash",
        shortcut: "Delete",
        scope: Scope::Global,
    },
    Command {
        name: "Bookmark current file",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Show bookmarks",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Toggle left sidebar",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Toggle right sidebar",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Switch appearance",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Refresh vault",
        shortcut: "",
        scope: Scope::Global,
    },
    Command {
        name: "Open settings",
        shortcut: "Ctrl+,",
        scope: Scope::Global,
    },
];

pub fn ranked(query: &str, scope: Scope) -> Vec<&'static Command> {
    let query = query.trim();
    let mut matches: Vec<_> = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    )
    .match_list(
        ALL.iter().filter(|command| scope.includes(command.scope)),
        &mut Matcher::new(Config::DEFAULT),
    )
    .into_iter()
    .map(|(command, _)| command)
    .collect();
    if let Some(index) = matches
        .iter()
        .position(|command| command.name.eq_ignore_ascii_case(query))
    {
        let exact = matches.remove(index);
        matches.insert(0, exact);
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_portable_and_native_highlight_names_win_over_similar_commands() {
        for command in crate::highlight::Color::ALL
            .into_iter()
            .map(|color| color.command())
            .chain(
                crate::highlight::NativeColor::ALL
                    .into_iter()
                    .map(|color| color.command()),
            )
        {
            assert_eq!(ranked(command, Scope::Editor)[0].name, command);
            assert_eq!(
                ranked(&command.to_lowercase(), Scope::Editor)[0].name,
                command
            );
        }
        assert!(ranked("not an available command 987654", Scope::Editor).is_empty());
    }
    #[test]
    fn native_color_toggles_require_an_actual_document_editor() {
        for color in crate::highlight::NativeColor::ALL {
            assert_eq!(
                ranked(color.command(), Scope::Editor)[0].name,
                color.command()
            );
            for unavailable in [Scope::Document, Scope::File, Scope::Tab, Scope::Global] {
                assert!(
                    ranked(color.command(), unavailable)
                        .iter()
                        .all(|command| command.name != color.command())
                );
            }
        }
    }

    #[test]
    fn unique_link_requires_an_editor_but_note_navigation_remains_global() {
        assert_eq!(
            ranked(ADD_UNIQUE_LINK, Scope::Editor)[0].name,
            ADD_UNIQUE_LINK
        );
        assert!(ranked(ADD_UNIQUE_LINK, Scope::Document).is_empty());
        assert!(ranked(ADD_UNIQUE_LINK, Scope::Tab).is_empty());
        assert!(ranked(ADD_UNIQUE_LINK, Scope::File).is_empty());
        assert!(ranked(ADD_UNIQUE_LINK, Scope::Global).is_empty());
        for name in [
            "Random note: Open random note",
            "Unique note creator: Create new unique note",
            OPEN_GRAPH,
        ] {
            assert_eq!(ranked(name, Scope::Global)[0].name, name);
        }
    }

    #[test]
    fn tab_actions_need_a_tab_but_creation_remains_available_without_one() {
        for name in [
            "Toggle pin",
            CLOSE_OTHER_TABS,
            "Close current tab",
            "Go to next tab",
            "Go to previous tab",
        ] {
            assert_eq!(ranked(name, Scope::Tab)[0].name, name);
            assert!(ranked(name, Scope::Global).is_empty());
        }
        assert_eq!(ranked("New tab", Scope::Global)[0].name, "New tab");
        assert_eq!(ranked(DUPLICATE_FILE, Scope::File)[0].name, DUPLICATE_FILE);
        assert!(ranked(DUPLICATE_FILE, Scope::Tab).is_empty());
        assert_eq!(ranked(MOVE_FILE, Scope::File)[0].name, MOVE_FILE);
        assert!(ranked(MOVE_FILE, Scope::Tab).is_empty());
        assert_eq!(
            ranked(OPEN_LOCAL_GRAPH, Scope::Document)[0].name,
            OPEN_LOCAL_GRAPH
        );
        for unavailable in [Scope::Global, Scope::Tab, Scope::File] {
            assert!(
                ranked(OPEN_LOCAL_GRAPH, unavailable)
                    .iter()
                    .all(|command| command.name != OPEN_LOCAL_GRAPH)
            );
        }
    }

    #[test]
    fn fuzzy_commands_find_abbreviations_and_exclude_image_edits() {
        assert_eq!(ranked("srcedt", Scope::Document)[0].name, "Source editing");
        assert_eq!(
            ranked("Highlight selection", Scope::Document)[0].name,
            "Highlight selection"
        );
        assert!(ranked("Highlight", Scope::File).is_empty());
        assert_eq!(
            ranked("", Scope::File)
                .iter()
                .filter(|command| command.name == "Open settings")
                .count(),
            1
        );
    }
}

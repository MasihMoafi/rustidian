//! The same available commands and shortcuts feed the palette and settings.
use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

pub struct Command {
    pub name: &'static str,
    pub shortcut: &'static str,
    pub markdown_only: bool,
}

impl AsRef<str> for Command {
    fn as_ref(&self) -> &str {
        self.name
    }
}

pub const ALL: &[Command] = &[
    Command {
        name: "New note",
        shortcut: "Ctrl+N",
        markdown_only: false,
    },
    Command {
        name: "New folder",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Open a file",
        shortcut: "Ctrl+O",
        markdown_only: false,
    },
    Command {
        name: "Open vault",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Open another vault",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Create new vault",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Save note",
        shortcut: "Ctrl+S",
        markdown_only: true,
    },
    Command {
        name: "Save a copy",
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: "Reading view",
        shortcut: "Ctrl+E",
        markdown_only: true,
    },
    Command {
        name: "Live preview",
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: "Source editing",
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: "Split source and preview",
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: "Add property",
        shortcut: "Ctrl+;",
        markdown_only: true,
    },
    Command {
        name: "Highlight selection",
        shortcut: "Ctrl+Shift+H",
        markdown_only: true,
    },
    Command {
        name: crate::highlight::Color::Cyan.command(),
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: crate::highlight::Color::Green.command(),
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: crate::highlight::Color::Orange.command(),
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: crate::highlight::Color::Red.command(),
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: crate::highlight::Color::Yellow.command(),
        shortcut: "",
        markdown_only: true,
    },
    Command {
        name: "Rename or move file",
        shortcut: "F2",
        markdown_only: false,
    },
    Command {
        name: "Move file to system Trash",
        shortcut: "Delete",
        markdown_only: false,
    },
    Command {
        name: "Bookmark current file",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Show bookmarks",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Toggle left sidebar",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Toggle right sidebar",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Switch appearance",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Refresh vault",
        shortcut: "",
        markdown_only: false,
    },
    Command {
        name: "Open settings",
        shortcut: "Ctrl+,",
        markdown_only: false,
    },
];

pub fn ranked(query: &str, image: bool) -> Vec<&'static Command> {
    Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    )
    .match_list(
        ALL.iter()
            .filter(|command| !image || !command.markdown_only),
        &mut Matcher::new(Config::DEFAULT),
    )
    .into_iter()
    .map(|(command, _)| command)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fuzzy_commands_find_abbreviations_and_exclude_image_edits() {
        assert_eq!(ranked("srcedt", false)[0].name, "Source editing");
        assert_eq!(
            ranked("Highlight selection", false)[0].name,
            "Highlight selection"
        );
        assert!(ranked("Highlight", true).is_empty());
        assert_eq!(
            ranked("", true)
                .iter()
                .filter(|command| command.name == "Open settings")
                .count(),
            1
        );
    }
}

//! What the chips look like, described rather than drawn.
//!
//! Four contributions to one slot, and each of them is a chip with something
//! hung under it: where the pane is, which branch it is on, how much has
//! changed, and what one chord would do. Nothing here names a colour, a pixel
//! or a font — the host resolves every one of those against the theme in
//! force, which is why a plugin written today is drawn correctly in a theme
//! written next year.
//!
//! # The panels are the host's too
//!
//! A [`Node::Picker`] is a field, a filtered list, arrow keys, Enter and
//! Escape, and none of that is here: this says what can be chosen and is told
//! which one was. That is the whole bargain of the second tier, and it is what
//! lets a stranger's plugin have a search box without ever being given a
//! keystroke.

use crook_plugin_api::{MenuItem, Node, Row, Tone};

use crate::state::{Chips, Panel, SHOWN_COMMAND};

/// What one of this plugin's contributions draws.
pub fn chip(chips: &Chips, entry: &str) -> Node {
    match entry {
        "directory" => directory(chips),
        "branch" => branch(chips),
        "diff" => diff(chips),
        "keys" => keys(chips),
        // A contribution this build does not know it made.
        _ => Node::Empty,
    }
}

/// Where the pane is, and the directories it could be in instead.
fn directory(chips: &Chips) -> Node {
    let Some(directory) = chips.directory() else {
        // A pane whose shell has not said where it is. Nothing rather than a
        // chip saying nothing: the row is a list, and an empty contribution
        // takes no room in it.
        return Node::Empty;
    };

    let panel = (chips.panel == Panel::Directory).then(|| {
        Box::new(panelled(
            chips,
            Node::Picker {
                placeholder: String::from("Search directories…"),
                rows: directories(chips),
                choose: String::from("choose-directory"),
            },
        ))
    });

    Node::Anchored {
        content: Box::new(Node::Pressable {
            content: Box::new(Node::Chip {
                icon: String::from("folder"),
                text: shorten(directory, chips.home.as_deref()),
                tone: Tone::Primary,
            }),
            action: String::from("open-directory"),
        }),
        panel,
        dismiss: String::from("dismiss"),
    }
}

/// The rows of the directory picker: the way up, and everything under here.
fn directories(chips: &Chips) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(parent) = parent_of(&chips.browsing) {
        rows.push(Row {
            key: parent,
            label: String::from(".. (parent directory)"),
            icon: String::from("corner-left-up"),
            tone: Tone::Muted,
        });
    }
    for entry in &chips.entries {
        rows.push(Row {
            key: join(&chips.browsing, &entry.name),
            label: entry.name.clone(),
            icon: String::from("folder"),
            tone: Tone::Primary,
        });
    }
    rows
}

/// Which branch the pane is on, and the ones it could be on instead.
fn branch(chips: &Chips) -> Node {
    let Some(branch) = chips.branch() else {
        return Node::Empty;
    };

    let panel = (chips.panel == Panel::Branch).then(|| {
        Box::new(panelled(
            chips,
            Node::Picker {
                placeholder: String::from("Search branches…"),
                rows: branches(chips),
                choose: String::from("choose-branch"),
            },
        ))
    });

    Node::Anchored {
        content: Box::new(Node::Pressable {
            content: Box::new(Node::Chip {
                icon: String::from("git-branch"),
                text: branch.to_owned(),
                tone: Tone::Success,
            }),
            action: String::from("open-branch"),
        }),
        panel,
        dismiss: String::from("dismiss"),
    }
}

/// Every branch, with the one that is checked out marked.
fn branches(chips: &Chips) -> Vec<Row> {
    chips
        .branches
        .iter()
        .map(|branch| {
            let current = chips.head.as_deref() == Some(branch.as_str());
            Row {
                key: branch.clone(),
                label: branch.clone(),
                // The one you are on reads as the one you are on, rather than
                // as a row that would do nothing — which is what choosing it
                // does, and what git says when you try.
                icon: String::from(if current { "check" } else { "git-branch" }),
                tone: if current { Tone::Accent } else { Tone::Primary },
            }
        })
        .collect()
}

/// How much has changed in the working tree, when anything has.
fn diff(chips: &Chips) -> Node {
    let (added, removed) = (chips.added, chips.removed);
    if chips.branch().is_none() || (added == 0 && removed == 0) {
        // Warp prints `± 0` and Crook does not: a chip that is always there
        // saying nothing changed is a chip the eye stops reading, and this row
        // is next to the line somebody is typing.
        return Node::Empty;
    }

    Node::Chip {
        icon: String::from("diff"),
        text: format!("+{added} −{removed}"),
        tone: Tone::Warning,
    }
}

/// What one chord does, and a way to change it.
fn keys(chips: &Chips) -> Node {
    let Some(command) = chips.shown_command() else {
        return Node::Empty;
    };
    let text = match command.chord.as_deref() {
        Some(chord) => format!("{chord}  {}", command.title.to_lowercase()),
        // Worth saying even unbound: this is the chip a person presses to find
        // out there is no chord, and then rebinds from its own menu.
        None => format!("{}  (no keys)", command.title.to_lowercase()),
    };

    Node::Menu {
        content: Box::new(Node::Pressable {
            content: Box::new(Node::Chip {
                icon: String::from("keyboard"),
                text,
                tone: Tone::Muted,
            }),
            action: String::from("run-command"),
        }),
        items: vec![MenuItem {
            label: String::from("Change keybinding"),
            action: String::from("rebind"),
            argument: String::from(SHOWN_COMMAND),
        }],
    }
}

/// A panel: whatever it holds, with the last thing that went wrong under it.
///
/// The trouble is a [`Node::Note`] rather than a log line because the
/// commonest trouble here is a refusal, and a refusal carries the sentence the
/// Plugins page will say — which is a thing to put in front of a person, not
/// in a file.
fn panelled(chips: &Chips, content: Node) -> Node {
    let Some(trouble) = chips.trouble.as_deref() else {
        return content;
    };

    Node::Column(vec![
        content,
        Node::Gap(crook_plugin_api::Gap::Small),
        Node::Rule,
        Node::Gap(crook_plugin_api::Gap::Small),
        Node::Note {
            text: trouble.to_owned(),
            tone: Tone::Danger,
        },
    ])
}

/// A path as a chip prints it: the home directory written `~`.
///
/// The prefix only counts when the next character is a separator, so a sibling
/// directory named `/home/euge` beside `/home/eugen` is not reprinted as `~n`.
/// Crook's own rows do exactly this and the chips have to match them.
pub fn shorten(path: &str, home: Option<&str>) -> String {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return path.to_owned();
    };
    let Some(rest) = path.strip_prefix(home) else {
        return path.to_owned();
    };
    match rest.chars().next() {
        None => String::from("~"),
        Some('/') => format!("~{rest}"),
        Some(_) => path.to_owned(),
    }
}

/// The directory above this one, or `None` at the root.
pub fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    let at = trimmed.rfind('/')?;
    match at {
        // `/thing` is under the root, and the root is written with the slash.
        0 => Some(String::from("/")),
        _ => Some(trimmed[..at].to_owned()),
    }
}

/// A directory and a name in it, with exactly one separator between them.
pub fn join(directory: &str, name: &str) -> String {
    match directory.ends_with('/') {
        true => format!("{directory}{name}"),
        false => format!("{directory}/{name}"),
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;

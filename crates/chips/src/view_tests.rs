//! What the chips describe, given what they know.

use crook_plugin_api::{Command, Entry, Place};

use super::*;

/// A plugin that knows where it is.
fn somewhere() -> Chips {
    let mut chips = Chips::new();
    chips.place = Some(Place {
        directory: String::from("/home/eugen/Work/crook"),
        branch: Some(String::from("main")),
        worktree: false,
    });
    chips.home = Some(String::from("/home/eugen"));
    chips
}

#[test]
fn a_chip_says_where_the_pane_is_with_the_home_directory_written_short() {
    let Node::Anchored { content, panel, .. } = chip(&somewhere(), "directory") else {
        panic!("the directory chip should hang a panel");
    };
    assert!(panel.is_none(), "nothing is open yet");

    let Node::Pressable { content, action } = *content else {
        panic!("the chip should be pressable");
    };
    assert_eq!(action, "open-directory");
    assert_eq!(
        *content,
        Node::Chip {
            icon: String::from("folder"),
            text: String::from("~/Work/crook"),
            tone: Tone::Primary,
        }
    );
}

#[test]
fn a_pane_that_has_not_said_where_it_is_draws_nothing() {
    // An empty contribution takes no room in the row, which is the difference
    // between a chip that is not there and a chip that says nothing.
    assert_eq!(chip(&Chips::new(), "directory"), Node::Empty);
    assert_eq!(chip(&Chips::new(), "branch"), Node::Empty);
    assert_eq!(chip(&Chips::new(), "diff"), Node::Empty);
    assert_eq!(chip(&Chips::new(), "keys"), Node::Empty);
}

#[test]
fn the_directory_picker_offers_the_way_up_and_everything_under_here() {
    let mut chips = somewhere();
    chips.panel = Panel::Directory;
    chips.browsing = String::from("/home/eugen/Work/crook");
    chips.entries = vec![Entry {
        name: String::from("app"),
        directory: true,
    }];

    let Node::Anchored {
        panel: Some(panel), ..
    } = chip(&chips, "directory")
    else {
        panic!("the panel should be up");
    };
    let Node::Picker { rows, choose, .. } = *panel else {
        panic!("the panel should be a picker");
    };

    assert_eq!(choose, "choose-directory");
    let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, ["/home/eugen/Work", "/home/eugen/Work/crook/app"]);
}

#[test]
fn the_branch_picker_marks_the_branch_that_is_checked_out() {
    let mut chips = somewhere();
    chips.panel = Panel::Branch;
    chips.head = Some(String::from("main"));
    chips.branches = vec![String::from("main"), String::from("pirate-ext")];

    let Node::Anchored {
        panel: Some(panel), ..
    } = chip(&chips, "branch")
    else {
        panic!("the panel should be up");
    };
    let Node::Picker { rows, .. } = *panel else {
        panic!("the panel should be a picker");
    };

    assert_eq!(rows[0].tone, Tone::Accent, "the one you are on");
    assert_eq!(rows[1].tone, Tone::Primary);
}

#[test]
fn the_diff_chip_is_absent_until_something_has_changed() {
    let mut chips = somewhere();
    assert_eq!(chip(&chips, "diff"), Node::Empty);

    chips.added = 12;
    chips.removed = 3;

    assert_eq!(
        chip(&chips, "diff"),
        Node::Chip {
            icon: String::from("diff"),
            text: String::from("+12 −3"),
            tone: Tone::Warning,
        }
    );
}

#[test]
fn the_keys_chip_carries_a_menu_that_changes_the_binding() {
    let mut chips = somewhere();
    chips.commands = vec![Command {
        name: String::from(SHOWN_COMMAND),
        title: String::from("New agent tab"),
        chord: Some(String::from("ctrl+shift+t")),
    }];

    let Node::Menu { content, items } = chip(&chips, "keys") else {
        panic!("the keys chip should carry a menu");
    };
    assert_eq!(items[0].label, "Change keybinding");
    assert_eq!(items[0].action, "rebind");
    assert_eq!(items[0].argument, SHOWN_COMMAND);

    let Node::Pressable { content, .. } = *content else {
        panic!("and be pressable");
    };
    let Node::Chip { text, .. } = *content else {
        panic!("and be a chip");
    };
    assert_eq!(text, "ctrl+shift+t  new agent tab");
}

#[test]
fn a_path_outside_the_home_directory_is_printed_whole() {
    assert_eq!(shorten("/etc/nginx", Some("/home/eugen")), "/etc/nginx");
    assert_eq!(
        shorten("/home/eugene/x", Some("/home/eugen")),
        "/home/eugene/x"
    );
    assert_eq!(shorten("/home/eugen", Some("/home/eugen")), "~");
}

#[test]
fn the_way_up_stops_at_the_root() {
    assert_eq!(
        parent_of("/home/eugen/Work").as_deref(),
        Some("/home/eugen")
    );
    assert_eq!(parent_of("/home").as_deref(), Some("/"));
    assert_eq!(parent_of("/"), None);
}

#[test]
fn a_directory_and_a_name_meet_at_exactly_one_separator() {
    assert_eq!(join("/home/eugen", "Work"), "/home/eugen/Work");
    assert_eq!(join("/", "home"), "/home");
}

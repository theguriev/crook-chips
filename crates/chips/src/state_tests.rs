//! What the chips do when they are told things.
//!
//! Every one of these runs on an ordinary machine: the imports are stubbed —
//! see [`sys::stub`](crate::sys::stub) — so what is being asserted is the
//! state machine, which is the only part of a plugin that can be wrong in a
//! way a person would notice.

use crook_plugin_api::{Command, Entry, Place, Request};

use super::*;
use crate::sys::stub;

/// The directory these tests pretend the pane is in.
fn somewhere() -> Place {
    Place {
        directory: String::from("/home/eugen/Work/crook"),
        branch: None,
        worktree: false,
    }
}

/// A plugin that has been built, with everything it asked for thrown away.
///
/// Taken rather than forgotten: the tickets go on counting, because the two
/// requests a build raises are still outstanding and a ticket handed out twice
/// would be answered as the wrong question — which is exactly the bug this
/// helper had.
fn built() -> Chips {
    stub::forget();
    let mut chips = Chips::new();
    chips.build();
    let _ = stub::taken();
    chips
}

/// Answers the last thing asked for, as the host would.
fn answer(chips: &mut Chips, answer: Answer) {
    let asked = stub::taken();
    let (ticket, _) = asked
        .requests
        .last()
        .expect("something should have been asked");
    chips.deliver(*ticket, answer);
}

#[test]
fn building_registers_the_four_chips_and_asks_where_it_is() {
    stub::forget();
    let mut chips = Chips::new();
    chips.build();
    let asked = stub::taken();

    let entries: Vec<&str> = asked
        .contributions
        .iter()
        .map(|(slot, entry, _)| {
            assert_eq!(slot, SLOT);
            entry.as_str()
        })
        .collect();
    assert_eq!(entries, ["directory", "branch", "diff", "keys"]);

    let requests: Vec<&Request> = asked.requests.iter().map(|(_, request)| request).collect();
    assert_eq!(requests, [&Request::Where, &Request::Commands]);
    assert_eq!(asked.timers, [2_000], "it has to keep asking");
}

#[test]
fn opening_the_directory_picker_lists_the_directory_the_pane_is_in() {
    let mut chips = built();
    chips.place = Some(somewhere());

    chips.run("open-directory", "");

    assert_eq!(chips.panel, Panel::Directory);
    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::List {
            path: String::from("/home/eugen/Work/crook")
        })
    );
}

#[test]
fn the_branch_picker_shuts_when_the_pane_moves_to_another_directory() {
    // Its rows are the branches of the repository it was opened in, and a
    // row chosen after the pane has moved types `git switch` into another.
    let mut chips = built();
    chips.place = Some(somewhere());
    chips.run("open-branch", "");
    answer(
        &mut chips,
        Answer::Repository {
            head: Some(String::from("main")),
            branches: vec![String::from("main"), String::from("old-work")],
        },
    );
    assert_eq!(chips.panel, Panel::Branch);

    chips.tick();
    answer(
        &mut chips,
        Answer::Where {
            place: Some(Place {
                directory: String::from("/home/eugen/Work/elsewhere"),
                branch: Some(String::from("trunk")),
                worktree: false,
            }),
            home: None,
            added: 0,
            removed: 0,
        },
    );

    assert_eq!(chips.panel, Panel::None);
}

#[test]
fn the_branch_picker_stays_up_while_the_pane_stays_put() {
    // The poll answers every two seconds whether or not anything moved.
    let mut chips = built();
    chips.place = Some(somewhere());
    chips.run("open-branch", "");
    let _ = stub::taken();

    chips.tick();
    answer(
        &mut chips,
        Answer::Where {
            place: Some(somewhere()),
            home: None,
            added: 0,
            removed: 0,
        },
    );

    assert_eq!(chips.panel, Panel::Branch);
}

#[test]
fn a_listing_keeps_the_directories_and_drops_the_files() {
    // A picker whose rows are places you can be. A file is not one, and a row
    // that did nothing when it was chosen would be worse than no row.
    let mut chips = built();
    chips.run("open-directory", "");
    answer(
        &mut chips,
        Answer::Listed(vec![
            Entry {
                name: String::from("app"),
                directory: true,
            },
            Entry {
                name: String::from("Cargo.toml"),
                directory: false,
            },
        ]),
    );

    let names: Vec<&str> = chips
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["app"]);
}

#[test]
fn choosing_a_directory_types_a_cd_and_shuts_the_panel() {
    let mut chips = built();
    chips.run("open-directory", "");
    let _ = stub::taken();

    chips.run("choose-directory", "/home/eugen/Work/crook/app");

    assert_eq!(chips.panel, Panel::None);
    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::Type {
            template: String::from(CD),
            argument: String::from("/home/eugen/Work/crook/app"),
        }),
        "it types the granted template and nothing else"
    );
}

#[test]
fn choosing_a_branch_switches_to_it() {
    let mut chips = built();
    chips.place = Some(Place {
        branch: Some(String::from("main")),
        ..somewhere()
    });
    chips.run("open-branch", "");
    let _ = stub::taken();

    chips.run("choose-branch", "pirate-ext");

    assert_eq!(chips.panel, Panel::None);
    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::Type {
            template: String::from(SWITCH),
            argument: String::from("pirate-ext"),
        })
    );
}

#[test]
fn an_action_with_nothing_to_say_asks_for_nothing() {
    // Every action that needs an argument is reachable by name, and a chord
    // bound to one arrives with an empty one. That must be a plugin that does
    // nothing rather than a plugin that types `cd ''`.
    let mut chips = built();

    chips.run("choose-directory", "");
    chips.run("choose-branch", "");

    assert!(stub::taken().requests.is_empty());
}

#[test]
fn the_keys_chip_runs_the_command_it_stands_for() {
    let mut chips = built();

    chips.run("run-command", "");

    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::Run {
            name: String::from(SHOWN_COMMAND),
            argument: String::new(),
        })
    );
}

#[test]
fn its_menu_asks_crook_to_record_a_new_chord_for_that_command() {
    let mut chips = built();

    chips.run("rebind", SHOWN_COMMAND);

    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::Run {
            name: String::from(REBIND),
            argument: String::from(SHOWN_COMMAND),
        })
    );
}

#[test]
fn a_refusal_is_kept_where_a_person_will_read_it() {
    // The commonest thing that goes wrong here is that nobody has allowed it
    // yet, and the refusal carries the sentence the Plugins page says. A
    // plugin that logged that and drew nothing would be a plugin that looks
    // broken.
    let mut chips = built();
    chips.run("open-directory", "");
    answer(
        &mut chips,
        Answer::Refused(String::from("See the names of the files in ~")),
    );

    assert_eq!(
        chips.trouble.as_deref(),
        Some("Not allowed to: See the names of the files in ~")
    );
}

#[test]
fn the_panel_shuts_when_the_pane_moves_out_from_under_it() {
    // A `cd` in the shell while the picker is open leaves it listing a
    // directory nobody is in. Warp's chooser does the same thing.
    let mut chips = built();
    chips.place = Some(somewhere());
    chips.run("open-directory", "");
    let _ = stub::taken();

    chips.tick();
    answer(
        &mut chips,
        Answer::Where {
            place: Some(Place {
                directory: String::from("/home/eugen/Work"),
                branch: None,
                worktree: false,
            }),
            home: None,
            added: 0,
            removed: 0,
        },
    );

    assert_eq!(chips.panel, Panel::None);
}

#[test]
fn a_tick_asks_again_and_goes_back_to_waiting() {
    let mut chips = built();

    chips.tick();

    let asked = stub::taken();
    assert_eq!(
        asked.requests.last().map(|(_, request)| request),
        Some(&Request::Where)
    );
    assert_eq!(asked.timers, [2_000]);
}

#[test]
fn the_chip_that_prints_a_chord_finds_its_command_by_name() {
    let mut chips = built();
    chips.commands = vec![
        Command {
            name: String::from("crook/window/close-pane"),
            title: String::from("Close the focused pane"),
            chord: Some(String::from("ctrl+shift+w")),
        },
        Command {
            name: String::from(SHOWN_COMMAND),
            title: String::from("New agent tab"),
            chord: Some(String::from("ctrl+shift+t")),
        },
    ];

    let shown = chips.shown_command().expect("it should find it");

    assert_eq!(shown.title, "New agent tab");
    assert_eq!(shown.chord.as_deref(), Some("ctrl+shift+t"));
}

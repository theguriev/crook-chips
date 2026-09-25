//! What the chips know, and what they ask for.
//!
//! Everything that decides anything is here, and nothing here touches an
//! import directly — [`sys`](crate::sys) does that — so this whole module runs
//! under `cargo test` on an ordinary machine.
//!
//! # It holds nothing the host already knows
//!
//! Where the pane is, which branch it is on and what Crook can be asked to do
//! are all the host's answers, and they are re-asked rather than remembered
//! across a `cd`: a chip that cached a directory would be a chip that lies
//! about where you are, which is worse than a chip that says nothing. The
//! only things kept between frames are the ones the host has no idea about —
//! which panel is open, and what was in the directory it is showing.

use crook_plugin_api::{Answer, Command, Entry, Place, Request};

use crate::sys::{self, Level};

/// The slot the chips go in.
pub const SLOT: &str = "pane.chips";

/// The command line typing a directory in runs. Granted as this exact string,
/// and the host puts the directory in the hole and quotes it.
pub const CD: &str = "cd {}";

/// The one that changes branch.
///
/// `git switch` rather than `git checkout`: it is the command git itself
/// recommends for this, it refuses to do the four other things `checkout`
/// does, and a plugin that may only type one command should be typing the
/// narrowest one that works.
pub const SWITCH: &str = "git switch {}";

/// The command that opens the recorder for a keybinding.
pub const REBIND: &str = "crook/shortcuts/rebind";

/// The command the keys chip stands for.
///
/// Warp's chip says what starts a new agent conversation, and this is Crook's
/// name for that. It is a constant rather than a setting because a chip that
/// could be pointed at any command would need a settings page, and a plugin
/// with a settings page is a bigger thing than this.
pub const SHOWN_COMMAND: &str = "crook/window/new-tab";

/// The root the directory picker may list under.
pub const ROOT: &str = "~";

/// How often the chips ask the host where the pane is.
///
/// Two seconds. The answer is a map lookup on the host's side — the working
/// directory the shell reported and the git facts a background gather left
/// behind — so this costs a request and a decode, not a `git` process. Faster
/// would be a chip that flickers; slower is a chip that is still showing the
/// directory you left.
const REFRESH: i32 = 2_000;

/// Which panel is up, if any.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    /// None: three chips and nothing over them.
    #[default]
    None,
    /// The directories under the one the pane is in.
    Directory,
    /// The repository's branches.
    Branch,
}

/// What one outstanding request was for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Asked {
    /// Where the pane is.
    Where,
    /// What is in a directory.
    List,
    /// What the repository has.
    Repository,
    /// What Crook can be asked to do.
    Commands,
    /// Something that changes the world: a line typed, a command run.
    Deed,
}

/// Everything the chips know.
#[derive(Debug, Default)]
pub struct Chips {
    /// Where the focused pane is, as the host last answered.
    pub place: Option<Place>,
    /// The person's home directory, for printing a path as `~/…`.
    pub home: Option<String>,
    /// How much has changed in the working tree.
    pub added: u32,
    pub removed: u32,
    /// Which panel is up.
    pub panel: Panel,
    /// The directory the picker is showing, and what is in it.
    pub browsing: String,
    pub entries: Vec<Entry>,
    /// What the repository under the pane has.
    pub head: Option<String>,
    pub branches: Vec<String>,
    /// Everything Crook can be asked to do, for the chip that prints a chord.
    pub commands: Vec<Command>,
    /// What went wrong with the last thing asked for, in the host's own words.
    ///
    /// Kept and shown rather than logged and forgotten: the commonest reason
    /// anything here fails is that nobody has allowed it yet, and the refusal
    /// carries the sentence the Plugins page will say — so the panel can tell
    /// a person what to go and allow.
    pub trouble: Option<String>,
    /// The tickets outstanding, and what each was for.
    waiting: Vec<(i32, Asked)>,
}

impl Chips {
    /// A plugin that knows nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers everything, and asks the first questions.
    pub fn build(&mut self) {
        sys::contribute(SLOT, "directory", 10);
        sys::contribute(SLOT, "branch", 20);
        sys::contribute(SLOT, "diff", 25);
        sys::contribute(SLOT, "keys", 30);

        // None of these is offered with a title: they are what the chips do
        // when they are pressed, and a command palette whose rows are
        // "choose-branch" is a palette nobody reads. What a person can reach
        // by name is still every one of them — the host registers them either
        // way — so a chord may open the branch picker if somebody binds one.
        for action in [
            "open-directory",
            "open-branch",
            "choose-directory",
            "choose-branch",
            "run-command",
            "rebind",
            "dismiss",
        ] {
            sys::register_action(action, None);
        }

        self.ask(Asked::Where, Request::Where);
        self.ask(Asked::Commands, Request::Commands);
        sys::set_timer(REFRESH);
    }

    /// The wait is over: ask again, and go back to waiting.
    pub fn tick(&mut self) {
        self.ask(Asked::Where, Request::Where);
        sys::set_timer(REFRESH);
    }

    /// Runs one of the actions registered above.
    ///
    /// `argument` is what the thing pressed had to say: the key of a chosen
    /// row, the entry of a menu. Empty for a chord and for the palette, which
    /// is why every arm that needs one checks.
    pub fn run(&mut self, action: &str, argument: &str) {
        self.trouble = None;
        match action {
            "open-directory" => {
                self.panel = Panel::Directory;
                self.browsing = self.directory().unwrap_or_default().to_owned();
                self.entries.clear();
                let path = self.browsing.clone();
                self.ask(Asked::List, Request::List { path });
            }
            "open-branch" => {
                self.panel = Panel::Branch;
                self.branches.clear();
                if let Some(path) = self.directory().map(str::to_owned) {
                    self.ask(Asked::Repository, Request::Repository { path });
                }
            }
            "dismiss" => self.panel = Panel::None,
            "choose-directory" if !argument.is_empty() => {
                self.panel = Panel::None;
                self.ask(
                    Asked::Deed,
                    Request::Type {
                        template: String::from(CD),
                        argument: argument.to_owned(),
                    },
                );
            }
            "choose-branch" if !argument.is_empty() => {
                self.panel = Panel::None;
                self.ask(
                    Asked::Deed,
                    Request::Type {
                        template: String::from(SWITCH),
                        argument: argument.to_owned(),
                    },
                );
            }
            // The chip stands for one command and says so on its face, so
            // pressing it needs nothing said: an empty argument means the one
            // the chip is about, which is also the only one this plugin is
            // allowed to run.
            "run-command" => {
                let name = match argument.is_empty() {
                    true => String::from(SHOWN_COMMAND),
                    false => argument.to_owned(),
                };
                self.ask(
                    Asked::Deed,
                    Request::Run {
                        name,
                        argument: String::new(),
                    },
                );
            }
            "rebind" if !argument.is_empty() => self.ask(
                Asked::Deed,
                Request::Run {
                    name: String::from(REBIND),
                    argument: argument.to_owned(),
                },
            ),
            _ => sys::log(Level::Warn, &format!("nothing to do for {action:?}")),
        }
    }

    /// The answer to something asked for.
    pub fn deliver(&mut self, ticket: i32, answer: Answer) {
        let Some(at) = self.waiting.iter().position(|(known, _)| *known == ticket) else {
            // A ticket nobody is waiting on. Not worth a line: a plugin that
            // was rebuilt while an answer was in flight gets exactly this.
            return;
        };
        let (_, asked) = self.waiting.remove(at);

        match (asked, answer) {
            (
                Asked::Where,
                Answer::Where {
                    place,
                    home,
                    added,
                    removed,
                },
            ) => {
                // The directory moved out from under the panel: what it is
                // showing is about a directory nobody is in any more. The
                // branch picker as much as the directory one — its rows are the
                // old repository's branches, and choosing one would type a
                // `git switch` into a repository that may have no such branch
                // or, worse, one of the same name.
                let directory = place.as_ref().map(|place| place.directory.clone());
                if self.panel != Panel::None && self.directory() != directory.as_deref() {
                    self.panel = Panel::None;
                }
                self.place = place;
                self.home = home;
                self.added = added;
                self.removed = removed;
            }
            (Asked::List, Answer::Listed(entries)) => {
                // Directories only. A picker that cds is a picker whose rows
                // are places you can be, and a file is not one.
                self.entries = entries
                    .into_iter()
                    .filter(|entry| entry.directory)
                    .collect();
            }
            (Asked::Repository, Answer::Repository { head, branches }) => {
                self.head = head;
                self.branches = branches;
            }
            (Asked::Commands, Answer::Commands(commands)) => self.commands = commands,
            (Asked::Deed, Answer::Done) => {}
            // Everything that did not work, in the host's own words. A refusal
            // says which permission is missing, which is the one thing worth
            // putting in front of a person.
            (_, Answer::Refused(sentence)) => {
                self.trouble = Some(format!("Not allowed to: {sentence}"));
            }
            (_, Answer::Failed(why)) => self.trouble = Some(why),
            (asked, answer) => sys::log(
                Level::Warn,
                &format!("{asked:?} was answered with something else: {answer:?}"),
            ),
        }
    }

    /// Where the pane is, or `None` before it has said.
    pub fn directory(&self) -> Option<&str> {
        self.place.as_ref().map(|place| place.directory.as_str())
    }

    /// Which branch it is on, when the directory is in a repository.
    pub fn branch(&self) -> Option<&str> {
        self.place
            .as_ref()
            .and_then(|place| place.branch.as_deref())
    }

    /// What the chip that prints a chord should say, if the host has told us.
    pub fn shown_command(&self) -> Option<&Command> {
        self.commands
            .iter()
            .find(|command| command.name == SHOWN_COMMAND)
    }

    /// Raises one request and remembers what it was for.
    fn ask(&mut self, asked: Asked, request: Request) {
        match sys::ask(&request) {
            Some(ticket) => self.waiting.push((ticket, asked)),
            None => sys::log(Level::Warn, "the host would not take a request"),
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

//! The chips beside the prompt, as a plugin Crook does not carry.
//!
//! Where the pane is, which branch it is on, how much has changed and what one
//! chord would do — four small things at the foot of the pane, each of which
//! opens something when it is pressed. Warp draws exactly this row and Crook
//! draws none of it: the row is a slot, and everything in it here is a
//! sandboxed plugin with no network, no filesystem and no thread of its own.
//! It describes, it asks, and it is answered.
//!
//! # What it is allowed to do, in four sentences
//!
//! * see which project the active pane is in — the directory, the branch and
//!   the line counts the chips print;
//! * see the names of the files under `~` — the directory picker's rows, names
//!   only and one directory at a time;
//! * type `cd …` and `git switch …` into the shell, and nothing else, only
//!   when somebody has pressed something, with the argument quoted by the host;
//! * use two of Crook's own commands by name: the one the keys chip stands for
//!   and the one that records a new chord for it.
//!
//! Everything not on that list is refused rather than failed, and a refusal
//! comes back carrying the sentence the Plugins page says — which is what the
//! panels print when something has not been allowed yet.
//!
//! # What this file is
//!
//! The ABI, and nothing that thinks. Every export the host calls is here, each
//! of them is three lines, and each hands straight over to [`state`] — so the
//! part that decides anything is a plain Rust module `cargo test` runs on an
//! ordinary machine. See [`sys`] for the other half of that trick.

use std::cell::UnsafeCell;

use crook_plugin_api::{
    ABI_VERSION, Answer, Capability, Manifest, Node, Render, from_bytes, to_bytes,
};

pub mod state;
pub mod sys;
pub mod view;

// The plugin's face, and what it looks like, for the Plugins page and the
// Store. Inside the module rather than beside it, for the reason a plugin is
// one file: what says what the plugin is travels with it. Custom sections,
// not data — they cost no memory and no fuel.
crook_plugin_api::icon!("../../../assets/icon.png");
crook_plugin_api::preview!(
    1,
    "../../../assets/chips.png",
    "The row of chips under the line being typed"
);
crook_plugin_api::preview!(2, "../../../assets/directories.png", "The directory picker");
crook_plugin_api::preview!(3, "../../../assets/branches.png", "The branch picker");

use state::Chips;

/// A `static` that is only ever touched by one thread, which on wasm32 is
/// every thread there is.
struct Single<T>(UnsafeCell<T>);

// SAFETY: wasm32 has one thread. Off wasm this crate is a library under test,
// where each test gets its own `Chips` rather than this one.
unsafe impl<T> Sync for Single<T> {}

impl<T> Single<T> {
    /// SAFETY: the caller must not be inside another borrow. Every export
    /// below takes one, does its work and returns, and the host does not call
    /// in while it is already inside.
    #[allow(clippy::mut_from_ref)]
    unsafe fn get(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

/// Everything the plugin knows.
static CHIPS: Single<Option<Chips>> = Single(UnsafeCell::new(None));

/// The last thing handed back to the host, kept alive until the next one.
///
/// A tree is answered as an offset and a length into this memory, so the bytes
/// have to outlive the call that returned them. Kept rather than leaked
/// because a render happens every frame, and a leak per frame is a plugin that
/// eventually stops fitting in its own sixteen megabytes.
static ANSWER: Single<Vec<u8>> = Single(UnsafeCell::new(Vec::new()));

/// Packs an answer as the host reads it: `(pointer << 32) | length`.
fn hand_back(bytes: Vec<u8>) -> i64 {
    // SAFETY: see `Single::get`.
    let answer = unsafe { ANSWER.get() };
    *answer = bytes;
    ((answer.as_ptr() as u64) << 32 | answer.len() as u64) as i64
}

/// The plugin's own state, made on first use.
fn chips() -> &'static mut Chips {
    // SAFETY: see `Single::get`.
    unsafe { CHIPS.get() }.get_or_insert_with(Chips::new)
}

/// Which version of the vocabulary this was built against.
#[unsafe(no_mangle)]
pub extern "C" fn crook_abi_version() -> i32 {
    ABI_VERSION as i32
}

/// Somewhere for the host to put a string it is handing over.
#[unsafe(no_mangle)]
pub extern "C" fn crook_alloc(length: i32) -> i32 {
    let Ok(layout) = std::alloc::Layout::from_size_align(length.max(1) as usize, 1) else {
        return 0;
    };
    // SAFETY: a non-zero size, and a layout built for it.
    unsafe { std::alloc::alloc(layout) as i32 }
}

/// Copies out what the host wrote there, and gives the memory back.
///
/// SAFETY: `pointer` and `length` must be exactly what a previous
/// [`crook_alloc`] answered and what the host wrote into.
unsafe fn take(pointer: i32, length: i32) -> Vec<u8> {
    if pointer <= 0 || length < 0 {
        return Vec::new();
    }
    // SAFETY: the host wrote `length` bytes at `pointer` before calling in.
    let bytes =
        unsafe { std::slice::from_raw_parts(pointer as *const u8, length as usize) }.to_vec();
    // SAFETY: the same layout `crook_alloc` used.
    unsafe {
        std::alloc::dealloc(
            pointer as *mut u8,
            std::alloc::Layout::from_size_align_unchecked(length.max(1) as usize, 1),
        );
    }
    bytes
}

/// What this plugin is and what it needs to be allowed to do.
///
/// Read before any of it runs, which is what lets a person see what it wants
/// and refuse it without running a line of it.
#[unsafe(no_mangle)]
pub extern "C" fn crook_manifest() -> i64 {
    hand_back(to_bytes(&manifest()).unwrap_or_default())
}

/// The manifest, as a value, so that a test can read it.
pub fn manifest() -> Manifest {
    Manifest {
        abi: ABI_VERSION,
        id: String::from("theguriev/chips"),
        name: String::from("Chips"),
        description: String::from(
            "Where the pane is, what branch it is on, and what a chord would do.",
        ),
        version: String::from(env!("CARGO_PKG_VERSION")),
        capabilities: vec![
            Capability::ReadWorkingDirectory,
            Capability::ListDirectories(vec![String::from(state::ROOT)]),
            Capability::TypeCommands(vec![String::from(state::CD), String::from(state::SWITCH)]),
            Capability::ReadCommands,
            Capability::RunCommands(vec![
                String::from(state::SHOWN_COMMAND),
                String::from(state::REBIND),
            ]),
        ],
    }
}

/// Registers the chips and the actions, and asks the first questions.
///
/// **From nothing, every time.** A build is not resumed: the host builds a
/// plugin again when it is switched back on and when a person answers what it
/// asked to be allowed, and what was left of the previous life is worse than
/// useless — a timer nobody will fire and a ticket nobody will answer.
#[unsafe(no_mangle)]
pub extern "C" fn crook_build() -> i32 {
    // SAFETY: see `Single::get`.
    let held = unsafe { CHIPS.get() };
    *held = Some(Chips::new());
    held.get_or_insert_with(Chips::new).build();
    0
}

/// What to draw for one of this plugin's contributions.
///
/// The host says which slot and which of this plugin's own entries it is
/// asking about — a row of chips is four contributions to one slot, and a
/// render told only the slot would have to draw all four in each of them.
#[unsafe(no_mangle)]
pub extern "C" fn crook_render(bytes: i32, length: i32) -> i64 {
    // SAFETY: the host allocated and wrote this before calling in.
    let bytes = unsafe { take(bytes, length) };

    let tree = match from_bytes::<Render>(&bytes) {
        Ok(render) if render.slot == state::SLOT => view::chip(chips(), &render.entry),
        // A slot this plugin does not contribute to, which cannot happen, and
        // a render this build cannot read, which means the host speaks a
        // version this one does not. Both draw nothing rather than a guess.
        _ => Node::Empty,
    };
    hand_back(to_bytes(&tree).unwrap_or_default())
}

/// Runs one of the actions registered while building, with whatever the thing
/// that was pressed had to say.
#[unsafe(no_mangle)]
pub extern "C" fn crook_run(name: i32, name_len: i32, argument: i32, argument_len: i32) -> i32 {
    // SAFETY: as above.
    let name = unsafe { take(name, name_len) };
    let argument = unsafe { take(argument, argument_len) };

    match (std::str::from_utf8(&name), std::str::from_utf8(&argument)) {
        (Ok(action), Ok(argument)) => chips().run(action, argument),
        _ => return 1,
    }
    0
}

/// The answer to something this plugin asked for.
#[unsafe(no_mangle)]
pub extern "C" fn crook_deliver(ticket: i32, bytes: i32, length: i32) -> i32 {
    // SAFETY: as above.
    let bytes = unsafe { take(bytes, length) };
    match from_bytes::<Answer>(&bytes) {
        Ok(answer) => chips().deliver(ticket, answer),
        // An answer this build cannot read is a host speaking a version this
        // one does not, which the ABI check should already have caught.
        Err(_) => return 1,
    }
    0
}

/// The wait asked for has passed.
#[unsafe(no_mangle)]
pub extern "C" fn crook_tick() -> i32 {
    chips().tick();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pictures_in_the_module_are_pngs() {
        // A wrong path fails at compile time; a wrong file fails here, on the
        // machine that runs the tests, rather than on the Plugins page.
        assert!(CROOK_ICON.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(CROOK_PREVIEW_1.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(CROOK_PREVIEW_2.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(CROOK_PREVIEW_3.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn the_manifest_says_what_it_needs_in_sentences_a_person_can_refuse() {
        let sentences: Vec<String> = manifest()
            .capabilities
            .iter()
            .map(Capability::sentence)
            .collect();

        assert_eq!(
            sentences,
            vec![
                String::from("See which project each tab is in"),
                String::from("See the names of the files in ~"),
                String::from("Type into your shell, and run: cd …, git switch …"),
                String::from("See what Crook can be asked to do, and the keys for it"),
                String::from("Use Crook's own crook/window/new-tab, crook/shortcuts/rebind"),
            ]
        );
    }

    #[test]
    fn what_it_asks_for_is_exactly_what_it_uses() {
        // The one invariant tying the manifest to the code: a plugin that
        // asked for less than it uses is refused at runtime, and one that asks
        // for more is one nobody should allow.
        let keys: Vec<String> = manifest()
            .capabilities
            .iter()
            .flat_map(Capability::keys)
            .collect();

        assert!(keys.contains(&format!("list:{}", state::ROOT)));
        assert!(keys.contains(&format!("type:{}", state::CD)));
        assert!(keys.contains(&format!("type:{}", state::SWITCH)));
        assert!(keys.contains(&format!("run:{}", state::REBIND)));
        assert!(keys.contains(&format!("run:{}", state::SHOWN_COMMAND)));
        assert_eq!(keys.len(), 7, "it asks for something it does not use");
    }

    #[test]
    fn the_manifest_crosses_the_wire_as_itself() {
        let bytes = to_bytes(&manifest()).expect("a manifest should encode");

        assert_eq!(
            from_bytes::<Manifest>(&bytes).expect("and decode"),
            manifest()
        );
    }
}

//! What the window shows of the history. The history is a database
//! (history.rs): nothing of it is held here but the few lines on screen.
//! Each part of the window — the address bar's suggestions, the history
//! page, the sidebar, the command palette, the new tab page — asks for its
//! lines again when what they answer changes (the text typed, the history
//! itself), on a thread of its own: never at every frame, and never in the
//! window's thread, which would wait for the disk.

use super::*;
use crate::history::{History, Hit, Listing, Row, Site};

/// How many lines of the history page are read at a time.
pub(super) const CHUNK: usize = 200;

/// How many addresses, the last visited, the Light engine colours as
/// visited links (the window keeps these in memory; the history itself
/// has no limit).
const VISITED_KEPT: usize = 20_000;

/// An answer of the history, and the question it answers.
pub(super) struct Asked<K, T> {
    /// What `got` answers (None: nothing was answered yet).
    key: Option<K>,
    /// What is being looked up.
    pending: Option<K>,
    pub got: T,
    /// How many answers came (0: none yet).
    pub answers: u64,
}

impl<K, T: Default> Default for Asked<K, T> {
    fn default() -> Self {
        Asked { key: None, pending: None, got: T::default(), answers: 0 }
    }
}

impl<K: PartialEq, T> Asked<K, T> {
    /// Is `got` the answer to this?
    pub fn answers_to(&self, key: &K) -> bool {
        self.key.as_ref() == Some(key)
    }
}

/// What the list of suggestions under the address bar was put together
/// from: put together again only when one of these changes.
#[derive(PartialEq, Default)]
pub(super) struct SuggestMade {
    pub typed: String,
    pub held: String,
    pub answers: u64,
    pub tabs: usize,
    pub active: usize,
    pub favourites: usize,
}

/// The lines of the history the window has on screen.
#[derive(Default)]
pub(super) struct Shown {
    /// Address bar: the pages that answer (the history's state, the text
    /// typed in lower case).
    pub suggest: Asked<(u64, String), Vec<Hit>>,
    pub suggest_made: SuggestMade,
    /// The site's name the address field was completed with, and where
    /// that site really is ("nas.lan:8080" → "http://nas.lan:8080/").
    pub completed: Option<(String, String)>,
    /// Command palette: the pages that answer (the history's state, its text).
    pub palette: Asked<(u64, String), Vec<Hit>>,
    /// Sidebar: the pages visited last, for (the history's state, the filter).
    pub recent: Asked<(u64, String), Vec<(String, String)>>,
    /// New tab page: the sites that count most, for the history's state.
    pub frequent: Asked<u64, Vec<Site>>,
    /// The addresses coloured as visited, for the history's state; and how
    /// many answers the window has taken.
    pub visited: Asked<u64, HashSet<String>>,
    visited_taken: u64,
    /// History page: what is listed for (the history's state, the search),
    /// the search the page was last drawn for…
    pub listing: Asked<(u64, String), Arc<Listing>>,
    pub listed_for: String,
    /// …and the lines read so far, by groups of `CHUNK`, each with the
    /// listing (its answer's number) it was read for.
    pub chunks: HashMap<usize, (u64, Vec<Row>)>,
    chunks_asked: HashSet<(u64, usize)>,
}

impl Sanaga {
    /// Ask the history something for a part of the window, off the
    /// window's thread: `slot` is where that part keeps its answer, `key`
    /// what the answer depends on. Called at every frame it costs one
    /// comparison: the history is asked only when `key` is a new one, and
    /// an answer that comes after the question changed is dropped.
    pub(super) fn history_ask<K, T>(&mut self, slot: fn(&mut Sanaga) -> &mut Asked<K, T>, key: K, look_up: impl FnOnce(&History) -> T + Send + 'static)
    where
        K: PartialEq + Clone + Send + 'static,
        T: Send + 'static,
    {
        let asked = slot(self);
        if asked.key.as_ref() == Some(&key) || asked.pending.as_ref() == Some(&key) {
            return;
        }
        asked.pending = Some(key.clone());
        let history = self.store.history.clone();
        self.off_thread(
            move || look_up(&history),
            move |app, got| {
                let asked = slot(app);
                if asked.pending.as_ref() == Some(&key) {
                    asked.pending = None;
                    asked.key = Some(key);
                    asked.got = got;
                    asked.answers += 1;
                }
            },
        );
    }

    /// The addresses coloured as visited links follow the history — the
    /// visits of the other windows too. Only while a page drawn by the
    /// Light engine is open: no other page shows them.
    pub(super) fn follow_visited(&mut self) {
        let state = self.store.history.version();
        if !self.hist.visited.answers_to(&state) && self.tabs.iter().any(|t| matches!(t.view, View::Doc(_))) {
            self.history_ask(|app| &mut app.hist.visited, state, |h| h.visited_addresses(VISITED_KEPT));
        }
        if self.hist.visited_taken != self.hist.visited.answers {
            self.hist.visited_taken = self.hist.visited.answers;
            self.visited = std::mem::take(&mut self.hist.visited.got);
        }
    }

    /// Read a group of lines of the history page (those coming on screen).
    pub(super) fn history_rows_ask(&mut self, chunk: usize) {
        let listing = self.hist.listing.answers;
        if !self.hist.chunks_asked.insert((listing, chunk)) {
            return;
        }
        let (history, list) = (self.store.history.clone(), self.hist.listing.got.clone());
        self.off_thread(
            move || history.rows(&list, chunk * CHUNK, CHUNK),
            move |app, rows| {
                app.hist.chunks_asked.remove(&(listing, chunk));
                // (the list changed meanwhile: these lines are of the one before)
                if app.hist.listing.answers == listing {
                    app.hist.chunks.insert(chunk, (listing, rows));
                }
            },
        );
    }
}

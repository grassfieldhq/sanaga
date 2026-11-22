//! The assistant's conversations, kept on this computer: `chats.db` in the
//! profile's folder (SQLite, like the history). A conversation can be
//! opened again and gone on with, and earlier ones can be searched — by
//! the person, and by the model when it is allowed to (`app/aitools.rs`).
//!
//! Nothing of a private tab is ever kept (the caller's rule, see
//! `app/assistant.rs`), and clearing the history clears the conversations
//! of the same period.

use crate::chat::{Entry, Tool};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

const DB_FILE: &str = "chats.db";
/// The layout below. A database written by a later Sanaga is left alone.
const LAYOUT: i64 = 1;
/// How many conversations are kept (the oldest go first).
pub const KEPT: usize = 300;

const TABLES: &str = "
CREATE TABLE IF NOT EXISTS chats (
    id INTEGER PRIMARY KEY,
    started INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    head TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS chats_updated ON chats(updated);
CREATE TABLE IF NOT EXISTS steps (
    chat INTEGER NOT NULL REFERENCES chats(id),
    n INTEGER NOT NULL,
    body TEXT NOT NULL,
    said TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (chat, n)
);
";

/// A conversation as it is kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conversation {
    pub title: String,
    /// Its instructions and tools: those it began with (see chat.rs).
    pub system: String,
    pub tools: Vec<Tool>,
    /// What the person sees for the questions shown under another name.
    pub labels: HashMap<usize, String>,
    pub entries: Vec<Entry>,
    /// It holds Sanaga's own extract of a site the administrator keeps
    /// from every model: it is never given to a model again.
    pub kept_from_models: bool,
}

/// One line of the list of conversations.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub id: i64,
    pub title: String,
    pub updated: u64,
    /// See `Conversation::kept_from_models`.
    pub kept_from_models: bool,
}

/// A profile's conversations. Cheap to copy: every copy is the same database.
#[derive(Clone)]
pub struct Chats(Arc<Mutex<Connection>>);

fn secs(t: u64) -> i64 {
    t.min(i64::MAX as u64) as i64
}

fn report(what: &str, e: &dyn std::fmt::Display) {
    eprintln!("conversations ({what}): {e}");
}

/// The words of a step that a search looks into: what the person asked
/// and what the model said — not the pages attached, which are other
/// people's words.
fn said(e: &Entry) -> String {
    match e {
        Entry::User { text, .. } => text.clone(),
        Entry::Assistant { text, .. } => text.clone(),
        Entry::Results(_) => String::new(),
    }
}

fn head_json(c: &Conversation) -> Value {
    json!({
        "system": c.system,
        "tools": c.tools.iter().map(|t| json!({"name": t.name, "description": t.description, "schema": t.schema, "acts": t.acts})).collect::<Vec<_>>(),
        "labels": c.labels.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "kept_from_models": c.kept_from_models,
    })
}

fn head_into(c: &mut Conversation, text: &str) {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return };
    c.system = v["system"].as_str().unwrap_or("").to_string();
    c.tools = v["tools"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|t| {
                    Some(Tool { name: t["name"].as_str()?.to_string(), description: t["description"].as_str().unwrap_or("").to_string(), schema: t["schema"].clone(), acts: t["acts"] == true })
                })
                .collect()
        })
        .unwrap_or_default();
    c.kept_from_models = v["kept_from_models"] == true;
    c.labels = v["labels"].as_array().map(|l| l.iter().filter_map(|p| Some((p[0].as_u64()? as usize, p[1].as_str()?.to_string()))).collect()).unwrap_or_default();
}

impl Chats {
    /// Open a profile's conversations. `None` when the file cannot be used
    /// (this run's conversations are then not kept; the file is left as it is).
    pub fn open(dir: &Path) -> Option<Chats> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = dir.join(DB_FILE);
        // (made here, closed to other users; SQLite gives its log the same rights)
        let _ = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(&path);
        let open = || -> rusqlite::Result<Connection> {
            let c = Connection::open(&path)?;
            let layout: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if layout > LAYOUT {
                return Err(rusqlite::Error::InvalidQuery);
            }
            c.busy_timeout(std::time::Duration::from_secs(5))?;
            // (what is removed is written over: a conversation the person
            // deletes must not stay readable in the file)
            c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA secure_delete=ON; PRAGMA journal_size_limit=4194304;")?;
            if layout < LAYOUT {
                c.execute_batch(&format!("BEGIN IMMEDIATE; {TABLES} PRAGMA user_version={LAYOUT}; COMMIT;"))?;
            }
            Ok(c)
        };
        match open() {
            Ok(c) => Some(Chats(Arc::new(Mutex::new(c)))),
            Err(e) => {
                report("open", &e);
                None
            }
        }
    }

    /// Conversations kept for a test only.
    #[cfg(test)]
    pub fn in_memory() -> Option<Chats> {
        let c = Connection::open_in_memory().ok()?;
        c.execute_batch(TABLES).ok()?;
        Some(Chats(Arc::new(Mutex::new(c))))
    }

    fn with<T>(&self, what: &str, ask: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> Option<T> {
        let mut c = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match ask(&mut c) {
            Ok(v) => Some(v),
            Err(e) => {
                report(what, &e);
                None
            }
        }
    }

    /// Keep a conversation as it is now: a new one (`id` None) gets its
    /// number, which is returned; one kept before is replaced.
    pub fn save(&self, id: Option<i64>, conv: &Conversation, now: u64) -> Option<i64> {
        self.with("save", |c| {
            let tx = c.transaction()?;
            let head = head_json(conv).to_string();
            let id = match id {
                Some(id) if tx.execute("UPDATE chats SET updated=?2, title=?3, head=?4 WHERE id=?1", params![id, secs(now), conv.title, head])? == 1 => id,
                _ => {
                    tx.execute("INSERT INTO chats(started, updated, title, head) VALUES (?1, ?1, ?2, ?3)", params![secs(now), conv.title, head])?;
                    tx.last_insert_rowid()
                }
            };
            tx.execute("DELETE FROM steps WHERE chat=?1", [id])?;
            {
                let mut put = tx.prepare_cached("INSERT INTO steps(chat, n, body, said) VALUES (?1, ?2, ?3, ?4)")?;
                for (n, e) in conv.entries.iter().enumerate() {
                    put.execute(params![id, n as i64, crate::chat::entry_json(e).to_string(), said(e)])?;
                }
            }
            // the oldest conversations give way
            tx.execute("DELETE FROM steps WHERE chat IN (SELECT id FROM chats ORDER BY updated DESC, id DESC LIMIT -1 OFFSET ?1)", [KEPT as i64])?;
            tx.execute("DELETE FROM chats WHERE id IN (SELECT id FROM chats ORDER BY updated DESC, id DESC LIMIT -1 OFFSET ?1)", [KEPT as i64])?;
            tx.commit()?;
            Ok(id)
        })
    }

    /// The conversations kept, the latest first; `words`: only those where
    /// each word was said (by the person or the model) or is in the title.
    pub fn list(&self, words: &str, limit: usize) -> Vec<Kept> {
        self.list_where(words, limit, false)
    }

    /// The conversations that may be given to a model (the assistant's
    /// search of earlier conversations): as `list`, without those that
    /// hold a site kept from every model.
    pub fn list_for_models(&self, words: &str, limit: usize) -> Vec<Kept> {
        self.list_where(words, limit, true)
    }

    fn list_where(&self, words: &str, limit: usize, for_models: bool) -> Vec<Kept> {
        let words = crate::history::words_of(words);
        self.with("list", |c| {
            // (the head is the JSON head_json writes: the mark is found as written)
            let mut st = c.prepare_cached("SELECT id, title, updated, instr(head, '\"kept_from_models\":true') > 0 FROM chats ORDER BY updated DESC, id DESC")?;
            let mut said = c.prepare_cached("SELECT said FROM steps WHERE chat=?1")?;
            let mut rows = st.query([])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                let kept = Kept { id: r.get(0)?, title: r.get(1)?, updated: r.get::<_, i64>(2)?.max(0) as u64, kept_from_models: r.get(3)? };
                if for_models && kept.kept_from_models {
                    continue;
                }
                if !words.is_empty() {
                    let mut all = kept.title.to_lowercase();
                    let mut steps = said.query([kept.id])?;
                    while let Some(s) = steps.next()? {
                        all.push('\n');
                        all.push_str(&s.get_ref(0)?.as_str().unwrap_or("").to_lowercase());
                    }
                    if !words.iter().all(|w| all.contains(w.as_str())) {
                        continue;
                    }
                }
                out.push(kept);
                if out.len() >= limit {
                    break;
                }
            }
            Ok(out)
        })
        .unwrap_or_default()
    }

    /// A conversation kept, to show it again or go on with it.
    pub fn load(&self, id: i64) -> Option<Conversation> {
        self.with("load", |c| {
            let mut conv = Conversation::default();
            let head: String = c.query_row("SELECT title, head FROM chats WHERE id=?1", [id], |r| {
                conv.title = r.get(0)?;
                r.get(1)
            })?;
            head_into(&mut conv, &head);
            let mut st = c.prepare_cached("SELECT body FROM steps WHERE chat=?1 ORDER BY n")?;
            let mut rows = st.query([id])?;
            while let Some(r) = rows.next()? {
                let body = r.get_ref(0)?.as_str().unwrap_or("");
                if let Some(e) = serde_json::from_str::<Value>(body).ok().and_then(|v| crate::chat::entry_from_json(&v)) {
                    conv.entries.push(e);
                }
            }
            Ok(conv)
        })
    }

    /// What was said in a conversation, as plain lines (for a search's answer).
    pub fn said_in(&self, id: i64, limit_chars: usize) -> String {
        let Some(conv) = self.load(id) else { return String::new() };
        let mut out = String::new();
        for e in &conv.entries {
            let (who, text) = match e {
                Entry::User { text, .. } => ("Person", text),
                Entry::Assistant { text, .. } if !text.trim().is_empty() => ("Assistant", text),
                _ => continue,
            };
            out.push_str(&format!("{who}: {}\n", text.trim()));
            if out.chars().count() > limit_chars {
                out = out.chars().take(limit_chars).collect::<String>() + "…";
                break;
            }
        }
        out
    }

    pub fn delete(&self, id: i64) {
        self.with("delete", |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM steps WHERE chat=?1", [id])?;
            tx.execute("DELETE FROM chats WHERE id=?1", [id])?;
            tx.commit()
        });
    }

    /// Remove the conversations begun or gone on with since `since`
    /// (0: all of them) — with the history of the same period.
    pub fn clear_since(&self, since: u64) {
        self.with("clear", |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM steps WHERE chat IN (SELECT id FROM chats WHERE updated >= ?1)", [secs(since)])?;
            tx.execute("DELETE FROM chats WHERE updated >= ?1", [secs(since)])?;
            tx.commit()
        });
    }
}

/// Clear the conversations of a period (`Chats::clear_since`) in those
/// already open — or, not opened yet, in the profile's file (`dir`), opened
/// for it. Run it on a thread: opening the file waits for the disk.
pub fn clear_period(open: Option<Chats>, dir: Option<&Path>, since: u64) {
    if let Some(chats) = open.or_else(|| dir.and_then(Chats::open)) {
        chats.clear_since(since);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{Call, ToolResult};

    fn conv(title: &str, question: &str, answer: &str) -> Conversation {
        Conversation {
            title: title.into(),
            system: "sys".into(),
            tools: vec![Tool { name: "list_tabs".into(), description: "d".into(), schema: json!({"type": "object"}), acts: false }],
            labels: HashMap::from([(0, "Résumer la page".to_string())]),
            entries: vec![
                Entry::User { text: question.into(), sources: Vec::new(), files: Vec::new() },
                Entry::Assistant {
                    text: answer.into(),
                    calls: vec![Call { id: "1".into(), name: "list_tabs".into(), input: json!({}) }],
                    blocks: Some(json!([{"type": "text", "text": answer}])),
                    stopped: false,
                },
                Entry::Results(vec![ToolResult { id: "1".into(), name: "list_tabs".into(), text: "secret de la page".into(), error: false }]),
            ],
            kept_from_models: false,
        }
    }

    #[test]
    fn a_conversation_kept_comes_back_as_it_was() {
        let chats = Chats::in_memory().unwrap();
        let first = conv("Sanaga", "Quelle longueur ?", "918 km.");
        let id = chats.save(None, &first, 100).unwrap();
        assert_eq!(chats.load(id).as_ref(), Some(&first));
        // gone on with: the same conversation, replaced
        let mut more = first.clone();
        more.entries.push(Entry::User { text: "Et le débit ?".into(), sources: Vec::new(), files: Vec::new() });
        assert_eq!(chats.save(Some(id), &more, 200), Some(id));
        assert_eq!(chats.load(id).unwrap().entries.len(), 4);
        assert_eq!(chats.list("", 10), vec![Kept { id, title: "Sanaga".into(), updated: 200, kept_from_models: false }]);
        // a number that is not kept (deleted meanwhile): kept as a new one
        chats.delete(id);
        assert!(chats.load(id).is_none() && chats.list("", 10).is_empty());
        let again = chats.save(Some(id), &more, 300).unwrap();
        assert_eq!(chats.list("", 10).len(), 1);
        assert_eq!(chats.load(again).unwrap().entries.len(), 4);
    }

    #[test]
    fn conversations_are_searched_by_what_was_said() {
        let chats = Chats::in_memory().unwrap();
        let a = chats.save(None, &conv("Fleuves", "Quelle est la longueur de la Sanaga ?", "Environ 918 km."), 100).unwrap();
        let b = chats.save(None, &conv("Villes", "Où est Douala ?", "Sur le Wouri."), 200).unwrap();
        assert_eq!(chats.list("", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![b, a], "the latest first");
        assert_eq!(chats.list("sanaga longueur", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![a]);
        assert_eq!(chats.list("WOURI", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![b]);
        assert_eq!(chats.list("villes", 10).len(), 1, "titles are searched too");
        // what tools returned (pages, the history…) is not what was said
        assert!(chats.list("secret", 10).is_empty());
        assert_eq!(chats.list("", 1).len(), 1);
        assert_eq!(chats.said_in(a, 1000), "Person: Quelle est la longueur de la Sanaga ?\nAssistant: Environ 918 km.\n");
        assert!(chats.said_in(a, 10).ends_with('…'));
    }

    #[test]
    fn a_conversation_kept_from_models_is_never_given_to_one() {
        let chats = Chats::in_memory().unwrap();
        let open = chats.save(None, &conv("Banque", "q", "le solde du compte"), 100).unwrap();
        let mut blocked = conv("Banque", "Résumer", "le solde du compte");
        blocked.kept_from_models = true;
        let kept = chats.save(None, &blocked, 200).unwrap();
        // the person sees both, marked
        let all = chats.list("solde", 10);
        assert_eq!(all.iter().map(|k| (k.id, k.kept_from_models)).collect::<Vec<_>>(), vec![(kept, true), (open, false)]);
        // a model's search never finds the one kept from models
        assert_eq!(chats.list_for_models("solde", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![open]);
        assert_eq!(chats.list_for_models("", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![open]);
        // the mark stays with it when it is opened again, and when it is saved again
        assert!(chats.load(kept).unwrap().kept_from_models);
        assert!(!chats.load(open).unwrap().kept_from_models);
        chats.save(Some(kept), &chats.load(kept).unwrap(), 300);
        assert!(chats.list_for_models("", 10).iter().all(|k| k.id != kept));
    }

    #[test]
    fn clearing_the_history_reaches_conversations_not_opened_yet() {
        let dir = std::env::temp_dir().join(format!("sanaga-chats-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Chats::open(&dir).unwrap().save(None, &conv("Gardée", "q", "r"), 5_000).unwrap();
        // the window has not opened them: the file is opened for the clearing
        clear_period(None, Some(&dir), 0);
        assert!(Chats::open(&dir).unwrap().list("", 10).is_empty());
        // already open: those are used
        let open = Chats::open(&dir).unwrap();
        open.save(None, &conv("Encore", "q", "r"), 5_000).unwrap();
        clear_period(Some(open), None, 0);
        assert!(Chats::open(&dir).unwrap().list("", 10).is_empty());
        // no profile folder (a guest window): nothing to do
        clear_period(None, None, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_a_period_and_the_limit_of_kept_conversations() {
        let chats = Chats::in_memory().unwrap();
        let old = chats.save(None, &conv("Ancienne", "q", "r"), 100).unwrap();
        let new = chats.save(None, &conv("Récente", "q", "r"), 5_000).unwrap();
        chats.clear_since(1_000);
        assert_eq!(chats.list("", 10).iter().map(|k| k.id).collect::<Vec<_>>(), vec![old]);
        assert!(chats.load(new).is_none());
        chats.clear_since(0);
        assert!(chats.list("", 10).is_empty());
        // only the latest are kept
        for n in 0..(KEPT as u64 + 5) {
            chats.save(None, &conv(&format!("c{n}"), "q", "r"), 10 + n);
        }
        let all = chats.list("", 1000);
        assert_eq!(all.len(), KEPT);
        assert_eq!(all.last().unwrap().title, "c5");
    }

    #[test]
    fn the_file_is_private_and_opens_again() {
        let dir = crate::store::PrivateDir::new("sanaga-chats-test").unwrap();
        let id = {
            let chats = Chats::open(dir.path()).unwrap();
            chats.save(None, &conv("Gardée", "q", "r"), 100).unwrap()
        };
        let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(dir.path().join(DB_FILE)).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(Chats::open(dir.path()).unwrap().load(id).unwrap().title, "Gardée");
        // a file that is not a database is left alone: nothing is kept this time
        let other = crate::store::PrivateDir::new("sanaga-chats-test").unwrap();
        std::fs::write(other.path().join(DB_FILE), b"not a database, but somebody's file").unwrap();
        assert!(Chats::open(other.path()).is_none());
        assert_eq!(std::fs::read(other.path().join(DB_FILE)).unwrap(), b"not a database, but somebody's file");
    }
}

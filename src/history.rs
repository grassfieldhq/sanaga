//! The browsing history: every page visited and when, in a small database
//! (SQLite, `history.db` in the profile's folder).
//!
//! It used to be a text file read whole into each window's memory, cut at
//! 5,000 visits (an import could push the person's own visits out), with
//! nothing counted — so nothing could be ranked by how often a page is
//! visited — and walked from end to end at every frame by the address bar.
//! Now nothing holds the history in memory: each part of the window asks
//! for the few lines it shows (`suggest`, `list`, `recent_pages`…), and
//! there is no limit to the number of visits kept.
//!
//! Never make the window wait: what is written (`add_visit`, `clear_since`…)
//! is handed to one thread, in order, and never waited for; what is read
//! waits for the disk, so it is called from a thread of its own (in the
//! window: `Sanaga::history_ask`), never from the window's.
//!
//! Several windows share one opening of a profile's history, and a second
//! Sanaga on the same profile (a web app's window) opens the same file:
//! SQLite's write-ahead log lets them read and write side by side.
//!
//! What is kept:
//! - `pages`: one line per address — its title, the name of its site
//!   (`host`, without "www.", with its port when it has one), where the site
//!   really is (`origin`: "http://nas.lan:8080"), how many times it was
//!   visited and when last;
//! - `visits`: one line per visit — the page and the time.

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};

const DB_FILE: &str = "history.db";
/// The history file of earlier versions (one visit per line), taken in
/// once and then kept under `OLD_KEPT`.
const OLD_FILE: &str = "history.tsv";
const OLD_KEPT: &str = "history.tsv.imported";
/// A database that could not be read as one is kept under this name.
const DAMAGED: &str = "history.db.damaged";
/// The layout below. A database written by a later Sanaga (a higher
/// number) is not written by this one.
const LAYOUT: i64 = 1;

const TABLES: &str = "
CREATE TABLE IF NOT EXISTS pages (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    host TEXT NOT NULL DEFAULT '',
    origin TEXT NOT NULL DEFAULT '',
    visits INTEGER NOT NULL DEFAULT 0,
    last INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS pages_host ON pages(host, visits, last);
CREATE INDEX IF NOT EXISTS pages_last ON pages(last);
CREATE TABLE IF NOT EXISTS visits (
    id INTEGER PRIMARY KEY,
    page INTEGER NOT NULL REFERENCES pages(id),
    time INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS visits_page ON visits(page, time);
CREATE INDEX IF NOT EXISTS visits_time ON visits(time);
";

/// The two ways into a database: one to write (the history thread's), one
/// to read (the threads that answer the window). With a file they are two
/// openings, so a long search never holds a visit back; in memory there is
/// only one.
struct Conns {
    write: Arc<Mutex<Connection>>,
    read: Arc<Mutex<Connection>>,
    /// False when nothing is kept (a guest window, a file that cannot be opened).
    on_disk: bool,
}

struct Db {
    /// The profile's folder (None: a history that lives and dies with its window).
    dir: Option<PathBuf>,
    /// Opened by the history thread at its first request — never by the
    /// window's thread, which would wait for the disk. None inside: no
    /// database could be made at all (nothing is recorded).
    conns: OnceLock<Option<Conns>>,
    /// Goes up at every change: what the windows compare to know that what
    /// they show is out of date (one number read per frame, no lock).
    version: AtomicU64,
    /// SQLite's own count of the changes made by other programs (a second
    /// Sanaga on this profile), as last seen.
    others: AtomicU64,
    /// How many requests of this history are still to be done by the
    /// history thread, and the word given when none is left: what is read
    /// waits for it, so that a page just visited is in what is read.
    waiting: Mutex<u64>,
    settled: Condvar,
}

/// A profile's history. Cheap to copy: every copy is the same history.
#[derive(Clone)]
pub struct History(Arc<Db>);

/// A page offered by the address bar or the command palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub url: String,
    pub title: String,
    pub visits: u64,
    pub last: u64,
    /// 0: the site's name starts with what was typed; 1: a part of its
    /// name does ("google" in "docs.google.com"); 2: found elsewhere.
    pub rank: u8,
    /// How much the page counts (see `frecency`).
    pub score: u64,
}

impl Hit {
    fn order(&self) -> (u8, std::cmp::Reverse<u64>, std::cmp::Reverse<u64>) {
        (self.rank, std::cmp::Reverse(self.score), std::cmp::Reverse(self.last))
    }
}

/// The site a typed beginning is completed with.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    /// What the field shows: "wikipedia.org", "nas.lan:8080".
    pub text: String,
    /// Where it really is, as it was visited: "http://nas.lan:8080/".
    pub url: String,
    pub score: u64,
}

/// A site of the new tab page's "frequently visited".
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    /// Its most visited page, and that page's title.
    pub url: String,
    pub title: String,
    /// Visits of the whole site.
    pub visits: u64,
}

/// One line of the history page.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: i64,
    pub time: u64,
    pub url: String,
    pub title: String,
}

/// What the history page lists: how many lines, how they fall into days —
/// not the lines themselves, which are read as they come on screen (`rows`).
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub total: usize,
    /// Lines in each group of days, newest first (see `store::DAY_GROUPS`).
    pub groups: [usize; 5],
    /// For a search: the visits found, newest first (their numbers only).
    found: Option<Arc<Vec<i64>>>,
}

/// How much a page counts when pages are ranked: its number of visits,
/// weighted by how recent the last one is — a page seen ten times this
/// week comes before one seen thirty times last year.
pub fn frecency(visits: u64, last: u64, now: u64) -> u64 {
    let weight = match now.saturating_sub(last) / 86_400 {
        0..=3 => 100,
        4..=13 => 70,
        14..=30 => 50,
        31..=89 => 30,
        _ => 10,
    };
    visits.saturating_mul(weight)
}

/// The words typed, in lower case.
pub fn words_of(typed: &str) -> Vec<String> {
    typed.to_lowercase().split_whitespace().map(str::to_string).collect()
}

/// Is `needle` (in lower case) in `text`, whatever the case of `text`?
/// Without copying `text` when it is plain ASCII, as nearly all addresses are.
fn has(text: &str, needle: &str) -> bool {
    if text.is_ascii() {
        let (t, n) = (text.as_bytes(), needle.as_bytes());
        n.is_empty() || (n.len() <= t.len() && t.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n)))
    } else {
        text.to_lowercase().contains(needle)
    }
}

/// Does a page answer the words typed (each of them in its address or its title)?
pub fn matches(words: &[String], url: &str, title: &str) -> bool {
    words.iter().all(|w| has(url, w) || has(title, w))
}

/// Where a page that answers the words typed stands (see `Hit::rank`);
/// None when it does not answer them. `host`: the site's name as kept in
/// the database (lower case, no "www.").
pub fn rank(words: &[String], host: &str, url: &str, title: &str) -> Option<u8> {
    if !matches(words, url, title) {
        return None;
    }
    let first = words.first()?.as_str();
    Some(if host.starts_with(first) {
        0
    } else if host.split('.').any(|part| part.starts_with(first)) {
        1
    } else {
        2
    })
}

/// The name of an address's site and where the site is: ("nas.lan:8080",
/// "http://nas.lan:8080"). Empty for what is not a web site (a file).
pub fn site_of(url: &str) -> (String, String) {
    let Ok(u) = url::Url::parse(url) else { return Default::default() };
    let (Some(host), "http" | "https") = (u.host_str(), u.scheme()) else { return Default::default() };
    let host = host.to_lowercase();
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    (format!("{}{port}", host.strip_prefix("www.").unwrap_or(&host)), format!("{}://{host}{port}", u.scheme()))
}

fn secs(t: u64) -> i64 {
    t.min(i64::MAX as u64) as i64
}

fn text<'a>(row: &'a rusqlite::Row, col: usize) -> rusqlite::Result<&'a str> {
    Ok(row.get_ref(col)?.as_str().unwrap_or(""))
}

fn number(row: &rusqlite::Row, col: usize) -> rusqlite::Result<u64> {
    Ok(row.get::<_, i64>(col)?.max(0) as u64)
}

fn lock(conn: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
    conn.lock().unwrap_or_else(|e| e.into_inner())
}

/// Said on the terminal, a few times at most (a disk that fails would say
/// it at every page).
fn report(what: &str, e: &dyn std::fmt::Display) {
    static SAID: AtomicU64 = AtomicU64::new(0);
    if SAID.fetch_add(1, Ordering::Relaxed) < 12 {
        eprintln!("Sanaga: history: {what}: {e}");
    }
}

// ------------------------------------------------------------- opening

/// The first name of the form `base`, `base2`, `base3`… that is free in `dir`.
fn free_name(dir: &Path, base: &str) -> PathBuf {
    (1..).map(|n| if n == 1 { dir.join(base) } else { dir.join(format!("{base}{n}")) }).find(|p| std::fs::symlink_metadata(p).is_err()).unwrap_or_else(|| dir.join(base))
}

fn memory() -> Option<Conns> {
    let conn = Connection::open_in_memory().and_then(|c| c.execute_batch(TABLES).map(|_| c));
    match conn {
        Ok(c) => {
            let c = Arc::new(Mutex::new(c));
            Some(Conns { write: c.clone(), read: c, on_disk: false })
        }
        Err(e) => {
            report("no history can be kept, not even in memory", &e);
            None
        }
    }
}

/// Why a database file could not be opened.
enum Refused {
    /// It is not a database, or a damaged one.
    Damaged(String),
    /// Anything else (the disk, a refusal, a later Sanaga's layout): the
    /// file may well be fine.
    Other(String),
}

fn open_file(dir: &Path) -> Result<Conns, Refused> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = dir.join(DB_FILE);
    // (made here, closed to other users; SQLite gives the files it makes
    // next to it — its log — the same rights)
    let _ = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(&path);
    let refused = |e: rusqlite::Error| match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt) => Refused::Damaged(e.to_string()),
        _ => Refused::Other(e.to_string()),
    };
    let ready = |conn: &Connection| -> rusqlite::Result<()> {
        // (another Sanaga may be writing: wait for it rather than fail)
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // The write-ahead log: readers and the writer do not wait for one
        // another, in this Sanaga and between two of them. What is removed
        // is written over, not only marked free: the history a person
        // clears must not stay readable in the file.
        // (And the log gives its room back once it is past 4 MB: after
        // the history file of before was taken in, it would stay as large
        // as the history.)
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA secure_delete=ON; PRAGMA journal_size_limit=4194304;")
    };
    let write = Connection::open(&path).map_err(refused)?;
    // (looked at before anything is changed in the file)
    let layout: i64 = write.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(refused)?;
    if layout > LAYOUT {
        return Err(Refused::Other(format!("it was written by a later Sanaga (layout {layout})")));
    }
    ready(&write).map_err(refused)?;
    if layout < LAYOUT {
        write.execute_batch(&format!("BEGIN IMMEDIATE; {TABLES} PRAGMA user_version={LAYOUT}; COMMIT;")).map_err(refused)?;
    }
    // (a first look at the tables: a damaged file often opens, and fails here)
    write.query_row("SELECT count(*) FROM pages WHERE id < 0", [], |r| r.get::<_, i64>(0)).map_err(refused)?;
    let read = Connection::open(&path).map_err(refused)?;
    ready(&read).and_then(|_| read.execute_batch("PRAGMA query_only=ON;")).map_err(refused)?;
    Ok(Conns { write: Arc::new(Mutex::new(write)), read: Arc::new(Mutex::new(read)), on_disk: true })
}

/// Open a profile's history. A file that is not a database (damaged, or
/// something else under that name) is never written over nor removed: it
/// is kept under another name, and a new history starts. A file that
/// cannot be opened for another reason is left as it is: this run's
/// visits are then kept in memory only, and the file is tried again at
/// the next start.
fn open(dir: Option<&Path>) -> Option<Conns> {
    let Some(dir) = dir else { return memory() };
    let path = dir.join(DB_FILE);
    match open_file(dir) {
        Ok(c) => return Some(c),
        Err(Refused::Damaged(e)) => {
            let aside = free_name(dir, DAMAGED);
            eprintln!("Sanaga: {} cannot be read as a history ({e}): it is kept as {}, and a new history starts", path.display(), aside.display());
            let moved = std::fs::rename(&path, &aside);
            // (its log goes with it: it belongs to that file, not to the new one)
            for end in ["-wal", "-shm"] {
                let mut from = path.clone().into_os_string();
                from.push(end);
                let mut to = aside.clone().into_os_string();
                to.push(end);
                let _ = std::fs::rename(from, to);
            }
            let again = moved.map_err(|e| e.to_string()).and_then(|_| open_file(dir).map_err(|(Refused::Damaged(e) | Refused::Other(e))| e));
            match again {
                Ok(c) => return Some(c),
                Err(e) => eprintln!("Sanaga: no history file can be made in {} ({e}): this run's visits are not kept", dir.display()),
            }
        }
        Err(Refused::Other(e)) => eprintln!("Sanaga: {} cannot be opened ({e}): it is left as it is, and this run's visits are not kept", path.display()),
    }
    memory()
}

// ------------------------------------------------------------- writing

enum Op {
    /// Open the database and take in the history file of earlier versions.
    Open,
    Visit(String, String, u64),
    Title(String, String),
    Past(Vec<(u64, String, String)>),
    Remove(i64),
    ClearSince(u64),
}

enum Job {
    Do(Arc<Db>, Op),
    /// Answer once everything asked before is done.
    Flush(mpsc::Sender<()>),
}

const THREAD: &str = "sanaga-history";

/// Hand a request to the history thread (asleep until something is asked;
/// started at the first request).
fn hand(job: Job) {
    static WRITER: OnceLock<Option<Mutex<mpsc::Sender<Job>>>> = OnceLock::new();
    let writer = WRITER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(THREAD.into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    do_job(job);
                }
            })
            .ok()
            .map(|_| Mutex::new(tx))
    });
    let sent = match writer.as_ref().and_then(|w| w.lock().ok()) {
        Some(tx) => tx.send(job).map_err(|e| e.0),
        None => Err(job),
    };
    // (no thread to hand it to: done here)
    if let Err(job) = sent {
        do_job(job);
    }
}

fn do_job(job: Job) {
    match job {
        Job::Do(db, op) => {
            // (counted as done whatever happens to it: nothing that reads
            // may wait for ever)
            struct Done<'a>(&'a Db);
            impl Drop for Done<'_> {
                fn drop(&mut self) {
                    let mut waiting = self.0.waiting.lock().unwrap_or_else(|e| e.into_inner());
                    *waiting = waiting.saturating_sub(1);
                    if *waiting == 0 {
                        self.0.settled.notify_all();
                    }
                }
            }
            let _done = Done(&db);
            db.apply(op);
        }
        Job::Flush(done) => {
            let _ = done.send(());
        }
    }
}

/// Wait until everything asked of every history so far is in it: before
/// Sanaga ends.
pub fn flush() {
    // (the history thread itself has nothing to wait for)
    if std::thread::current().name() == Some(THREAD) {
        return;
    }
    let (tx, rx) = mpsc::channel();
    hand(Job::Flush(tx));
    let _ = rx.recv();
}

/// A page and one visit of it. True when the visit is a new one (the same
/// page at the same second is one visit, however many times it is told:
/// reading the same file twice adds nothing).
/// `keep_title`: a title the page already has stays (for what is brought
/// from elsewhere, which may be older than what Sanaga knows).
fn record(c: &Connection, url: &str, title: &str, time: u64, keep_title: bool) -> rusqlite::Result<bool> {
    let (host, origin) = site_of(url);
    let sql = if keep_title {
        "INSERT INTO pages(url, title, host, origin) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(url) DO UPDATE SET title = CASE WHEN pages.title = '' THEN excluded.title ELSE pages.title END RETURNING id"
    } else {
        "INSERT INTO pages(url, title, host, origin) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(url) DO UPDATE SET title = CASE WHEN excluded.title <> '' THEN excluded.title ELSE pages.title END RETURNING id"
    };
    let page: i64 = c.prepare_cached(sql)?.query_row(params![url, title, host, origin], |r| r.get(0))?;
    let new = c.prepare_cached("INSERT OR IGNORE INTO visits(page, time) VALUES (?1, ?2)")?.execute(params![page, secs(time)])? == 1;
    if new {
        c.prepare_cached("UPDATE pages SET visits = visits + 1, last = max(last, ?2) WHERE id = ?1")?.execute(params![page, secs(time)])?;
    }
    Ok(new)
}

/// A page visited now. Told twice in a row (a reload, a page that moves
/// inside itself and comes back), the same page is one visit, at the
/// later time.
fn visit(c: &mut Connection, url: &str, title: &str, time: u64) -> rusqlite::Result<bool> {
    let tx = c.transaction()?;
    let latest: Option<(i64, i64, String)> = tx
        .prepare_cached("SELECT v.id, v.page, p.url FROM visits v JOIN pages p ON p.id = v.page ORDER BY v.time DESC, v.id DESC LIMIT 1")?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    match latest {
        Some((id, page, ref latest_url)) if latest_url == url => {
            tx.execute("UPDATE OR IGNORE visits SET time = ?2 WHERE id = ?1", params![id, secs(time)])?;
            tx.execute("UPDATE pages SET last = max(last, ?2), title = CASE WHEN ?3 <> '' THEN ?3 ELSE title END WHERE id = ?1", params![page, secs(time), title])?;
        }
        _ => {
            record(&tx, url, title, time, false)?;
        }
    }
    tx.commit()?;
    Ok(true)
}

fn rename(c: &Connection, url: &str, title: &str) -> rusqlite::Result<bool> {
    Ok(c.prepare_cached("UPDATE pages SET title = ?2 WHERE url = ?1 AND title <> ?2")?.execute(params![url, title])? > 0)
}

fn past(c: &mut Connection, visits: &[(u64, String, String)], keep_title: bool) -> rusqlite::Result<bool> {
    let tx = c.transaction()?;
    let mut added = false;
    for (time, url, title) in visits {
        if !url.trim().is_empty() && !url.starts_with("sanaga://") {
            added |= record(&tx, url, title, *time, keep_title)?;
        }
    }
    tx.commit()?;
    Ok(added)
}

/// The visit, and its page when it was the page's only visit, are removed.
/// Gives the visit's time and address (None: there was no such visit).
fn remove(c: &mut Connection, id: i64) -> rusqlite::Result<Option<(u64, String)>> {
    let tx = c.transaction()?;
    let found: Option<(i64, u64, String)> =
        tx.query_row("SELECT v.page, v.time, p.url FROM visits v JOIN pages p ON p.id = v.page WHERE v.id = ?1", [id], |r| Ok((r.get(0)?, number(r, 1)?, r.get(2)?))).optional()?;
    let Some((page, time, url)) = found else { return Ok(None) };
    tx.execute("DELETE FROM visits WHERE id = ?1", [id])?;
    tx.execute("UPDATE pages SET visits = (SELECT count(*) FROM visits WHERE page = ?1), last = (SELECT coalesce(max(time), 0) FROM visits WHERE page = ?1) WHERE id = ?1", [page])?;
    tx.execute("DELETE FROM pages WHERE id = ?1 AND visits = 0", [page])?;
    tx.commit()?;
    Ok(Some((time, url)))
}

/// Forget the visits made since `since` (0: all of them), and the pages
/// that have no visit left.
fn clear_since(c: &mut Connection, since: u64) -> rusqlite::Result<bool> {
    let tx = c.transaction()?;
    if since == 0 {
        tx.execute("DELETE FROM visits", [])?;
        tx.execute("DELETE FROM pages", [])?;
    } else {
        tx.execute("DELETE FROM visits WHERE time >= ?1", [secs(since)])?;
        tx.execute(
            "UPDATE pages SET visits = (SELECT count(*) FROM visits WHERE page = pages.id), last = (SELECT coalesce(max(time), 0) FROM visits WHERE page = pages.id) WHERE last >= ?1",
            [secs(since)],
        )?;
        tx.execute("DELETE FROM pages WHERE visits = 0", [])?;
    }
    tx.commit()?;
    Ok(true)
}

/// After something was removed: nothing of it stays in the log either.
/// (When another program is reading, the log is emptied a little later,
/// by whichever writes next.) Also after a large addition, for the room
/// the log takes.
fn empty_the_log(c: &Connection, all: bool) {
    if all {
        // (the file gives its room back)
        let _ = c.execute_batch("VACUUM");
    }
    let _ = c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
}

// ----------------------------------------- the history file of before

/// A line of the history file of earlier versions: a visit
/// (`time⇥address⇥title`), or a title learnt after its visit was written
/// (`=address⇥title`). None for anything else: a damaged line is skipped,
/// the others are still read.
#[derive(Debug, PartialEq)]
enum Old<'a> {
    Visit(u64, &'a str, &'a str),
    Title(&'a str, &'a str),
}

fn old_line(line: &str) -> Option<Old<'_>> {
    if let Some(rest) = line.strip_prefix('=') {
        return rest.split_once('\t').map(|(url, title)| Old::Title(url, title));
    }
    let mut p = line.splitn(3, '\t');
    let (time, url, title) = (p.next()?, p.next()?, p.next()?);
    if url.trim().is_empty() || url.starts_with("sanaga://") {
        return None;
    }
    Some(Old::Visit(time.parse().unwrap_or(0), url, title))
}

fn import_text(c: &mut Connection, text: &str) -> rusqlite::Result<usize> {
    let tx = c.transaction()?;
    let mut added = 0;
    for line in text.lines() {
        match old_line(line) {
            Some(Old::Visit(time, url, title)) => added += usize::from(record(&tx, url, title, time, false)?),
            Some(Old::Title(url, title)) => {
                rename(&tx, url, title)?;
            }
            None => {}
        }
    }
    tx.commit()?;
    Ok(added)
}

/// Take in the history file of earlier versions, all of it (it was cut at
/// 5,000 visits; nothing is cut any more), then keep it under another name
/// — never removed here: it is the person's history as it was.
///
/// Nothing is lost on the way: a file that cannot be read is left as it is
/// and tried again at the next start; damaged lines are skipped and the
/// rest is read; the visits enter the database all together or not at all;
/// and reading the same file a second time (it could not be renamed, an
/// earlier Sanaga wrote a new one) adds only what is not there yet.
fn import_old(c: &mut Connection, dir: &Path) -> bool {
    let old = dir.join(OLD_FILE);
    let bytes = match std::fs::read(&old) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        Err(e) => {
            eprintln!("Sanaga: {} could not be read ({e}): it is left as it is, and read again at the next start", old.display());
            return false;
        }
    };
    let started = std::time::Instant::now();
    match import_text(c, &String::from_utf8_lossy(&bytes)) {
        Ok(added) => {
            let kept = free_name(dir, OLD_KEPT);
            match std::fs::rename(&old, &kept) {
                Ok(()) => eprintln!("Sanaga: history: {added} visits taken from {} in {} ms; the file is kept as {}", old.display(), started.elapsed().as_millis(), kept.display()),
                Err(e) => report("the earlier history file was read but could not be renamed", &e),
            }
            added > 0
        }
        Err(e) => {
            report("the earlier history file could not be taken in (it is left as it is)", &e);
            false
        }
    }
}

/// What the person removes from the history must be gone from the files
/// kept beside it too (the earlier history file, a damaged database).
/// `keep`: is this visit (time, address) still in the history? None: all
/// of it was cleared.
fn forget_in_kept_files(dir: &Path, keep: Option<&dyn Fn(u64, &str) -> bool>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // (what a damaged database holds cannot be told apart: it goes whole)
        if name.starts_with(DAMAGED) || (keep.is_none() && (name == OLD_FILE || name.starts_with(OLD_KEPT))) {
            let _ = std::fs::remove_file(e.path());
            continue;
        }
        let (Some(keep), true) = (keep, name.starts_with(OLD_KEPT)) else { continue };
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let stays = |line: &str| !matches!(old_line(line), Some(Old::Visit(time, url, _)) if !keep(time, url));
        let visited: HashSet<&str> = text.lines().filter(|l| stays(l)).filter_map(|l| if let Some(Old::Visit(_, url, _)) = old_line(l) { Some(url) } else { None }).collect();
        // (a title of a page that has no visit left goes with it)
        let kept: String = text.lines().filter(|l| stays(l) && !matches!(old_line(l), Some(Old::Title(url, _)) if !visited.contains(url))).flat_map(|l| [l, "\n"]).collect();
        if kept.len() != text.len() {
            let _ = crate::store::write_quietly(&e.path(), kept);
        }
    }
}

impl Db {
    fn new(dir: Option<PathBuf>) -> Arc<Db> {
        Arc::new(Db { dir, conns: OnceLock::new(), version: AtomicU64::new(1), others: AtomicU64::new(0), waiting: Mutex::new(0), settled: Condvar::new() })
    }

    /// Hand a request of this history to the history thread.
    fn ask(self: &Arc<Self>, op: Op) {
        *self.waiting.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        hand(Job::Do(self.clone(), op));
    }

    /// Wait until every request of this history is done (of this one
    /// only: what another profile's history, or a guest window's, is
    /// writing is not waited for).
    fn settle(&self) {
        // (the history thread itself has nothing to wait for)
        if std::thread::current().name() == Some(THREAD) {
            return;
        }
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        while *waiting > 0 {
            waiting = self.settled.wait(waiting).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn conns(&self) -> Option<&Conns> {
        self.conns
            .get_or_init(|| {
                let conns = open(self.dir.as_deref());
                if let Some(c) = conns.as_ref().filter(|c| c.on_disk) {
                    self.others.store(data_version(&lock(&c.write)), Ordering::Relaxed);
                }
                conns
            })
            .as_ref()
    }

    fn changed(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// One request, on the history thread.
    fn apply(&self, op: Op) {
        let Some(conns) = self.conns() else { return };
        let mut c = lock(&conns.write);
        let dir = self.dir.as_deref().filter(|_| conns.on_disk);
        let done = match op {
            Op::Open => Ok(dir.is_some_and(|dir| import_old(&mut c, dir))).inspect(|taken| {
                if *taken {
                    empty_the_log(&c, false);
                }
            }),
            Op::Visit(url, title, time) => visit(&mut c, &url, &title, time),
            Op::Title(url, title) => rename(&c, &url, &title),
            Op::Past(visits) => past(&mut c, &visits, true),
            Op::Remove(id) => remove(&mut c, id).map(|gone| {
                if let (Some((time, url)), Some(dir)) = (&gone, dir) {
                    forget_in_kept_files(dir, Some(&|t, u| !(t == *time && u == url.as_str())));
                }
                empty_the_log(&c, false);
                gone.is_some()
            }),
            Op::ClearSince(since) => clear_since(&mut c, since).inspect(|_| {
                if let Some(dir) = dir {
                    if since == 0 {
                        forget_in_kept_files(dir, None);
                    } else {
                        forget_in_kept_files(dir, Some(&|t, _| t < since));
                    }
                }
                empty_the_log(&c, since == 0);
            }),
        };
        match done {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(e) => report("a change could not be written", &e),
        }
    }

    /// Did another program change the file since the last look? Then what
    /// the windows show is out of date too. (Looked at whenever something
    /// is read: no timer runs for it.)
    fn look_at_others(&self, conns: &Conns) {
        if !conns.on_disk {
            return;
        }
        let now = data_version(&lock(&conns.write));
        if self.others.swap(now, Ordering::Relaxed) != now {
            self.changed();
        }
    }
}

fn data_version(c: &Connection) -> u64 {
    c.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64
}

/// The histories open in this Sanaga, by folder: every window of a profile
/// uses the same one.
static OPEN: Mutex<Vec<(PathBuf, Arc<Db>)>> = Mutex::new(Vec::new());

impl History {
    /// The history kept in the profile folder `dir`: opened once per run
    /// and folder, shared by all its windows. Nothing is read here: the
    /// file is opened, and the history file of earlier versions taken in,
    /// by the history thread.
    pub fn open(dir: &Path) -> History {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, db)) = open.iter().find(|(d, _)| d == dir) {
            return History(db.clone());
        }
        let db = Db::new(Some(dir.to_path_buf()));
        open.push((dir.to_path_buf(), db.clone()));
        db.ask(Op::Open);
        History(db)
    }

    /// A history that leaves no trace (a guest window, a screenshot): in
    /// memory, its own, gone with it.
    pub fn in_memory() -> History {
        let db = Db::new(None);
        let _ = db.conns.set(memory());
        History(db)
    }

    /// A second opening of the same folder, as another Sanaga would make.
    #[cfg(test)]
    fn open_apart(dir: &Path) -> History {
        let db = Db::new(Some(dir.to_path_buf()));
        db.ask(Op::Open);
        History(db)
    }

    /// Is this profile's history not kept this time — its file could not be
    /// opened (the disk, a refusal, a file written by a later Sanaga), so
    /// this run's visits live in memory only? `None` until the file has
    /// been tried (read at a frame: no lock, no disk).
    pub fn not_kept(&self) -> Option<bool> {
        self.0.dir.as_ref()?;
        self.0.conns.get().map(|c| c.as_ref().is_none_or(|c| !c.on_disk))
    }

    /// A number that changes whenever the history does (read at every
    /// frame: no lock, no disk).
    pub fn version(&self) -> u64 {
        self.0.version.load(Ordering::Relaxed)
    }

    fn write(&self, op: Op) {
        self.0.ask(op);
    }

    pub fn add_visit(&self, url: &str, title: &str, time: u64) {
        // (no address: a page that never loaded, a tab still empty — not a
        // visit; nor are Sanaga's own pages)
        if url.trim().is_empty() || url.starts_with("sanaga://") {
            return;
        }
        self.write(Op::Visit(url.to_string(), title.to_string(), time));
    }

    /// A page told its title after its visit was recorded.
    pub fn set_title(&self, url: &str, title: &str) {
        self.write(Op::Title(url.to_string(), title.to_string()));
    }

    /// Visits brought from another browser: added to what is there — the
    /// same visit twice counts once — and nothing is pushed out.
    pub fn add_past(&self, visits: Vec<(u64, String, String)>) {
        if !visits.is_empty() {
            self.write(Op::Past(visits));
        }
    }

    /// Remove one line of the history page.
    pub fn remove_visit(&self, id: i64) {
        self.write(Op::Remove(id));
    }

    /// Forget the visits made since `since` (seconds since 1970; 0: everything).
    pub fn clear_since(&self, since: u64) {
        self.write(Op::ClearSince(since));
    }

    /// Read something. It waits for the disk and for what is being
    /// written: never call it from the window's thread.
    fn read<T>(&self, what: &str, ask: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        self.0.settle();
        let conns = self.0.conns()?;
        self.0.look_at_others(conns);
        let c = lock(&conns.read);
        match ask(&c) {
            Ok(found) => Some(found),
            Err(e) => {
                report(what, &e);
                None
            }
        }
    }

    /// How many visits are kept.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.read("counting", |c| c.query_row("SELECT count(*) FROM visits", [], |r| number(r, 0))).unwrap_or(0) as usize
    }

    /// The pages that answer what is typed in the address bar, the best
    /// first: sites whose name starts with it, then sites with a part of
    /// their name starting with it, then pages that hold it in their
    /// address or title; among equals, the most visited lately (`frecency`).
    pub fn suggest(&self, typed: &str, limit: usize) -> Vec<Hit> {
        let words = words_of(typed);
        if words.is_empty() || limit == 0 {
            return Vec::new();
        }
        let now = crate::store::now();
        self.read("suggestions", |c| {
            let mut st = c.prepare_cached("SELECT url, title, host, visits, last FROM pages")?;
            let mut rows = st.query([])?;
            let mut best: Vec<Hit> = Vec::with_capacity(limit + 1);
            while let Some(r) = rows.next()? {
                let (url, title) = (text(r, 0)?, text(r, 1)?);
                let Some(rank) = rank(&words, text(r, 2)?, url, title) else { continue };
                let (visits, last) = (number(r, 3)?, number(r, 4)?);
                // (only the few best are copied out of the database)
                let mut hit = Hit { url: String::new(), title: String::new(), visits, last, rank, score: frecency(visits, last, now) };
                if best.len() == limit && best.last().is_some_and(|worst| worst.order() <= hit.order()) {
                    continue;
                }
                (hit.url, hit.title) = (url.to_string(), title.to_string());
                let at = best.partition_point(|h| h.order() <= hit.order());
                best.insert(at, hit);
                best.truncate(limit);
            }
            Ok(best)
        })
        .unwrap_or_default()
    }

    /// The site to complete what is being typed with ("wiki" →
    /// "wikipedia.org"): among the sites whose name starts that way, the
    /// one that counts most (`frecency`), and where it was really visited
    /// — "http://nas.lan:8080/", not "https://nas.lan".
    pub fn complete_site(&self, typed: &str) -> Option<Completion> {
        let t = typed.to_lowercase();
        if t.chars().count() < 2 || t.contains(|c: char| c.is_whitespace() || c == '/') {
            return None;
        }
        let now = crate::store::now();
        self.read("completing a site's name", |c| {
            // (the names that start with what was typed: one stretch of the index)
            let mut st = c.prepare_cached("SELECT host, origin, sum(visits), max(last) FROM pages WHERE host >= ?1 AND host < ?2 GROUP BY host, origin ORDER BY host")?;
            let mut rows = st.query(params![t, format!("{t}\u{10ffff}")])?;
            // per site: its visits, its last one, and its most visited origin
            let mut sites: Vec<(String, u64, u64, String, u64)> = Vec::new();
            while let Some(r) = rows.next()? {
                let (host, origin, visits, last) = (text(r, 0)?, text(r, 1)?, number(r, 2)?, number(r, 3)?);
                match sites.last_mut().filter(|s| s.0 == host) {
                    Some(site) => {
                        if visits > site.4 {
                            (site.3, site.4) = (origin.to_string(), visits);
                        }
                        (site.1, site.2) = (site.1 + visits, site.2.max(last));
                    }
                    None => sites.push((host.to_string(), visits, last, origin.to_string(), visits)),
                }
            }
            // (among equals, the shortest name)
            let best = sites.into_iter().max_by(|a, b| frecency(a.1, a.2, now).cmp(&frecency(b.1, b.2, now)).then(b.0.len().cmp(&a.0.len())));
            Ok(best.map(|(host, visits, last, origin, _)| Completion { text: host, url: format!("{origin}/"), score: frecency(visits, last, now) }))
        })
        .flatten()
    }

    /// The pages visited last, each once, newest first — those that hold
    /// `needle` (lower case; empty: all) in their address or title.
    pub fn recent_pages(&self, needle: &str, limit: usize) -> Vec<(String, String)> {
        self.read("recent pages", |c| {
            let mut st = c.prepare_cached("SELECT url, title FROM pages ORDER BY last DESC, id DESC")?;
            let mut rows = st.query([])?;
            let mut found = Vec::new();
            while found.len() < limit {
                let Some(r) = rows.next()? else { break };
                let (url, title) = (text(r, 0)?, text(r, 1)?);
                if has(url, needle) || has(title, needle) {
                    found.push((url.to_string(), title.to_string()));
                }
            }
            Ok(found)
        })
        .unwrap_or_default()
    }

    /// The sites that count most (`frecency` of the whole site: every page
    /// of it adds up — counting each address apart, a site read page after
    /// page never showed), each with its most visited page.
    pub fn top_sites(&self, limit: usize) -> Vec<Site> {
        let now = crate::store::now();
        self.read("frequent sites", |c| {
            // (the most visited sites, a few more than asked: how recent
            // they are then puts them in order)
            let mut st = c.prepare_cached("SELECT host, sum(visits), max(last) FROM pages WHERE host <> '' GROUP BY host ORDER BY 2 DESC, 3 DESC LIMIT ?1")?;
            let mut sites: Vec<(String, u64, u64)> = st.query_map([limit.saturating_mul(4) as i64], |r| Ok((r.get(0)?, number(r, 1)?, number(r, 2)?)))?.collect::<rusqlite::Result<_>>()?;
            sites.sort_by(|a, b| frecency(b.1, b.2, now).cmp(&frecency(a.1, a.2, now)).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
            sites.truncate(limit);
            let mut page = c.prepare_cached("SELECT url, title FROM pages WHERE host = ?1 ORDER BY visits DESC, last DESC LIMIT 1")?;
            sites.into_iter().map(|(host, visits, _)| page.query_row([host], |r| Ok(Site { url: r.get(0)?, title: r.get(1)?, visits }))).collect()
        })
        .unwrap_or_default()
    }

    /// The addresses visited last (`limit` of them at most): what the
    /// Light engine colours as visited links.
    pub fn visited_addresses(&self, limit: usize) -> HashSet<String> {
        self.read("visited addresses", |c| c.prepare_cached("SELECT url FROM pages ORDER BY last DESC LIMIT ?1")?.query_map([limit as i64], |r| r.get(0))?.collect()).unwrap_or_default()
    }

    /// The sites visited since `since` (their names, without "www." and
    /// port): the ones whose cookies "Clear browsing data" removes.
    pub fn sites_since(&self, since: u64) -> HashSet<String> {
        self.read("sites visited", |c| {
            let mut st = c.prepare_cached("SELECT DISTINCT host FROM pages WHERE host <> '' AND last >= ?1")?;
            let mut rows = st.query([secs(since)])?;
            let mut sites = HashSet::new();
            while let Some(r) = rows.next()? {
                let host = text(r, 0)?;
                let name = match host.rsplit_once(':') {
                    Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
                    _ => host,
                };
                sites.insert(name.to_string());
            }
            Ok(sites)
        })
        .unwrap_or_default()
    }

    /// What the history page lists for `filter` (the words searched;
    /// empty: everything): how many visits, and how many in each group of
    /// days as counted from `now`. The whole history is searched; only
    /// numbers are kept.
    pub fn list(&self, filter: &str, now: u64) -> Listing {
        let words = words_of(filter);
        let bounds = crate::store::DAY_GROUPS.map(|age| secs(now.saturating_sub(age)));
        self.read("listing", |c| {
            if words.is_empty() {
                let total = c.query_row("SELECT count(*) FROM visits", [], |r| number(r, 0))? as usize;
                let mut newer = c.prepare_cached("SELECT count(*) FROM visits WHERE time > ?1")?;
                let mut groups = [0; 5];
                let mut before = 0;
                for (k, bound) in bounds.iter().enumerate() {
                    let n = newer.query_row([bound], |r| number(r, 0))? as usize;
                    groups[k] = n.saturating_sub(before);
                    before = n.max(before);
                }
                groups[4] = total.saturating_sub(before);
                return Ok(Listing { total, groups, found: None });
            }
            // the pages that answer the search…
            let mut pages: HashSet<i64> = HashSet::new();
            let mut st = c.prepare_cached("SELECT id, url, title FROM pages")?;
            let mut rows = st.query([])?;
            while let Some(r) = rows.next()? {
                if matches(&words, text(r, 1)?, text(r, 2)?) {
                    pages.insert(r.get(0)?);
                }
            }
            // …and their visits, newest first
            let mut found = Vec::new();
            let mut groups = [0; 5];
            if !pages.is_empty() {
                let mut st = c.prepare_cached("SELECT id, page, time FROM visits ORDER BY time DESC, id DESC")?;
                let mut rows = st.query([])?;
                while let Some(r) = rows.next()? {
                    if pages.contains(&r.get::<_, i64>(1)?) {
                        let time: i64 = r.get(2)?;
                        groups[bounds.iter().position(|b| time > *b).unwrap_or(4)] += 1;
                        found.push(r.get(0)?);
                    }
                }
            }
            Ok(Listing { total: found.len(), groups, found: Some(Arc::new(found)) })
        })
        .unwrap_or_default()
    }

    /// `count` lines of a listing, from line `from` (0: the newest).
    pub fn rows(&self, listing: &Listing, from: usize, count: usize) -> Vec<Row> {
        fn row(r: &rusqlite::Row) -> rusqlite::Result<Row> {
            Ok(Row { id: r.get(0)?, time: number(r, 1)?, url: r.get(2)?, title: r.get(3)? })
        }
        self.read("history lines", |c| match &listing.found {
            Some(found) => {
                let mut st = c.prepare_cached("SELECT v.id, v.time, p.url, p.title FROM visits v JOIN pages p ON p.id = v.page WHERE v.id = ?1")?;
                // (a visit removed since the search is skipped)
                Ok(found.iter().skip(from).take(count).filter_map(|id| st.query_row([id], row).ok()).collect())
            }
            // (the lines before `from` are passed over in the index alone)
            None => c
                .prepare_cached("SELECT v.id, v.time, p.url, p.title FROM visits v JOIN pages p ON p.id = v.page WHERE v.id IN (SELECT id FROM visits ORDER BY time DESC, id DESC LIMIT ?2 OFFSET ?1) ORDER BY v.time DESC, v.id DESC")?
                .query_map(params![from as i64, count as i64], row)?
                .collect(),
        })
        .unwrap_or_default()
    }
}

/// One input for the fuzzers (see docs/FUZZING.md): any bytes read as the
/// history file of earlier versions, into a history of its own.
#[cfg(any(test, fuzzing))]
pub fn fuzz_one(data: &[u8]) {
    let text = String::from_utf8_lossy(data);
    let lines = text.lines().filter(|l| matches!(old_line(l), Some(Old::Visit(..)))).count();
    let Some(conns) = memory() else { return };
    let mut c = lock(&conns.write);
    let added = import_text(&mut c, &text).expect("any text can be taken in");
    assert!(added <= lines, "no more visits than lines");
    let kept: i64 = c.query_row("SELECT count(*) FROM visits", [], |r| r.get(0)).unwrap();
    let counted: i64 = c.query_row("SELECT coalesce(sum(visits), 0) FROM pages", [], |r| r.get(0)).unwrap();
    assert_eq!((kept, counted), (added as i64, added as i64), "every visit is counted once");
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sanaga-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.ends_with("-wal") && !n.ends_with("-shm")).collect();
        names.sort();
        names
    }

    fn urls(h: &History, filter: &str) -> Vec<String> {
        let l = h.list(filter, crate::store::now());
        h.rows(&l, 0, 1000).into_iter().map(|r| r.url).collect()
    }

    /// Thousands of damaged history files (see mutate.rs): none may panic,
    /// and each visit read is counted once.
    #[test]
    fn damaged_history_files_are_read_as_far_as_they_go() {
        let seeds: [&[u8]; 3] = [
            b"10\thttps://a.test/\t\n20\thttps://b.test/\tB\n=https://a.test/\tThe A page\n",
            "1700000000\thttps://é.example/ü?q=1\tÉcole 👍\nbroken line\n=\n\t\t\n".as_bytes(),
            b"x\thttps://a.test/\tA\n-5\thttps://a.test/\tA\n",
        ];
        crate::mutate::run(&seeds, 1_500, fuzz_one);
    }

    #[test]
    fn lines_of_the_earlier_file() {
        assert_eq!(old_line("10\thttps://a.test/\tA\tb"), Some(Old::Visit(10, "https://a.test/", "A\tb")));
        assert_eq!(old_line("soon\thttps://a.test/\t"), Some(Old::Visit(0, "https://a.test/", "")));
        assert_eq!(old_line("=https://a.test/\tThe A page"), Some(Old::Title("https://a.test/", "The A page")));
        for broken in ["", "broken line", "10\thttps://a.test/", "10\t\tA", "10\tsanaga://newtab\tNew tab", "=no tab"] {
            assert_eq!(old_line(broken), None, "{broken:?}");
        }
    }

    #[test]
    fn a_site_and_where_it_is() {
        assert_eq!(site_of("https://www.Wikipedia.org/wiki/Sanaga"), ("wikipedia.org".into(), "https://www.wikipedia.org".into()));
        assert_eq!(site_of("http://nas.lan:8080/files?x=1"), ("nas.lan:8080".into(), "http://nas.lan:8080".into()));
        assert_eq!(site_of("https://example.org:443/"), ("example.org".into(), "https://example.org".into()));
        for not_a_site in ["file:///home/awa/a.html", "about:blank", "not an address", ""] {
            assert_eq!(site_of(not_a_site), (String::new(), String::new()));
        }
    }

    #[test]
    fn what_is_typed_is_found_whatever_its_case() {
        assert!(has("https://Example.org/Page", "example") && has("École normale", "école") && has("ÉCOLE", "éco") && has("abc", ""));
        assert!(!has("https://example.org/", "exemple") && !has("abc", "abcd") && !has("ascii only", "é"));
        let w = words_of("  Sanaga   RIVER ");
        assert_eq!(w, ["sanaga", "river"]);
        assert!(matches(&w, "https://en.wikipedia.org/wiki/Sanaga", "The River"));
        assert!(!matches(&w, "https://en.wikipedia.org/wiki/Sanaga", "A stream"));
        assert_eq!(rank(&words_of("wiki"), "wikipedia.org", "https://www.wikipedia.org/", ""), Some(0));
        assert_eq!(rank(&words_of("wiki"), "en.wikipedia.org", "https://en.wikipedia.org/", ""), Some(1));
        assert_eq!(rank(&words_of("wiki"), "example.org", "https://example.org/", "About wikis"), Some(2));
        assert_eq!(rank(&words_of("wiki"), "example.org", "https://example.org/", "Nothing"), None);
    }

    #[test]
    fn frequent_and_recent_count_together() {
        let now = 1_800_000_000;
        // ten visits this week come before thirty of last year…
        assert!(frecency(10, now - DAY, now) > frecency(30, now - 400 * DAY, now));
        // …and among pages seen the same day, the most visited
        assert!(frecency(11, now - DAY, now) > frecency(10, now - DAY, now));
        assert_eq!(frecency(0, now, now), 0);
    }

    #[test]
    fn visits_are_counted_and_titles_follow() {
        let h = History::in_memory();
        let now = crate::store::now();
        h.add_visit("", "", now);
        h.add_visit("  ", "Untitled", now);
        h.add_visit("sanaga://newtab", "", now);
        assert_eq!(h.count(), 0);
        h.add_visit("https://a.test/", "", now - 30);
        h.add_visit("https://b.test/", "B", now - 20);
        h.add_visit("https://a.test/", "", now - 10);
        // the same page told twice in a row is one visit, at the later time
        h.add_visit("https://a.test/", "", now - 5);
        // a title learnt late renames the page; of a page never visited, nothing
        h.set_title("https://a.test/", "The A page");
        h.set_title("https://not-visited.test/", "x");
        let l = h.list("", now);
        let rows = h.rows(&l, 0, 10);
        assert_eq!(
            rows.iter().map(|r| (r.url.as_str(), r.title.as_str(), now - r.time)).collect::<Vec<_>>(),
            [("https://a.test/", "The A page", 5), ("https://b.test/", "B", 20), ("https://a.test/", "The A page", 30)]
        );
        assert_eq!((l.total, l.groups), (3, [3, 0, 0, 0, 0]));
        let hits = h.suggest("test", 10);
        assert_eq!(hits.iter().map(|x| (x.url.as_str(), x.visits)).collect::<Vec<_>>(), [("https://a.test/", 2), ("https://b.test/", 1)]);
        // one line removed: the page is counted once less; its last line: the page goes
        h.remove_visit(rows[0].id);
        assert_eq!(h.suggest("a.test", 10).iter().map(|x| (x.visits, now - x.last)).collect::<Vec<_>>(), [(1, 30)]);
        h.remove_visit(rows[2].id);
        assert!(h.suggest("a.test", 10).is_empty());
        assert_eq!(urls(&h, ""), ["https://b.test/"]);
    }

    #[test]
    fn the_most_visited_site_comes_first_and_names_come_before_mentions() {
        let h = History::in_memory();
        let now = crate::store::now();
        let mut past = Vec::new();
        // a site visited often and lately, one visited more but long ago,
        // one that only mentions the word, one seen once
        for k in 0..12 {
            past.push((now - k * 3600, "https://sanaga.example/".to_string(), "Sanaga".to_string()));
        }
        for k in 0..40 {
            past.push((now - 500 * DAY - k, "https://sanaga-old.example/".to_string(), "Old".to_string()));
        }
        for k in 0..90 {
            past.push((now - k * 60, "https://news.example/river".to_string(), "The Sanaga in flood".to_string()));
        }
        past.push((now - 50, "https://maps.sanaga.example/".to_string(), "Maps".to_string()));
        past.push((now - 40, "https://other.example/".to_string(), "Nothing to see".to_string()));
        h.add_past(past);
        let order: Vec<String> = h.suggest("sana", 10).into_iter().map(|x| x.url).collect();
        assert_eq!(order, ["https://sanaga.example/", "https://sanaga-old.example/", "https://maps.sanaga.example/", "https://news.example/river"]);
        // only as many as asked, the best ones
        assert_eq!(h.suggest("sana", 2).len(), 2);
        assert_eq!(h.suggest("SANAGA flood", 10).into_iter().map(|x| x.url).collect::<Vec<_>>(), ["https://news.example/river"]);
        assert!(h.suggest("", 10).is_empty() && h.suggest("zzz", 10).is_empty());
        // the new tab page: sites by their visits lately, each with its most visited page
        let top = h.top_sites(3);
        assert_eq!(top.iter().map(|s| (s.url.as_str(), s.visits)).collect::<Vec<_>>(), [("https://news.example/river", 90), ("https://sanaga.example/", 12), ("https://sanaga-old.example/", 40)]);
    }

    #[test]
    fn a_site_is_completed_to_where_it_was_visited() {
        let h = History::in_memory();
        let now = crate::store::now();
        let mut past = Vec::new();
        for k in 0..5 {
            past.push((now - k, "http://nas.lan:8080/files".to_string(), "Files".to_string()));
            past.push((now - 100 - k, format!("http://nas.lan:8080/photos/{k}"), "Photos".to_string()));
        }
        past.push((now - 7, "https://www.wikipedia.org/".to_string(), "Wikipedia".to_string()));
        past.push((now - 8, "https://www.wikipedia.org/wiki/Sanaga".to_string(), "Sanaga".to_string()));
        past.push((now - 9, "http://wikipedia.org/".to_string(), "Wikipedia".to_string()));
        past.push((now - 9, "https://wiktionary.org/".to_string(), "Wiktionary".to_string()));
        h.add_past(past);
        // over http, on its port: completed to that very address (it was always https://name)
        let nas = h.complete_site("na").unwrap();
        assert_eq!((nas.text.as_str(), nas.url.as_str()), ("nas.lan:8080", "http://nas.lan:8080/"));
        assert_eq!(h.complete_site("NAS.lan:80").unwrap().url, "http://nas.lan:8080/");
        // the most visited site whose name starts so, where it is most visited
        let wiki = h.complete_site("wik").unwrap();
        assert_eq!((wiki.text.as_str(), wiki.url.as_str()), ("wikipedia.org", "https://www.wikipedia.org/"));
        assert_eq!(h.complete_site("wikt").unwrap().text, "wiktionary.org");
        for nothing in ["w", "zz", "wiki pedia", "wikipedia.org/wiki", "lan"] {
            assert_eq!(h.complete_site(nothing), None, "{nothing:?}");
        }
    }

    #[test]
    fn the_earlier_history_file_is_taken_in_whole_and_kept() {
        let dir = temp("import");
        let now = crate::store::now();
        // more visits than the earlier limit of 5,000, with a damaged byte,
        // a broken line and a title learnt late
        let mut file = Vec::new();
        for k in 0..6000u64 {
            file.extend_from_slice(format!("{}\thttps://site{}.example/page/{k}\tPage {k}\n", now - 6000 * 60 + k * 60, k % 50).as_bytes());
        }
        file.extend_from_slice(b"broken line\n1\thttps://old.example/\tOld \xff\xfe title\n=https://site0.example/page/0\tRenamed\n");
        std::fs::write(dir.join(OLD_FILE), &file).unwrap();
        let h = History::open_apart(&dir);
        assert_eq!(h.count(), 6001);
        assert_eq!(names_in(&dir), ["history.db", "history.tsv.imported"]);
        // (kept as it was, byte for byte)
        assert_eq!(std::fs::read(dir.join(OLD_KEPT)).unwrap(), file);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.join(DB_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        // the oldest ones are there, the late title too
        let l = h.list("", now);
        assert_eq!(l.total, 6001);
        let oldest = h.rows(&l, 5999, 10);
        assert_eq!(oldest.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Renamed", "Old \u{fffd}\u{fffd} title"]);
        assert_eq!(urls(&h, "page/4321"), ["https://site21.example/page/4321"]);
        // a second start finds nothing more to take in, and loses nothing
        let again = History::open_apart(&dir);
        assert_eq!(again.count(), 6001);
        // an earlier Sanaga wrote a new file meanwhile: taken in too, with
        // what it repeats counted once; the first copy stays
        std::fs::write(dir.join(OLD_FILE), format!("1\thttps://old.example/\tOld\n{now}\thttps://new.example/\tNew\n")).unwrap();
        let third = History::open_apart(&dir);
        assert_eq!(third.count(), 6002);
        assert_eq!(names_in(&dir), ["history.db", "history.tsv.imported", "history.tsv.imported2"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_cannot_be_read_is_left_as_it_is() {
        // an earlier history that cannot be read (here: a folder under its
        // name; on a real disk: a damaged block, a refusal)
        let dir = temp("unreadable");
        std::fs::create_dir(dir.join(OLD_FILE)).unwrap();
        let h = History::open_apart(&dir);
        h.add_visit("https://a.test/", "A", 10);
        assert_eq!(h.count(), 1);
        assert!(dir.join(OLD_FILE).is_dir());
        // a database that is not one: kept under another name, never written over
        let dir = temp("damaged");
        let junk = b"this is not a database, it is something the person may want back".repeat(40);
        std::fs::write(dir.join(DB_FILE), &junk).unwrap();
        std::fs::write(dir.join(OLD_FILE), "5\thttps://kept.test/\tKept\n").unwrap();
        let h = History::open_apart(&dir);
        assert_eq!(urls(&h, ""), ["https://kept.test/"]);
        assert_eq!(names_in(&dir), ["history.db", "history.db.damaged", "history.tsv.imported"]);
        assert_eq!(std::fs::read(dir.join(DAMAGED)).unwrap(), junk);
        // a database that cannot be opened at all (a folder in its place):
        // left alone, and the earlier file with it — nothing is taken in
        // and renamed when it cannot be kept
        let dir = temp("closed");
        std::fs::create_dir(dir.join(DB_FILE)).unwrap();
        std::fs::write(dir.join(OLD_FILE), "5\thttps://kept.test/\tKept\n").unwrap();
        let h = History::open_apart(&dir);
        h.add_visit("https://a.test/", "A", 10);
        assert_eq!(urls(&h, ""), ["https://a.test/"]);
        assert_eq!(names_in(&dir), ["history.db", "history.tsv"]);
        assert!(dir.join(DB_FILE).is_dir());
        // a database of a later Sanaga: not written by this one
        let dir = temp("later");
        let c = Connection::open(dir.join(DB_FILE)).unwrap();
        c.execute_batch("CREATE TABLE pages(id INTEGER PRIMARY KEY, address TEXT); PRAGMA user_version=99;").unwrap();
        drop(c);
        let before = std::fs::read(dir.join(DB_FILE)).unwrap();
        let h = History::open_apart(&dir);
        h.add_visit("https://a.test/", "A", 10);
        assert_eq!(h.count(), 1);
        assert_eq!(std::fs::read(dir.join(DB_FILE)).unwrap(), before);
        for name in ["unreadable", "damaged", "closed", "later"] {
            let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("sanaga-history-{name}-{}", std::process::id())));
        }
    }

    #[test]
    fn clearing_since_a_time_and_everything() {
        let dir = temp("clear");
        let now = crate::store::now();
        std::fs::write(
            dir.join(OLD_FILE),
            format!(
                "{}\thttps://old.example/\tOld\n{}\thttps://both.example/\tBoth\n{}\thttps://both.example/\tBoth\n{}\thttps://new.example/\tNew\n=https://new.example/\tNewer\n",
                now - 9 * DAY,
                now - 8 * DAY,
                now - 600,
                now - 300
            ),
        )
        .unwrap();
        let h = History::open_apart(&dir);
        assert_eq!(h.sites_since(now - 3600), HashSet::from(["both.example".to_string(), "new.example".to_string()]));
        assert_eq!(h.sites_since(0).len(), 3);
        // the last hour: what was visited before stays, counted as it now is
        h.clear_since(now - 3600);
        assert_eq!(urls(&h, ""), ["https://both.example/", "https://old.example/"]);
        assert_eq!(h.suggest("both", 5).iter().map(|x| (x.visits, now - x.last)).collect::<Vec<_>>(), [(1, 8 * DAY)]);
        assert!(h.sites_since(now - 3600).is_empty());
        // gone from the copy of the earlier file too, and only that
        let kept = std::fs::read_to_string(dir.join(OLD_KEPT)).unwrap();
        assert_eq!(kept, format!("{}\thttps://old.example/\tOld\n{}\thttps://both.example/\tBoth\n", now - 9 * DAY, now - 8 * DAY));
        // one line removed: gone from the copy as well
        let l = h.list("old", now);
        h.remove_visit(h.rows(&l, 0, 1)[0].id);
        assert_eq!(urls(&h, ""), ["https://both.example/"]);
        assert_eq!(std::fs::read_to_string(dir.join(OLD_KEPT)).unwrap(), format!("{}\thttps://both.example/\tBoth\n", now - 8 * DAY));
        // everything: nothing left, in the database or beside it
        std::fs::write(dir.join(DAMAGED), "x").unwrap();
        h.clear_since(0);
        assert_eq!((h.count(), h.suggest("example", 5).len(), h.top_sites(5).len()), (0, 0, 0));
        assert_eq!(names_in(&dir), ["history.db"]);
        let bytes = std::fs::read(dir.join(DB_FILE)).unwrap();
        assert!(!bytes.windows(12).any(|w| w == b"both.example"), "a cleared address is still in the file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_share_one_history_and_two_sanagas_one_file() {
        let dir = temp("shared");
        let now = crate::store::now();
        // two windows of this Sanaga: the same history, and each knows at
        // once that it changed
        let (a, b) = (History::open(&dir), History::open(&dir));
        let seen = b.version();
        a.add_visit("https://a.test/", "A", now - 2);
        assert_eq!(urls(&b, ""), ["https://a.test/"]);
        assert!(b.version() > seen);
        // a history that keeps nothing, and another folder's: they hear nothing
        let (guest, other_dir) = (History::in_memory(), temp("shared-other"));
        let other = History::open(&other_dir);
        assert_eq!((guest.count(), other.count()), (0, 0));
        // another Sanaga on the same profile: both write, each reads the other
        // (what the other Sanaga writes is not waited for — it cannot be —
        // so the test looks once it is written)
        let c = History::open_apart(&dir);
        c.add_visit("https://c.test/", "C", now - 1);
        assert_eq!(c.count(), 2);
        let seen = a.version();
        assert_eq!(urls(&a, ""), ["https://c.test/", "https://a.test/"]);
        assert!(a.version() > seen, "a change made by another Sanaga is noticed when the history is read");
        // both at once, many times: nothing lost, nothing refused
        let threads: Vec<_> = [a.clone(), c.clone()]
            .into_iter()
            .enumerate()
            .map(|(n, h)| {
                std::thread::spawn(move || {
                    for k in 0..200u64 {
                        h.add_visit(&format!("https://w{n}.test/{k}"), "x", 1000 + k);
                        if k % 20 == 0 {
                            let _ = h.suggest("test", 5);
                        }
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let _ = (a.count(), c.count());
        assert_eq!((a.count(), c.count()), (402, 402));
        // one clears: gone for the other
        c.clear_since(0);
        assert_eq!((c.count(), a.count()), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other_dir);
    }

    #[test]
    fn two_hundred_thousand_visits_stay_fast() {
        let dir = temp("large");
        let now = crate::store::now();
        let h = History::open_apart(&dir);
        // 200,000 visits of 60,000 pages on 3,000 sites, over three years
        let started = std::time::Instant::now();
        for chunk in 0..20u64 {
            let visits: Vec<(u64, String, String)> = (0..10_000u64)
                .map(|k| {
                    let n = chunk * 10_000 + k;
                    let page = (n * 7919) % 60_000;
                    (
                        now - n * 470,
                        format!("https://site{}.example/section/{}/page-{page}.html?ref=home", page % 3000, page % 17),
                        format!("Page {page} of site {} — a fairly long title, as real ones are", page % 3000),
                    )
                })
                .collect();
            h.add_past(visits);
        }
        // one site visited more than the others, lately
        h.add_past((0..300u64).map(|k| (now - k * 600, "https://sanaga.example/".to_string(), "Sanaga".to_string())).collect());
        assert_eq!(h.count(), 200_300);
        eprintln!("history: 200,300 visits written in {} ms", started.elapsed().as_millis());
        let timed = |what: &str, limit_ms: u128, ask: &dyn Fn() -> usize| {
            let t = std::time::Instant::now();
            let n = ask();
            let ms = t.elapsed().as_millis();
            eprintln!("history: {what}: {n} in {ms} ms");
            assert!(ms < limit_ms, "{what} took {ms} ms");
            n
        };
        // (generous limits: the machines that run the tests are busy ones)
        assert_eq!(timed("suggestions for \"s\"", 1500, &|| h.suggest("s", 8).len()), 8);
        assert_eq!(h.suggest("s", 8)[0].url, "https://sanaga.example/");
        assert_eq!(timed("suggestions for \"page-59999 site\"", 1500, &|| h.suggest("page-59999 site", 8).len()), 1);
        assert_eq!(timed("suggestions for a word found nowhere", 1500, &|| h.suggest("nowhere-at-all", 8).len()), 0);
        assert_eq!(timed("completion of \"san\"", 300, &|| usize::from(h.complete_site("san").is_some_and(|c| c.url == "https://sanaga.example/"))), 1);
        assert_eq!(timed("completion of \"si\" (3,000 sites)", 800, &|| usize::from(h.complete_site("si").is_some())), 1);
        let l = h.list("", now);
        assert_eq!((l.total, l.groups.iter().sum::<usize>()), (200_300, 200_300));
        assert_eq!(timed("the newest 200 lines", 300, &|| h.rows(&l, 0, 200).len()), 200);
        assert_eq!(timed("the oldest 200 lines", 500, &|| h.rows(&l, 200_100, 200).len()), 200);
        assert_eq!(h.rows(&l, 200_299, 5).len(), 1);
        timed("listing everything", 500, &|| h.list("", now).total);
        let found = timed("a search of the whole history", 2000, &|| h.list("page-59999", now).total);
        assert!(found >= 1);
        let l = h.list("page-59999", now);
        assert!(h.rows(&l, 0, 200).iter().all(|r| r.url.contains("page-59999")));
        assert_eq!(timed("the ten pages visited last", 300, &|| h.recent_pages("", 10).len()), 10);
        assert_eq!(timed("the eight sites that count most", 800, &|| h.top_sites(8).len()), 8);
        assert_eq!(h.top_sites(8)[0].url, "https://sanaga.example/");
        assert_eq!(timed("20,000 visited addresses", 800, &|| h.visited_addresses(20_000).len()), 20_000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

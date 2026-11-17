//! What Sanaga keeps when it, or a page's engine, stops unexpectedly: a
//! short text report in `crashes/` (in Sanaga's data folder, closed to other
//! users), which the person can read, copy, send as a GitHub issue, or
//! delete — on the page `sanaga://crashes`. Nothing is ever sent by itself.
//!
//! Sanaga also notes while it runs (the file `running` of the profile), so
//! the next start knows the last one ended abruptly — also when the system
//! killed it, where no report could be written — and brings the tabs back.

use std::path::{Path, PathBuf};

/// How many reports are kept (the oldest go first).
pub const KEEP: usize = 20;
/// The most a report may weigh (a panic message can quote anything).
const MAX_TEXT: usize = 16 << 10;
/// Where reports are sent when the person chooses to.
const ISSUES: &str = "https://github.com/grassfieldhq/sanaga/issues/new";

/// What stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Sanaga itself (a panic: the report says where).
    Interface,
    /// The engine process of one page (the other tabs went on).
    Page,
    /// A page ended by the engine because it used too much memory.
    PageMemory,
    /// The last run ended without a word (killed, power cut, a crash inside
    /// a system library): known from the `running` note only.
    Abrupt,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Interface => "interface",
            Kind::Page => "page",
            Kind::PageMemory => "page-memory",
            Kind::Abrupt => "abrupt",
        }
    }
    fn from_word(w: &str) -> Option<Kind> {
        [Kind::Interface, Kind::Page, Kind::PageMemory, Kind::Abrupt].into_iter().find(|k| k.word() == w)
    }
}

/// One report on disk.
#[derive(Clone, Debug)]
pub struct Report {
    pub path: PathBuf,
    /// Its date and time (UTC), as written in its name: "2026-11-11 18:40:02".
    pub when: String,
    pub kind: Kind,
    pub text: String,
}

/// The folder of the reports, inside Sanaga's data folder.
pub fn folder(root: &Path) -> PathBuf {
    root.join("crashes")
}

/// Seconds since 1970 as a UTC date and time, for a file name (no colon:
/// "2026-11-11T18-40-02Z"). Names sort by date.
pub fn stamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // (days since 1970 to a calendar date: Howard Hinnant's civil_from_days)
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}-{:02}-{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// Private details taken out of a report before it is kept: the person's
/// home folder (and so their user name) becomes `~`, addresses lose
/// everything after the site's name (a path or a query can hold a name, a
/// search, a token), and the whole is kept short.
pub fn scrub(text: &str, home: Option<&str>) -> String {
    let mut t = text.to_string();
    if let Some(h) = home.map(|h| h.trim_end_matches('/')).filter(|h| h.len() > 1) {
        t = t.replace(h, "~");
    }
    let mut out = String::with_capacity(t.len());
    let mut rest = t.as_str();
    while let Some(i) = rest.find("://") {
        let (before, after) = rest.split_at(i + 3);
        out.push_str(before);
        // the site's name stays, what follows it goes
        let host_end = after.find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '"' || c == '\'').unwrap_or(after.len());
        // (user:password@ before the name goes too)
        let host = &after[..host_end];
        out.push_str(host.rsplit('@').next().unwrap_or(host));
        let tail = &after[host_end..];
        let cut = tail.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == '>').unwrap_or(tail.len());
        if cut > 0 {
            out.push_str("/…");
        }
        rest = &tail[cut..];
    }
    out.push_str(rest);
    if out.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("\n[…]");
    }
    out
}

/// The system, in a few words, for a report: nothing that names the person
/// or the computer.
pub fn system_lines() -> String {
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let display = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let mut t = format!("System: {os} ({})\n", std::env::consts::ARCH);
    let desk = format!("{desktop} {display}");
    if !desk.trim().is_empty() {
        t.push_str(&format!("Desktop: {}\n", desk.trim()));
    }
    t
}

/// The text of a report: what stopped, Sanaga's version, the system, the
/// engine's version when known, then the details (scrubbed).
pub fn compose(kind: Kind, when: &str, engine: &str, detail: &str, home: Option<&str>) -> String {
    let what = match kind {
        Kind::Interface => "Sanaga stopped (panic)",
        Kind::Page => "A page's engine process stopped",
        Kind::PageMemory => "A page was ended: it used too much memory",
        Kind::Abrupt => "Sanaga's last run ended without closing (killed, power cut, or a crash in a system library)",
    };
    let mut t = format!("{what}\nWhen: {when} UTC\nSanaga: {}\n{}", env!("CARGO_PKG_VERSION"), system_lines());
    if !engine.is_empty() {
        t.push_str(&format!("WebKitGTK: {engine}\n"));
    }
    let detail = scrub(detail.trim(), home);
    if !detail.is_empty() {
        t.push_str(&format!("\n{detail}\n"));
    }
    t
}

/// Keep a report; returns its file. Only the newest `KEEP` stay. Safe to
/// call from the panic hook: it only touches files.
pub fn write(root: &Path, kind: Kind, engine: &str, detail: &str) -> Option<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = folder(root);
    crate::guard::ensure_private_dir(&dir);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = stamp(secs);
    let when = name.replace('T', " ").trim_end_matches('Z').to_string();
    let home = std::env::var("HOME").ok();
    let text = compose(kind, &pretty_time(&when), engine, detail, home.as_deref());
    // (two reports in the same second: the process number and a counter tell them apart)
    for n in 0..1000u32 {
        let path = dir.join(format!("{name}-{}-{:x}_{n}.txt", kind.word(), std::process::id()));
        // a new file only (never through a link, never over another report)
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&path) {
            Ok(mut f) => {
                let _ = f.write_all(text.as_bytes());
                prune(&dir);
                return Some(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// "2026-11-11 18-40-02" → "2026-11-11 18:40:02".
fn pretty_time(when: &str) -> String {
    match when.split_once(' ') {
        Some((d, t)) => format!("{d} {}", t.replace('-', ":")),
        None => when.to_string(),
    }
}

fn report_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.metadata().is_ok_and(|m| m.is_file())).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect())
        .unwrap_or_default();
    // newest first (the names start with the date)
    v.sort();
    v.reverse();
    v
}

fn prune(dir: &Path) {
    for old in report_files(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
}

/// The reports kept, newest first.
pub fn list(root: &Path) -> Vec<Report> {
    use std::io::Read;
    report_files(&folder(root))
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_string();
            // "2026-11-11T18-40-02Z-<kind>-<id>"
            let (date, rest) = name.split_once("Z-")?;
            let kind = Kind::from_word(rest.rsplit_once('-')?.0)?;
            // (never more than a report can hold, whatever the file became.
            // Read as bytes: the limit may fall in the middle of a character,
            // and a file changed by hand may not be text at all — read as
            // text, such a report vanished from the list without a word.)
            let mut bytes = Vec::new();
            std::fs::File::open(&path).ok()?.take(MAX_TEXT as u64 + 64).read_to_end(&mut bytes).ok()?;
            // (a character cut by the limit is left out rather than shown as "\u{fffd}")
            if let Err(e) = std::str::from_utf8(&bytes) {
                if e.error_len().is_none() {
                    bytes.truncate(e.valid_up_to());
                }
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            Some(Report { path, when: pretty_time(&date.replace('T', " ")), kind, text })
        })
        .collect()
}

/// Delete one report (a file of the reports folder only), or all of them.
pub fn remove(root: &Path, path: &Path) {
    if path.parent() == Some(folder(root).as_path()) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn remove_all(root: &Path) {
    for p in report_files(&folder(root)) {
        let _ = std::fs::remove_file(p);
    }
}

/// The address that opens a new GitHub issue with this report in it, for the
/// person to read once more and send (or not). The text is already scrubbed.
pub fn issue_url(report: &Report) -> String {
    let title = match report.kind {
        Kind::Interface => "Crash: Sanaga stopped",
        Kind::Page | Kind::PageMemory => "Crash: a page stopped",
        Kind::Abrupt => "Sanaga ended without closing",
    };
    // (GitHub refuses very long addresses: the start of the report is what matters)
    let mut body: String = report.text.chars().take(3000).collect();
    body = format!("**What I was doing**\n\n(please describe)\n\n**Report**\n\n```\n{}\n```\n", body.trim_end());
    let enc = |s: &str| percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string();
    format!("{ISSUES}?title={}&body={}", enc(title), enc(&body))
}

// ------------------------------------------------------------ the running note

/// Is this process still there? (A note left by a Sanaga that still runs —
/// a web app window, a second launch — is not a crash.)
fn alive(pid: u32) -> bool {
    pid != 0 && Path::new(&format!("/proc/{pid}")).exists()
}

/// What is known of a run that ended without closing, from its note.
#[derive(Clone, Debug, PartialEq)]
pub struct Abrupt {
    /// When that run had started.
    pub started: std::time::SystemTime,
    pub why: Ended,
}

/// How a run that left its note behind came to end, as far as the next
/// start can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// A signal ended it (a crash in Sanaga or in a system library, a
    /// stack that ran out): its number was noted as it happened.
    Signal(i32),
    /// The computer has been started again since: shut down, restarted or
    /// cut off with Sanaga open.
    Restart,
    /// The desktop session that run was shown in is gone: logging out (or
    /// the desktop itself stopping) with Sanaga open ends it without a word.
    Session,
    /// The same session still runs: Sanaga was killed, or ended inside a
    /// system library that leaves no trace.
    Unknown,
}

impl Abrupt {
    /// The lines of the report that say what is known ("Started: …").
    pub fn detail(&self) -> String {
        let secs = self.started.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let started = pretty_time(stamp(secs).replace('T', " ").trim_end_matches('Z'));
        let why = match self.why {
            Ended::Signal(n) => format!("Ended by: signal {n}{} (a crash in Sanaga or in a system library)", signal_name(n).map(|s| format!(" ({s})")).unwrap_or_default()),
            Ended::Restart => "Since then: the computer was started again (shut down, restarted or cut off while Sanaga was open)".to_string(),
            Ended::Session => "Since then: the desktop session Sanaga was in has ended (logging out with Sanaga open ends it this way)".to_string(),
            Ended::Unknown => "Since then: the same desktop session still runs (Sanaga was killed, or ended inside a system library)".to_string(),
        };
        format!("That run started: {started} UTC\n{why}")
    }
}

fn signal_name(n: i32) -> Option<&'static str> {
    Some(match n {
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGABRT => "SIGABRT",
        libc::SIGBUS => "SIGBUS",
        libc::SIGILL => "SIGILL",
        libc::SIGFPE => "SIGFPE",
        _ => return None,
    })
}

/// This start of the computer (the kernel gives each one a random name).
fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id").map(|t| t.trim().to_string()).unwrap_or_default()
}

/// The desktop session Sanaga is shown in: the display server's socket and
/// when it was made ("path inode seconds"). A session that ended, or
/// started again, no longer has that socket. Empty when it can't be told.
fn session_mark() -> String {
    let socket = match (std::env::var("WAYLAND_DISPLAY").ok().filter(|v| !v.is_empty()), std::env::var("DISPLAY").ok()) {
        (Some(w), _) if w.starts_with('/') => PathBuf::from(w),
        (Some(w), _) => match std::env::var("XDG_RUNTIME_DIR") {
            Ok(dir) if !dir.is_empty() => Path::new(&dir).join(w),
            _ => return String::new(),
        },
        // ":0", ":1.0", "unix:0"
        (None, Some(d)) => match d.rsplit(':').next().and_then(|n| n.split('.').next()).filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
            Some(n) => PathBuf::from(format!("/tmp/.X11-unix/X{n}")),
            None => return String::new(),
        },
        _ => return String::new(),
    };
    socket_mark(&socket).unwrap_or_default()
}

fn socket_mark(socket: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(socket).ok()?;
    Some(format!("{} {} {}", socket.display(), m.ino(), m.ctime()))
}

/// Is the session of this mark still the one running?
fn session_alive(mark: &str) -> bool {
    // (the path may hold spaces: the two numbers are the last two words)
    let mut words = mark.rsplitn(3, ' ');
    let (Some(_), Some(_), Some(path)) = (words.next(), words.next(), words.next()) else { return true };
    socket_mark(Path::new(path)).is_some_and(|now| now == mark)
}

/// What a note says of its run: the process, then "boot …", "session …"
/// and, if a signal ended it, "signal N".
fn read_note(text: &str, boot_now: &str) -> (u32, Ended) {
    let mut lines = text.lines();
    let pid = lines.next().and_then(|l| l.trim().parse::<u32>().ok()).unwrap_or(0);
    let (mut boot, mut session, mut signal) = (None, None, None);
    for l in lines {
        if let Some(v) = l.strip_prefix("boot ") {
            boot = Some(v.trim());
        } else if let Some(v) = l.strip_prefix("session ") {
            session = Some(v.trim());
        } else if let Some(v) = l.strip_prefix("signal ") {
            // (the first one noted is the cause: a stack that ran out is
            // told as a fault, then ended by an abort)
            signal = signal.or(v.trim().parse::<i32>().ok());
        }
    }
    let why = match (signal, boot, session) {
        (Some(n), _, _) => Ended::Signal(n),
        (None, Some(b), _) if !boot_now.is_empty() && b != boot_now => Ended::Restart,
        (None, _, Some(s)) if !s.is_empty() && !session_alive(s) => Ended::Session,
        _ => Ended::Unknown,
    };
    (pid, why)
}

/// Note that Sanaga runs with this profile. When the note of an earlier run
/// is still there and that run is gone, it ended abruptly: returns when it
/// had started and what is known of its end.
pub fn mark_running(profile_dir: &Path) -> Option<Abrupt> {
    let note = profile_dir.join("running");
    let started = std::fs::metadata(&note).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
    let boot_now = boot_id();
    let before = std::fs::read_to_string(&note).ok().map(|t| read_note(&t, &boot_now));
    // (a note of this start of the computer whose process is still there is
    // a Sanaga that runs; after a restart the same number is another program)
    let abrupt = before.filter(|(pid, why)| *pid != std::process::id() && (*why == Ended::Restart || !alive(*pid) || !Path::new("/proc").is_dir()));
    // (another Sanaga still runs with this profile: its note stays)
    if before.is_none() || abrupt.is_some() {
        let _ = std::fs::write(&note, format!("{}\nboot {boot_now}\nsession {}\n", std::process::id(), session_mark()));
        note_fatal_signals(&note);
        if let Ok(mut notes) = NOTES.lock() {
            notes.push(note);
        }
    }
    abrupt.map(|(_, why)| Abrupt { started, why })
}

/// The notes this process left (one per profile it opened).
static NOTES: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Sanaga closes as it should: its notes go.
pub fn mark_stopped() {
    if let Ok(mut notes) = NOTES.lock() {
        for n in notes.drain(..) {
            let _ = std::fs::remove_file(n);
        }
    }
}

// ------------------------------------------------------- signals that end Sanaga

/// The signals that end a program on the spot: a fault in Sanaga or in a
/// system library, an abort (a stack that ran out ends that way).
const FATAL: [libc::c_int; 5] = [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGFPE, libc::SIGABRT];

/// The note, kept open for the handler below (-1: none).
static NOTE_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// What each of these signals did before Sanaga asked to be told (Rust's
/// own handler for a stack that ran out, or the system's way).
static mut BEFORE: [std::mem::MaybeUninit<libc::sigaction>; FATAL.len()] = [std::mem::MaybeUninit::uninit(); FATAL.len()];

/// Runs inside the signal: writes its number at the end of the note, gives
/// the signal back to whoever handled it before, and returns — the fault
/// happens again (or the abort goes on) and ends Sanaga as it would have.
/// `raise` is async-signal-safe too.
extern "C" fn fatal_signal(signal: libc::c_int, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
    let fd = NOTE_FD.load(std::sync::atomic::Ordering::Relaxed);
    // "signal NN\n", built without allocating
    let mut line = *b"signal 00\n";
    line[7] = b'0' + (signal / 10 % 10) as u8;
    line[8] = b'0' + (signal % 10) as u8;
    // SAFETY: `write` and `sigaction` may be called inside a signal
    // handler; `BEFORE` was filled before this handler was set and is
    // never written again.
    unsafe {
        if fd >= 0 {
            libc::write(fd, line.as_ptr().cast(), line.len());
        }
        if let Some(k) = FATAL.iter().position(|s| *s == signal) {
            libc::sigaction(signal, (*std::ptr::addr_of!(BEFORE))[k].as_ptr(), std::ptr::null_mut());
        }
        // (sent by a program — `kill`, an abort — rather than raised by a
        // fault: nothing happens again by itself, so it is sent on to
        // whoever handled it before)
        if !info.is_null() && (*info).si_code <= 0 {
            libc::raise(signal);
        }
    }
}

/// From now on, a signal that ends Sanaga leaves its number in the note:
/// the next start can say "ended by SIGSEGV" instead of knowing nothing.
/// (Once per process: the first profile's note.)
fn note_fatal_signals(note: &Path) {
    use std::os::unix::ffi::OsStrExt;
    static ONCE: std::sync::Once = std::sync::Once::new();
    let Ok(path) = std::ffi::CString::new(note.as_os_str().as_bytes()) else { return };
    ONCE.call_once(|| {
        // SAFETY: a valid path; the descriptor stays open as long as Sanaga
        // runs (appending, closed on exec so no other program inherits it).
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_APPEND | libc::O_CLOEXEC | libc::O_NOFOLLOW) };
        if fd < 0 {
            return;
        }
        NOTE_FD.store(fd, std::sync::atomic::Ordering::Relaxed);
        for (k, signal) in FATAL.iter().enumerate() {
            // SAFETY: a handler that only calls async-signal-safe
            // functions, run on the signal stack where the thread has one
            // (a stack that ran out leaves no room on the normal one); the
            // action it replaces is kept in `BEFORE` first.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = fatal_signal as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) as libc::sighandler_t;
                action.sa_flags = libc::SA_ONSTACK | libc::SA_SIGINFO;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(*signal, &action, (*std::ptr::addr_of_mut!(BEFORE))[k].as_mut_ptr());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sanaga-crash-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dates_are_written_in_utc() {
        assert_eq!(stamp(0), "1970-01-01T00-00-00Z");
        assert_eq!(stamp(951_782_400), "2000-02-29T00-00-00Z");
        assert_eq!(stamp(1_794_422_402), "2026-11-11T18-40-02Z");
        assert_eq!(pretty_time("2026-11-11 18-40-02"), "2026-11-11 18:40:02");
    }

    #[test]
    fn private_details_are_taken_out() {
        let t = scrub("failed at /home/awa/.local/share/sanaga/x: https://bank.example/account/42?token=abc and \"http://user:pw@a.example\" then file:///home/awa/doc.pdf.", Some("/home/awa"));
        assert_eq!(t, "failed at ~/.local/share/sanaga/x: https://bank.example/… and \"http://a.example\" then file://~/…");
        assert!(!t.contains("awa") && !t.contains("token") && !t.contains("pw"));
        // nothing to scrub, nothing changed
        assert_eq!(scrub("index out of bounds: the len is 3", None), "index out of bounds: the len is 3");
        // long texts are cut, on a character boundary
        let long = "é".repeat(MAX_TEXT);
        assert!(scrub(&long, None).len() <= MAX_TEXT + 8);
    }

    #[test]
    fn a_long_or_damaged_report_is_still_listed() {
        let root = temp("long");
        let dir = folder(&root);
        crate::guard::ensure_private_dir(&dir);
        // longer than a report may be, and the limit falls inside an "é"
        let long = format!("x{}", "é".repeat(MAX_TEXT));
        assert!(!long.is_char_boundary(MAX_TEXT + 64));
        std::fs::write(dir.join(format!("2026-11-11T18-40-02Z-{}-1_0.txt", Kind::Interface.word())), &long).unwrap();
        // not text any more (changed by hand, a disk error)
        std::fs::write(dir.join(format!("2026-11-11T18-40-03Z-{}-1_0.txt", Kind::Page.word())), b"page stopped \xff\xfe here").unwrap();
        let all = list(&root);
        assert_eq!(all.len(), 2);
        let cut = all.iter().find(|r| r.kind == Kind::Interface).unwrap();
        assert!(cut.text.len() >= MAX_TEXT && cut.text.len() <= MAX_TEXT + 64 && cut.text.ends_with('é') && !cut.text.contains('\u{fffd}'));
        let odd = all.iter().find(|r| r.kind == Kind::Page).unwrap();
        assert!(odd.text.starts_with("page stopped ") && odd.text.ends_with(" here"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_are_kept_listed_and_removed() {
        let root = temp("reports");
        let p = write(&root, Kind::Interface, "2.52.6", "panic at src/app/mod.rs:1: boom").unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(folder(&root)).unwrap().permissions().mode() & 0o777, 0o700);
        write(&root, Kind::PageMemory, "", "").unwrap();
        let all = list(&root);
        assert_eq!(all.len(), 2);
        let first = all.iter().find(|r| r.kind == Kind::Interface).unwrap();
        assert!(first.text.contains("boom") && first.text.contains("WebKitGTK: 2.52.6") && first.text.contains(env!("CARGO_PKG_VERSION")));
        let url = issue_url(first);
        assert!(url.starts_with("https://github.com/grassfieldhq/sanaga/issues/new?title=") && url.contains("boom") && !url.contains(' '));
        // a path outside the folder is never deleted
        let other = root.join("keep.txt");
        std::fs::write(&other, "x").unwrap();
        remove(&root, &other);
        assert!(other.exists());
        remove(&root, &first.path);
        assert_eq!(list(&root).len(), 1);
        // only the newest stay
        for _ in 0..KEEP + 5 {
            write(&root, Kind::Page, "", "x").unwrap();
        }
        assert_eq!(list(&root).len(), KEEP);
        remove_all(&root);
        assert!(list(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_abrupt_end_is_noticed_once() {
        let dir = temp("running");
        // a first start: nothing before
        assert!(mark_running(&dir).is_none());
        // (the note says which start of the computer and which session)
        let note = std::fs::read_to_string(dir.join("running")).unwrap();
        assert!(note.starts_with(&format!("{}\nboot ", std::process::id())) && note.contains("\nsession "));
        // closing as it should removes the note
        mark_stopped();
        assert!(!dir.join("running").exists());
        // a note left by a process that is gone: the last run ended abruptly
        std::fs::write(dir.join("running"), "4194999").unwrap();
        assert_eq!(mark_running(&dir).map(|a| a.why), Some(Ended::Unknown));
        // (the note is now this run's: said once)
        assert!(mark_running(&dir).is_none());
        mark_stopped();
        // a note of a Sanaga that still runs is not a crash, and stays
        std::fs::write(dir.join("running"), "1").unwrap();
        assert!(mark_running(&dir).is_none());
        assert_eq!(std::fs::read_to_string(dir.join("running")).unwrap(), "1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_note_says_how_its_run_ended() {
        // a signal was noted: that is the cause, whatever else changed
        assert_eq!(read_note("4194999\nboot abc\nsession /nowhere/wayland-1 5 6\nsignal 11\nsignal 06\n", "abc"), (4194999, Ended::Signal(11)));
        // the computer was started again since
        assert_eq!(read_note("4194999\nboot abc\nsession \n", "def").1, Ended::Restart);
        // the same start, but the session's socket is gone
        assert_eq!(read_note("4194999\nboot abc\nsession /nowhere/sanaga test/wayland-1 5 6\n", "abc").1, Ended::Session);
        // a session that is still there: nothing more is known
        let here = temp("session");
        let socket = here.join("wayland 1");
        std::fs::write(&socket, "").unwrap();
        let mark = socket_mark(&socket).unwrap();
        assert!(session_alive(&mark));
        assert_eq!(read_note(&format!("4194999\nboot abc\nsession {mark}\n"), "abc").1, Ended::Unknown);
        // (the same path made again is another session)
        std::fs::remove_file(&socket).unwrap();
        assert!(!session_alive(&mark));
        // a note of an older Sanaga (the process number alone)
        assert_eq!(read_note("4194999", "abc"), (4194999, Ended::Unknown));
        // the report's lines
        let a = Abrupt { started: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_794_422_402), why: Ended::Signal(11) };
        assert_eq!(a.detail(), "That run started: 2026-11-11 18:40:02 UTC\nEnded by: signal 11 (SIGSEGV) (a crash in Sanaga or in a system library)");
        let _ = std::fs::remove_dir_all(&here);
    }
}

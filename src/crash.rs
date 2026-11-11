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
            let mut text = String::new();
            // (never more than a report can hold, whatever the file became)
            std::fs::File::open(&path).ok()?.take(MAX_TEXT as u64 + 64).read_to_string(&mut text).ok()?;
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

/// Note that Sanaga runs with this profile. When the note of an earlier run
/// is still there and that run is gone, it ended abruptly: returns when it
/// had started.
pub fn mark_running(profile_dir: &Path) -> Option<std::time::SystemTime> {
    let note = profile_dir.join("running");
    let started = std::fs::metadata(&note).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
    let before = std::fs::read_to_string(&note).ok().map(|t| t.trim().parse::<u32>().unwrap_or(0));
    let abrupt = before.is_some_and(|pid| pid != std::process::id() && (!alive(pid) || !Path::new("/proc").is_dir()));
    // (another Sanaga still runs with this profile: its note stays)
    if before.is_none() || abrupt {
        let _ = std::fs::write(&note, std::process::id().to_string());
        if let Ok(mut notes) = NOTES.lock() {
            notes.push(note);
        }
    }
    abrupt.then_some(started)
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
        // closing as it should removes the note
        mark_stopped();
        assert!(!dir.join("running").exists());
        // a note left by a process that is gone: the last run ended abruptly
        std::fs::write(dir.join("running"), "4194999").unwrap();
        assert!(mark_running(&dir).is_some());
        // (the note is now this run's: said once)
        assert!(mark_running(&dir).is_none());
        mark_stopped();
        // a note of a Sanaga that still runs is not a crash, and stays
        std::fs::write(dir.join("running"), "1").unwrap();
        assert!(mark_running(&dir).is_none());
        assert_eq!(std::fs::read_to_string(dir.join("running")).unwrap(), "1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

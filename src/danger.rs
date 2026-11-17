//! Sites reported as phishing (a copy of a bank's or a mail service's page,
//! made to take passwords) or as spreading malware: Sanaga warns before
//! showing one.
//!
//! The lists are fetched from their authors once a day, like the block
//! lists: each computer has its own copy, they are not part of Sanaga.
//! Everything is decided on this computer — the address of a page is never
//! sent anywhere to be checked.
//!
//! - phishing: malware-filter's "phishing-filter" (from OpenPhish, PhishTank
//!   and IPThreat; CC BY-SA 4.0);
//! - malware: malware-filter's "urlhaus-filter" (from abuse.ch's URLhaus; CC0).
//!
//! Both leave out the well-known sites, so that a whole hosting service is
//! not refused for one bad page.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// The lists, as their authors publish them: one site per line.
pub const SOURCES: [(&str, &str); 2] =
    [("phishing", "https://malware-filter.gitlab.io/malware-filter/phishing-filter-domains.txt"), ("malware", "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-domains.txt")];

/// A list is fetched again after a day (reported sites change by the hour;
/// their authors publish twice a day).
const KEEP_SECS: u64 = 86_400;

/// The setting, and the sites listed (as numbers: see `key`), sorted.
static ON: AtomicBool = AtomicBool::new(false);
static LIST: RwLock<Option<Arc<Vec<u64>>>> = RwLock::new(None);

/// A site's name as one number (FNV-1a): 40 000 names take 320 KB and are
/// looked up in a few steps. (Two names with the same number: one chance in
/// millions of millions.)
fn key(host: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in host.bytes() {
        h ^= b.to_ascii_lowercase() as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The site named by one line of a list: a bare name or number address, or
/// the blockers' `||name^`. Comments and anything else: nothing.
fn site(line: &str) -> Option<&str> {
    let l = line.trim();
    let l = l.strip_prefix("||").map(|r| r.trim_end_matches('^')).unwrap_or(l);
    let plain = !l.is_empty() && l.len() <= 253 && l.contains('.') && !l.starts_with('.') && !l.ends_with('.') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_');
    plain.then_some(l)
}

/// The sites of a list's text, ready to be looked up.
pub fn read(text: &str) -> Vec<u64> {
    let mut list: Vec<u64> = text.lines().filter_map(site).map(key).collect();
    list.sort_unstable();
    list.dedup();
    list
}

/// Is this site in the list — itself, or the site it is a part of
/// (`login.bad.example` when `bad.example` is listed)? A number address
/// only by itself.
pub fn listed_in(list: &[u64], host: &str) -> bool {
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return false;
    }
    let has = |name: &str| list.binary_search(&key(name)).is_ok();
    if has(host) {
        return true;
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() || host.contains(':') {
        return false;
    }
    // (down to a name of two parts: never a whole ending like `.com`)
    let mut rest = host;
    while let Some((_, parent)) = rest.split_once('.') {
        if !parent.contains('.') {
            break;
        }
        if has(parent) {
            return true;
        }
        rest = parent;
    }
    false
}

/// The setting ("Warn before dangerous sites").
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Should Sanaga warn before showing this site?
pub fn listed(host: &str) -> bool {
    if !ON.load(Ordering::Relaxed) {
        return false;
    }
    let list = LIST.read().ok().and_then(|l| l.clone());
    list.is_some_and(|l| listed_in(&l, host))
}

fn file(dir: &Path) -> PathBuf {
    dir.join("danger.txt")
}

fn keep(text: &str) {
    if let Ok(mut l) = LIST.write() {
        *l = Some(Arc::new(read(text)));
    }
}

/// Read the list kept in `dir` and fetch a new one when it is a day old (or
/// not there yet), away from the window. A fetch that fails changes nothing:
/// the list kept goes on being used.
pub fn prepare(dir: &Path) {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if !ON.load(Ordering::Relaxed) || BUSY.swap(true, Ordering::Relaxed) {
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        // (test aid, debug builds only: a list of this computer, never fetched)
        #[cfg(debug_assertions)]
        if let Some(text) = std::env::var_os("SANAGA_DANGER_LIST").and_then(|p| std::fs::read_to_string(p).ok()) {
            keep(&text);
            BUSY.store(false, Ordering::Relaxed);
            return;
        }
        let path = file(&dir);
        let loaded = LIST.read().is_ok_and(|l| l.is_some());
        if !loaded {
            if let Ok(text) = std::fs::read_to_string(&path) {
                keep(&text);
            }
        }
        let age = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|a| a.as_secs());
        if age.is_none_or(|a| a > KEEP_SECS) {
            match fetch() {
                Ok(text) => {
                    let _ = std::fs::create_dir_all(&dir);
                    if crate::store::write_atomic(&path, &text).is_ok() {
                        keep(&text);
                    }
                }
                Err(e) => {
                    if std::env::var_os("SANAGA_DEBUG_LISTS").is_some() {
                        eprintln!("dangerous sites: not updated ({e})");
                    }
                }
            }
        }
        BUSY.store(false, Ordering::Relaxed);
    });
}

/// The lists as one text (blocking: on a thread).
fn fetch() -> Result<String, String> {
    use std::io::Read;
    let agent = crate::net::builder().timeout_connect(std::time::Duration::from_secs(10)).timeout_read(std::time::Duration::from_secs(40)).user_agent("Mozilla/5.0").build();
    let mut all = String::new();
    for (name, url) in SOURCES {
        let mut text = String::new();
        agent.get(url).call().map_err(|e| format!("{name}: {e}"))?.into_reader().take(16 << 20).read_to_string(&mut text).map_err(|e| format!("{name}: {e}"))?;
        // (an error page or an empty answer is not a list)
        if text.lines().filter_map(site).count() < 500 {
            return Err(format!("{name}: liste illisible"));
        }
        all.push_str(&text);
        all.push('\n');
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listed_site_and_its_parts_are_found() {
        let list = read("# Title: a list\n! another comment\nbad.example\n||worse.example^\n203.0.113.7\nlogin-bank.pages.example\n\nnot a name\nhttp://x.example/path\n.com\n");
        assert_eq!(list.len(), 4);
        for bad in ["bad.example", "BAD.Example", "bad.example.", "www.bad.example", "a.b.bad.example", "worse.example", "203.0.113.7", "login-bank.pages.example"] {
            assert!(listed_in(&list, bad), "{bad}");
        }
        // the hosting service of a listed page, another site, part of a number address
        for fine in ["pages.example", "other.pages.example", "example", "notbad.example", "bad.example.org", "7.203.0.113.7", "203.0.113.70", "", "localhost"] {
            assert!(!listed_in(&list, fine), "{fine}");
        }
    }

    #[test]
    fn nothing_is_listed_while_the_setting_is_off() {
        // (the list in memory is shared: only the switch is tried here)
        set_on(false);
        assert!(!listed("bad.example"));
    }
}

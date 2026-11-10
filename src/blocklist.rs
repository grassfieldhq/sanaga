//! The big block lists (EasyList, EasyPrivacy): fetched from their authors
//! once a week, turned into WebKit's content-blocker rules on this computer,
//! and compiled by WebKit (`page.rs`) — the requests they stop never leave.
//! The lists are not part of Sanaga: each computer fetches its own copy.
//!
//! Only what WebKit's blocker can do safely is kept: network rules, their
//! exceptions, resource types, first / third party and per-site rules. A
//! rule that can't be said exactly (a regular expression, a redirect, a
//! header change…) is left out rather than approximated: a wrong rule
//! breaks sites, and one rule WebKit refuses would lose the whole list.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Where the lists come from.
pub const SOURCES: [(&str, &str); 2] = [("easylist", "https://easylist.to/easylist/easylist.txt"), ("easyprivacy", "https://easylist.to/easylist/easyprivacy.txt")];
/// Fetched again after a week.
const KEEP_SECS: u64 = 7 * 86_400;
/// Rules per compiled list (WebKit's own limit was 50 000 for a long time).
const CHUNK: usize = 40_000;
/// The compiled lists are called sanaga-lists-0, -1… (at most this many).
pub const MAX_LISTS: usize = 8;

pub fn list_id(k: usize) -> String {
    format!("sanaga-lists-{k}")
}

/// What WebKit loads when a rule names no type: everything but the page
/// itself (a rule must never stop the person from opening a site).
const TYPES: [&str; 7] = ["image", "style-sheet", "script", "font", "raw", "svg-document", "media"];

#[derive(Default)]
pub struct Rules {
    pub blocks: Vec<String>,
    pub exceptions: Vec<String>,
    /// Rules left out (not network rules, or not sayable exactly).
    pub skipped: usize,
    seen: HashSet<String>,
}

/// A filter's pattern as one of WebKit's regular expressions (None: can't be said).
fn pattern_regex(pattern: &str) -> Option<String> {
    let mut p = pattern.trim_matches('*');
    if p.is_empty() || !p.is_ascii() || p.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // a filter written as a regular expression: WebKit's are not the same
    if p.len() > 1 && p.starts_with('/') && p.ends_with('/') {
        return None;
    }
    let mut out = String::new();
    if let Some(rest) = p.strip_prefix("||") {
        out.push_str("^[^:]+://+([^:/]+\\.)?");
        p = rest;
    } else if let Some(rest) = p.strip_prefix('|') {
        out.push('^');
        p = rest;
    }
    let end = p.ends_with('|');
    if end {
        p = &p[..p.len() - 1];
    }
    if p.is_empty() || p.contains('|') {
        return None;
    }
    let chars: Vec<char> = p.chars().collect();
    let mut closed = false;
    for (i, c) in chars.iter().enumerate() {
        match c {
            '*' => {
                if !out.ends_with(".*") {
                    out.push_str(".*");
                }
            }
            // "^": a separator (anything but a letter, a digit or _ - . %), or the address's end
            '^' if i + 1 == chars.len() => {
                out.push_str("([^a-zA-Z0-9_.%-].*)?$");
                closed = true;
            }
            '^' => out.push_str("[^a-zA-Z0-9_.%-]"),
            '.' | '+' | '?' | '$' | '{' | '}' | '(' | ')' | '[' | ']' | '\\' => {
                out.push('\\');
                out.push(*c);
            }
            c => out.push(*c),
        }
    }
    if end && !closed {
        out.push('$');
    }
    Some(out)
}

fn json_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn json_list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| json_text(s)).collect::<Vec<_>>().join(","))
}

/// One line of a list as a WebKit rule: (is an exception, the rule's JSON).
fn rule(line: &str) -> Option<(bool, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('!') || line.starts_with('[') || line.contains("##") || line.contains("#@#") || line.contains("#?#") || line.contains("#$#") || line.contains("#%#") {
        return None;
    }
    let (exception, body) = match line.strip_prefix("@@") {
        Some(b) => (true, b),
        None => (false, line),
    };
    // options come after the last "$" (when what follows reads as options)
    let (pattern, options) = match body.rfind('$') {
        Some(i) if !body[i + 1..].is_empty() && body[i + 1..].chars().all(|c| c.is_ascii_alphanumeric() || "~_=,.|-*".contains(c)) => (&body[..i], &body[i + 1..]),
        _ => (body, ""),
    };
    let filter = pattern_regex(pattern)?;
    let (mut types, mut not_types): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let (mut party, mut case, mut frames_only) = (None::<&str>, false, false);
    let (mut if_domain, mut unless_domain): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for opt in options.split(',').filter(|o| !o.is_empty()) {
        let (negated, name) = match opt.strip_prefix('~') {
            Some(n) => (true, n),
            None => (false, opt),
        };
        let kind = match name {
            "script" => Some("script"),
            "image" => Some("image"),
            "stylesheet" | "css" => Some("style-sheet"),
            "font" => Some("font"),
            "media" => Some("media"),
            "xmlhttprequest" | "xhr" => Some("raw"),
            "subdocument" | "frame" => Some("document"),
            _ => None,
        };
        if let Some(k) = kind {
            if negated {
                not_types.push(k);
            } else {
                types.push(k);
                frames_only |= k == "document";
            }
            continue;
        }
        match name {
            "third-party" | "3p" => party = Some(if negated { "first-party" } else { "third-party" }),
            "first-party" | "1p" => party = Some(if negated { "third-party" } else { "first-party" }),
            "match-case" if !negated => case = true,
            "important" | "all" if !negated => {}
            // kinds of loads WebKit's blocker doesn't tell apart: without
            // them the rule says less than it should when they are its only kinds
            "object" | "other" | "ping" | "websocket" | "beacon" | "object-subrequest" if !negated => types.push(""),
            "object" | "other" | "ping" | "websocket" | "beacon" | "object-subrequest" => {}
            _ => {
                let (key, value) = name.split_once('=')?;
                if negated || (key != "domain" && key != "from") {
                    return None; // redirect, csp, removeparam, popup, document, generichide…
                }
                for d in value.split('|').filter(|d| !d.is_empty()) {
                    let (list, d) = match d.strip_prefix('~') {
                        Some(d) => (&mut unless_domain, d),
                        None => (&mut if_domain, d),
                    };
                    if !d.is_ascii() || d.contains('*') || d.contains('/') {
                        return None;
                    }
                    list.push(format!("*{}", d.to_ascii_lowercase()));
                }
            }
        }
    }
    // (WebKit takes "only on these sites" or "not on these sites", not both)
    if !if_domain.is_empty() && !unless_domain.is_empty() {
        return None;
    }
    let kinds: Vec<String> = if types.is_empty() {
        TYPES.iter().filter(|t| !not_types.contains(t)).map(|t| t.to_string()).collect()
    } else {
        let mut k: Vec<String> = types.iter().filter(|t| !t.is_empty()).map(|t| t.to_string()).collect();
        k.dedup();
        k
    };
    if kinds.is_empty() {
        return None;
    }
    // frames: only those of other sites (a rule never stops opening a site itself)
    let party = if frames_only && !exception { Some(party.unwrap_or("third-party")) } else { party };
    if frames_only && party == Some("first-party") {
        return None;
    }
    let mut trigger = format!("\"url-filter\":{}", json_text(&filter));
    if case {
        trigger.push_str(",\"url-filter-is-case-sensitive\":true");
    }
    trigger.push_str(&format!(",\"resource-type\":{}", json_list(&kinds)));
    if let Some(p) = party {
        trigger.push_str(&format!(",\"load-type\":[\"{p}\"]"));
    }
    if !if_domain.is_empty() {
        trigger.push_str(&format!(",\"if-domain\":{}", json_list(&if_domain)));
    }
    if !unless_domain.is_empty() {
        trigger.push_str(&format!(",\"unless-domain\":{}", json_list(&unless_domain)));
    }
    let action = if exception { "ignore-previous-rules" } else { "block" };
    Some((exception, format!("{{\"trigger\":{{{trigger}}},\"action\":{{\"type\":\"{action}\"}}}}")))
}

/// Add a list's rules.
pub fn convert(text: &str, rules: &mut Rules) {
    for line in text.lines() {
        match rule(line) {
            Some((exception, json)) => {
                if rules.seen.insert(json.clone()) {
                    if exception { &mut rules.exceptions } else { &mut rules.blocks }.push(json);
                }
            }
            None => {
                let l = line.trim();
                if !l.is_empty() && !l.starts_with('!') && !l.starts_with('[') {
                    rules.skipped += 1;
                }
            }
        }
    }
}

/// The rules as WebKit rule lists (JSON): the blocking rules in parts, each
/// followed by every exception (an exception only undoes rules of its own list).
pub fn lists(rules: &Rules) -> Vec<String> {
    rules
        .blocks
        .chunks(CHUNK)
        .take(MAX_LISTS)
        .map(|part| {
            let all: Vec<&str> = part.iter().chain(rules.exceptions.iter()).map(|s| s.as_str()).collect();
            format!("[{}]", all.join(","))
        })
        .collect()
}

fn folder(dir: &Path) -> PathBuf {
    dir.join("listes")
}

fn files(dir: &Path) -> Vec<PathBuf> {
    (0..MAX_LISTS).map(|k| folder(dir).join(format!("{}.json", list_id(k)))).take_while(|p| p.exists()).collect()
}

/// The lists converted earlier, and whether they are due for a new fetch.
pub fn kept(dir: &Path) -> (Vec<PathBuf>, bool) {
    let list = files(dir);
    let age = list.first().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|a| a.as_secs());
    let stale = age.is_none_or(|a| a > KEEP_SECS);
    (list, stale)
}

/// Fetch the lists and convert them (blocking: call on a thread). Returns
/// the rule-list files and how many rules they hold, or why it failed (the
/// lists kept from last time stay in use).
pub fn update(dir: &Path) -> Result<(Vec<PathBuf>, usize), String> {
    // (like every request Sanaga makes itself: the system's certificate authorities)
    let agent = crate::net::builder().timeout_connect(std::time::Duration::from_secs(10)).timeout_read(std::time::Duration::from_secs(40)).user_agent("Mozilla/5.0").build();
    let mut rules = Rules::default();
    for (name, url) in SOURCES {
        let mut text = String::new();
        use std::io::Read;
        agent.get(url).call().map_err(|e| format!("{name}: {e}"))?.into_reader().take(32 << 20).read_to_string(&mut text).map_err(|e| format!("{name}: {e}"))?;
        // (an error page or an empty answer is not a list)
        if text.lines().filter(|l| l.starts_with("||")).count() < 1000 {
            return Err(format!("{name}: liste illisible"));
        }
        convert(&text, &mut rules);
    }
    let parts = lists(&rules);
    let out = folder(dir);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    for (k, json) in parts.iter().enumerate() {
        let p = out.join(format!("{}.json", list_id(k)));
        crate::store::write_atomic(&p, json).map_err(|e| e.to_string())?;
        written.push(p);
    }
    for k in parts.len()..MAX_LISTS {
        let _ = std::fs::remove_file(out.join(format!("{}.json", list_id(k))));
    }
    Ok((written, rules.blocks.len() + rules.exceptions.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Option<String> {
        rule(line).map(|r| r.1)
    }

    #[test]
    fn filters_become_webkit_rules() {
        let r = one("||ads.example.com^$third-party").unwrap();
        assert!(r.contains(r#""url-filter":"^[^:]+://+([^:/]+\\.)?ads\\.example\\.com([^a-zA-Z0-9_.%-].*)?$""#), "{r}");
        assert!(r.contains(r#""load-type":["third-party"]"#) && r.contains(r#""type":"block""#));
        // no type named: everything but the page itself
        assert!(r.contains(r#""resource-type":["image","style-sheet","script","font","raw","svg-document","media"]"#));
        let r = one("/banner/*/img^$script,image,domain=a.com|b.org").unwrap();
        assert!(r.contains(r#""url-filter":"/banner/.*/img([^a-zA-Z0-9_.%-].*)?$""#), "{r}");
        assert!(r.contains(r#""resource-type":["script","image"]"#) && r.contains(r#""if-domain":["*a.com","*b.org"]"#));
        let r = one("|https://x.example/a.js|$~image,match-case").unwrap();
        assert!(r.contains(r#""url-filter":"^https://x\\.example/a\\.js$""#) && r.contains("case-sensitive") && !r.contains("\"image\""));
        // exceptions undo earlier rules
        let (ex, r) = rule("@@||cdn.example.com/lib.js$script,domain=~bad.example").unwrap();
        assert!(ex && r.contains("ignore-previous-rules") && r.contains(r#""unless-domain":["*bad.example"]"#));
        // frames of other sites only: a rule never stops opening a site itself
        let r = one("||tracker.example^$subdocument").unwrap();
        assert!(r.contains(r#""resource-type":["document"]"#) && r.contains("third-party"));
    }

    #[test]
    fn what_cannot_be_said_exactly_is_left_out() {
        for line in [
            "! a comment",
            "[Adblock Plus 2.0]",
            "example.com##.ad",
            "#@#.ad",
            "/ads[0-9]+\\.js/",
            "||a.example^$redirect=noop.js",
            "||a.example^$csp=script-src 'none'",
            "||a.example^$removeparam=utm",
            "||a.example^$popup",
            "||a.example^$document",
            "@@||a.example^$generichide",
            "||a.example^$domain=a.com|~b.com",
            "||a.example^$domain=shop.*",
            "||bücher.example^",
            "$script,third-party",
            "||a.example^$websocket",
            "*",
            "",
        ] {
            assert!(rule(line).is_none(), "{line}");
        }
        // the result is JSON WebKit can read, in parts that each end with the exceptions
        let mut rules = Rules::default();
        convert("||a.example^\n||a.example^\n@@||a.example/ok.js\n||b.example^$script\nexample.com##.ad\n", &mut rules);
        assert_eq!((rules.blocks.len(), rules.exceptions.len(), rules.skipped), (2, 1, 1));
        let parts = lists(&rules);
        assert_eq!(parts.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&parts[0]).unwrap();
        let list = v.as_array().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[2]["action"]["type"], "ignore-previous-rules");
    }
}

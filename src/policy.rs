//! Settings fixed by the administrator of the computer, for the places
//! where Sanaga is installed for others (an office, a school): a text file
//! only the administrator can write, `/etc/sanaga/policy.txt`. What it
//! fixes cannot be changed in Sanaga, and Sanaga says so.
//!
//! ```text
//! # no AI at all
//! ai=off
//! # or: never the text of these sites (and their sub-sites) to a model
//! ai_no_sites=intranet.example.org, bank.example
//! ```
//!
//! It is read once, at start. A file that is missing, empty or unreadable
//! fixes nothing.

use std::sync::OnceLock;

const FILE: &str = "/etc/sanaga/policy.txt";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Policy {
    /// No AI model is ever used (summaries are Sanaga's own extract).
    pub ai_off: bool,
    /// Sites whose pages never go to a model.
    pub ai_no_sites: Vec<String>,
}

impl Policy {
    /// May the page at this address go to a model?
    pub fn ai_allowed_on(&self, address: &str) -> bool {
        if self.ai_off {
            return false;
        }
        let Some(host) = url::Url::parse(address).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_ascii_lowercase())) else { return true };
        !self.ai_no_sites.iter().any(|site| host == *site || host.ends_with(&format!(".{site}")))
    }
}

/// The file's lines: `key=value`, `#` comments; what is not understood is
/// passed over (a later Sanaga may know more keys).
pub fn parse(text: &str) -> Policy {
    let mut p = Policy::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "ai" => p.ai_off = matches!(value.to_ascii_lowercase().as_str(), "off" | "no" | "0" | "false"),
            "ai_no_sites" => {
                for site in value.split([',', ' ']).map(|s| s.trim().trim_start_matches("*.").trim_start_matches("www.").trim_end_matches('/').to_ascii_lowercase()).filter(|s| !s.is_empty()) {
                    // (a name, not an address: "example.org", not "https://example.org/x")
                    let site = site.split_once("://").map(|x| x.1.to_string()).unwrap_or(site);
                    let site = site.split('/').next().unwrap_or("").to_string();
                    if !site.is_empty() && !p.ai_no_sites.contains(&site) {
                        p.ai_no_sites.push(site);
                    }
                }
            }
            _ => {}
        }
    }
    p
}

/// What the administrator fixed (read once).
pub fn get() -> &'static Policy {
    static POLICY: OnceLock<Policy> = OnceLock::new();
    POLICY.get_or_init(|| {
        // (a debug build can be given another file, to try it: docs/TESTING.md)
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("SANAGA_POLICY") {
            return std::fs::read_to_string(path).map(|t| parse(&t)).unwrap_or_default();
        }
        // (bounded: a huge file is not a policy)
        match std::fs::metadata(FILE) {
            Ok(m) if m.is_file() && m.len() <= 64 << 10 => std::fs::read_to_string(FILE).map(|t| parse(&t)).unwrap_or_default(),
            _ => Policy::default(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_read_and_what_it_fixes_holds() {
        let p = parse("# Sanaga\n\nai = on\nai_no_sites= Intranet.Example.org , *.bank.example https://wiki.example/x www.docs.example/\nunknown=1\nno equals sign\n");
        assert_eq!(p, Policy { ai_off: false, ai_no_sites: vec!["intranet.example.org".into(), "bank.example".into(), "wiki.example".into(), "docs.example".into()] });
        assert!(!p.ai_allowed_on("https://intranet.example.org/page"));
        assert!(!p.ai_allowed_on("https://www.bank.example/"));
        assert!(!p.ai_allowed_on("https://a.b.bank.example/x?y"));
        assert!(p.ai_allowed_on("https://notbank.example/"));
        assert!(p.ai_allowed_on("https://bank.example.evil.test/"));
        assert!(p.ai_allowed_on("https://example.org/"));
        assert!(p.ai_allowed_on("file:///home/me/a.html"));
        // no AI at all
        let off = parse("ai=off");
        assert!(off.ai_off && !off.ai_allowed_on("https://example.org/"));
        assert!(parse("ai=OFF\n").ai_off && parse("ai=0").ai_off && !parse("ai=").ai_off);
        // nothing fixed
        assert_eq!(parse(""), Policy::default());
        assert_eq!(parse("# only a comment\n"), Policy::default());
        crate::mutate::run(&[b"ai=off\nai_no_sites=a.example, *.b.example https://c.example/x\n"], 10_000, |d| {
            let p = parse(&String::from_utf8_lossy(d));
            let _ = p.ai_allowed_on(&String::from_utf8_lossy(d));
        });
    }
}

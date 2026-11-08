//! Interface languages. French is written in the code; the other languages are
//! tables in `assets/i18n/<code>.tsv` (one line per text: French, then the
//! translation, separated by a tab). A missing translation shows the English
//! one, then the French one.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// (code, name in its own language)
pub const LANGUAGES: [(&str, &str); 5] = [("fr", "Français"), ("en", "English"), ("es", "Español"), ("pt", "Português"), ("de", "Deutsch")];

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static TABLES: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();

fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('\t') {
            if !v.trim().is_empty() {
                // "\n" in the file stands for a line break
                let k: &'static str = if k.contains("\\n") { Box::leak(k.replace("\\n", "\n").into_boxed_str()) } else { k };
                let v: &'static str = if v.contains("\\n") { Box::leak(v.replace("\\n", "\n").into_boxed_str()) } else { v };
                m.insert(k, v);
            }
        }
    }
    m
}

fn tables() -> &'static Vec<HashMap<&'static str, &'static str>> {
    TABLES.get_or_init(|| {
        vec![
            HashMap::new(),
            parse(include_str!("../assets/i18n/en.tsv")),
            parse(include_str!("../assets/i18n/es.tsv")),
            parse(include_str!("../assets/i18n/pt.tsv")),
            parse(include_str!("../assets/i18n/de.tsv")),
        ]
    })
}

/// The language of the computer, if Sanaga speaks it (else English).
pub fn system_language() -> &'static str {
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            for part in v.split(':') {
                let code = part.split(['_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
                if let Some((c, _)) = LANGUAGES.iter().find(|(c, _)| *c == code) {
                    return c;
                }
            }
        }
    }
    "en"
}

pub fn set_language(code: &str) {
    let i = LANGUAGES.iter().position(|(c, _)| *c == code).unwrap_or(1);
    CURRENT.store(i, Ordering::Relaxed);
}

pub fn language() -> &'static str {
    LANGUAGES[CURRENT.load(Ordering::Relaxed)].0
}

/// Translate an interface text (written in French in the code).
pub fn tr(s: &str) -> &str {
    let i = CURRENT.load(Ordering::Relaxed);
    if i == 0 {
        return s;
    }
    let t = tables();
    t[i].get(s).or_else(|| t[1].get(s)).copied().unwrap_or(s)
}

/// Translate a text with `{}` slots, then fill them in order.
pub fn tf(template: &str, args: &[&dyn Display]) -> String {
    let t = tr(template);
    let mut out = String::with_capacity(t.len() + 16);
    let mut it = args.iter();
    let mut rest = t;
    while let Some(p) = rest.find("{}") {
        out.push_str(&rest[..p]);
        if let Some(a) = it.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[p + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_load_and_fill() {
        set_language("en");
        assert_eq!(tr("Nouvel onglet"), "New tab");
        assert_eq!(tf("Groupe {}", &[&3]), "Group 3");
        set_language("fr");
        assert_eq!(tr("Nouvel onglet"), "Nouvel onglet");
        // every table keeps the same number of slots as the French text
        for t in tables().iter().skip(1) {
            for (k, v) in t {
                assert_eq!(k.matches("{}").count(), v.matches("{}").count(), "{k} / {v}");
            }
        }
    }
}

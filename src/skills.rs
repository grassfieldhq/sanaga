//! Saved requests for the assistant ("skills"): a request the person asks
//! often, kept under a name and called with `/name` in the assistant's
//! field — `/explain`, `/compare`, or their own. What is typed after the
//! name goes with it ("/explain the second paragraph").
//!
//! A saved request is a text file in the profile's `skills` folder, like
//! an extension is a file: it can be read, edited, copied to another
//! computer or removed with any tool.
//!
//! ```text
//! ---
//! description: What it does, in a few words
//! page: yes
//! ---
//! The request, as it is sent to the model.
//! ```
//!
//! A few are provided; a file of the same name takes their place.

use crate::i18n::tr;
use std::path::Path;

/// What a summary asks the model (the provided "/summarise").
pub const SUMMARY_ASK: &str = "Summarise this page: one sentence that says what it is about, then 3 to 6 short bullet points with the key facts (start each with \"• \"). No preamble, no headings.";

/// The biggest file read as a saved request, and how many are read.
const MAX_FILE: u64 = 64 << 10;
const MAX_SKILLS: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    /// What is typed after "/".
    pub name: String,
    pub description: String,
    /// The page shown goes with it (when the model has not been given it yet).
    pub page: bool,
    /// The request sent to the model.
    pub prompt: String,
    /// A file of the person's (not one of the provided ones).
    pub own: bool,
    /// The provided summary: Sanaga's own extract stands in when no model can answer.
    pub summary: bool,
}

/// The requests Sanaga provides, named in the interface's language.
pub fn provided() -> Vec<Skill> {
    let one =
        |name: &str, description: &str, prompt: &str, summary: bool| Skill { name: slug(name), description: description.to_string(), page: true, prompt: prompt.to_string(), own: false, summary };
    vec![
        one(tr("résumer"), tr("L'essentiel de la page en quelques points"), SUMMARY_ASK, true),
        one(
            tr("expliquer"),
            tr("La page, ou la sélection, dite simplement"),
            "Explain this in plain words, for someone who does not know the subject: what it is, why it matters, and the two or three terms one needs to understand it. \
             If a selection is attached, explain the selection; otherwise the page. Short paragraphs, no jargon left unexplained.",
            false,
        ),
        one(
            tr("comparer"),
            tr("Les pages jointes : ce qui les rapproche, ce qui les sépare"),
            "Compare the pages attached: first what they agree on, then where they differ, point by point (start each point with \"• \" and name the page it comes from), \
             then one sentence on which suits what. If only one page is attached, say so and ask which other tab to attach with @.",
            false,
        ),
        one(
            tr("à-faire"),
            tr("Ce que la page demande de faire : étapes, dates, montants"),
            "List what this page asks its reader to do: each step or action as a \"• \" line, with its deadline, amount or condition when the page gives one, in the order to do them. \
             Leave out what is only information. If the page asks for nothing, say so in one sentence.",
            false,
        ),
    ]
}

/// A name as it is typed after "/": lower case, letters and digits of any
/// alphabet, "-" between words.
pub fn slug(name: &str) -> String {
    let s: String = name.trim().trim_start_matches('/').to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(40).collect()
}

/// A saved request read from its file's text.
pub fn parse(name: &str, text: &str) -> Skill {
    let mut skill = Skill { name: slug(name), description: String::new(), page: true, prompt: String::new(), own: true, summary: false };
    let text = text.trim_start_matches('\u{feff}');
    let body = match text.strip_prefix("---").and_then(|rest| rest.split_once("\n---")) {
        Some((head, body)) => {
            for line in head.lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                let value = value.trim();
                match key.trim().to_lowercase().as_str() {
                    "description" => skill.description = value.chars().take(200).collect(),
                    "page" => skill.page = !matches!(value.to_lowercase().as_str(), "no" | "non" | "false" | "0"),
                    _ => {}
                }
            }
            body
        }
        // (a file with no heading is its request)
        None => text,
    };
    skill.prompt = body.trim().to_string();
    skill
}

/// A saved request as its file's text.
pub fn format(skill: &Skill) -> String {
    let one_line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("---\ndescription: {}\npage: {}\n---\n{}\n", one_line(&skill.description), if skill.page { "yes" } else { "no" }, skill.prompt.trim())
}

/// The person's saved requests (the files of their folder), by name.
pub fn load(dir: &Path) -> Vec<Skill> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten().take(MAX_SKILLS * 4) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") || !entry.metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE) {
            continue;
        }
        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let skill = parse(&name, &text);
        if !skill.name.is_empty() && !skill.prompt.is_empty() {
            out.push(skill);
        }
        if out.len() >= MAX_SKILLS {
            break;
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every saved request: the person's, then the provided ones a file of
/// theirs has not replaced.
pub fn all(dir: Option<&Path>) -> Vec<Skill> {
    let mut out = dir.map(load).unwrap_or_default();
    for p in provided() {
        if !out.iter().any(|s| s.name == p.name) {
            out.push(p);
        }
    }
    out
}

/// Keep a saved request in the folder (made if need be), in place of the
/// one of the same name.
pub fn save(dir: &Path, skill: &Skill) -> std::io::Result<()> {
    let name = slug(&skill.name);
    if name.is_empty() || skill.prompt.trim().is_empty() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    std::fs::create_dir_all(dir)?;
    crate::store::write_atomic(&dir.join(format!("{name}.md")), format(skill))
}

pub fn delete(dir: &Path, name: &str) -> std::io::Result<()> {
    let name = slug(name);
    if name.is_empty() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    std::fs::remove_file(dir.join(format!("{name}.md")))
}

/// The "/name" a question begins with, and what follows it. `None` when
/// the question does not begin with a slash and a name.
pub fn called(input: &str) -> Option<(String, &str)> {
    let rest = input.trim_start().strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = slug(&rest[..end]);
    (!name.is_empty()).then(|| (name, rest[end..].trim()))
}

/// The saved request a question calls, with what was typed after its
/// name: the one of that name, or the only one whose name begins so
/// ("/expl" for "/explain").
pub fn find<'a, 'b>(skills: &'a [Skill], input: &'b str) -> Option<(&'a Skill, &'b str)> {
    let (name, rest) = called(input)?;
    let exact = skills.iter().find(|s| s.name == name);
    let found = exact.or_else(|| {
        let mut begin = skills.iter().filter(|s| s.name.starts_with(&name));
        begin.next().filter(|_| begin.next().is_none())
    });
    found.map(|s| (s, rest))
}

/// What goes to the model: the request, then what the person added.
pub fn expand(skill: &Skill, rest: &str) -> String {
    if rest.trim().is_empty() {
        skill.prompt.clone()
    } else {
        format!("{}\n\n{}", skill.prompt, rest.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_typed_after_a_slash() {
        assert_eq!(slug(" /Résumé Court! "), "résumé-court");
        assert_eq!(slug("à--faire"), "à-faire");
        assert_eq!(slug("///"), "");
        assert_eq!(called("/expliquer le deuxième paragraphe"), Some(("expliquer".to_string(), "le deuxième paragraphe")));
        assert_eq!(called("  /Comparer"), Some(("comparer".to_string(), "")));
        assert_eq!(called("explique /ceci"), None);
        assert_eq!(called("/ rien"), None);
        assert_eq!(called("1/2 tasse"), None);
    }

    #[test]
    fn a_file_is_read_and_written_back() {
        let s = parse("Mon Avis", "---\ndescription: Un avis   en deux lignes\npage: non\nautre: ignoré\n---\n\nDonne ton avis.\nEn deux lignes.\n");
        assert_eq!(s, Skill { name: "mon-avis".into(), description: "Un avis   en deux lignes".into(), page: false, prompt: "Donne ton avis.\nEn deux lignes.".into(), own: true, summary: false });
        assert_eq!(parse("mon-avis", &format(&s)).prompt, s.prompt);
        assert_eq!(format(&s), "---\ndescription: Un avis en deux lignes\npage: no\n---\nDonne ton avis.\nEn deux lignes.\n");
        // a file with no heading is its request; the page goes with it unless said otherwise
        let plain = parse("court", "Réponds en une phrase.");
        assert_eq!((plain.prompt.as_str(), plain.page, plain.description.as_str()), ("Réponds en une phrase.", true, ""));
        // a heading never closed is part of the request
        assert!(parse("x", "---\ndescription: y\nle reste").prompt.starts_with("---"));
        crate::mutate::run(&["---\ndescription: d\npage: no\n---\nrequest".as_bytes(), b"\xef\xbb\xbf---\n---\n"], 10_000, |d| {
            let s = parse("n", &String::from_utf8_lossy(d));
            let _ = format(&s);
            let _ = called(&String::from_utf8_lossy(d));
        });
    }

    #[test]
    fn the_persons_requests_come_first_and_replace_the_provided_ones() {
        let dir = crate::store::PrivateDir::new("sanaga-skills-test").unwrap();
        let folder = dir.path().join("skills");
        assert_eq!(all(Some(&folder)).len(), provided().len(), "no folder yet: the provided ones");
        let mine = Skill { name: "Avis".into(), description: "d".into(), page: false, prompt: "Donne ton avis.".into(), own: true, summary: false };
        save(&folder, &mine).unwrap();
        // one of the provided names, with the person's own request
        let first = provided()[0].name.clone();
        save(&folder, &Skill { name: first.clone(), prompt: "À ma façon.".into(), ..mine.clone() }).unwrap();
        // what is not a saved request is passed over
        std::fs::write(folder.join("notes.txt"), "x").unwrap();
        std::fs::write(folder.join("vide.md"), "---\ndescription: rien\n---\n").unwrap();
        let skills = all(Some(&folder));
        assert_eq!(skills.len(), provided().len() + 1);
        assert_eq!(skills.iter().filter(|s| s.name == first).map(|s| (s.own, s.prompt.as_str())).collect::<Vec<_>>(), vec![(true, "À ma façon.")]);
        let (found, rest) = find(&skills, "/avis  sur ce point ").unwrap();
        assert_eq!((found.name.as_str(), rest), ("avis", "sur ce point"));
        assert_eq!(expand(found, rest), "Donne ton avis.\n\nsur ce point");
        assert_eq!(expand(found, " "), "Donne ton avis.");
        assert!(find(&skills, "/inconnu").is_none());
        // the beginning of a name, when only one begins so
        assert_eq!(find(&skills, "/av court").map(|(s, rest)| (s.name.as_str(), rest)), Some(("avis", "court")));
        save(&folder, &Skill { name: "avenir".into(), ..mine.clone() }).unwrap();
        let skills = all(Some(&folder));
        assert!(find(&skills, "/av").is_none(), "two begin so: neither is guessed");
        assert_eq!(find(&skills, "/avis").map(|(s, _)| s.name.as_str()), Some("avis"));
        // removed: the provided one is back
        delete(&folder, &first).unwrap();
        assert!(all(Some(&folder)).iter().any(|s| s.name == first && !s.own));
        // a name that is no name, an empty request: not kept
        assert!(save(&folder, &Skill { name: "!!".into(), ..mine.clone() }).is_err());
        assert!(save(&folder, &Skill { prompt: "  ".into(), ..mine }).is_err());
        assert!(delete(&folder, "../settings").is_err(), "a name never leaves the folder");
    }
}

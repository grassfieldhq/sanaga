//! Notes: an answer of the assistant the person chose to keep. Each is a
//! plain text file in the profile's `notes` folder (Markdown as the models
//! write it: "# " headings, "• " or "- " points, **bold**), with where it
//! came from at its end. It can be read in Sanaga, gone on with in a new
//! conversation, opened as a page to print or save as PDF — or simply
//! opened with any other program.

use std::path::Path;

/// The biggest note read, and how many are listed.
const MAX_FILE: u64 = 2 << 20;
const MAX_NOTES: usize = 500;

/// One line of the list of notes.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// Its file's name without ".md".
    pub name: String,
    pub title: String,
    /// When it was last written.
    pub time: u64,
}

/// A note's name from its file's (nothing that could leave the folder).
fn safe(name: &str) -> Option<&str> {
    (!name.is_empty() && name.len() <= 120 && !name.starts_with('.') && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')).then_some(name)
}

fn slug(title: &str) -> String {
    let s: String = title.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(48).collect()
}

/// A title on one line, from what the person asked.
pub fn title_of(asked: &str) -> String {
    let t: String = asked.split_whitespace().collect::<Vec<_>>().join(" ").trim_start_matches(['#', '/']).trim().chars().take(80).collect();
    if t.is_empty() {
        "Note".to_string()
    } else {
        t
    }
}

/// A note's text: its title, the answer, then where it came from.
/// `sources`: the pages given to the model (title, address); `kept`: a
/// line that says when and by which model.
pub fn compose(title: &str, answer: &str, sources: &[(String, String)], kept: &str) -> String {
    let mut out = format!("# {}\n\n{}\n", title_of(title), answer.trim());
    if !sources.is_empty() || !kept.is_empty() {
        out.push_str("\n---\n");
    }
    for (name, address) in sources {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push_str(&match (name.is_empty(), address.is_empty()) {
            (false, false) => format!("- {name} — {address}\n"),
            (true, _) => format!("- {address}\n"),
            (false, true) => format!("- {name}\n"),
        });
    }
    if !kept.is_empty() {
        out.push_str(&format!("\n{kept}\n"));
    }
    out
}

/// Keep a note in the folder (made if need be) under a new name that
/// begins with `stamp` (a date, so that the files sort by time); gives the
/// name it got.
pub fn save(dir: &Path, stamp: &str, title: &str, text: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    // ("2026-11-17 14:02" → "2026-11-17-1402")
    let stamp: String = stamp.trim().chars().map(|c| if c == ' ' { '-' } else { c }).filter(|c| c.is_ascii_digit() || *c == '-').collect();
    let base = [stamp.as_str(), slug(title).as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join("-");
    let base = if base.is_empty() { "note".to_string() } else { base };
    // (never over a note that is there)
    let mut name = base.clone();
    let mut n = 2;
    while dir.join(format!("{name}.md")).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    crate::store::write_atomic(&dir.join(format!("{name}.md")), text)?;
    Ok(name)
}

/// The notes kept, the latest first.
pub fn list(dir: &Path) -> Vec<Note> {
    use std::io::{BufRead, BufReader};
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in read.flatten().take(MAX_NOTES * 4) {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|n| n.to_str()).and_then(safe).map(str::to_string) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("md") || !meta.is_file() {
            continue;
        }
        // (its title is its first line: only that line is read)
        let first = std::fs::File::open(&path).ok().and_then(|f| BufReader::new(f).lines().next()).and_then(Result::ok).unwrap_or_default();
        let title = title_of(&first.chars().take(200).collect::<String>());
        let time = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        out.push(Note { name, title, time });
    }
    out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| b.name.cmp(&a.name)));
    out.truncate(MAX_NOTES);
    out
}

pub fn load(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join(format!("{}.md", safe(name)?));
    if std::fs::metadata(&path).ok()?.len() > MAX_FILE {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

pub fn delete(dir: &Path, name: &str) -> std::io::Result<()> {
    let name = safe(name).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    std::fs::remove_file(dir.join(format!("{name}.md")))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A line's words as HTML, `**bold**` as bold (a "**" left alone stays).
fn inline(line: &str) -> String {
    let parts: Vec<&str> = line.split("**").collect();
    let mut out = String::new();
    for (k, part) in parts.iter().enumerate() {
        // (odd parts are between two "**" — when a closing one follows)
        if k % 2 == 1 && k + 1 < parts.len() {
            out.push_str(&format!("<strong>{}</strong>", esc(part)));
        } else {
            if k % 2 == 1 {
                out.push_str("**");
            }
            out.push_str(&esc(part));
        }
    }
    out
}

/// A note as a page to print or save as PDF: nothing in it can run (no
/// script, nothing loaded from anywhere), and what the model wrote is
/// text, never HTML.
pub fn html(text: &str) -> String {
    let mut body = String::new();
    let mut in_list = false;
    let mut title = String::new();
    let close = |body: &mut String, in_list: &mut bool| {
        if *in_list {
            body.push_str("</ul>\n");
            *in_list = false;
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            close(&mut body, &mut in_list);
            continue;
        }
        if let Some(point) = line.strip_prefix("• ").or_else(|| line.strip_prefix("- ")).or_else(|| line.strip_prefix("* ")) {
            if !in_list {
                body.push_str("<ul>\n");
                in_list = true;
            }
            body.push_str(&format!("<li>{}</li>\n", inline(point.trim())));
            continue;
        }
        close(&mut body, &mut in_list);
        if line.chars().all(|c| c == '-') && line.len() >= 3 {
            body.push_str("<hr>\n");
        } else if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count().min(3);
            let words = line.trim_start_matches('#').trim();
            if title.is_empty() {
                title = words.to_string();
            }
            body.push_str(&format!("<h{level}>{}</h{level}>\n", inline(words)));
        } else {
            body.push_str(&format!("<p>{}</p>\n", inline(line)));
        }
    }
    close(&mut body, &mut in_list);
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n<title>{}</title>\n\
         <style>body{{font:16px/1.55 system-ui,sans-serif;max-width:46em;margin:2.5em auto;padding:0 1.2em;color:#1b1b1b}}h1{{font-size:1.6em}}h2{{font-size:1.25em}}h3{{font-size:1.05em}}\
         hr{{border:0;border-top:1px solid #bbb;margin:2em 0 1em}}hr~*{{color:#555;font-size:.9em}}li{{margin:.25em 0}}@media print{{body{{margin:0;max-width:none}}}}</style>\n</head><body>\n{}</body></html>\n",
        esc(&title),
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_composed_kept_listed_and_read() {
        let dir = crate::store::PrivateDir::new("sanaga-notes-test").unwrap();
        let folder = dir.path().join("notes");
        assert!(list(&folder).is_empty());
        let text = compose(
            "  Quelle est la\nlongueur du fleuve ? ",
            "918 km.\n• Du Djérem à l'océan.\n",
            &[("La Sanaga".into(), "https://example.org/sanaga".into()), (String::new(), "https://example.org/b".into())],
            "Gardée le 2026-11-17.",
        );
        assert_eq!(
            text,
            "# Quelle est la longueur du fleuve ?\n\n918 km.\n• Du Djérem à l'océan.\n\n---\n- La Sanaga — https://example.org/sanaga\n- https://example.org/b\n\nGardée le 2026-11-17.\n"
        );
        let name = save(&folder, "2026-11-17 14:02", "Quelle est la longueur du fleuve ?", &text).unwrap();
        assert_eq!(name, "2026-11-17-1402-quelle-est-la-longueur-du-fleuve");
        // a second note of the same name does not take the first one's place
        let other = save(&folder, "2026-11-17 14:02", "Quelle est la longueur du fleuve ?", "# Autre\n").unwrap();
        assert_eq!(other, format!("{name}-2"));
        let notes = list(&folder);
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.name == name && n.title == "Quelle est la longueur du fleuve ?"));
        assert_eq!(load(&folder, &name).as_deref(), Some(text.as_str()));
        delete(&folder, &other).unwrap();
        assert_eq!(list(&folder).len(), 1);
        // a name never leaves the folder
        std::fs::write(dir.path().join("secret.md"), "x").unwrap();
        assert!(load(&folder, "../secret").is_none() && delete(&folder, "../secret").is_err());
        assert!(dir.path().join("secret.md").exists());
        assert_eq!(compose("", "Texte.", &[], ""), "# Note\n\nTexte.\n");
    }

    #[test]
    fn a_note_becomes_a_page_where_nothing_runs() {
        let page = html("# Résumé <b>\n\nLa **Sanaga** est un fleuve.\n• Premier <script>alert(1)</script>\n- Deuxième **gras\n\n---\n- Source — https://example.org/?a=1&b=2\n");
        assert!(page.contains("<title>Résumé &lt;b&gt;</title>"));
        assert!(page.contains("<h1>Résumé &lt;b&gt;</h1>"));
        assert!(page.contains("<p>La <strong>Sanaga</strong> est un fleuve.</p>"));
        assert!(page.contains("<li>Premier &lt;script&gt;alert(1)&lt;/script&gt;</li>"));
        assert!(page.contains("<li>Deuxième **gras</li>"), "an opening ** never closed stays as written");
        assert!(page.contains("<hr>") && page.contains("https://example.org/?a=1&amp;b=2"));
        assert!(!page.contains("<script") && page.contains("default-src 'none'"));
        assert_eq!(page.matches("<ul>").count(), page.matches("</ul>").count());
        // whatever a model wrote, the page holds no tag of its making
        crate::mutate::run(&["# t\n• a **b** <i>\n---\n<script>x</script> \" & ' > <".as_bytes()], 10_000, |d| {
            let page = html(&String::from_utf8_lossy(d));
            let body = page.split_once("<body>").map(|x| x.1).unwrap_or("");
            for tag in body.split('<').skip(1) {
                assert!(
                    ["ul>", "/ul>", "li>", "/li>", "p>", "/p>", "hr>", "strong>", "/strong>", "h1>", "/h1>", "h2>", "/h2>", "h3>", "/h3>", "/body>", "/html>"].iter().any(|ok| tag.starts_with(ok)),
                    "{tag:?}"
                );
            }
        });
    }
}

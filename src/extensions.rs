//! Extensions: small scripts and style sheets added to web pages, like Safari
//! extensions of the "user script" kind. Three come with Sanaga; more can be
//! dropped as `.js` or `.css` files in the extensions folder, with a header:
//!
//! ```text
//! // @name        Nom affiché
//! // @description À quoi elle sert
//! // @match       *://*.exemple.com/*
//! ```

use crate::i18n::{tf, tr};
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    #[test]
    fn patterns() {
        assert_eq!(super::glob_to_regex("*://*.a.com/*"), "^.*:\\/\\/.*\\.a\\.com\\/.*$");
        let all: Vec<String> = super::all().into_iter().map(|e| e.id).collect();
        let js = super::init_script(&all);
        assert!(js.contains("adsbygoogle"));
        let _ = std::fs::write(std::env::temp_dir().join("sanaga-extensions-test.js"), js);
    }
}

#[derive(Clone, Debug)]
pub struct Extension {
    pub id: String,
    pub name: String,
    pub description: String,
    pub matches: Vec<String>,
    pub js: String,
    pub css: String,
    pub builtin: bool,
}

const ADBLOCK_CSS: &str = r#"
.adsbygoogle, ins.adsbygoogle, [id^="google_ads"], [id*="google_ads_iframe"], [id^="div-gpt-ad"],
[data-ad-slot], [data-ad-unit], [data-google-query-id], .ad-container, .ad-banner, .ad-slot, .ad-wrapper,
.advertisement, .advert, .sponsored-content, [class*="sponsored-post"], [aria-label="Publicité"],
[aria-label="Advertisement"], [aria-label="Sponsored"], iframe[src*="doubleclick.net"],
iframe[src*="googlesyndication"], iframe[src*="adservice"], iframe[id^="google_ads"],
div[id^="taboola-"], .taboola, .OUTBRAIN, [id^="outbrain_widget"] { display: none !important; }
"#;

const DARK_CSS: &str = r#"
html { filter: invert(0.92) hue-rotate(180deg) !important; background: #111 !important; }
img, video, picture, canvas, iframe, svg image, [style*="background-image"] { filter: invert(1) hue-rotate(180deg) !important; }
"#;

const NO_AUTOPLAY_JS: &str = r#"
(function () {
  function calm(root) {
    (root.querySelectorAll ? root.querySelectorAll('video[autoplay], audio[autoplay]') : []).forEach(function (m) {
      m.removeAttribute('autoplay'); m.autoplay = false; try { m.pause(); } catch (e) {}
    });
  }
  document.addEventListener('DOMContentLoaded', function () {
    calm(document);
    new MutationObserver(function (list) { list.forEach(function (r) { r.addedNodes.forEach(function (n) { if (n.nodeType === 1) calm(n.parentNode || n); }); }); })
      .observe(document.documentElement, { childList: true, subtree: true });
  });
})();
"#;

fn builtins() -> Vec<Extension> {
    vec![
        Extension {
            id: "bloqueur-de-publicites".into(),
            name: tr("Bloqueur de publicités").into(),
            description: tr("Masque les emplacements publicitaires les plus courants.").into(),
            matches: vec![],
            js: String::new(),
            css: ADBLOCK_CSS.into(),
            builtin: true,
        },
        Extension {
            id: "pas-de-lecture-auto".into(),
            name: tr("Pas de lecture automatique").into(),
            description: tr("Empêche les vidéos et les sons de démarrer tout seuls.").into(),
            matches: vec![],
            js: NO_AUTOPLAY_JS.into(),
            css: String::new(),
            builtin: true,
        },
        Extension {
            id: "mode-sombre".into(),
            name: tr("Mode sombre pour tous les sites").into(),
            description: tr("Affiche les pages claires en sombre, plus reposant le soir.").into(),
            matches: vec![],
            js: String::new(),
            css: DARK_CSS.into(),
            builtin: true,
        },
    ]
}

pub fn folder() -> PathBuf {
    crate::store::Store::root_dir().join("extensions")
}

fn header(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .take(40)
        .filter_map(|l| {
            let i = l.find(key)?;
            let v = l[i + key.len()..].trim().trim_end_matches("*/").trim();
            (!v.is_empty()).then(|| v.to_string())
        })
        .collect()
}

/// All extensions: the built-in ones, then the files in the extensions folder.
pub fn all() -> Vec<Extension> {
    let mut v = builtins();
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder()).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    files.sort();
    for p in files {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext != "js" && ext != "css" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let name = header(&text, "@name").into_iter().next().unwrap_or_else(|| stem.clone());
        let description = header(&text, "@description").into_iter().next().unwrap_or_else(|| tf("Fichier {}", &[&p.file_name().unwrap_or_default().to_string_lossy()]));
        let matches = header(&text, "@match");
        let (js, css) = if ext == "js" { (text, String::new()) } else { (String::new(), text) };
        v.push(Extension { id: format!("fichier:{}", p.file_name().unwrap_or_default().to_string_lossy()), name, description, matches, js, css, builtin: false });
    }
    v
}

/// `*://*.site.com/*` → a regular expression matching whole addresses.
fn glob_to_regex(p: &str) -> String {
    let mut r = String::from("^");
    for c in p.chars() {
        match c {
            '*' => r.push_str(".*"),
            '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' | '/' => {
                r.push('\\');
                r.push(c);
            }
            c => r.push(c),
        }
    }
    r.push('$');
    r
}

fn js_lit(s: &str) -> String {
    crate::keyring::js_str(s)
}

/// One script that runs the switched-on extensions on matching pages.
pub fn init_script(enabled: &[String]) -> String {
    let mut out = String::new();
    for e in all().into_iter().filter(|e| enabled.contains(&e.id)) {
        let pats: Vec<String> = e.matches.iter().map(|m| format!("new RegExp({})", js_lit(&glob_to_regex(m)))).collect();
        out.push_str(&format!("(function(){{var pats=[{}];var href=location.href;if(pats.length&&!pats.some(function(r){{return r.test(href);}}))return;", pats.join(",")));
        if !e.css.is_empty() {
            out.push_str(&format!(
                "var css={};function add(){{var s=document.createElement('style');s.textContent=css;(document.head||document.documentElement).appendChild(s);}}\
                 if(document.documentElement)add();else document.addEventListener('DOMContentLoaded',add);",
                js_lit(&e.css)
            ));
        }
        if !e.js.is_empty() {
            out.push_str(&format!("try{{{}\n}}catch(err){{console.warn('extension',err);}}", e.js));
        }
        out.push_str("})();\n");
    }
    out
}

/// Creates the folder with an example the person can copy.
pub fn prepare_folder() -> PathBuf {
    let dir = folder();
    let _ = std::fs::create_dir_all(&dir);
    let readme = dir.join("LISEZ-MOI.txt");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            "Extensions de Sanaga\n\nDéposez ici des fichiers .js (scripts) ou .css (styles).\nEn tête du fichier, vous pouvez écrire :\n\n// @name        Nom affiché\n// @description À quoi elle sert\n// @match       *://*.exemple.com/*\n\nPuis activez-la dans Sanaga › Extensions. Elle s'applique aux nouveaux onglets.\n",
        );
    }
    dir
}

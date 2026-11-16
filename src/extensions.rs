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
// (a script's header and the script that says where it runs: webext.rs,
// with what else comes from other browsers — and within the fuzzers' reach)
use crate::webext::{header, name_in, PATTERN_JS};
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    #[test]
    fn user_scripts_say_where_they_act() {
        let js = "// @name x\n// @match       *://youtube.com/*\n// @include *://*.youtube.com/*\nalert(1)";
        assert_eq!(super::matches_in(js), vec!["*://youtube.com/*".to_string(), "*://*.youtube.com/*".to_string()]);
        assert!(super::matches_in("// @name y\n").is_empty());
    }

    #[test]
    fn the_whole_header_is_read_and_keys_are_whole_words() {
        // a long header: the sites come after the 40th line
        let mut js = String::from("// ==UserScript==\n// @namespace   https://scripts.example/moi\n// @name        Ma banque\n// @name:de     Meine Bank\n");
        for k in 0..60 {
            js.push_str(&format!("// @grant       GM_{k}\n"));
        }
        js.push_str("// @match       https://*.banque.com/*\n// @include     https://banque.example/*\n// @exclude     https://www.banque.com/aide/*\n// @exclude-match *://*.banque.com/pub/*\n// ==/UserScript==\nvar notes = '// @match https://not-the-header.example/*';\n");
        assert_eq!(super::matches_in(&js), ["https://*.banque.com/*", "https://banque.example/*"]);
        assert_eq!(super::name_in(&js).as_deref(), Some("Ma banque"));
        assert_eq!(super::header(&js, "exclude"), ["https://www.banque.com/aide/*"]);
        assert_eq!(super::header(&js, "exclude-match"), ["*://*.banque.com/pub/*"]);
        // only @namespace: no name (the file's is used)
        assert_eq!(super::name_in("// ==UserScript==\n// @namespace https://x.example/\n// ==/UserScript==\n"), None);
        // Sanaga's short form and a style sheet: the comment at the top, not the code under it
        assert_eq!(super::matches_in("// @name x\n\n// @match *://a.example/*\nvar s = '@match *://b.example/*';\n// @match *://c.example/*\n"), ["*://a.example/*"]);
        let css = "/* @name Sombre\n   @match *://a.example/*\n*/\nbody { color: red } /* @match *://b.example/* */\n";
        assert_eq!((super::name_in(css).as_deref(), super::matches_in(css)), (Some("Sombre"), vec!["*://a.example/*".to_string()]));
        assert_eq!(super::name_in("/* @name Une ligne */\nbody{}").as_deref(), Some("Une ligne"));
    }

    /// Does the pattern match the address? Asked of `PATTERN_JS` itself.
    fn pattern_matches(pattern: &str, address: &str, loose: bool) -> bool {
        super::in_page(address, &format!("{}__sm({},{loose})", super::PATTERN_JS, super::js_lit(pattern)))
    }

    #[test]
    fn a_pattern_matches_scheme_host_and_path_apart() {
        let yes = |p: &str, a: &str| assert!(pattern_matches(p, a, false), "{p} should match {a}");
        let no = |p: &str, a: &str| assert!(!pattern_matches(p, a, false), "{p} should not match {a}");
        // the bank's pattern: the bank, not a page that writes it in its address
        let bank = "*://*.banque.com/*";
        yes(bank, "https://www.banque.com/comptes?id=1");
        yes(bank, "http://banque.com/");
        yes(bank, "https://a.b.banque.com:8443/x");
        no(bank, "https://pirate.example/?x=://a.banque.com/");
        no(bank, "https://pirate.example/a/://www.banque.com/b");
        no(bank, "https://banque.com.pirate.example/");
        no(bank, "https://fausse-banque.com/");
        no(bank, "https://pirate.example/#://x.banque.com/");
        no(bank, "file:///home/moi/banque.com/index.html");
        no(bank, "ftp://www.banque.com/");
        // an exact host, a scheme, a path
        yes("https://banque.com/comptes/*", "https://banque.com/comptes/1?x=2");
        no("https://banque.com/comptes/*", "https://www.banque.com/comptes/1");
        no("https://banque.com/comptes/*", "http://banque.com/comptes/1");
        no("https://banque.com/comptes/*", "https://banque.com/aide");
        no("https://banque.com/comptes/*", "https://pirate.example/https://banque.com/comptes/1");
        yes("https://banque.com/*.pdf?*", "https://banque.com/a/b.pdf?dl=1");
        yes("*://*/*", "http://localhost:8080/");
        no("*://*/*", "file:///tmp/a.html");
        yes("file:///*", "file:///tmp/a.html");
        yes("https://banque.com", "https://banque.com/x");
        yes("HTTPS://Banque.COM/*", "https://banque.com/");
        yes("http://localhost:8080/*", "http://localhost:8080/a");
        no("http://localhost:8080/*", "http://localhost:9090/a");
        // every page
        for all in ["<all_urls>", "*"] {
            yes(all, "https://a.example/x");
            yes(all, "http://a.example/");
            yes(all, "file:///tmp/a.html");
        }
        // a user script's looser @include, still part by part
        yes("http*://www.google.*/search*", "https://www.google.fr/search?q=a");
        no("http*://www.google.*/search*", "https://pirate.example/search?u=https://www.google.fr/search");
        // no scheme, as some user scripts write: a host and a path, on http and https
        yes("banque.com", "https://banque.com/x");
        yes("*.banque.com/comptes*", "http://www.banque.com/comptes/1");
        yes("*banque.com*", "https://www.banque.com/x");
        no("*banque.com*", "https://pirate.example/?banque.com");
        no("*.banque.com/*", "https://pirate.example/x.banque.com/");
        no("banque.com", "file:///banque.com");
        yes("http*", "https://a.example/");
        yes("http*", "http://a.example/");
        no("http*", "file:///tmp/a.html");
        no("https*", "http://a.example/");
        // not a pattern at all (an expression between slashes): no page
        no("/^https?://banque\\.com/.*$/", "https://banque.com/");
        // where a script must not run, a pattern without a scheme is also laid over the whole address
        assert!(pattern_matches("*deconnexion*", "https://www.banque.com/deconnexion", true));
        assert!(!pattern_matches("*deconnexion*", "https://www.banque.com/deconnexion", false));
        assert!(pattern_matches("banque.com/aide/*", "https://banque.com/aide/x", true));
        assert!(pattern_matches("*://*.banque.com/aide/*", "https://www.banque.com/aide/x", true));
        assert!(!pattern_matches("*://*.banque.com/aide/*", "https://www.banque.com/comptes", true));
    }

    #[test]
    fn an_extension_runs_on_its_sites_and_nowhere_else() {
        // a user script as it is handed to pages; `ran` tells whether it ran
        let script = super::Extension {
            id: "fichier:banque.js".into(),
            name: "Banque".into(),
            description: String::new(),
            matches: vec!["*://*.banque.com/*".into()],
            excludes: vec!["*://*.banque.com/aide/*".into(), "*deconnexion*".into()],
            js: "ran=true;".into(),
            css: String::new(),
            builtin: false,
        };
        let runs = |e: &super::Extension, address: &str| super::in_page(address, &format!("var window=this;var ran=false;{};ran", super::script_of(e)));
        assert!(runs(&script, "https://www.banque.com/comptes"));
        assert!(!runs(&script, "https://pirate.example/?x=://a.banque.com/"));
        assert!(!runs(&script, "https://www.banque.com/aide/contact"));
        assert!(!runs(&script, "https://www.banque.com/deconnexion"));
        // no list of sites: every page
        let everywhere = super::Extension { matches: vec![], excludes: vec![], ..script.clone() };
        assert!(runs(&everywhere, "https://pirate.example/"));
        let all_urls = super::Extension { matches: vec!["<all_urls>".into()], excludes: vec![], ..script.clone() };
        assert!(runs(&all_urls, "https://a.example/x"));
    }

    #[test]
    fn patterns() {
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
    /// Pages where it must not run (user scripts' @exclude).
    pub excludes: Vec<String>,
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

/// Cookie banners of the common consent tools, hidden (nothing is accepted:
/// a site that sees no answer keeps its default, which in Europe is "no").
/// A page that froze its scrolling behind the banner scrolls again.
const COOKIE_BANNERS_CSS: &str = r#"
#onetrust-consent-sdk, #onetrust-banner-sdk, #CybotCookiebotDialog, #CybotCookiebotDialogBodyUnderlay,
#usercentrics-root, #didomi-host, .didomi-popup-backdrop, #qc-cmp2-container, .qc-cmp2-container,
#truste-consent-track, .truste_overlay, .truste_box_overlay, #cookie-law-info-bar, .cli-modal-backdrop,
#cookie-notice, #cookiescript_injected, #cmplz-cookiebanner-container, .cmplz-cookiebanner, #BorlabsCookieBox,
#tarteaucitronRoot, #axeptio_overlay, .axeptio_mount, #sp_message_container, [id^="sp_message_container"],
.fc-consent-root, #gdpr-cookie-message, .cc-window.cc-banner, .cc-revoke, #cookieChoiceInfo,
#consent-banner, #cookie-consent-banner, .cookie-consent-banner, #iubenda-cs-banner, .iubenda-cs-container,
#klaro, .klaro .cookie-notice, #moove_gdpr_cookie_info_bar, #cookie-information-template-wrapper
{ display: none !important; }
html:has(#onetrust-consent-sdk), body:has(#onetrust-consent-sdk), html:has(#didomi-host), body:has(#didomi-host),
html:has(.qc-cmp2-container), body:has(.qc-cmp2-container), html:has(#usercentrics-root), body:has(#usercentrics-root),
html:has([id^="sp_message_container"]), body:has([id^="sp_message_container"]), body:has(.fc-consent-root),
body:has(#CybotCookiebotDialog), body:has(#tarteaucitronRoot), body:has(#axeptio_overlay)
{ overflow: auto !important; position: static !important; }
"#;

const DARK_CSS: &str = r#"
html { filter: invert(0.92) hue-rotate(180deg) !important; background: #fff !important; }
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

/// The Greasemonkey / Tampermonkey functions user scripts use most, kept
/// in the page's storage (per script).
const GM_SHIM: &str = "var unsafeWindow=window;var __gmk='sanaga-gm:'+location.host+':';\
function GM_addStyle(c){var s=document.createElement('style');s.textContent=c;(document.head||document.documentElement).appendChild(s);return s;}\
function GM_getValue(k,d){try{var v=localStorage.getItem(__gmk+k);return v===null?d:JSON.parse(v);}catch(e){return d;}}\
function GM_setValue(k,v){try{localStorage.setItem(__gmk+k,JSON.stringify(v));}catch(e){}}\
function GM_deleteValue(k){try{localStorage.removeItem(__gmk+k);}catch(e){}}\
function GM_openInTab(u){window.open(u,'_blank');}\
function GM_setClipboard(t){try{navigator.clipboard.writeText(t);}catch(e){}}\
function GM_log(){console.log.apply(console,arguments);}\
var GM={addStyle:GM_addStyle,getValue:function(k,d){return Promise.resolve(GM_getValue(k,d));},setValue:function(k,v){GM_setValue(k,v);return Promise.resolve();},deleteValue:function(k){GM_deleteValue(k);return Promise.resolve();},openInTab:GM_openInTab,setClipboard:GM_setClipboard};";

fn builtins() -> Vec<Extension> {
    vec![
        Extension {
            id: "bloqueur-de-publicites".into(),
            name: tr("Bloqueur de publicités").into(),
            description: tr("Masque les emplacements publicitaires les plus courants.").into(),
            matches: vec![],
            excludes: vec![],
            js: String::new(),
            css: ADBLOCK_CSS.into(),
            builtin: true,
        },
        Extension {
            id: "pas-de-lecture-auto".into(),
            name: tr("Pas de lecture automatique").into(),
            description: tr("Empêche les vidéos et les sons de démarrer tout seuls.").into(),
            matches: vec![],
            excludes: vec![],
            js: NO_AUTOPLAY_JS.into(),
            css: String::new(),
            builtin: true,
        },
        Extension {
            id: "sans-bannieres-cookies".into(),
            name: tr("Sans bannières de cookies").into(),
            description: tr("Masque les bannières de consentement des outils les plus courants, sans rien accepter.").into(),
            matches: vec![],
            excludes: vec![],
            js: String::new(),
            css: COOKIE_BANNERS_CSS.into(),
            builtin: true,
        },
        Extension {
            id: "mode-sombre".into(),
            name: tr("Mode sombre pour tous les sites").into(),
            description: tr("Affiche les pages claires en sombre, plus reposant le soir.").into(),
            matches: vec![],
            excludes: vec![],
            js: String::new(),
            css: DARK_CSS.into(),
            builtin: true,
        },
    ]
}

pub fn folder() -> PathBuf {
    crate::store::Store::root_dir().join("extensions")
}

/// Where a user script says it acts (@match, @include): empty for all sites.
pub fn matches_in(text: &str) -> Vec<String> {
    let mut m = header(text, "match");
    m.extend(header(text, "include"));
    m
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
        let name = name_in(&text).unwrap_or_else(|| stem.clone());
        let description = header(&text, "description").into_iter().next().unwrap_or_else(|| tf("Fichier {}", &[&p.file_name().unwrap_or_default().to_string_lossy()]));
        // (user scripts also say where they run with @include)
        let matches = matches_in(&text);
        let mut excludes = header(&text, "exclude-match");
        excludes.extend(header(&text, "exclude"));
        // (an extension of another browser, installed by an earlier Sanaga:
        // it compares addresses as today's do)
        let (js, css) = if ext == "js" { (crate::webext::with_todays_matching(text), String::new()) } else { (String::new(), text) };
        v.push(Extension { id: format!("fichier:{}", p.file_name().unwrap_or_default().to_string_lossy()), name, description, matches, excludes, js, css, builtin: false });
    }
    v
}

fn js_lit(s: &str) -> String {
    crate::keyring::js_str(s)
}

/// For the tests: what a piece of script answers (true or false) in a page
/// at `address`. Run by the engine's own JavaScript, with a `location` made
/// from the address — no page, no window, only the language: the scripts
/// that decide where an extension runs are tested as they are handed to pages.
#[cfg(test)]
pub(crate) fn in_page(address: &str, script: &str) -> bool {
    let u = url::Url::parse(address).unwrap();
    let host = u.host_str().unwrap_or("").to_string();
    let with_port = u.port().map(|p| format!("{host}:{p}")).unwrap_or_else(|| host.clone());
    let search = u.query().map(|q| format!("?{q}")).unwrap_or_default();
    let location = format!("var location={{href:{},protocol:{},host:{},hostname:{},pathname:{},search:{}}};", js_lit(address), js_lit(&format!("{}:", u.scheme())), js_lit(&with_port), js_lit(&host), js_lit(u.path()), js_lit(&search));
    let engine = webkit6::javascriptcore::Context::new();
    let answer = engine.evaluate(&format!("{location}{script}"));
    assert!(engine.exception().is_none(), "{script}: {:?}", engine.exception().and_then(|e| e.message()));
    answer.is_some_and(|a| a.to_boolean())
}

/// One script that runs the switched-on extensions on matching pages.
pub fn init_script(enabled: &[String]) -> String {
    all().iter().filter(|e| enabled.contains(&e.id)).map(script_of).collect()
}

/// One extension as it is handed to pages: it runs where its patterns say,
/// and nowhere else.
fn script_of(e: &Extension) -> String {
    let mut out = String::from("(function(){");
    // (where it acts and where it must not: see `PATTERN_JS`)
    if !e.matches.is_empty() || !e.excludes.is_empty() {
        let list = |patterns: &[String]| patterns.iter().map(|m| js_lit(m)).collect::<Vec<_>>().join(",");
        out.push_str(&format!(
            "{PATTERN_JS}var pats=[{}];var ex=[{}];if(pats.length&&!pats.some(function(g){{return __sm(g);}}))return;if(ex.some(function(g){{return __sm(g,true);}}))return;",
            list(&e.matches),
            list(&e.excludes)
        ));
    }
    if !e.builtin && !e.js.is_empty() {
        out.push_str(&format!("{GM_SHIM}var GM_info={{script:{{name:{}}}}};", js_lit(&e.name)));
    }
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

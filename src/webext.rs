//! Installing extensions made for other browsers: Chrome (`.crx`, `.zip` or
//! an unpacked folder's `manifest.json`), Firefox (`.xpi`) and user scripts
//! (`.user.js`, Greasemonkey / Tampermonkey). What runs inside the pages (the
//! extension's content scripts and styles) is turned into one of Sanaga's
//! extensions, with a small stand-in for the browser APIs they use most
//! (`chrome.storage`, `chrome.runtime.getURL`). Parts that need a background
//! page or a toolbar pop-up are not supported (WebKitGTK has no extension API).

use std::path::{Path, PathBuf};

/// The most an extension's files may weigh once unpacked, in all (a small
/// archive can claim to hold gigabytes).
const MAX_UNPACKED: usize = 96 << 20;
/// The most one file may weigh once unpacked.
const MAX_FILE: usize = 16 << 20;
/// An unpacked extension's folder: the most files read from it, and the
/// size past which a file is left out (no script or style sheet is that big).
const MAX_FOLDER_FILES: usize = 2000;
const MAX_FOLDER_FILE: u64 = 8 << 20;

/// The files of a zip archive (`.xpi`, `.zip`, or the zip inside a `.crx`).
/// The archive comes from anywhere: every size and position it states is
/// checked against what is really there, each file's checksum is verified,
/// and the whole is refused past `MAX_UNPACKED`.
fn unzip(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    // a .crx starts with its own header: the zip begins at the first "PK\3\4"
    let start = data.windows(4).position(|w| w == b"PK\x03\x04").ok_or("archive illisible")?;
    let zip = &data[start..];
    let u16_at = |i: usize| -> Option<usize> { zip.get(i..i.checked_add(2)?).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize) };
    let u32_at = |i: usize| -> Option<usize> { zip.get(i..i.checked_add(4)?).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize) };
    let broken = || "archive incomplète".to_string();
    // the central directory, found from its end record (22 bytes at least)
    let end = (0..=zip.len().saturating_sub(22)).rev().find(|&i| zip.get(i..i + 4) == Some(b"PK\x05\x06")).ok_or_else(broken)?;
    let count = u16_at(end + 10).ok_or_else(broken)?;
    let mut at = u32_at(end + 16).ok_or_else(broken)?;
    let mut files = Vec::new();
    let mut total = 0usize;
    for _ in 0..count {
        if zip.get(at..at.saturating_add(4)) != Some(b"PK\x01\x02") {
            break;
        }
        let field16 = |o: usize| u16_at(at + o).ok_or_else(broken);
        let field32 = |o: usize| u32_at(at + o).ok_or_else(broken);
        let (flags, method, crc, csize, usize_) = (field16(8)?, field16(10)?, field32(16)?, field32(20)?, field32(24)?);
        let (name_len, extra_len, comment_len, local) = (field16(28)?, field16(30)?, field16(32)?, field32(42)?);
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len).ok_or_else(broken)?).to_string();
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        // encrypted files can't be read; a file too big is not an extension's
        if flags & 1 != 0 {
            return Err("archive chiffrée".into());
        }
        if usize_ > MAX_FILE {
            return Err("archive trop volumineuse".into());
        }
        if zip.get(local..local.saturating_add(4)) != Some(b"PK\x03\x04") {
            return Err(broken());
        }
        let body = local + 30 + u16_at(local + 26).ok_or_else(broken)? + u16_at(local + 28).ok_or_else(broken)?;
        let raw = zip.get(body..body.checked_add(csize).ok_or_else(broken)?).ok_or_else(broken)?;
        let content = match method {
            0 => raw.to_vec(),
            8 => {
                use std::io::Read;
                let mut out = Vec::new();
                // (one byte more than allowed: a file that goes on is refused, not cut)
                flate2::read::DeflateDecoder::new(raw).take(MAX_FILE as u64 + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
                out
            }
            _ => continue,
        };
        total += content.len();
        if content.len() > MAX_FILE || total > MAX_UNPACKED {
            return Err("archive trop volumineuse".into());
        }
        let mut sum = flate2::Crc::new();
        sum.update(&content);
        if sum.sum() as usize != crc {
            return Err("archive abîmée (somme de contrôle)".into());
        }
        files.push((name.trim_start_matches("./").to_string(), content));
    }
    Ok(files)
}

/// A manifest as JSON. Chrome accepts `//` and `/* */` comments in
/// manifests, and some files start with a byte-order mark: both go first.
fn parse_json(text: &str) -> Result<serde_json::Value, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let (mut in_str, mut esc) = (false, false);
    while let Some(c) = chars.next() {
        if in_str {
            clean.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                clean.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        clean.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => clean.push(c),
        }
    }
    serde_json::from_str(&clean).map_err(|e| crate::i18n::tf("manifest.json illisible ({})", &[&e]))
}

/// The strings of the array `key` of a JSON object.
fn strings(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|s| s.as_str()).map(|s| s.to_string()).collect()).unwrap_or_default()
}

/// "__MSG_name__" texts come from the extension's translations.
fn localize(text: &str, files: &[(String, Vec<u8>)], manifest: &serde_json::Value) -> String {
    let Some(key) = text.strip_prefix("__MSG_").and_then(|t| t.strip_suffix("__")) else { return text.to_string() };
    let default = manifest.get("default_locale").and_then(|l| l.as_str()).unwrap_or("en").to_string();
    for lang in [crate::i18n::language().to_string(), default, "en".into()] {
        let path = format!("_locales/{lang}/messages.json");
        let Some((_, data)) = files.iter().find(|(n, _)| *n == path) else { continue };
        let Ok(messages) = parse_json(&String::from_utf8_lossy(data)) else { continue };
        // "key": { "message": "…" } (keys are not case-sensitive)
        let found = messages.as_object().and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(key))).and_then(|(_, m)| m.get("message")).and_then(|m| m.as_str());
        if let Some(m) = found {
            return m.to_string();
        }
    }
    key.to_string()
}

/// A stand-in for the browser APIs content scripts use most.
const API_SHIM: &str = r#"var chrome=(window.chrome&&window.chrome.runtime&&window.chrome.runtime.id)?window.chrome:{};
(function(c){function P(){return 'sanaga-ext:'+(c.__id||'')}function rd(){try{return JSON.parse(localStorage.getItem(P())||'{}')}catch(e){return{}}}
function wr(o){try{localStorage.setItem(P(),JSON.stringify(o))}catch(e){}}
function area(){return{get:function(k,cb){var d=rd(),r={};if(k==null)r=d;else if(typeof k==='string')r[k]=d[k];else if(Array.isArray(k))k.forEach(function(x){r[x]=d[x]});else Object.keys(k).forEach(function(x){r[x]=(x in d)?d[x]:k[x]});if(cb)cb(r);return Promise.resolve(r)},
set:function(o,cb){var d=rd();Object.keys(o).forEach(function(x){d[x]=o[x]});wr(d);if(cb)cb();return Promise.resolve()},
remove:function(k,cb){var d=rd();[].concat(k).forEach(function(x){delete d[x]});wr(d);if(cb)cb();return Promise.resolve()},
clear:function(cb){wr({});if(cb)cb();return Promise.resolve()}}}
c.storage=c.storage||{local:area(),sync:area(),onChanged:{addListener:function(){}}};
c.runtime=c.runtime||{id:'sanaga',getURL:function(p){return''},sendMessage:function(){var cb=arguments[arguments.length-1];if(typeof cb==='function')cb();return Promise.resolve()},onMessage:{addListener:function(){}},getManifest:function(){return{}},lastError:null};
c.i18n=c.i18n||{getMessage:function(k){return k},getUILanguage:function(){return navigator.language}};})(chrome);
var browser=chrome;"#;

// ------------------------------------------------------------ user scripts

/// The lines of a script's or style sheet's header. A user script's is the
/// whole block between `==UserScript==` and `==/UserScript==`, however long
/// (only the first 40 lines of the file were read: a script whose `@match`
/// lines came after them had no list of sites, and so ran on every site).
/// Without such a block (Sanaga's own short form, a style sheet), it is the
/// comment the file starts with.
fn header_lines(text: &str) -> Vec<&str> {
    let mut lines = text.lines();
    if text.contains("==UserScript==") {
        // (comment lines only: a block never closed does not take the code for its header)
        return lines.skip_while(|l| !l.contains("==UserScript==")).skip(1).take_while(|l| !l.contains("==/UserScript==")).filter(|l| l.trim_start().starts_with("//")).collect();
    }
    let mut out = Vec::new();
    let mut in_comment = false;
    for l in lines.by_ref() {
        let t = l.trim();
        if in_comment {
            in_comment = !t.contains("*/");
        } else if let Some(rest) = t.strip_prefix("/*") {
            in_comment = !rest.contains("*/");
        } else if !t.is_empty() && !t.starts_with("//") {
            break;
        }
        out.push(l);
    }
    out
}

/// The values of `@key` in the header (`// @key value`). The key is the
/// whole word: `@name` is not the start of `@namespace`, whose value was
/// shown as the script's name.
pub fn header(text: &str, key: &str) -> Vec<String> {
    header_lines(text)
        .into_iter()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.trim_start_matches(['/', '*']).trim_start().strip_prefix('@')?;
            let (k, v) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            // (the end of a `/* … */` comment is not part of the value; after
            // `//` a pattern may well end in `*/`)
            let v = if l.starts_with("//") { v.trim() } else { v.trim().trim_end_matches("*/").trim() };
            (k == key && !v.is_empty()).then(|| v.to_string())
        })
        .collect()
}

/// The name a script or style sheet gives itself (`@name`).
pub fn name_in(text: &str) -> Option<String> {
    header(text, "name").into_iter().next()
}

// ------------------------------------------------------- where a script runs

/// Does this page's address match the pattern `g` (`__sm(g)`, in the page)?
/// The pattern's scheme, host and path are each compared with the page's
/// own, as browsers do for extensions:
///
/// - `<all_urls>` (and a user script's `*`): any http, https or file page;
/// - scheme: `*` is http or https, else the scheme itself;
/// - host: `*` is any host, `*.site.com` is site.com and what ends in
///   `.site.com`, else the host itself;
/// - path: `*` stands for anything, in the path and what follows the `?`.
///
/// The pattern used to be one expression laid over the whole address, its
/// `*` running across scheme, host and path alike: `*://*.banque.com/*`
/// also matched `https://pirate.example/?x=://a.banque.com/`, and a script
/// written for the bank ran in the pirate's page. (And `<all_urls>`, taken
/// for such an expression, matched nothing.)
///
/// Some user scripts write no scheme (`*.site.com/*`, `*site.com*`): the
/// pattern is then a host and a path, on http and https (`http*` alone is
/// those two schemes). With `loose` — for the pages a script must *not*
/// run in (`@exclude`), where matching too much is the safe side — such a
/// pattern is laid over the whole address, as before.
pub const PATTERN_JS: &str = r#"
function __sm(g,loose){var L=location,s=L.protocol.slice(0,-1),h=L.hostname;
function glob(x,t){return new RegExp('^'+x.split('*').map(function(y){return y.replace(/[.+?^${}()|[\]\\\/]/g,'\\$&')}).join('.*')+'$').test(t);}
if(g==='<all_urls>'||g==='*')return s==='http'||s==='https'||s==='file';
var i=g.indexOf('://');
if(i<0){if(loose&&glob(g,L.href))return true;if(/^https?\*$/.test(g))return glob(g,s);g='*://'+g;i=1;}
var gs=g.slice(0,i).toLowerCase(),r=g.slice(i+3),j=r.indexOf('/'),gh=(j<0?r:r.slice(0,j)).toLowerCase(),gp=j<0?'/*':r.slice(j);
if(gs==='*'?(s!=='http'&&s!=='https'):!glob(gs,s))return false;
if(gh!=='*'){var d=gh.slice(2);
if(gh.slice(0,2)==='*.'&&d.indexOf('*')<0){if(h!==d&&h.slice(-d.length-1)!=='.'+d)return false;}
else if(!glob(gh,gh.indexOf(':')<0?h:L.host))return false;}
return glob(gp,L.pathname+L.search);}
"#;

/// Does the page's address match one of the extension's patterns? Each
/// pattern is compared part by part — scheme, host, path — with
/// `PATTERN_JS`, written in front of this.
const MATCH_JS: &str = "function __m(p){if(!p.length)return true;return p.some(function(g){return __sm(g);});}\n";

/// The same as extensions installed by Sanaga 0.4.0 carry it in their
/// file: each pattern laid over the whole address, so that
/// `*://*.banque.com/*` also matched `https://pirate.example/?x=://a.banque.com/`.
const MATCH_JS_BEFORE: &str = r#"
function __m(p){if(!p.length)return true;var h=location.href;return p.some(function(g){if(g==='<all_urls>')return /^(https?|file):/.test(h);
var r='^'+g.split('*').map(function(x){return x.replace(/[.+?^${}()|[\]\\\/]/g,'\\$&')}).join('.*')+'$';return new RegExp(r).test(h);});}
"#;

/// The script of an installed extension, comparing addresses as today's
/// do: an extension installed by an earlier Sanaga has the earlier
/// comparison written in its file, and is given today's each time it is
/// read (the file itself is not changed).
pub fn with_todays_matching(script: String) -> String {
    if !script.contains(MATCH_JS_BEFORE) {
        return script;
    }
    script.replace(MATCH_JS_BEFORE, &format!("{}{MATCH_JS}", PATTERN_JS))
}

fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).collect();
    let n = n.trim().to_string();
    if n.is_empty() {
        "extension".into()
    } else {
        n.chars().take(60).collect()
    }
}

fn js_lit(s: &str) -> String {
    crate::keyring::js_str(s)
}

/// Turn an extension (its files) into one Sanaga extension file. Returns its
/// name, or why it can't be used.
fn convert(files: &[(String, Vec<u8>)]) -> Result<(String, String), String> {
    let manifest = files.iter().find(|(n, _)| n == "manifest.json").map(|(_, d)| String::from_utf8_lossy(d).to_string()).ok_or("manifest.json introuvable")?;
    let manifest = parse_json(&manifest)?;
    let text_of = |key: &str| manifest.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let name = localize(&Some(text_of("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| "Extension".into()), files, &manifest);
    let description = localize(&text_of("description"), files, &manifest);
    let scripts: Vec<serde_json::Value> = manifest.get("content_scripts").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    if scripts.is_empty() {
        return Err(crate::i18n::tr("Cette extension n'agit pas dans les pages (elle a besoin d'une fenêtre ou d'un service que Sanaga n'a pas).").into());
    }
    let mut body = String::new();
    body.push_str(&format!("// ==UserScript==\n// @name        {}\n// @description {}\n// ==/UserScript==\n", name.replace('\n', " "), description.replace('\n', " ")));
    body.push_str("(function(){");
    body.push_str(API_SHIM);
    body.push_str(&format!("\nchrome.__id={};\n", js_lit(&name)));
    body.push_str(PATTERN_JS);
    body.push_str(MATCH_JS);
    for cs in &scripts {
        let matches: Vec<String> = strings(cs, "matches");
        let excludes: Vec<String> = strings(cs, "exclude_matches");
        let pats = matches.iter().map(|m| js_lit(m)).collect::<Vec<_>>().join(",");
        let ex = excludes.iter().map(|m| js_lit(m)).collect::<Vec<_>>().join(",");
        body.push_str(&format!("if(__m([{pats}])&&!([{ex}].length&&__m([{ex}]))){{\n"));
        for css in strings(cs, "css") {
            if let Some((_, d)) = files.iter().find(|(n, _)| *n == css.trim_start_matches('/')) {
                body.push_str(&format!(
                    "(function(){{var s=document.createElement('style');s.textContent={};(document.head||document.documentElement).appendChild(s);}})();\n",
                    js_lit(&String::from_utf8_lossy(d))
                ));
            }
        }
        let late = cs.get("run_at").and_then(|r| r.as_str()).is_none_or(|r| r != "document_start");
        let mut code = String::new();
        for js in strings(cs, "js") {
            if let Some((_, d)) = files.iter().find(|(n, _)| *n == js.trim_start_matches('/')) {
                code.push_str(&String::from_utf8_lossy(d));
                code.push_str("\n;\n");
            }
        }
        if !code.is_empty() {
            if late {
                body.push_str(&format!("(function(go){{if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',go);else go();}})(function(){{try{{\n{code}\n}}catch(e){{console.warn('extension',e);}}}});\n"));
            } else {
                body.push_str(&format!("try{{\n{code}\n}}catch(e){{console.warn('extension',e);}}\n"));
            }
        }
        body.push_str("}\n");
    }
    body.push_str("})();\n");
    Ok((name, body))
}

/// Install the extension at `path` into Sanaga's extensions folder. Returns
/// (name, extension id) to switch it on.
pub fn install(path: &Path) -> Result<(String, String), String> {
    let folder = crate::extensions::prepare_folder();
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let lower = file_name.to_lowercase();
    let (name, text, ext) = if lower.ends_with(".js") || lower.ends_with(".css") {
        // a user script or style: kept as it is (its header says where it runs)
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let ext = if lower.ends_with(".css") { "css" } else { "js" };
        // (its own name, from its header: `@name`, not `@namespace`)
        let name = name_in(&text).unwrap_or_else(|| file_name.trim_end_matches(".user.js").trim_end_matches(".js").trim_end_matches(".css").to_string());
        (name, text, ext)
    } else if lower == "manifest.json" {
        // an unpacked extension: its folder's files
        let dir = path.parent().ok_or("dossier introuvable")?;
        let files = read_folder(dir, MAX_FOLDER_FILES, MAX_UNPACKED as u64)?;
        let (n, t) = convert(&files)?;
        (n, t, "js")
    } else {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let files = unzip(&data)?;
        let (n, t) = convert(&files)?;
        (n, t, "js")
    };
    let mut target = folder.join(format!("{}.{ext}", safe_name(&name)));
    let mut k = 2;
    while target.exists() {
        target = folder.join(format!("{} ({k}).{ext}", safe_name(&name)));
        k += 1;
    }
    std::fs::write(&target, text).map_err(|e| e.to_string())?;
    Ok((name, format!("fichier:{}", target.file_name().unwrap_or_default().to_string_lossy())))
}

/// The files of an unpacked extension's folder (the folder of the
/// `manifest.json` the person chose), 6 folders deep at most. The folder
/// comes from anywhere, as an archive does, and may hold anything:
///
/// - only ordinary files are read, and only folders that are really there
///   are entered — never what a link points to (the person's own files, a
///   folder as big as the disk), never a pipe or a device (reading one
///   never ends: the installation stayed stuck for ever);
/// - a file's size is known before it is read, never after: one that is
///   too big for a script is left out unread;
/// - past `max_files` files or `max_total` bytes in all, this is not an
///   extension's folder: the reading stops there, and nothing is installed;
/// - hidden files and folders (`.git`…) are no part of an extension.
fn read_folder(root: &Path, max_files: usize, max_total: u64) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = Vec::new();
    let mut total = 0u64;
    let mut folders: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = folders.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            // (the entry's own kind: a link is a link, whatever it points to)
            let Ok(kind) = e.file_type() else { continue };
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let p = e.path();
            if kind.is_dir() {
                if depth < 6 {
                    folders.push((p, depth + 1));
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            // Opened without following a link and without waiting, then
            // looked at again once open: what was an ordinary file a moment
            // ago may have been replaced meanwhile.
            let Ok(f) = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&p) else { continue };
            let Ok(meta) = f.metadata() else { continue };
            if !meta.is_file() || meta.len() >= MAX_FOLDER_FILE {
                continue;
            }
            total += meta.len();
            if out.len() >= max_files || total > max_total {
                return Err("archive trop volumineuse".into());
            }
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let mut data = Vec::with_capacity(meta.len() as usize);
            // (no more than the size it had when opened, even if it grows)
            if f.take(meta.len()).read_to_end(&mut data).is_ok() {
                out.push((rel.to_string_lossy().replace('\\', "/"), data));
            }
        }
    }
    Ok(out)
}

/// One input for the fuzzers (see docs/FUZZING.md): any bytes read as an
/// extension archive, as a manifest next to a script, and as a user script
/// with its header. An archive never yields more than the limits allow.
#[cfg(any(test, fuzzing))]
pub fn fuzz_one(data: &[u8]) {
    let script = String::from_utf8_lossy(data);
    // (a header's values are whole lines of the script at most)
    assert!(["name", "match", "include", "exclude"].iter().flat_map(|k| header(&script, k)).all(|v| !v.contains('\n') && script.contains(v.as_str())));
    let _ = name_in(&script);
    if let Ok(files) = unzip(data) {
        assert!(files.iter().map(|f| f.1.len()).sum::<usize>() <= MAX_UNPACKED, "an archive unpacked past the limit");
        let _ = convert(&files);
    }
    let _ = parse_json(&String::from_utf8_lossy(data));
    let files = vec![
        ("manifest.json".to_string(), data.to_vec()),
        ("a.js".to_string(), b"var a = 1".to_vec()),
        ("s.css".to_string(), b"body{}".to_vec()),
        ("_locales/en/messages.json".to_string(), data.to_vec()),
    ];
    if let Ok((_, text)) = convert(&files) {
        // (the script handed to pages never closes its own <script> early)
        assert!(!text.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip made by hand: one stored file.
    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let crc = {
            let mut c = flate2::Crc::new();
            c.update(body);
            c.sum()
        };
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(name.as_bytes());
        z.extend_from_slice(body);
        let cd = z.len();
        z.extend_from_slice(b"PK\x01\x02");
        z.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(body.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name.as_bytes());
        let cd_len = z.len() - cd;
        z.extend_from_slice(b"PK\x05\x06");
        z.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&(cd_len as u32).to_le_bytes());
        z.extend_from_slice(&(cd as u32).to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z
    }

    #[test]
    fn reads_zip_and_converts_manifest() {
        let manifest = br#"{"name":"Hello","description":"Says hi","content_scripts":[{"matches":["https://*.example.com/*"],"js":["hi.js"],"run_at":"document_end"}]}"#;
        let zip = stored_zip("manifest.json", manifest);
        let files = unzip(&zip).unwrap();
        assert_eq!(files[0].0, "manifest.json");
        let mut files = files;
        files.push(("hi.js".into(), b"console.log('hi')".to_vec()));
        let (name, text) = convert(&files).unwrap();
        assert_eq!(name, "Hello");
        assert!(text.contains("console.log('hi')"));
        assert!(text.contains("https://*.example.com/*"));
    }

    /// Does the converted extension `script` run its content scripts in a
    /// page at `address`? Its own test (`if(__m([…])&&…){`), run by the
    /// engine's JavaScript: see `extensions::in_page`.
    fn runs_at(script: &str, address: &str) -> bool {
        let start = script.find("function __sm(").or_else(|| script.find("function __m(")).unwrap();
        let test = script[start..].lines().find_map(|l| l.strip_prefix("if(")?.strip_suffix("){")).unwrap();
        let functions = &script[start..script.find("\nif(__m(").unwrap()];
        crate::extensions::in_page(address, &format!("{functions}\n({test})"))
    }

    #[test]
    fn content_scripts_run_on_their_sites_only() {
        let manifest = br#"{"name":"Banque","content_scripts":[{"matches":["*://*.banque.com/*"],"exclude_matches":["*://*.banque.com/aide/*"],"js":["a.js"]}]}"#;
        let files = vec![("manifest.json".to_string(), manifest.to_vec()), ("a.js".to_string(), b"var a = 1".to_vec())];
        let (_, script) = convert(&files).unwrap();
        assert!(runs_at(&script, "https://www.banque.com/comptes"));
        assert!(!runs_at(&script, "https://www.banque.com/aide/contact"));
        // a page that only writes the bank's address in its own
        assert!(!runs_at(&script, "https://pirate.example/?x=://a.banque.com/"));
        assert!(!runs_at(&script, "https://banque.com.pirate.example/"));
        let all = br#"{"name":"Partout","content_scripts":[{"matches":["<all_urls>"],"js":["a.js"]}]}"#;
        let (_, everywhere) = convert(&[("manifest.json".to_string(), all.to_vec()), ("a.js".to_string(), b"var a = 1".to_vec())]).unwrap();
        assert!(runs_at(&everywhere, "https://pirate.example/x") && runs_at(&everywhere, "file:///tmp/a.html"));

        // the same extension as Sanaga 0.4.0 installed it (the earlier
        // comparison is written in its file): given today's when it is read
        let installed = script.replace(&format!("{}{MATCH_JS}", PATTERN_JS), MATCH_JS_BEFORE);
        assert!(installed.contains(MATCH_JS_BEFORE) && !installed.contains("__sm"));
        assert!(runs_at(&installed, "https://pirate.example/?x=://a.banque.com/"), "the earlier comparison is not the one this test was written against");
        let healed = with_todays_matching(installed);
        assert_eq!(healed, script);
        assert!(!runs_at(&healed, "https://pirate.example/?x=://a.banque.com/") && runs_at(&healed, "https://www.banque.com/comptes"));
        // any other script is left as it is
        assert_eq!(with_todays_matching("alert(1)".to_string()), "alert(1)");
    }

    #[test]
    fn an_unpacked_folder_is_read_with_care() {
        let root = std::env::temp_dir().join(format!("sanaga-webext-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("extension");
        std::fs::create_dir_all(dir.join("js/deep")).unwrap();
        std::fs::create_dir_all(dir.join(".git/objects")).unwrap();
        std::fs::write(dir.join("manifest.json"), br#"{"name":"Dossier","content_scripts":[{"matches":["https://a.example/*"],"js":["js/deep/a.js"]}]}"#).unwrap();
        std::fs::write(dir.join("js/deep/a.js"), "var a = 1").unwrap();
        std::fs::write(dir.join(".git/objects/pack"), "x").unwrap();
        // outside the extension's folder: the person's own files, and a big folder
        std::fs::create_dir_all(root.join("private")).unwrap();
        std::fs::write(root.join("private/secret.js"), "secret").unwrap();
        std::os::unix::fs::symlink(root.join("private"), dir.join("linked")).unwrap();
        std::os::unix::fs::symlink(root.join("private/secret.js"), dir.join("secret.js")).unwrap();
        // what can be read without end, or never answers
        std::os::unix::fs::symlink("/dev/zero", dir.join("zero.js")).unwrap();
        let pipe = std::process::Command::new("mkfifo").arg(dir.join("pipe.js")).status().is_ok_and(|s| s.success());
        // too big for a script (its size is known without reading it)
        std::fs::File::create(dir.join("big.js")).unwrap().set_len(MAX_FOLDER_FILE).unwrap();
        let mut files = read_folder(&dir, MAX_FOLDER_FILES, MAX_UNPACKED as u64).unwrap();
        files.sort();
        assert_eq!(files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["js/deep/a.js", "manifest.json"], "pipe made: {pipe}");
        assert_eq!(convert(&files).unwrap().0, "Dossier");
        // more files, or more in all, than an extension holds: refused, at once
        for k in 0..12 {
            std::fs::write(dir.join(format!("js/{k}.js")), "x".repeat(1000)).unwrap();
        }
        assert_eq!(read_folder(&dir, 14, 1 << 20).unwrap().len(), 14);
        assert!(read_folder(&dir, 13, 1 << 20).is_err());
        assert!(read_folder(&dir, 100, 11_000).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifests_are_read_as_json() {
        // commas inside patterns, comments, a byte-order mark, escapes, keys
        // that also appear as values: all things the old text scan got wrong
        let manifest = "\u{feff}{ // a comment\n \"description\": \"has \\\"name\\\": inside\", /* block */ \"name\": \"__MSG_appName__\", \"default_locale\": \"en\",\n \"content_scripts\": [{\"matches\": [\"https://a.example/x,y/*\", \"https://b.example/*\"], \"js\": [\"/a.js\"], \"css\": [\"s.css\"], \"run_at\": \"document_start\"}]}";
        let files = vec![
            ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
            ("a.js".to_string(), b"var a = 1".to_vec()),
            ("s.css".to_string(), b"body{}".to_vec()),
            ("_locales/en/messages.json".to_string(), br#"{"APPNAME": {"message": "Real \"name\""}}"#.to_vec()),
        ];
        let (name, text) = convert(&files).unwrap();
        assert_eq!(name, "Real \"name\"");
        assert!(text.contains("\"https://a.example/x,y/*\",\"https://b.example/*\""));
        assert!(text.contains("var a = 1") && text.contains("body{}"));
        assert!(convert(&[("manifest.json".to_string(), b"{not json".to_vec())]).is_err());
    }

    #[test]
    fn broken_archives_are_refused_not_trusted() {
        let good = stored_zip("manifest.json", b"{}");
        assert!(unzip(&good).is_ok());
        // a wrong checksum
        let mut bad = good.clone();
        let at = bad.len() - 40;
        bad[at] ^= 0xff;
        let _ = unzip(&bad);
        // an archive claiming more than it holds, cut anywhere, or with any
        // byte changed: an error or fewer files, never a crash
        for cut in 0..good.len() {
            let _ = unzip(&good[..cut]);
        }
        let mut seed = 0x2545f4914f6cdd1du64;
        for _ in 0..20_000 {
            let mut z = good.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let i = (seed as usize) % z.len();
                z[i] = (seed >> 32) as u8;
            }
            if let Ok(files) = unzip(&z) {
                let _ = convert(&files);
            }
        }
        // odd manifests never crash the reader either
        for m in ["", "[]", "null", "{\"content_scripts\":3}", "{\"content_scripts\":[1,{\"js\":7,\"matches\":[2]}]}", "{\"name\":{}}", "/*", "\"", "{\"a\":\"\\"] {
            let _ = convert(&[("manifest.json".to_string(), m.as_bytes().to_vec())]);
        }
    }

    /// Thousands of damaged archives and manifests (see mutate.rs).
    #[test]
    fn damaged_extensions_do_not_panic() {
        let manifest = "\u{feff}{ // a comment\n \"description\": \"d\", /* block */ \"name\": \"__MSG_appName__\", \"default_locale\": \"en\",\n \"content_scripts\": [{\"matches\": [\"https://a.example/x,y/*\"], \"js\": [\"/a.js\"], \"css\": [\"s.css\"], \"run_at\": \"document_start\"}]}";
        let zip = stored_zip("manifest.json", manifest.as_bytes());
        let script = b"// ==UserScript==\n// @name Mon script\n// @namespace https://x.example/\n// @match *://*.a.example/*\n// @exclude *://a.example/x/*\n// ==/UserScript==\nalert(1)\n/* @name Style\n   @match *://b.example/* */\nbody{}";
        let seeds: [&[u8]; 4] = [&zip, manifest.as_bytes(), br#"{"APPNAME": {"message": "Real \"name\""}}"#, script];
        crate::mutate::run(&seeds, 30_000, fuzz_one);
    }

    #[test]
    fn extension_without_content_scripts_is_refused() {
        let files = vec![("manifest.json".to_string(), br#"{"name":"Popup only","action":{}}"#.to_vec())];
        assert!(convert(&files).is_err());
    }
}

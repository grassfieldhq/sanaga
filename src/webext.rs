//! Installing extensions made for other browsers: Chrome (`.crx`, `.zip` or
//! an unpacked folder's `manifest.json`), Firefox (`.xpi`) and user scripts
//! (`.user.js`, Greasemonkey / Tampermonkey). What runs inside the pages (the
//! extension's content scripts and styles) is turned into one of Sanaga's
//! extensions, with a small stand-in for the browser APIs they use most
//! (`chrome.storage`, `chrome.runtime.getURL`). Parts that need a background
//! page or a toolbar pop-up are not supported (WebKitGTK has no extension API).

use std::path::{Path, PathBuf};

/// The files of a zip archive (`.xpi`, `.zip`, or the zip inside a `.crx`).
fn unzip(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    // a .crx starts with its own header: the zip begins at the first "PK\3\4"
    let start = data.windows(4).position(|w| w == b"PK\x03\x04").ok_or("archive illisible")?;
    let zip = &data[start..];
    let u16_at = |b: &[u8], i: usize| -> usize { b.get(i..i + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize).unwrap_or(0) };
    let u32_at = |b: &[u8], i: usize| -> usize { b.get(i..i + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize).unwrap_or(0) };
    // the central directory, found from its end record
    let end = (0..zip.len().saturating_sub(21)).rev().find(|&i| &zip[i..i + 4] == b"PK\x05\x06").ok_or("archive incomplète")?;
    let count = u16_at(zip, end + 10);
    let mut at = u32_at(zip, end + 16);
    let mut files = Vec::new();
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(b"PK\x01\x02") {
            break;
        }
        let method = u16_at(zip, at + 10);
        let csize = u32_at(zip, at + 20);
        let name_len = u16_at(zip, at + 28);
        let extra_len = u16_at(zip, at + 30);
        let comment_len = u16_at(zip, at + 32);
        let local = u32_at(zip, at + 42);
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len).unwrap_or_default()).to_string();
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') || zip.get(local..local + 4) != Some(b"PK\x03\x04") {
            continue;
        }
        let body = local + 30 + u16_at(zip, local + 26) + u16_at(zip, local + 28);
        let Some(raw) = zip.get(body..body + csize) else { continue };
        let content = match method {
            0 => raw.to_vec(),
            8 => {
                use std::io::Read;
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(raw).take(16 << 20).read_to_end(&mut out).map_err(|e| e.to_string())?;
                out
            }
            _ => continue,
        };
        files.push((name.trim_start_matches("./").to_string(), content));
    }
    Ok(files)
}

/// Text of a JSON string value right after `"key":` (enough for manifests).
fn json_str(text: &str, key: &str) -> Option<String> {
    let i = text.find(&format!("\"{key}\""))?;
    let rest = &text[i + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('?'));
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
    None
}

/// The strings of a JSON array value after `"key":`.
fn json_list(text: &str, key: &str) -> Vec<String> {
    let Some(i) = text.find(&format!("\"{key}\"")) else { return Vec::new() };
    let rest = &text[i..];
    let Some(open) = rest.find('[') else { return Vec::new() };
    let Some(close) = rest[open..].find(']') else { return Vec::new() };
    rest[open + 1..open + close].split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect()
}

/// The objects of the `content_scripts` array (as text).
fn content_scripts(manifest: &str) -> Vec<String> {
    let Some(i) = manifest.find("\"content_scripts\"") else { return Vec::new() };
    let rest = &manifest[i..];
    let Some(open) = rest.find('[') else { return Vec::new() };
    let mut out = Vec::new();
    let (mut depth, mut start, mut in_str, mut esc) = (0i32, None, false, false);
    for (k, c) in rest[open + 1..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(k);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push(rest[open + 1 + s..open + 2 + k].to_string());
                    }
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    out
}

/// "__MSG_name__" texts come from the extension's translations.
fn localize(text: &str, files: &[(String, Vec<u8>)], manifest: &str) -> String {
    let Some(key) = text.strip_prefix("__MSG_").and_then(|t| t.strip_suffix("__")) else { return text.to_string() };
    let default = json_str(manifest, "default_locale").unwrap_or_else(|| "en".into());
    for lang in [crate::i18n::language().to_string(), default, "en".into()] {
        let path = format!("_locales/{lang}/messages.json");
        if let Some((_, data)) = files.iter().find(|(n, _)| *n == path) {
            let t = String::from_utf8_lossy(data);
            // "key": { "message": "…" } (keys are not case-sensitive)
            let lower = t.to_lowercase();
            if let Some(i) = lower.find(&format!("\"{}\"", key.to_lowercase())) {
                if let Some(m) = json_str(&t[i..], "message") {
                    return m;
                }
            }
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

/// Does the page's address match one of the extension's patterns?
const MATCH_JS: &str = r#"
function __m(p){if(!p.length)return true;var h=location.href;return p.some(function(g){if(g==='<all_urls>')return /^(https?|file):/.test(h);
var r='^'+g.split('*').map(function(x){return x.replace(/[.+?^${}()|[\]\\\/]/g,'\\$&')}).join('.*')+'$';return new RegExp(r).test(h);});}
"#;

fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).collect();
    let n = n.trim().to_string();
    if n.is_empty() { "extension".into() } else { n.chars().take(60).collect() }
}

fn js_lit(s: &str) -> String {
    crate::keyring::js_str(s)
}

/// Turn an extension (its files) into one Sanaga extension file. Returns its
/// name, or why it can't be used.
fn convert(files: &[(String, Vec<u8>)]) -> Result<(String, String), String> {
    let manifest = files.iter().find(|(n, _)| n == "manifest.json").map(|(_, d)| String::from_utf8_lossy(d).to_string()).ok_or("manifest.json introuvable")?;
    let name = localize(&json_str(&manifest, "name").unwrap_or_else(|| "Extension".into()), files, &manifest);
    let description = localize(&json_str(&manifest, "description").unwrap_or_default(), files, &manifest);
    let scripts = content_scripts(&manifest);
    if scripts.is_empty() {
        return Err(crate::i18n::tr("Cette extension n'agit pas dans les pages (elle a besoin d'une fenêtre ou d'un service que Sanaga n'a pas).").into());
    }
    let mut body = String::new();
    body.push_str(&format!("// ==UserScript==\n// @name        {}\n// @description {}\n// ==/UserScript==\n", name.replace('\n', " "), description.replace('\n', " ")));
    body.push_str("(function(){");
    body.push_str(API_SHIM);
    body.push_str(&format!("\nchrome.__id={};\n", js_lit(&name)));
    body.push_str(MATCH_JS);
    for cs in &scripts {
        let matches: Vec<String> = json_list(cs, "matches");
        let excludes: Vec<String> = json_list(cs, "exclude_matches");
        let pats = matches.iter().map(|m| js_lit(m)).collect::<Vec<_>>().join(",");
        let ex = excludes.iter().map(|m| js_lit(m)).collect::<Vec<_>>().join(",");
        body.push_str(&format!("if(__m([{pats}])&&!([{ex}].length&&__m([{ex}]))){{\n"));
        for css in json_list(cs, "css") {
            if let Some((_, d)) = files.iter().find(|(n, _)| *n == css.trim_start_matches('/')) {
                body.push_str(&format!(
                    "(function(){{var s=document.createElement('style');s.textContent={};(document.head||document.documentElement).appendChild(s);}})();\n",
                    js_lit(&String::from_utf8_lossy(d))
                ));
            }
        }
        let late = json_str(cs, "run_at").map(|r| r != "document_start").unwrap_or(true);
        let mut code = String::new();
        for js in json_list(cs, "js") {
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
        let name = text
            .lines()
            .take(40)
            .find_map(|l| l.split_once("@name").map(|(_, v)| v.trim().to_string()))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| file_name.trim_end_matches(".user.js").trim_end_matches(".js").trim_end_matches(".css").to_string());
        (name, text, ext)
    } else if lower == "manifest.json" {
        // an unpacked extension: its folder's files
        let dir = path.parent().ok_or("dossier introuvable")?;
        let mut files = Vec::new();
        read_folder(dir, dir, &mut files, 0);
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

fn read_folder(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>, depth: usize) {
    if depth > 6 || out.len() > 2000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p: PathBuf = e.path();
        if p.is_dir() {
            read_folder(root, &p, out, depth + 1);
        } else if let (Ok(rel), Ok(data)) = (p.strip_prefix(root), std::fs::read(&p)) {
            if data.len() < 8 << 20 {
                out.push((rel.to_string_lossy().replace('\\', "/"), data));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip made by hand: one stored file.
    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let crc = 0u32;
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

    #[test]
    fn extension_without_content_scripts_is_refused() {
        let files = vec![("manifest.json".to_string(), br#"{"name":"Popup only","action":{}}"#.to_vec())];
        assert!(convert(&files).is_err());
    }
}

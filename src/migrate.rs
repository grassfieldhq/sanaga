//! Import from another browser installed on this computer: favourites,
//! browsing history and passwords from the Chrome family (Chrome, Chromium,
//! Brave, Edge, Vivaldi), favourites and history from Firefox, and passwords
//! from a CSV file exported by any browser. Only the person's own data, only
//! when they ask for it.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use std::path::{Path, PathBuf};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

#[derive(Clone, Debug)]
pub enum Kind {
    /// Chrome-family profile folder, and the names its key may have in the keyring.
    Chromium { profile: PathBuf, keyring_apps: Vec<&'static str> },
    Firefox { profile: PathBuf },
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: String,
    pub kind: Kind,
}

impl Source {
    pub fn has_passwords(&self) -> bool {
        matches!(self.kind, Kind::Chromium { .. })
    }
}

#[derive(Default, Debug)]
pub struct Imported {
    pub bookmarks: Vec<(String, String)>,
    /// (seconds since 1970, address, title)
    pub history: Vec<(u64, String, String)>,
    /// (site, user name, password)
    pub logins: Vec<(String, String, String)>,
    pub problems: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Browsers whose data can be imported.
pub fn detect() -> Vec<Source> {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    let mut out = Vec::new();
    for (name, dir, apps) in [
        ("Google Chrome", "google-chrome", vec!["chrome"]),
        ("Chromium", "chromium", vec!["chromium"]),
        ("Brave", "BraveSoftware/Brave-Browser", vec!["brave", "chromium"]),
        ("Microsoft Edge", "microsoft-edge", vec!["chromium", "microsoft-edge"]),
        ("Vivaldi", "vivaldi", vec!["vivaldi", "chromium"]),
    ] {
        let profile = config.join(dir).join("Default");
        if profile.join("History").is_file() || profile.join("Bookmarks").is_file() {
            out.push(Source { name: name.to_string(), kind: Kind::Chromium { profile, keyring_apps: apps } });
        }
    }
    if let Some(profile) = firefox_profile() {
        out.push(Source { name: "Firefox".into(), kind: Kind::Firefox { profile } });
    }
    out
}

fn firefox_profile() -> Option<PathBuf> {
    let base = home().join(".mozilla/firefox");
    // the profile marked as default in profiles.ini, else a *.default-release folder
    if let Ok(ini) = std::fs::read_to_string(base.join("profiles.ini")) {
        let mut path: Option<String> = None;
        let mut relative = true;
        let mut is_default = false;
        let mut found: Option<PathBuf> = None;
        for line in ini.lines().chain(std::iter::once("[end]")) {
            if line.starts_with('[') {
                if let (Some(p), true) = (&path, is_default) {
                    found = Some(if relative { base.join(p) } else { PathBuf::from(p) });
                }
                path = None;
                relative = true;
                is_default = false;
            } else if let Some(v) = line.strip_prefix("Path=") {
                path = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("IsRelative=") {
                relative = v.trim() == "1";
            } else if line.starts_with("Default=1") {
                is_default = true;
            } else if let Some(v) = line.strip_prefix("Default=") {
                // [Install...] sections name the default profile directly
                if !v.trim().is_empty() && v.trim() != "0" && v.trim() != "1" {
                    let p = base.join(v.trim());
                    if p.join("places.sqlite").is_file() {
                        return Some(p);
                    }
                }
            }
        }
        if let Some(p) = found.filter(|p| p.join("places.sqlite").is_file()) {
            return Some(p);
        }
    }
    std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).find(|p| p.join("places.sqlite").is_file())
}

/// Browsers lock their databases while running: read a copy.
fn open_copy(db: &Path) -> Result<(rusqlite::Connection, PathBuf), String> {
    let tmp = std::env::temp_dir().join(format!("sanaga-import-{}-{}", std::process::id(), db.file_name().and_then(|n| n.to_str()).unwrap_or("db").replace(' ', "_")));
    std::fs::copy(db, &tmp).map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    Ok((conn, tmp))
}

fn site_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }
    Some(u.origin().ascii_serialization())
}

// ---- Chrome family ----

fn derive_key(secret: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(secret, b"saltysalt", 1, &mut k);
    k
}

/// Chrome's saved-password encryption on Linux ("v10" / "v11" + AES-128-CBC).
pub fn decrypt_chrome(blob: &[u8], keys: &[[u8; 16]], strip_domain_hash: bool) -> Option<String> {
    if blob.len() < 3 {
        return None;
    }
    let (prefix, data) = blob.split_at(3);
    if prefix != b"v10" && prefix != b"v11" {
        return String::from_utf8(blob.to_vec()).ok(); // very old, unencrypted
    }
    for k in keys {
        let mut buf = data.to_vec();
        let Ok(plain) = Aes128CbcDec::new(k.into(), &[b' '; 16].into()).decrypt_padded_mut::<Pkcs7>(&mut buf) else { continue };
        // recent Chrome puts a 32-byte hash of the site before the password
        let plain = if strip_domain_hash && plain.len() >= 32 { &plain[32..] } else { plain };
        if let Ok(s) = std::str::from_utf8(plain) {
            return Some(s.to_string());
        }
    }
    None
}

fn chromium_keys(apps: &[&str]) -> Vec<[u8; 16]> {
    let mut keys = Vec::new();
    for app in apps {
        if let Some(secret) = crate::keyring::lookup_attrs(&["application", app]) {
            keys.push(derive_key(secret.as_bytes()));
        }
    }
    keys.push(derive_key(b"peanuts")); // used when no keyring is available
    keys
}

fn chromium_import(profile: &Path, apps: &[&str], bookmarks: bool, history: bool, passwords: bool, out: &mut Imported) {
    if bookmarks {
        match std::fs::read_to_string(profile.join("Bookmarks")) {
            Ok(t) => out.bookmarks = crate::import::parse_chrome(&t),
            Err(e) => out.problems.push(crate::i18n::tf("Favoris : {}", &[&e])),
        }
    }
    if history {
        match open_copy(&profile.join("History")) {
            Ok((db, tmp)) => {
                let rows = db
                    .prepare("SELECT url, title, last_visit_time FROM urls WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT 5000")
                    .and_then(|mut st| {
                        st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, i64>(2)?)))
                            .map(|it| it.flatten().collect::<Vec<_>>())
                    });
                match rows {
                    Ok(rows) => {
                        for (url, title, t) in rows {
                            // Chrome counts microseconds since 1601
                            let secs = (t / 1_000_000) - 11_644_473_600;
                            if secs > 0 && (url.starts_with("http://") || url.starts_with("https://")) {
                                out.history.push((secs as u64, url, title));
                            }
                        }
                    }
                    Err(e) => out.problems.push(crate::i18n::tf("Historique : {}", &[&e])),
                }
                drop(db);
                let _ = std::fs::remove_file(tmp);
            }
            Err(e) => out.problems.push(crate::i18n::tf("Historique : {}", &[&e])),
        }
    }
    if passwords {
        match open_copy(&profile.join("Login Data")) {
            Ok((db, tmp)) => {
                let version: i64 = db.query_row("SELECT value FROM meta WHERE key = 'version'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| v.parse().ok()).unwrap_or(0);
                let keys = chromium_keys(apps);
                let rows = db
                    .prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")
                    .and_then(|mut st| {
                        st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))
                            .map(|it| it.flatten().collect::<Vec<_>>())
                    });
                match rows {
                    Ok(rows) => {
                        let mut failed = 0;
                        for (origin, user, blob) in rows {
                            let Some(site) = site_of(&origin) else { continue };
                            match decrypt_chrome(&blob, &keys, version >= 24) {
                                Some(p) if !p.is_empty() => out.logins.push((site, user, p)),
                                _ => failed += 1,
                            }
                        }
                        if failed > 0 {
                            out.problems.push(crate::i18n::tf("Mots de passe illisibles : {}", &[&failed]));
                        }
                    }
                    Err(e) => out.problems.push(crate::i18n::tf("Mots de passe : {}", &[&e])),
                }
                drop(db);
                let _ = std::fs::remove_file(tmp);
            }
            Err(e) => out.problems.push(crate::i18n::tf("Mots de passe : {}", &[&e])),
        }
    }
}

// ---- Firefox ----

fn firefox_import(profile: &Path, bookmarks: bool, history: bool, out: &mut Imported) {
    let (db, tmp) = match open_copy(&profile.join("places.sqlite")) {
        Ok(x) => x,
        Err(e) => {
            out.problems.push(crate::i18n::tf("Firefox : {}", &[&e]));
            return;
        }
    };
    if history {
        let rows = db
            .prepare("SELECT url, title, last_visit_date FROM moz_places WHERE last_visit_date IS NOT NULL AND hidden = 0 ORDER BY last_visit_date DESC LIMIT 5000")
            .and_then(|mut st| {
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, i64>(2)?)))
                    .map(|it| it.flatten().collect::<Vec<_>>())
            });
        match rows {
            Ok(rows) => {
                for (url, title, t) in rows {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        out.history.push(((t / 1_000_000).max(0) as u64, url, title));
                    }
                }
            }
            Err(e) => out.problems.push(crate::i18n::tf("Historique : {}", &[&e])),
        }
    }
    if bookmarks {
        let rows = db
            .prepare("SELECT p.url, COALESCE(b.title, p.title, '') FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1 AND (p.url LIKE 'http://%' OR p.url LIKE 'https://%')")
            .and_then(|mut st| st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map(|it| it.flatten().collect::<Vec<_>>()));
        match rows {
            Ok(rows) => out.bookmarks = rows,
            Err(e) => out.problems.push(crate::i18n::tf("Favoris : {}", &[&e])),
        }
    }
    drop(db);
    let _ = std::fs::remove_file(tmp);
}

pub fn import(src: &Source, bookmarks: bool, history: bool, passwords: bool) -> Imported {
    let mut out = Imported::default();
    match &src.kind {
        Kind::Chromium { profile, keyring_apps } => chromium_import(profile, keyring_apps, bookmarks, history, passwords, &mut out),
        Kind::Firefox { profile } => firefox_import(profile, bookmarks, history, &mut out),
    }
    out
}

// ---- CSV password files (Chrome, Firefox, Safari, Bitwarden... exports) ----

fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                c => field.push(c),
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                c => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// (site, user, password) from a CSV export with a header line.
pub fn parse_password_csv(text: &str) -> Result<Vec<(String, String, String)>, String> {
    let rows = csv_rows(text.trim_start_matches('\u{feff}'));
    let header: Vec<String> = rows.first().ok_or("fichier vide")?.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
    let col = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let url_c = col(&["url", "origin", "website", "login_uri", "web site"]).ok_or("colonne « url » introuvable")?;
    let user_c = col(&["username", "login", "user", "login_username", "user name"]).ok_or("colonne « username » introuvable")?;
    let pass_c = col(&["password", "login_password"]).ok_or("colonne « password » introuvable")?;
    let mut out = Vec::new();
    for r in rows.iter().skip(1) {
        let (Some(u), Some(name), Some(p)) = (r.get(url_c), r.get(user_c), r.get(pass_c)) else { continue };
        if p.is_empty() {
            continue;
        }
        if let Some(site) = site_of(u.trim()) {
            out.push((site, name.clone(), p.clone()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncryptMut;

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    fn chrome_encrypt(secret: &[u8], plain: &[u8]) -> Vec<u8> {
        let key = derive_key(secret);
        let mut buf = vec![0u8; plain.len() + 16];
        buf[..plain.len()].copy_from_slice(plain);
        let ct = Aes128CbcEnc::new(&key.into(), &[b' '; 16].into()).encrypt_padded_mut::<Pkcs7>(&mut buf, plain.len()).unwrap().to_vec();
        [b"v10".to_vec(), ct].concat()
    }

    #[test]
    fn chrome_passwords_round_trip() {
        let blob = chrome_encrypt(b"peanuts", b"motdepasse123");
        assert_eq!(decrypt_chrome(&blob, &[derive_key(b"peanuts")], false).as_deref(), Some("motdepasse123"));
        // newer format with the 32-byte site hash in front
        let with_hash = [vec![7u8; 32], b"secret".to_vec()].concat();
        let blob = chrome_encrypt(b"peanuts", &with_hash);
        assert_eq!(decrypt_chrome(&blob, &[derive_key(b"wrong"), derive_key(b"peanuts")], true).as_deref(), Some("secret"));
    }

    #[test]
    fn chrome_history_and_logins_from_databases() {
        let dir = std::env::temp_dir().join(format!("sanaga-migrate-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let h = rusqlite::Connection::open(dir.join("History")).unwrap();
        h.execute_batch("CREATE TABLE urls(url TEXT, title TEXT, last_visit_time INTEGER, hidden INTEGER DEFAULT 0);").unwrap();
        // 2024-01-01 00:00:00 UTC in Chrome time
        h.execute("INSERT INTO urls(url, title, last_visit_time) VALUES ('https://example.org/', 'Exemple', ?1)", [(1_704_067_200i64 + 11_644_473_600) * 1_000_000]).unwrap();
        drop(h);
        let l = rusqlite::Connection::open(dir.join("Login Data")).unwrap();
        l.execute_batch("CREATE TABLE meta(key TEXT, value TEXT); INSERT INTO meta VALUES ('version', '40'); CREATE TABLE logins(origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER DEFAULT 0);").unwrap();
        let blob = chrome_encrypt(b"peanuts", &[vec![1u8; 32], b"pa55".to_vec()].concat());
        l.execute("INSERT INTO logins(origin_url, username_value, password_value) VALUES ('https://example.org/login', 'ana', ?1)", [blob]).unwrap();
        drop(l);
        let mut out = Imported::default();
        chromium_import(&dir, &[], false, true, true, &mut out);
        assert_eq!(out.history, vec![(1_704_067_200, "https://example.org/".into(), "Exemple".into())]);
        assert_eq!(out.logins, vec![("https://example.org".into(), "ana".into(), "pa55".into())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn csv_exports() {
        let chrome = "name,url,username,password,note\nex,https://ex.com/login,bob,\"p,\"\"q\",\n";
        assert_eq!(parse_password_csv(chrome).unwrap(), vec![("https://ex.com".into(), "bob".into(), "p,\"q".into())]);
        let firefox = "\"url\",\"username\",\"password\",\"httpRealm\"\n\"https://a.org\",\"x\",\"y\",\"\"\n";
        assert_eq!(parse_password_csv(firefox).unwrap(), vec![("https://a.org".into(), "x".into(), "y".into())]);
    }
}

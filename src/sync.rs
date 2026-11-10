//! Sync between computers through a shared folder (Syncthing, Nextcloud,
//! Dropbox, a USB key...): favourites and reading list are merged item by
//! item (what one computer adds, renames or removes reaches the others, and
//! two computers changing different things lose nothing), and each computer
//! publishes its open tabs ("Onglets sur vos autres appareils", like Safari's
//! iCloud tabs).
//!
//! With a secret phrase, everything Sanaga puts in the folder is encrypted
//! on this computer first (XChaCha20-Poly1305, key from the phrase with
//! Argon2id): the service that carries the folder only sees sealed files.
//! The phrase stays in the system keyring.

use crate::store::Store;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default, Clone)]
pub struct SyncStatus {
    pub last: Option<u64>,
    pub error: String,
    /// Other computers and their open tabs (address, title).
    pub devices: Vec<(String, Vec<(String, String)>)>,
    /// The folder's files are encrypted with the secret phrase.
    pub encrypted: bool,
}

pub fn device_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cet-ordinateur".into())
}

// ------------------------------------------------------------- secret phrase

/// Where the secret phrase is kept in the keyring (profile, site, user).
pub const KEY_SITE: &str = "sanaga://sync";
const KEY_USER: &str = "phrase";

enum Secret {
    /// The keyring has not been asked yet.
    Unasked,
    /// Being asked (on a thread: it may wait for the person to unlock it).
    Loading,
    None,
    Phrase(String),
}

static SECRET: Mutex<Secret> = Mutex::new(Secret::Unasked);
/// The key made from the phrase and the folder's salt (Argon2 takes a
/// moment: made once per run).
static KEY: Mutex<Option<(Vec<u8>, String, [u8; 32])>> = Mutex::new(None);

/// Ask the keyring for the phrase (once, on a thread). False until it answered.
fn secret_ready(profile: &str) -> bool {
    let Ok(mut s) = SECRET.lock() else { return false };
    match *s {
        Secret::Unasked => {
            *s = Secret::Loading;
            let profile = profile.to_string();
            std::thread::spawn(move || {
                let found = crate::keyring::lookup(&profile, KEY_SITE, KEY_USER).filter(|p| !p.is_empty());
                if let Ok(mut s) = SECRET.lock() {
                    if matches!(*s, Secret::Loading) {
                        *s = found.map(Secret::Phrase).unwrap_or(Secret::None);
                    }
                }
            });
            false
        }
        Secret::Loading => false,
        _ => true,
    }
}

fn phrase() -> Option<String> {
    match &*SECRET.lock().ok()? {
        Secret::Phrase(p) => Some(p.clone()),
        _ => None,
    }
}

/// Is a secret phrase set? (None: the keyring has not answered yet.)
pub fn has_secret(profile: &str) -> Option<bool> {
    secret_ready(profile).then(|| phrase().is_some())
}

/// Set the secret phrase (kept in the keyring), or forget it (None).
/// False when the keyring can't keep it.
pub fn set_secret(profile: &str, new: Option<&str>) -> bool {
    let ok = match new {
        Some(p) => crate::keyring::store(profile, KEY_SITE, KEY_USER, p),
        None => {
            crate::keyring::delete(profile, KEY_SITE, KEY_USER);
            true
        }
    };
    if ok {
        if let Ok(mut s) = SECRET.lock() {
            *s = new.map(|p| Secret::Phrase(p.to_string())).unwrap_or(Secret::None);
        }
        if let Ok(mut k) = KEY.lock() {
            *k = None;
        }
    }
    ok
}

// ---------------------------------------------------------------- encryption

/// Sealed files start with this.
const MAGIC: &[u8; 8] = b"SANAGA\x01\n";
const SALT_FILE: &str = "sel.bin";

/// The key of this folder: from the phrase and the folder's salt (made by
/// the first computer that encrypts, then shared with the folder).
fn folder_key(dir: &Path, phrase: &str) -> Result<[u8; 32], String> {
    use chacha20poly1305::aead::{rand_core::RngCore, OsRng};
    let salt_path = dir.join(SALT_FILE);
    let salt = match std::fs::read(&salt_path) {
        Ok(s) if s.len() >= 16 => s,
        _ => {
            let mut s = vec![0u8; 16];
            OsRng.fill_bytes(&mut s);
            crate::store::write_atomic(&salt_path, &s).map_err(|e| e.to_string())?;
            s
        }
    };
    if let Ok(k) = KEY.lock() {
        if let Some((s, p, key)) = k.as_ref() {
            if *s == salt && p == phrase {
                return Ok(*key);
            }
        }
    }
    let mut key = [0u8; 32];
    argon2::Argon2::default().hash_password_into(phrase.as_bytes(), &salt, &mut key).map_err(|e| e.to_string())?;
    if let Ok(mut k) = KEY.lock() {
        *k = Some((salt, phrase.to_string(), key));
    }
    Ok(key)
}

/// Seal `plain` for the file called `name` (a sealed file can't be passed
/// off as another one: its name is part of the seal).
fn seal(key: &[u8; 32], name: &str, plain: &[u8]) -> Vec<u8> {
    use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
    let cipher = chacha20poly1305::XChaCha20Poly1305::new(key.into());
    let nonce = chacha20poly1305::XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let sealed = cipher.encrypt(&nonce, Payload { msg: plain, aad: name.as_bytes() }).unwrap_or_default();
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    out
}

/// Open a sealed file; None when the phrase is not the right one, or the
/// file was changed or is not one of Sanaga's.
fn unseal(key: &[u8; 32], name: &str, data: &[u8]) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    let rest = data.strip_prefix(MAGIC)?;
    if rest.len() < 24 {
        return None;
    }
    let (nonce, sealed) = rest.split_at(24);
    let cipher = chacha20poly1305::XChaCha20Poly1305::new(key.into());
    cipher.decrypt(nonce.into(), Payload { msg: sealed, aad: name.as_bytes() }).ok()
}

fn sealed_path(plain: &Path) -> PathBuf {
    let mut p = plain.as_os_str().to_owned();
    p.push(".enc");
    PathBuf::from(p)
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// A file of the shared folder (None: not there yet). A sealed file needs
/// the phrase; a plain one left from before encryption is still read.
fn read_shared(plain: &Path, key: Option<&[u8; 32]>) -> Result<Option<String>, String> {
    if let Ok(data) = std::fs::read(sealed_path(plain)) {
        let Some(key) = key else { return Err(crate::i18n::tr("Ce dossier est chiffré : saisissez sa phrase secrète.").into()) };
        return match unseal(key, &name_of(plain), &data) {
            Some(text) => Ok(Some(String::from_utf8_lossy(&text).to_string())),
            None => Err(crate::i18n::tr("Phrase secrète incorrecte pour ce dossier.").into()),
        };
    }
    Ok(std::fs::read_to_string(plain).ok())
}

/// Write a file of the shared folder: sealed when there is a key (and its
/// plain copy from before goes), plain otherwise.
fn write_shared(plain: &Path, key: Option<&[u8; 32]>, text: &str) -> std::io::Result<()> {
    match key {
        Some(key) => {
            crate::store::write_atomic(&sealed_path(plain), seal(key, &name_of(plain), text.as_bytes()))?;
            let _ = std::fs::remove_file(plain);
            Ok(())
        }
        None => crate::store::write_atomic(plain, text),
    }
}

// --------------------------------------------------------------------- merge

/// One favourite or reading-list entry: address, title.
type Item = (String, String);

fn parse(text: &str) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for (u, t) in text.lines().filter_map(|l| l.split_once('\t')) {
        if !u.is_empty() && !out.iter().any(|i| i.0 == u) {
            out.push((u.to_string(), t.to_string()));
        }
    }
    out
}

fn format(list: &[Item]) -> String {
    list.iter().map(|(u, t)| format!("{u}\t{}\n", t.replace(['\t', '\n', '\r'], " "))).collect()
}

/// Merge this computer's list with the folder's. `base` is the list as it
/// was after the last sync here: what is in it and gone from one side was
/// removed there (and goes from the other); what is new on a side is added;
/// a title changed on one side wins over the unchanged one. Without a base
/// (first sync) nothing is removed: the two lists are joined. This
/// computer's order is kept; the folder's new entries come after the entry
/// they follow there.
fn merge(base: Option<&[Item]>, local: &[Item], remote: &[Item]) -> Vec<Item> {
    let has = |list: &[Item], u: &str| list.iter().any(|i| i.0 == u);
    let title = |list: &[Item], u: &str| list.iter().find(|i| i.0 == u).map(|i| i.1.clone());
    let in_base = |u: &str| base.is_some_and(|b| has(b, u));
    // what stays, with its title
    let mut kept: Vec<Item> = Vec::new();
    for (u, t) in local {
        // (in the base, gone from the folder: removed on another computer)
        if in_base(u) && !has(remote, u) {
            continue;
        }
        let unchanged_here = base.and_then(|b| title(b, u)).is_some_and(|b| b == *t);
        let t = match title(remote, u) {
            Some(r) if unchanged_here => r,
            _ => t.clone(),
        };
        kept.push((u.clone(), t));
    }
    for (u, t) in remote {
        // (already here, or in the base and gone from here: removed on this computer)
        if !has(local, u) && !in_base(u) {
            kept.push((u.clone(), t.clone()));
        }
    }
    // The order: this computer's when it was changed here since the last
    // sync (or at the first sync), else the folder's — so two computers
    // settle on one order instead of each writing its own for ever. The
    // other side's new entries come after the entry they follow there.
    let moved_here = match base {
        Some(b) => {
            let mine: Vec<&String> = local.iter().map(|i| &i.0).filter(|u| has(b, u)).collect();
            let before: Vec<&String> = b.iter().map(|i| &i.0).filter(|u| has(local, u)).collect();
            mine != before
        }
        None => true,
    };
    let (lead, follow) = if moved_here { (local, remote) } else { (remote, local) };
    let mut out: Vec<Item> = lead.iter().filter_map(|i| kept.iter().find(|k| k.0 == i.0).cloned()).collect();
    for (k, (u, _)) in follow.iter().enumerate() {
        if has(&out, u) {
            continue;
        }
        let Some(item) = kept.iter().find(|i| i.0 == *u).cloned() else { continue };
        let after = follow[..k].iter().rev().find_map(|p| out.iter().position(|o| o.0 == p.0));
        match after {
            Some(i) => out.insert(i + 1, item),
            None => out.push(item),
        }
    }
    out
}

/// Merge one list file with the folder's. True when this computer's list changed.
fn exchange(local: &Path, base: &Path, remote: &Path, key: Option<&[u8; 32]>) -> Result<bool, String> {
    let local_text = std::fs::read_to_string(local).ok();
    let remote_text = read_shared(remote, key)?;
    // the base only counts when both sides are there: a list file that is
    // missing (a new folder, a profile started again) removes nothing
    let base_list = match (&local_text, &remote_text) {
        (Some(_), Some(_)) => std::fs::read_to_string(base).ok().map(|t| parse(&t)),
        _ => None,
    };
    let l = parse(local_text.as_deref().unwrap_or(""));
    let r = parse(remote_text.as_deref().unwrap_or(""));
    let merged = merge(base_list.as_deref(), &l, &r);
    let text = format(&merged);
    // the folder first: if it can't be written, nothing is noted as synced
    let remote_stale = remote_text.is_none() || merged != r || (key.is_some() && remote.exists());
    if remote_stale {
        write_shared(remote, key, &text).map_err(|e| e.to_string())?;
    }
    let changed = merged != l;
    if changed {
        crate::store::write_atomic(local, &text).map_err(|e| e.to_string())?;
    }
    if let Some(dir) = base.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = crate::store::write_atomic(base, &text);
    Ok(changed)
}

/// One sync round. Returns true when local lists were replaced (reload them).
pub fn sync(store: &Store, open_tabs: &[(String, String)], status: &mut SyncStatus) -> bool {
    let folder = store.settings.sync_dir.trim();
    if folder.is_empty() {
        status.error.clear();
        return false;
    }
    let root = PathBuf::from(folder);
    if !root.is_dir() {
        status.error = crate::i18n::tr("Dossier de synchronisation introuvable.").into();
        return false;
    }
    let Some(local_dir) = store.data_dir() else { return false };
    // (the keyring is still being asked for the phrase: next round)
    if !secret_ready(&store.profile) {
        return false;
    }
    let profile = if store.profile.is_empty() { "Personnel" } else { store.profile.as_str() };
    let dir = root.join("Sanaga").join(profile);
    let devices_dir = dir.join("appareils");
    if std::fs::create_dir_all(&devices_dir).is_err() {
        status.error = crate::i18n::tr("Impossible d'écrire dans le dossier de synchronisation.").into();
        return false;
    }
    let key = match phrase() {
        Some(p) => match folder_key(&dir, &p) {
            Ok(k) => Some(k),
            Err(e) => {
                status.error = e;
                return false;
            }
        },
        None => None,
    };
    status.encrypted = key.is_some();
    let mut changed = false;
    for f in ["bookmarks.tsv", "reading.tsv"] {
        match exchange(&local_dir.join(f), &local_dir.join("sync-base").join(f), &dir.join(f), key.as_ref()) {
            Ok(c) => changed |= c,
            Err(e) => {
                // (a wrong or missing phrase: nothing was written anywhere)
                status.error = e;
                return changed;
            }
        }
    }
    // publish this computer's tabs, read the others'
    let me = device_name();
    let tabs: String = open_tabs.iter().map(|(u, t)| format!("{}\t{}\n", u, t.replace(['\t', '\n'], " "))).collect();
    let _ = write_shared(&devices_dir.join(format!("{me}.tsv")), key.as_ref(), &tabs);
    let mut devices: Vec<(String, Vec<(String, String)>)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&devices_dir) {
        for e in rd.flatten() {
            let file = name_of(&e.path());
            let Some(name) = file.strip_suffix(".tsv.enc").or_else(|| file.strip_suffix(".tsv")) else { continue };
            if name == me || devices.iter().any(|d| d.0 == name) {
                continue;
            }
            let Ok(Some(text)) = read_shared(&devices_dir.join(format!("{name}.tsv")), key.as_ref()) else { continue };
            let list: Vec<(String, String)> = text.lines().filter_map(|l| l.split_once('\t').map(|(u, t)| (u.to_string(), t.to_string()))).collect();
            if !list.is_empty() {
                devices.push((name.to_string(), list));
            }
        }
    }
    devices.sort_by(|a, b| a.0.cmp(&b.0));
    status.devices = devices;
    status.error.clear();
    status.last = Some(crate::store::now());
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, &str)]) -> Vec<Item> {
        list.iter().map(|(u, t)| (u.to_string(), t.to_string())).collect()
    }

    #[test]
    fn two_computers_changing_different_things_lose_nothing() {
        let base = items(&[("a", "A"), ("b", "B"), ("c", "C")]);
        // here: b removed, d added; there: a renamed, c removed, e added after a
        let local = items(&[("a", "A"), ("c", "C"), ("d", "D")]);
        let remote = items(&[("a", "A2"), ("e", "E"), ("b", "B")]);
        assert_eq!(merge(Some(&base), &local, &remote), items(&[("a", "A2"), ("d", "D"), ("e", "E")]));
        // a title changed here wins over the folder's unchanged one
        assert_eq!(merge(Some(&base), &items(&[("a", "Mine")]), &items(&[("a", "A")])), items(&[("a", "Mine")]));
        // first sync: nothing is removed, the lists are joined
        assert_eq!(merge(None, &items(&[("a", "A")]), &items(&[("b", "B"), ("a", "A")])), items(&[("a", "A"), ("b", "B")]));
        // merging again changes nothing
        let m = merge(Some(&base), &local, &remote);
        assert_eq!(merge(Some(&m), &m, &m), m);
    }

    #[test]
    fn lists_merge_through_the_folder_and_removals_travel() {
        let root = std::env::temp_dir().join(format!("sanaga-sync-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (pc1, pc2, shared) = (root.join("pc1"), root.join("pc2"), root.join("shared"));
        for d in [&pc1, &pc2, &shared] {
            std::fs::create_dir_all(d).unwrap();
        }
        let f = "bookmarks.tsv";
        let round = |pc: &Path, key: Option<&[u8; 32]>| exchange(&pc.join(f), &pc.join("base").join(f), &shared.join(f), key).unwrap();
        std::fs::write(pc1.join(f), "a\tA\nb\tB\n").unwrap();
        std::fs::write(pc2.join(f), "c\tC\n").unwrap();
        assert!(!round(&pc1, None)); // the folder was empty: pc1 publishes
        assert!(round(&pc2, None)); // pc2 gets a and b, keeps c
        assert_eq!(std::fs::read_to_string(pc2.join(f)).unwrap(), "c\tC\na\tA\nb\tB\n");
        assert!(round(&pc1, None)); // pc1 gets c
        // pc1 removes b; pc2 adds d meanwhile
        assert_eq!(std::fs::read_to_string(pc1.join(f)).unwrap(), "c\tC\na\tA\nb\tB\n"); // one order for both
        std::fs::write(pc1.join(f), "c\tC\na\tA\n").unwrap();
        std::fs::write(pc2.join(f), "c\tC\na\tA\nb\tB\nd\tD\n").unwrap();
        round(&pc1, None);
        assert!(round(&pc2, None));
        assert_eq!(std::fs::read_to_string(pc2.join(f)).unwrap(), "c\tC\na\tA\nd\tD\n");
        assert!(round(&pc1, None));
        assert_eq!(std::fs::read_to_string(pc1.join(f)).unwrap(), "c\tC\na\tA\nd\tD\n");
        // settled: further rounds write nothing
        let before = std::fs::read(shared.join(f)).unwrap();
        assert!(!round(&pc2, None) && !round(&pc1, None));
        assert_eq!(std::fs::read(shared.join(f)).unwrap(), before);
        // an order changed on one computer reaches the other
        std::fs::write(pc2.join(f), "d\tD\nc\tC\na\tA\n").unwrap();
        round(&pc2, None);
        assert!(round(&pc1, None));
        assert_eq!(std::fs::read_to_string(pc1.join(f)).unwrap(), "d\tD\nc\tC\na\tA\n");

        // with a secret phrase the folder only holds sealed files
        let key = folder_key(&shared, "une phrase secrète").unwrap();
        round(&pc1, Some(&key));
        assert!(!shared.join(f).exists());
        let sealed = std::fs::read(sealed_path(&shared.join(f))).unwrap();
        assert!(sealed.starts_with(MAGIC) && !String::from_utf8_lossy(&sealed).contains("a\tA"));
        // the other computer, same phrase: reads it; a wrong phrase or none: refused, nothing written
        assert!(!round(&pc2, Some(&key)));
        let wrong = folder_key(&shared, "autre phrase").unwrap();
        assert_ne!(wrong, key);
        assert!(exchange(&pc2.join(f), &pc2.join("base").join(f), &shared.join(f), Some(&wrong)).is_err());
        assert!(exchange(&pc2.join(f), &pc2.join("base").join(f), &shared.join(f), None).is_err());
        assert_eq!(std::fs::read(sealed_path(&shared.join(f))).unwrap(), sealed);
        // a sealed file can't be passed off as another, nor changed
        assert!(unseal(&key, "reading.tsv", &sealed).is_none());
        let mut changed = sealed.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(unseal(&key, f, &changed).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}

//! Sync between computers through a shared folder (Syncthing, Nextcloud,
//! Dropbox, a USB key...): favourites and reading list follow the most recent
//! change, and each computer publishes its open tabs ("Onglets sur vos autres
//! appareils", like Safari's iCloud tabs).

use crate::store::Store;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Default, Clone)]
pub struct SyncStatus {
    pub last: Option<u64>,
    pub error: String,
    /// Other computers and their open tabs (address, title).
    pub devices: Vec<(String, Vec<(String, String)>)>,
}

pub fn device_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cet-ordinateur".into())
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Copy whichever side changed last, when the two differ.
fn exchange(local: &Path, remote: &Path) -> bool {
    let (l, r) = (std::fs::read(local).ok(), std::fs::read(remote).ok());
    if l == r {
        return false;
    }
    let remote_newer = match (mtime(local), mtime(remote)) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(a), Some(b)) => b > a,
    };
    if remote_newer {
        if let Some(r) = r {
            return crate::store::write_atomic(local, r).is_ok();
        }
    } else if let Some(l) = l {
        let _ = crate::store::write_atomic(remote, l);
    }
    false
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
    let profile = if store.profile.is_empty() { "Personnel" } else { store.profile.as_str() };
    let dir = root.join("Sanaga").join(profile);
    let devices_dir = dir.join("appareils");
    if std::fs::create_dir_all(&devices_dir).is_err() {
        status.error = crate::i18n::tr("Impossible d'écrire dans le dossier de synchronisation.").into();
        return false;
    }
    let mut changed = false;
    for f in ["bookmarks.tsv", "reading.tsv"] {
        changed |= exchange(&local_dir.join(f), &dir.join(f));
    }
    // publish this computer's tabs, read the others'
    let me = device_name();
    let tabs: String = open_tabs.iter().map(|(u, t)| format!("{}\t{}\n", u, t.replace(['\t', '\n'], " "))).collect();
    let _ = crate::store::write_atomic(&devices_dir.join(format!("{me}.tsv")), tabs);
    let mut devices = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&devices_dir) {
        for e in rd.flatten() {
            let name = e.path().file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if name == me || e.path().extension().and_then(|x| x.to_str()) != Some("tsv") {
                continue;
            }
            let list: Vec<(String, String)> = std::fs::read_to_string(e.path())
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.split_once('\t').map(|(u, t)| (u.to_string(), t.to_string())))
                .collect();
            if !list.is_empty() {
                devices.push((name, list));
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

    #[test]
    fn newest_side_wins_and_equal_files_rest() {
        let base = std::env::temp_dir().join(format!("sanaga-sync-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        let (a, b) = (base.join("a.tsv"), base.join("b.tsv"));
        std::fs::write(&a, "x\n").unwrap();
        assert!(!exchange(&a, &b)); // remote missing: local is published
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "x\n");
        assert!(!exchange(&a, &b)); // same content: nothing to do
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&b, "y\n").unwrap();
        assert!(exchange(&a, &b)); // remote newer: taken
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "y\n");
    }
}

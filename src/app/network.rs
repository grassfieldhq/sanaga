//! The settings that reach the network: the proxy, "HTTPS first", where
//! downloads go, and the daily look for a new version.

use super::Sanaga;
use crate::i18n::tf;
use std::path::Path;

/// A day, in seconds: how often a new version is looked for.
const DAY: u64 = 24 * 3600;

impl Sanaga {
    /// The settings about the network and downloads, told to the parts that
    /// act on them: at start, and whenever a setting changes.
    pub(super) fn apply_network_settings(&mut self) {
        tell(&self.store.settings);
        // (the list of reported sites: read, and fetched when a day old)
        if let Some(dir) = &self.web_dir {
            crate::danger::prepare(&dir.join("filters"));
        }
        // The system's own proxy, for Sanaga's requests (the engine asks the
        // system by itself for the pages): asked once, away from the window —
        // an automatic configuration is fetched from the network.
        if self.first_time("system-proxy") {
            self.off_thread(system_proxy, |_, answer| {
                // (it could not say: taken as none, like the engine does)
                crate::net::set_system_proxy(answer.unwrap_or_else(|| vec!["direct://".to_string()]));
            });
        }
        self.apply_dns_relay();
    }

    /// Encrypted DNS: the pages go through Sanaga's relay while it is on
    /// and nothing else stands between Sanaga and the sites (a proxy of the
    /// settings or of the system resolves the names itself).
    fn apply_dns_relay(&self) {
        let relay = if crate::doh::on() && crate::net::goes_direct() { crate::doh::relay() } else { None };
        crate::page::set_dns_relay(relay);
    }

    /// A shortcut named by the window itself (the keyboard's play / pause keys).
    pub fn run_command(&mut self, name: &str) {
        self.run_shortcut(name);
    }

    /// The keyboard's play / pause key: for the tab that is heard, else the
    /// tab shown.
    pub(super) fn media_key(&mut self, play: bool) {
        let tab = self.tabs.iter().position(|t| t.audio && !t.muted).or(Some(self.active));
        if let Some(w) = tab.and_then(|i| self.tabs.get(i)).and_then(|t| t.web.as_ref()) {
            let _ = w.evaluate_script(&format!("{}({play})", crate::page::MEDIA_KEY_JS));
        }
    }

    /// Links, files or words dropped on the interface: each address or
    /// file opens in a new tab, words are searched for (once).
    pub fn open_dropped(&mut self, items: Vec<String>) {
        // (a text dropped may hold several addresses, one per line)
        let lines = items.iter().flat_map(|i| if i.lines().all(|l| l.trim().is_empty() || l.contains("://")) { i.lines().map(str::to_string).collect() } else { vec![i.clone()] });
        let mut searched = false;
        // (a whole folder of files dropped by mistake does not open a hundred tabs)
        for item in lines.filter_map(|l| crate::guard::dropped_item(&l)).take(20) {
            match item {
                Ok(address) => {
                    self.open_tab(&address, false);
                }
                Err(words) if !searched && !words.is_empty() => {
                    searched = true;
                    let search = self.store.settings.engine.search_url(&words);
                    self.open_tab(&search, false);
                }
                Err(_) => {}
            }
        }
    }

    /// Every frame (two numbers compared): Sanaga's own network clients are
    /// made again when the proxy changed, and a new version is looked for
    /// once a day when the settings say so.
    pub(super) fn network_step(&mut self, now: f64) {
        let generation = crate::net::proxy_generation();
        if generation != self.proxy_seen {
            self.proxy_seen = generation;
            self.agent = crate::net::Clients::new();
            self.private_agent = crate::net::Clients::new();
            self.apply_dns_relay();
        }
        // (asked again every hour while Sanaga stays open: the file says
        // whether a day has passed)
        if self.store.settings.update_check && !self.guest && self.screenshot.is_none() && now - self.update_looked_at > 3600.0 {
            self.update_looked_at = now;
            let file = crate::store::Store::root_dir().join("update-check");
            self.off_thread(
                move || daily_update(&file),
                |app, found| {
                    if let Some(version) = found {
                        app.say(&tf("Sanaga {} est disponible (voir la page À propos).", &[&version]));
                        app.update_check = Some(Some(Ok(version)));
                    }
                },
            );
        }
    }
}

/// The settings about the network, told to the parts that act on them
/// (nothing here waits or needs the window).
pub(super) fn tell(st: &crate::store::Settings) {
    crate::net::set_download_choice(&st.download_dir, st.download_ask);
    crate::net::set_proxy(&st.proxy);
    crate::page::set_proxy(&st.proxy);
    crate::page::set_https_first(st.https_first);
    crate::danger::set_on(st.safe_sites);
    // (encrypted DNS: Sanaga's own clients are made again with it)
    if crate::doh::set(&st.doh) {
        crate::net::bump_generation();
    }
}

/// What the system answers for its proxy (GNOME's and KDE's network
/// settings, the environment's variables): proxies in order of preference,
/// `direct://` for none. `None`: it could not say.
fn system_proxy() -> Option<Vec<String>> {
    use gtk::gio::prelude::ProxyResolverExt;
    let resolver = gtk::gio::ProxyResolver::default();
    let answer = resolver.lookup("https://example.org/", None::<&gtk::gio::Cancellable>).ok()?;
    Some(answer.iter().map(|a| a.to_string()).collect())
}

/// The daily look for a new version: at most one request a day (the file
/// keeps when the last one was made and the version already announced).
/// The version to announce, when there is a newer one not announced yet.
fn daily_update(file: &Path) -> Option<String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let kept = std::fs::read_to_string(file).unwrap_or_default();
    let (last, told) = last_look(&kept);
    if !due(now, last) {
        return None;
    }
    // (noted before asking: a network that does not answer is not asked
    // again at every start)
    let _ = crate::store::write_quietly(file, format!("{now}\t{told}\n"));
    let latest = crate::net::latest_release().ok()?;
    if !crate::net::is_newer(&latest, env!("CARGO_PKG_VERSION")) || latest == told {
        return None;
    }
    let _ = crate::store::write_quietly(file, format!("{now}\t{latest}\n"));
    Some(latest)
}

/// What the file of the daily look keeps: when the last one was made
/// (seconds), and the version already announced.
fn last_look(kept: &str) -> (u64, String) {
    let mut parts = kept.trim().split('\t');
    let last = parts.next().and_then(|t| t.parse().ok()).unwrap_or(0);
    (last, parts.next().unwrap_or("").to_string())
}

/// Is it time to look again? A day after the last look — or now when the
/// clock says the last look is in the future (a clock put back).
fn due(now: u64, last: u64) -> bool {
    last > now || now - last >= DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_version_is_looked_for_once_a_day() {
        assert_eq!(last_look(""), (0, String::new()));
        assert_eq!(last_look("1700000000\t0.5.0\n"), (1_700_000_000, "0.5.0".to_string()));
        assert_eq!(last_look("words"), (0, String::new()));
        assert!(due(1_700_000_000, 0));
        assert!(!due(1_700_000_000, 1_700_000_000 - DAY + 1));
        assert!(due(1_700_000_000, 1_700_000_000 - DAY));
        // the clock was put back since the last look
        assert!(due(1_700_000_000, 1_800_000_000));
    }
}

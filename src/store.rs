//! Saved data: settings, history, bookmarks, open tabs. Plain text files in
//! ~/.local/share/sanaga, small and fast to read.

use crate::i18n::{tf, tr};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Engine {
    DuckDuckGo,
    Google,
    Bing,
    Wikipedia,
}

impl Engine {
    pub const ALL: [Engine; 4] = [Engine::DuckDuckGo, Engine::Google, Engine::Bing, Engine::Wikipedia];
    pub fn label(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => tr("DuckDuckGo (version légère)"),
            Engine::Google => "Google",
            Engine::Bing => "Bing",
            Engine::Wikipedia => "Wikipédia",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "ddg",
            Engine::Google => "google",
            Engine::Bing => "bing",
            Engine::Wikipedia => "wiki",
        }
    }
    pub fn from_key(k: &str) -> Engine {
        Engine::ALL.into_iter().find(|e| e.key() == k).unwrap_or(Engine::DuckDuckGo)
    }
    pub fn search_url(self, q: &str) -> String {
        let q: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
        match self {
            Engine::DuckDuckGo => format!("https://html.duckduckgo.com/html/?q={q}"),
            Engine::Google => format!("https://www.google.com/search?q={q}"),
            Engine::Bing => format!("https://www.bing.com/search?q={q}"),
            Engine::Wikipedia => format!("https://fr.wikipedia.org/w/index.php?search={q}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub home: String,
    pub engine: Engine,
    pub images: bool,
    pub dark: bool,
    pub zoom: f32,
    pub restore_tabs: bool,
    pub show_bookmarks_bar: bool,
    /// true: WebKit (full web engine); false: Sanaga's light engine.
    pub webkit: bool,
    /// Reader: text size and theme (0 white, 1 sepia, 2 night).
    pub reader_size: f32,
    pub reader_theme: u8,
    /// Prevent cross-site tracking (WebKit's Intelligent Tracking Prevention).
    pub itp: bool,
    /// Start page sections.
    pub ntp_reading: bool,
    pub ntp_frequent: bool,
    pub ntp_reports: bool,
    /// AutoFill: the person's contact details.
    pub fill_name: String,
    pub fill_email: String,
    pub fill_phone: String,
    pub fill_address: String,
    /// Offer to save passwords.
    pub passwords: bool,
    /// Sync folder (shared by Syncthing, Nextcloud, a USB key...); empty = off.
    pub sync_dir: String,
    /// Memory saver: 0 off, 1 moderate (60 min), 2 balanced (20 min), 3 maximum (5 min).
    pub memory_saver: u8,
    /// Interface language ("fr", "en", "es", "pt", "de").
    pub language: String,
    /// Toolbar buttons shown (see `TOOLBAR_BUTTONS`), comma-separated.
    pub toolbar: String,
}

/// Buttons the toolbar can show: key, shown by default.
pub const TOOLBAR_BUTTONS: [(&str, bool); 17] = [
    ("sidebar", true),
    ("back", true),
    ("forward", true),
    ("reload_left", false),
    ("home", false),
    ("star", true),
    ("reader", true),
    ("reload", true),
    ("newtab", false),
    ("bookmarks", false),
    ("reading", false),
    ("print", false),
    ("downloads", true),
    ("memory", true),
    ("share", true),
    ("overview", true),
    ("mobile", true),
];

pub fn default_toolbar() -> String {
    TOOLBAR_BUTTONS.iter().filter(|b| b.1).map(|b| b.0).collect::<Vec<_>>().join(",")
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            home: "sanaga://nouvel-onglet".into(),
            engine: Engine::DuckDuckGo,
            images: true,
            dark: false,
            zoom: 1.0,
            restore_tabs: true,
            show_bookmarks_bar: true,
            webkit: true,
            reader_size: 19.0,
            reader_theme: 0,
            itp: true,
            ntp_reading: true,
            ntp_frequent: true,
            ntp_reports: true,
            fill_name: String::new(),
            fill_email: String::new(),
            fill_phone: String::new(),
            fill_address: String::new(),
            passwords: true,
            sync_dir: String::new(),
            memory_saver: 2,
            language: crate::i18n::system_language().to_string(),
            toolbar: default_toolbar(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Visit {
    pub time: u64,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub ok: bool,
    pub time: u64,
    pub error: String,
}

pub struct Store {
    dir: Option<PathBuf>,
    /// Profile name ("" = the main profile).
    pub profile: String,
    /// Saved logins (site, user name); the passwords themselves are in the system keyring.
    pub logins: Vec<(String, String)>,
    /// Extensions that are switched on.
    pub extensions_on: Vec<String>,
    /// Site permissions decided by the person: (site, kind, allowed).
    pub permissions: Vec<(String, String, bool)>,
    pub settings: Settings,
    pub history: Vec<Visit>,
    pub bookmarks: Vec<Bookmark>,
    /// Safari-style reading list: pages kept for later.
    pub reading: Vec<Bookmark>,
    /// Zoom remembered per site, like Safari's website settings.
    pub site_zoom: std::collections::HashMap<String, f32>,
    pub downloads: Vec<Download>,
    pub first_run: bool,
}

const MAX_HISTORY: usize = 5000;

impl Store {
    /// In-memory only (used for screenshots so nothing on disk changes).
    pub fn ephemeral() -> Store {
        Store {
            dir: None,
            profile: String::new(),
            logins: Vec::new(),
            extensions_on: vec!["bloqueur-de-publicites".into()],
            permissions: Vec::new(),
            settings: Settings::default(),
            history: Vec::new(),
            bookmarks: default_bookmarks(&Settings::default().language),
            reading: Vec::new(),
            site_zoom: Default::default(),
            downloads: Vec::new(),
            first_run: false,
        }
    }

    /// Sanaga's data folder (the main profile lives here, others in `profils/`).
    pub fn root_dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("sanaga")
    }

    /// Names of the extra profiles.
    pub fn profiles() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(Self::root_dir().join("profils"))
            .map(|d| d.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    pub fn data_dir(&self) -> Option<&PathBuf> {
        self.dir.as_ref()
    }

    #[allow(dead_code)]
    pub fn load() -> Store {
        Self::load_profile("")
    }

    pub fn load_profile(profile: &str) -> Store {
        let dir = if profile.is_empty() { Self::root_dir() } else { Self::root_dir().join("profils").join(profile) };
        let first_run = !dir.join("settings.txt").exists();
        let _ = std::fs::create_dir_all(&dir);
        let mut s = Store { dir: Some(dir.clone()), profile: profile.to_string(), logins: Vec::new(), extensions_on: Vec::new(), permissions: Vec::new(), settings: Settings::default(), history: Vec::new(), bookmarks: Vec::new(), reading: Vec::new(), site_zoom: Default::default(), downloads: Vec::new(), first_run };
        if let Ok(t) = std::fs::read_to_string(dir.join("zoom.tsv")) {
            for line in t.lines() {
                if let Some((h, z)) = line.split_once('\t') {
                    if let Ok(z) = z.parse() {
                        s.site_zoom.insert(h.to_string(), z);
                    }
                }
            }
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("permissions.tsv")) {
            s.permissions = t
                .lines()
                .filter_map(|l| {
                    let mut p = l.split('\t');
                    Some((p.next()?.to_string(), p.next()?.to_string(), p.next()? == "1"))
                })
                .collect();
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("logins.tsv")) {
            s.logins = t.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect();
        }
        match std::fs::read_to_string(dir.join("extensions.txt")) {
            Ok(t) => s.extensions_on = t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect(),
            Err(_) => s.extensions_on = vec!["bloqueur-de-publicites".into()],
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("reading.tsv")) {
            for line in t.lines() {
                if let Some((u, ti)) = line.split_once('\t') {
                    s.reading.push(Bookmark { url: u.into(), title: ti.into() });
                }
            }
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("settings.txt")) {
            // Sanaga was French-only before: an existing install keeps French
            // until the person picks another language.
            if !t.lines().any(|l| l.starts_with("language=")) {
                s.settings.language = "fr".into();
            }
            for line in t.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let st = &mut s.settings;
                    match k {
                        "home" => st.home = v.to_string(),
                        "engine" => st.engine = Engine::from_key(v),
                        "images" => st.images = v == "1",
                        "dark" => st.dark = v == "1",
                        "zoom" => st.zoom = v.parse().unwrap_or(1.0),
                        "restore_tabs" => st.restore_tabs = v == "1",
                        "bookmarks_bar" => st.show_bookmarks_bar = v == "1",
                        "renderer" => st.webkit = v != "lite",
                        "reader_size" => st.reader_size = v.parse().unwrap_or(19.0),
                        "reader_theme" => st.reader_theme = v.parse().unwrap_or(0),
                        "itp" => st.itp = v == "1",
                        "ntp_reading" => st.ntp_reading = v == "1",
                        "ntp_frequent" => st.ntp_frequent = v == "1",
                        "ntp_reports" => st.ntp_reports = v == "1",
                        "fill_name" => st.fill_name = v.to_string(),
                        "fill_email" => st.fill_email = v.to_string(),
                        "fill_phone" => st.fill_phone = v.to_string(),
                        "fill_address" => st.fill_address = v.to_string(),
                        "passwords" => st.passwords = v == "1",
                        "sync_dir" => st.sync_dir = v.to_string(),
                        "memory_saver" => st.memory_saver = v.parse().unwrap_or(2),
                        "language" => st.language = v.to_string(),
                        "toolbar" => {
                            st.toolbar = v.to_string();
                            // buttons added since: shown unless switched off ("-key")
                            {
                                let key = "mobile";
                                if !st.toolbar.split(',').any(|k| k == key || k.strip_prefix('-') == Some(key)) {
                                    st.toolbar.push_str(&format!(",{key}"));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("history.tsv")) {
            for line in t.lines() {
                let mut p = line.splitn(3, '\t');
                if let (Some(tm), Some(u), Some(ti)) = (p.next(), p.next(), p.next()) {
                    s.history.push(Visit { time: tm.parse().unwrap_or(0), url: u.into(), title: ti.into() });
                }
            }
            if s.history.len() > MAX_HISTORY {
                let cut = s.history.len() - MAX_HISTORY;
                s.history.drain(..cut);
                s.rewrite_history();
            }
        }
        match std::fs::read_to_string(dir.join("bookmarks.tsv")) {
            Ok(t) => {
                for line in t.lines() {
                    if let Some((u, ti)) = line.split_once('\t') {
                        s.bookmarks.push(Bookmark { url: u.into(), title: ti.into() });
                    }
                }
            }
            Err(_) => {
                s.bookmarks = default_bookmarks(&s.settings.language);
                s.save_bookmarks();
            }
        }
        if first_run {
            s.save_settings();
        }
        s
    }

    pub fn save_settings(&self) {
        let Some(dir) = &self.dir else { return };
        let st = &self.settings;
        let b = |v: bool| if v { "1" } else { "0" };
        let t = format!(
            "home={}\nengine={}\nimages={}\ndark={}\nzoom={}\nrestore_tabs={}\nbookmarks_bar={}\nrenderer={}\nreader_size={}\nreader_theme={}\nitp={}\nntp_reading={}\nntp_frequent={}\nntp_reports={}\nfill_name={}\nfill_email={}\nfill_phone={}\nfill_address={}\npasswords={}\nsync_dir={}\nmemory_saver={}\nlanguage={}\ntoolbar={}\n",
            clean(&st.home), st.engine.key(), b(st.images), b(st.dark), st.zoom, b(st.restore_tabs), b(st.show_bookmarks_bar), if st.webkit { "webkit" } else { "lite" }, st.reader_size, st.reader_theme, b(st.itp), b(st.ntp_reading), b(st.ntp_frequent), b(st.ntp_reports), clean(&st.fill_name), clean(&st.fill_email), clean(&st.fill_phone), clean(&st.fill_address), b(st.passwords), clean(&st.sync_dir), st.memory_saver, clean(&st.language), clean(&st.toolbar)
        );
        let _ = write_atomic(&dir.join("settings.txt"), t);
    }

    pub fn add_visit(&mut self, url: &str, title: &str) {
        if url.starts_with("sanaga://") {
            return;
        }
        // Collapse an immediate repeat of the same page.
        if let Some(last) = self.history.last_mut() {
            if last.url == url {
                last.time = now();
                if !title.is_empty() {
                    last.title = title.to_string();
                }
                return;
            }
        }
        let v = Visit { time: now(), url: url.to_string(), title: title.to_string() };
        if let Some(dir) = &self.dir {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("history.tsv")) {
                let _ = writeln!(f, "{}\t{}\t{}", v.time, clean(&v.url), clean(&v.title));
            }
        }
        self.history.push(v);
    }

    pub fn rewrite_history(&self) {
        let Some(dir) = &self.dir else { return };
        let mut t = String::new();
        for v in &self.history {
            t.push_str(&format!("{}\t{}\t{}\n", v.time, clean(&v.url), clean(&v.title)));
        }
        let _ = write_atomic(&dir.join("history.tsv"), t);
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.rewrite_history();
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    pub fn toggle_bookmark(&mut self, url: &str, title: &str) {
        if self.is_bookmarked(url) {
            self.bookmarks.retain(|b| b.url != url);
        } else {
            let title = if title.is_empty() { url } else { title };
            self.bookmarks.push(Bookmark { url: url.to_string(), title: title.to_string() });
        }
        self.save_bookmarks();
    }

    pub fn in_reading_list(&self, url: &str) -> bool {
        self.reading.iter().any(|b| b.url == url)
    }

    /// Adds the page to the reading list, or takes it out if it is already there.
    pub fn toggle_reading(&mut self, url: &str, title: &str) {
        if self.in_reading_list(url) {
            self.reading.retain(|b| b.url != url);
        } else {
            let title = if title.is_empty() { url } else { title };
            self.reading.insert(0, Bookmark { url: url.to_string(), title: title.to_string() });
        }
        self.save_reading();
    }

    pub fn save_reading(&self) {
        let Some(dir) = &self.dir else { return };
        let mut t = String::new();
        for b in &self.reading {
            t.push_str(&format!("{}\t{}\n", clean(&b.url), clean(&b.title)));
        }
        let _ = write_atomic(&dir.join("reading.tsv"), t);
    }

    pub fn set_site_zoom(&mut self, host: &str, zoom: f32) {
        if host.is_empty() {
            return;
        }
        if (zoom - self.settings.zoom).abs() < 0.01 {
            self.site_zoom.remove(host);
        } else {
            self.site_zoom.insert(host.to_string(), zoom);
        }
        let Some(dir) = &self.dir else { return };
        let t: String = self.site_zoom.iter().map(|(h, z)| format!("{}\t{}\n", clean(h), z)).collect();
        let _ = write_atomic(&dir.join("zoom.tsv"), t);
    }

    pub fn save_bookmarks(&self) {
        let Some(dir) = &self.dir else { return };
        let mut t = String::new();
        for b in &self.bookmarks {
            t.push_str(&format!("{}\t{}\n", clean(&b.url), clean(&b.title)));
        }
        let _ = write_atomic(&dir.join("bookmarks.tsv"), t);
    }

    /// Read favourites and reading list again (after a sync brought newer ones).
    pub fn reload_lists(&mut self) {
        let Some(dir) = self.dir.clone() else { return };
        let read = |f: &str| -> Vec<Bookmark> {
            std::fs::read_to_string(dir.join(f))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.split_once('\t'))
                .map(|(u, t)| Bookmark { url: u.into(), title: t.into() })
                .collect()
        };
        self.bookmarks = read("bookmarks.tsv");
        self.reading = read("reading.tsv");
    }

    pub fn permission(&self, site: &str, kind: &str) -> Option<bool> {
        self.permissions.iter().find(|(s, k, _)| s == site && k == kind).map(|p| p.2)
    }

    pub fn set_permission(&mut self, site: &str, kind: &str, allow: Option<bool>) {
        self.permissions.retain(|(s, k, _)| !(s == site && k == kind));
        if let Some(a) = allow {
            self.permissions.push((site.to_string(), kind.to_string(), a));
        }
        let Some(dir) = &self.dir else { return };
        let t: String = self.permissions.iter().map(|(s, k, a)| format!("{}\t{}\t{}\n", clean(s), k, if *a { "1" } else { "0" })).collect();
        let _ = write_atomic(&dir.join("permissions.tsv"), t);
    }

    /// Forget visits made since `since` (seconds since 1970; 0 = everything).
    pub fn clear_history_since(&mut self, since: u64) {
        self.history.retain(|v| v.time < since);
        self.rewrite_history();
    }

    pub fn save_logins(&self) {
        let Some(dir) = &self.dir else { return };
        let t: String = self.logins.iter().map(|(a, b)| format!("{}\t{}\n", clean(a), clean(b))).collect();
        let _ = write_atomic(&dir.join("logins.tsv"), t);
    }

    pub fn save_extensions(&self) {
        let Some(dir) = &self.dir else { return };
        let _ = write_atomic(&dir.join("extensions.txt"), self.extensions_on.join("\n"));
    }

    pub fn save_session(&self, urls: &[String]) {
        let Some(dir) = &self.dir else { return };
        let _ = write_atomic(&dir.join("session.txt"), urls.join("\n"));
    }

    pub fn load_session(&self) -> Vec<String> {
        let Some(dir) = &self.dir else { return Vec::new() };
        std::fs::read_to_string(dir.join("session.txt"))
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect())
            .unwrap_or_default()
    }
}

/// Write a data file safely: into a copy first, then put in place in one go,
/// so a crash or power cut while writing never leaves a half-written file.
pub fn write_atomic(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// First bookmarks, in the interface language (Wikipedia in that language too).
fn default_bookmarks(lang: &str) -> Vec<Bookmark> {
    crate::i18n::set_language(lang);
    let wiki = format!("https://{}.wikipedia.org/", if lang.is_empty() { "en" } else { lang });
    [
        (wiki.as_str(), crate::i18n::tr("Wikipédia")),
        ("https://html.duckduckgo.com/html/", "DuckDuckGo"),
        ("https://news.ycombinator.com/", "Hacker News"),
        ("https://text.npr.org/", crate::i18n::tr("NPR Texte")),
        ("https://lite.cnn.com/", "CNN Lite"),
        ("sanaga://bienvenue", crate::i18n::tr("Bienvenue")),
    ]
    .iter()
    .map(|(u, t)| Bookmark { url: u.to_string(), title: t.to_string() })
    .collect()
}

/// "il y a 5 min", "hier", ...
pub fn relative_time(t: u64) -> String {
    let d = now().saturating_sub(t);
    if d < 60 {
        tr("à l'instant").into()
    } else if d < 3600 {
        tf("il y a {} min", &[&(d / 60)])
    } else if d < 86400 {
        tf("il y a {} h", &[&(d / 3600)])
    } else if d < 2 * 86400 {
        tr("hier").into()
    } else if d < 30 * 86400 {
        tf("il y a {} jours", &[&(d / 86400)])
    } else {
        tf("il y a {} mois", &[&(d / (30 * 86400))])
    }
}

pub fn day_group(t: u64) -> &'static str {
    let d = now().saturating_sub(t);
    if d < 86400 {
        tr("Aujourd'hui")
    } else if d < 2 * 86400 {
        tr("Hier")
    } else if d < 7 * 86400 {
        tr("Cette semaine")
    } else if d < 30 * 86400 {
        tr("Ce mois-ci")
    } else {
        tr("Plus ancien")
    }
}

#[cfg(test)]
mod write_tests {
    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_copy() {
        let dir = std::env::temp_dir().join(format!("sanaga-atomic-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("settings.txt");
        super::write_atomic(&f, "a=1\n").unwrap();
        super::write_atomic(&f, "a=2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "a=2\n");
        assert!(!dir.join("settings.txt.tmp").exists());
    }
}

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
    /// Accent colour (index in `theme::ACCENTS`; 0 = the system's, as on
    /// Grassfield; 1 = Grassfield green).
    pub accent: u8,
    /// Light or dark: 0 like the system, 1 light, 2 dark. (`dark` is the
    /// look in use.)
    pub theme: u8,
    /// Where the tabs are: 0 top, 1 bottom, 2 left, 3 right.
    pub tabs_pos: u8,
    /// Where the address bar is: 0 top, 1 bottom, 2 floating.
    pub bar_pos: u8,
    /// Page summaries and extensions written for you: "" (Sanaga's own,
    /// no AI), "ollama" (a model on this computer) or "claude" (Anthropic,
    /// with your API key kept in the keyring).
    pub ai: String,
    /// The model used ("" = the provider's default).
    pub ai_model: String,
    /// Let sites ask to send notifications (off: their requests are refused
    /// quietly, as Firefox's "block new requests").
    pub notif_prompts: bool,
    /// Page translation: "" ask the first time, "google", or "ai" (the AI
    /// chosen for summaries).
    pub translate_with: String,
    /// A border around the window: 0 none, 1 accent colour, 2 gradient,
    /// 3 animated colours; its width (points) and opacity (%).
    pub border: u8,
    pub border_size: u8,
    pub border_alpha: u8,
    /// Compact mode: lower toolbar and tab bar, more room for the page.
    pub compact: bool,
    /// Network blocker of ads and trackers (requests never leave).
    pub blocker: bool,
    /// Low power mode: 0 off, 1 automatic on battery, 2 always.
    pub low_power: u8,
    /// Vertical tabs as a tree (tabs opened from a page under it).
    pub tree_tabs: bool,
    /// The application's icon (0 = the usual one).
    pub app_icon: u8,
    /// The profile's emoji (shown on its badge and its launcher icon).
    pub profile_emoji: String,
    /// The profile's name as shown (empty: its folder's name, "Personnel"
    /// for the main profile) and its colour (0: chosen from the name).
    pub profile_name: String,
    pub profile_color: u8,
    /// Sites protected by a DRM (Netflix…) open by themselves in a browser
    /// that can play them.
    pub drm_auto: bool,
    /// Ctrl+Tab shows the tabs used last as pictures (else: tab bar order).
    pub tab_switcher: bool,
    /// Language pages are translated into ("" = the interface's).
    pub translate_to: String,
    /// Where a tab's close button is: false on the left (macOS), true on the
    /// right (Chrome, Windows).
    pub close_right: bool,
    /// The address bar shows the whole address (Chrome's "Always show full
    /// URLs"); otherwise https:// and www. are left out.
    pub full_urls: bool,
}

/// A container: tabs that keep their own cookies and logins, separate from
/// the other tabs of the same profile (two accounts of one site side by side).
#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub name: String,
    /// Colour (index in the tab-group colours).
    pub color: u8,
}

/// A toolbar button made by the person: opens an address, runs one of
/// Sanaga's commands, or runs a script in the page.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomButton {
    pub label: String,
    /// Emoji or one or two letters drawn on the button.
    pub symbol: String,
    /// 0 address, 1 Sanaga command, 2 script in the page.
    pub kind: u8,
    pub value: String,
}

/// Buttons the toolbar can show: key, shown by default.
pub const TOOLBAR_BUTTONS: [(&str, bool); 18] = [
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
    ("summary", true),
];

pub fn default_toolbar() -> String {
    TOOLBAR_BUTTONS.iter().filter(|b| b.1).map(|b| b.0).collect::<Vec<_>>().join(",")
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            home: "sanaga://newtab".into(),
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
            accent: 0,
            theme: 0,
            tabs_pos: 0,
            bar_pos: 0,
            ai: String::new(),
            ai_model: String::new(),
            notif_prompts: false,
            translate_with: String::new(),
            border: 0,
            border_size: 4,
            border_alpha: 90,
            compact: false,
            blocker: true,
            low_power: 1,
            tree_tabs: true,
            app_icon: 0,
            profile_emoji: String::new(),
            profile_name: String::new(),
            profile_color: 0,
            drm_auto: false,
            tab_switcher: true,
            translate_to: String::new(),
            close_right: false,
            full_urls: false,
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

/// A profile as shown in the profile menu and on the profiles page.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileInfo {
    /// Its folder ("" for the main profile).
    pub key: String,
    pub name: String,
    /// 1..=8: a colour of the palette; 0: chosen from the name.
    pub color: u8,
    pub emoji: String,
    /// Its own picture (avatar.png in its folder).
    pub photo: Option<PathBuf>,
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
    /// Browser identity chosen per site (host).
    pub identities: Vec<(String, crate::identity::Identity)>,
    /// Containers of this profile.
    pub containers: Vec<Container>,
    /// The person's own toolbar buttons.
    pub buttons: Vec<CustomButton>,
}

const MAX_HISTORY: usize = 5000;

impl Store {
    /// In-memory only (used for screenshots so nothing on disk changes).
    pub fn ephemeral() -> Store {
        Store {
            dir: None,
            profile: String::new(),
            logins: Vec::new(),
            extensions_on: vec!["bloqueur-de-publicites".into(), "sans-bannieres-cookies".into()],
            permissions: Vec::new(),
            settings: Settings::default(),
            history: Vec::new(),
            bookmarks: default_bookmarks(&Settings::default().language),
            reading: Vec::new(),
            site_zoom: Default::default(),
            downloads: Vec::new(),
            first_run: false,
            identities: Vec::new(),
            containers: Vec::new(),
            buttons: Vec::new(),
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

    /// The folder of a profile ("": the main one).
    pub fn profile_dir(key: &str) -> PathBuf {
        if key.is_empty() {
            Self::root_dir()
        } else {
            Self::root_dir().join("profils").join(key)
        }
    }

    /// How a profile looks (name, colour, emoji, photo), read from its own
    /// settings: the profile menu shows the other profiles too.
    pub fn profile_info(key: &str) -> ProfileInfo {
        let dir = Self::profile_dir(key);
        let mut info = ProfileInfo { key: key.to_string(), name: String::new(), color: 0, emoji: String::new(), photo: None };
        if let Ok(t) = std::fs::read_to_string(dir.join("settings.txt")) {
            for l in t.lines() {
                match l.split_once('=') {
                    Some(("profile_name", v)) => info.name = v.trim().to_string(),
                    Some(("profile_color", v)) => info.color = v.trim().parse().unwrap_or(0),
                    Some(("profile_emoji", v)) => info.emoji = v.trim().to_string(),
                    _ => {}
                }
            }
        }
        if info.name.is_empty() {
            info.name = if key.is_empty() { crate::i18n::tr("Personnel").to_string() } else { key.to_string() };
        }
        let photo = dir.join("avatar.png");
        if photo.exists() {
            info.photo = Some(photo);
        }
        info
    }

    /// Every profile: the main one first, then the others by name.
    pub fn all_profiles() -> Vec<ProfileInfo> {
        std::iter::once(String::new()).chain(Self::profiles()).map(|k| Self::profile_info(&k)).collect()
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
        // the person's data: closed to other users (folders 0700, files 0600)
        ensure_private_dir(&Self::root_dir());
        ensure_private_dir(&dir);
        close_files(&dir);
        let mut s = Store { dir: Some(dir.clone()), profile: profile.to_string(), logins: Vec::new(), extensions_on: Vec::new(), permissions: Vec::new(), settings: Settings::default(), history: Vec::new(), bookmarks: Vec::new(), reading: Vec::new(), site_zoom: Default::default(), downloads: Vec::new(), first_run, identities: Vec::new(), containers: Vec::new(), buttons: Vec::new() };
        if let Ok(t) = std::fs::read_to_string(dir.join("containers.tsv")) {
            s.containers = t.lines().filter_map(|l| l.split_once('\t')).map(|(n, c)| Container { name: n.to_string(), color: c.parse().unwrap_or(1) }).collect();
        }
        if let Ok(t) = std::fs::read_to_string(dir.join("buttons.tsv")) {
            s.buttons = t
                .lines()
                .filter_map(|l| {
                    let mut p = l.splitn(4, '\t');
                    Some(CustomButton { label: p.next()?.to_string(), symbol: p.next()?.to_string(), kind: p.next()?.parse().ok()?, value: p.next()?.replace("\\n", "\n") })
                })
                .collect();
        }
        crate::site_prefs::set_all(std::fs::read_to_string(dir.join("sites.tsv")).map(|t| t.lines().filter_map(crate::site_prefs::SitePrefs::from_line).collect()).unwrap_or_default());
        if let Ok(t) = std::fs::read_to_string(dir.join("identity.tsv")) {
            s.identities = t.lines().filter_map(|l| l.split_once('\t')).map(|(h, k)| (h.to_string(), crate::identity::Identity::from_key(k))).collect();
        }
        crate::identity::set_choices(s.identities.clone());
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
            Err(_) => s.extensions_on = vec!["bloqueur-de-publicites".into(), "sans-bannieres-cookies".into()],
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
            // the dark switch of earlier versions becomes the "Dark" choice
            if !t.lines().any(|l| l.starts_with("theme=")) && t.lines().any(|l| l == "dark=1") {
                s.settings.theme = 2;
            }
            if !t.lines().any(|l| l.starts_with("language=")) {
                s.settings.language = "fr".into();
            }
            for line in t.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let st = &mut s.settings;
                    match k {
                        "home" => st.home = crate::app::english_address(v),
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
                        "accent" => st.accent = v.parse().unwrap_or(0),
                        "theme" => st.theme = v.parse().unwrap_or(0),
                        "tabs_pos" => st.tabs_pos = v.parse().unwrap_or(0),
                        "bar_pos" => st.bar_pos = v.parse().unwrap_or(0),
                        "ai" => st.ai = v.to_string(),
                        "ai_model" => st.ai_model = v.to_string(),
                        "notif_prompts" => st.notif_prompts = v == "1",
                        "translate_with" => st.translate_with = v.to_string(),
                        "border" => st.border = v.parse().unwrap_or(0),
                        "border_size" => st.border_size = v.parse().unwrap_or(4),
                        "border_alpha" => st.border_alpha = v.parse().unwrap_or(90),
                        "compact" => st.compact = v == "1",
                        "blocker" => st.blocker = v == "1",
                        "low_power" => st.low_power = v.parse().unwrap_or(1),
                        "tree_tabs" => st.tree_tabs = v == "1",
                        "app_icon" => st.app_icon = v.parse().unwrap_or(0),
                        "profile_emoji" => st.profile_emoji = v.to_string(),
                        "profile_name" => st.profile_name = v.to_string(),
                        "profile_color" => st.profile_color = v.parse().unwrap_or(0),
                        "drm_auto" => st.drm_auto = v == "1",
                        "tab_switcher" => st.tab_switcher = v == "1",
                        "translate_to" => st.translate_to = v.to_string(),
                        "close_right" => st.close_right = v == "1",
                        "full_urls" => st.full_urls = v == "1",
                        "toolbar" => {
                            st.toolbar = v.to_string();
                            // buttons added since: shown unless switched off ("-key")
                            for key in ["mobile", "summary"] {
                                if !st.toolbar.split(',').any(|k| k == key || k.strip_prefix('-') == Some(key) || k.strip_prefix('~') == Some(key)) {
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
        // the favourites of earlier versions, never changed by the person:
        // replaced by today's
        if is_old_default(&s.bookmarks) {
            s.bookmarks = default_bookmarks(&s.settings.language);
            s.save_bookmarks();
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
            "home={}\nengine={}\nimages={}\ndark={}\nzoom={}\nrestore_tabs={}\nbookmarks_bar={}\nrenderer={}\nreader_size={}\nreader_theme={}\nitp={}\nntp_reading={}\nntp_frequent={}\nntp_reports={}\nfill_name={}\nfill_email={}\nfill_phone={}\nfill_address={}\npasswords={}\nsync_dir={}\nmemory_saver={}\nlanguage={}\ntoolbar={}\naccent={}\ntheme={}\ntabs_pos={}\nbar_pos={}\nai={}\nai_model={}\nnotif_prompts={}\ntranslate_with={}\n",
            clean(&st.home), st.engine.key(), b(st.images), b(st.dark), st.zoom, b(st.restore_tabs), b(st.show_bookmarks_bar), if st.webkit { "webkit" } else { "lite" }, st.reader_size, st.reader_theme, b(st.itp), b(st.ntp_reading), b(st.ntp_frequent), b(st.ntp_reports), clean(&st.fill_name), clean(&st.fill_email), clean(&st.fill_phone), clean(&st.fill_address), b(st.passwords), clean(&st.sync_dir), st.memory_saver, clean(&st.language), clean(&st.toolbar), st.accent, st.theme, st.tabs_pos, st.bar_pos, clean(&st.ai), clean(&st.ai_model), b(st.notif_prompts), clean(&st.translate_with)
        );
        let more = format!(
            "border={}\nborder_size={}\nborder_alpha={}\ncompact={}\nblocker={}\nlow_power={}\ntree_tabs={}\napp_icon={}\nprofile_emoji={}\ndrm_auto={}\ntab_switcher={}\ntranslate_to={}\nclose_right={}\nprofile_name={}\nprofile_color={}\nfull_urls={}\n",
            st.border, st.border_size, st.border_alpha, b(st.compact), b(st.blocker), st.low_power, b(st.tree_tabs), st.app_icon, clean(&st.profile_emoji), b(st.drm_auto), b(st.tab_switcher), clean(&st.translate_to), b(st.close_right), clean(&st.profile_name), st.profile_color, b(st.full_urls)
        );
        let _ = write_atomic(&dir.join("settings.txt"), t + &more);
    }

    pub fn save_containers(&self) {
        let Some(dir) = &self.dir else { return };
        let t: String = self.containers.iter().map(|c| format!("{}\t{}\n", clean(&c.name).replace('\t', " "), c.color)).collect();
        let _ = write_atomic(&dir.join("containers.tsv"), t);
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn save_buttons(&self) {
        let Some(dir) = &self.dir else { return };
        let t: String = self
            .buttons
            .iter()
            .map(|b| format!("{}\t{}\t{}\t{}\n", b.label.replace(['\t', '\n'], " "), b.symbol.replace(['\t', '\n'], " "), b.kind, b.value.replace('\t', "    ").replace('\n', "\\n")))
            .collect();
        let _ = write_atomic(&dir.join("buttons.tsv"), t);
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Website settings (JavaScript, fonts, blocker, look) of every site.
    pub fn save_site_prefs(&self, list: &[(String, crate::site_prefs::SitePrefs)]) {
        crate::site_prefs::set_all(list.to_vec());
        let Some(dir) = &self.dir else { return };
        let t: String = list.iter().filter(|(_, p)| !p.is_default()).map(|(h, p)| p.to_line(h) + "\n").collect();
        let _ = write_atomic(&dir.join("sites.tsv"), t);
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
            use std::os::unix::fs::OpenOptionsExt;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(dir.join("history.tsv")) {
                let _ = writeln!(f, "{}\t{}\t{}", v.time, clean(&v.url), clean(&v.title));
                changed();
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

    /// How Sanaga introduces itself to sites the person chose for.
    pub fn save_identities(&self) {
        let Some(dir) = &self.dir else { return };
        let t: String = self.identities.iter().map(|(h, i)| format!("{}\t{}\n", clean(h), i.key())).collect();
        let _ = write_atomic(&dir.join("identity.tsv"), t);
        crate::identity::set_choices(self.identities.clone());
    }

    pub fn save_extensions(&self) {
        let Some(dir) = &self.dir else { return };
        let _ = write_atomic(&dir.join("extensions.txt"), self.extensions_on.join("\n"));
    }

    /// The tabs of window `slot` (0 = the first window: `session.txt`; the
    /// others: `session-N.txt`).
    fn session_file(dir: &Path, slot: usize) -> PathBuf {
        if slot == 0 { dir.join("session.txt") } else { dir.join(format!("session-{slot}.txt")) }
    }

    pub fn save_session(&self, slot: usize, urls: &[String]) {
        let Some(dir) = &self.dir else { return };
        let _ = write_atomic(&Self::session_file(dir, slot), urls.join("\n"));
    }

    pub fn load_session(&self, slot: usize) -> Vec<String> {
        let Some(dir) = &self.dir else { return Vec::new() };
        read_lines(&Self::session_file(dir, slot))
    }

    /// A window was closed while others stay open: its tabs are not reopened.
    pub fn forget_session(&self, slot: usize) {
        let Some(dir) = &self.dir else { return };
        let _ = std::fs::remove_file(Self::session_file(dir, slot));
    }

    /// The tabs of the other windows of the last session (their files are
    /// removed: the windows write them again under their new numbers).
    pub fn take_extra_sessions(&self) -> Vec<Vec<String>> {
        let Some(dir) = &self.dir else { return Vec::new() };
        let mut found: Vec<(usize, PathBuf)> = std::fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let n = name.strip_prefix("session-")?.strip_suffix(".txt")?.parse().ok()?;
                        Some((n, e.path()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        let mut out = Vec::new();
        for (_, path) in found {
            let tabs = read_lines(&path);
            let _ = std::fs::remove_file(&path);
            if !tabs.is_empty() {
                out.push(tabs);
            }
        }
        out
    }
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect()).unwrap_or_default()
}

/// Changes to the data files made by this Sanaga (any window): another window
/// of the same profile sees the number change and reads the files again.
pub static CHANGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn changed() {
    CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

// (private folders and permissions: see guard.rs, testable without GTK)
pub use crate::guard::{close_files, ensure_private_dir, runtime_dir, PrivateDir};

/// Write a data file safely: into a copy first, then put in place in one go,
/// so a crash or power cut while writing never leaves a half-written file.
/// The copy is a new file only this user can read (0600, never a file or
/// link that was already there).
pub fn write_atomic(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(data.as_ref())?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    // (each window has its own session file: no need to tell the others)
    if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("session")) {
        changed();
    }
    Ok(())
}

/// First bookmarks, in the interface language (Wikipedia in that language too).
/// Exactly the favourites Sanaga used to start with (any language).
fn is_old_default(list: &[Bookmark]) -> bool {
    let urls: Vec<&str> = list.iter().map(|b| b.url.trim_end_matches('/')).collect();
    urls.len() == 6
        && urls[0].starts_with("https://")
        && urls[0].ends_with(".wikipedia.org")
        && urls[1..] == ["https://html.duckduckgo.com/html", "https://news.ycombinator.com", "https://text.npr.org", "https://lite.cnn.com", "sanaga://welcome"]
}

fn default_bookmarks(lang: &str) -> Vec<Bookmark> {
    crate::i18n::set_language(lang);
    [
        ("https://wajub.com/", "Wajub"),
        ("https://komerxe.com/", "Komerxe"),
        ("https://moniwa.app/", "Moniwa"),
        ("https://lekalao.com/", "Lekalao"),
        ("https://grassfield.site/", "Grassfield"),
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
    fn data_files_and_folders_are_closed_to_others() {
        use super::{close_files, ensure_private_dir};
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("sanaga-perms-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let inner = dir.join("profils").join("Travail");
        ensure_private_dir(&inner);
        assert_eq!(std::fs::metadata(&inner).unwrap().permissions().mode() & 0o777, 0o700);
        let f = inner.join("session.txt");
        super::write_atomic(&f, "https://a.example/").unwrap();
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        // a file an earlier version left open is closed
        let old = inner.join("history.tsv");
        std::fs::write(&old, "x").unwrap();
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o644)).unwrap();
        close_files(&inner);
        assert_eq!(std::fs::metadata(&old).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn each_window_keeps_its_own_tabs() {
        let dir = std::env::temp_dir().join(format!("sanaga-windows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let mut s = super::Store::ephemeral();
        s.dir = Some(dir.clone());
        s.save_session(0, &["https://a.example/".into()]);
        s.save_session(3, &["https://b.example/".into(), "https://c.example/".into()]);
        s.save_session(5, &["https://d.example/".into()]);
        // a window closed while others stayed open is not reopened
        s.forget_session(5);
        assert_eq!(s.load_session(0), vec!["https://a.example/".to_string()]);
        assert_eq!(s.take_extra_sessions(), vec![vec!["https://b.example/".to_string(), "https://c.example/".to_string()]]);
        assert!(s.take_extra_sessions().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

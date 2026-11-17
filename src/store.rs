//! Saved data: settings, bookmarks, open tabs. Plain text files in
//! ~/.local/share/sanaga, small and fast to read. (The history, which
//! grows without end, is a database of its own: see history.rs.)

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
    /// Memory saver: 0 off, 1 moderate (6 h), 2 balanced (4 h), 3 maximum (2 h).
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
    /// The big block lists (EasyList, EasyPrivacy), fetched once a week.
    pub block_lists: bool,
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
    /// Offer to translate a page written in another language than the one
    /// pages are translated into; the sites where the person said no.
    pub translate_offer: bool,
    pub translate_skip: Vec<String>,
    /// The assistant may be given tools (to look into the history, the
    /// tabs, the favourites, and to act when the person accepts), and its
    /// conversations are kept on this computer.
    pub ai_tools: bool,
    pub keep_chats: bool,
    /// The engine version the person was last told is too old (told once
    /// per version, not at every start).
    pub engine_warned: String,
    /// Where downloads go ("" = the system's Downloads folder), and whether
    /// each download asks where to save it.
    pub download_dir: String,
    pub download_ask: bool,
    /// Sites are tried over https first when an address says http:// or
    /// nothing (the engine falls back to http when the site has no https).
    pub https_first: bool,
    /// The proxy pages go through: "" the system's, "none" a direct
    /// connection, else an address (http://host:3128, socks5://host:1080).
    pub proxy: String,
    /// Look once a day whether a newer Sanaga was published (one request to
    /// the releases page; off unless the person asks).
    pub update_check: bool,
    /// Warn before a page listed as phishing or malware (the list is
    /// fetched with the block lists; no address is ever sent anywhere).
    pub safe_sites: bool,
    /// Encrypted DNS: the address of the resolver asked over https for the
    /// sites' addresses (`https://…/dns-query`); "" = the system's DNS.
    pub doh: String,
}

impl Settings {
    /// Put a category of the settings (its name in the settings' address:
    /// "appearance", "privacy"…) back as Sanaga comes. What is the person's
    /// own stays: the language, the contact details, the sync folder, the
    /// profile's name and picture. True when something changed.
    pub fn restore(&mut self, category: &str) -> bool {
        let d = Settings::default();
        let before = format!("{self:?}");
        match category {
            "general" => {
                self.home = d.home;
                self.engine = d.engine;
                self.restore_tabs = d.restore_tabs;
                self.drm_auto = d.drm_auto;
                self.download_ask = d.download_ask;
                self.update_check = d.update_check;
            }
            "appearance" => {
                self.theme = d.theme;
                self.accent = d.accent;
                self.tabs_pos = d.tabs_pos;
                self.bar_pos = d.bar_pos;
                self.show_bookmarks_bar = d.show_bookmarks_bar;
                self.zoom = d.zoom;
                self.toolbar = d.toolbar;
                self.border = d.border;
                self.border_size = d.border_size;
                self.border_alpha = d.border_alpha;
                self.compact = d.compact;
                self.tree_tabs = d.tree_tabs;
                self.close_right = d.close_right;
                self.full_urls = d.full_urls;
                self.tab_switcher = d.tab_switcher;
                self.ntp_reading = d.ntp_reading;
                self.ntp_frequent = d.ntp_frequent;
                self.ntp_reports = d.ntp_reports;
                self.reader_size = d.reader_size;
                self.reader_theme = d.reader_theme;
            }
            "intelligence" => {
                self.ai = d.ai;
                self.ai_model = d.ai_model;
                self.translate_with = d.translate_with;
                self.translate_to = d.translate_to;
                self.translate_offer = d.translate_offer;
                self.translate_skip = d.translate_skip;
                self.ai_tools = d.ai_tools;
                self.keep_chats = d.keep_chats;
            }
            "privacy" => {
                self.blocker = d.blocker;
                self.block_lists = d.block_lists;
                self.itp = d.itp;
                self.notif_prompts = d.notif_prompts;
                self.https_first = d.https_first;
                self.safe_sites = d.safe_sites;
                self.doh = d.doh;
            }
            "passwords" => self.passwords = d.passwords,
            "performance" => {
                self.memory_saver = d.memory_saver;
                self.low_power = d.low_power;
                self.images = d.images;
                self.webkit = d.webkit;
            }
            _ => {}
        }
        format!("{self:?}") != before
    }

    /// Would `restore` change something in this category?
    pub fn differs_from_origin(&self, category: &str) -> bool {
        self.clone().restore(category)
    }
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
            block_lists: true,
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
            translate_offer: true,
            translate_skip: Vec::new(),
            ai_tools: true,
            keep_chats: true,
            engine_warned: String::new(),
            download_dir: String::new(),
            download_ask: false,
            https_first: true,
            proxy: String::new(),
            update_check: false,
            safe_sites: true,
            doh: String::new(),
        }
    }
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
    /// The pages visited: asked for what is shown, never read whole (see
    /// history.rs).
    pub history: crate::history::History,
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
    /// Pages that told a new title a moment after another one (address,
    /// title), and since when: written to the history a little later
    /// (`settle_history`).
    titles_late: Vec<(String, String)>,
    titles_since: Option<std::time::Instant>,
    /// The pages whose title was written to the history a moment ago, and when.
    titles_written: Vec<(String, std::time::Instant)>,
    /// The files of the profile that were there and could not be read when
    /// it was loaded (a disk error, a refusal): what they hold is not in
    /// this store, so this store never writes them (see `file`).
    unreadable: Vec<&'static str>,
}

/// What reading one of the profile's files gave.
enum Found {
    /// There is no such file: nothing was saved there yet.
    Missing,
    Text(String),
    /// The file is there and could not be read.
    Unreadable,
}

/// Read one of the profile's files. Only a file that is not there counts as
/// "nothing saved yet". A file holding bytes that are not text (a damaged
/// block, a save cut short) is still read: every line that can be read is
/// kept, the damaged characters show as "\u{fffd}". Any other failure says
/// nothing of what the file holds. (Every failure used to read as "no
/// file": one damaged byte in the favourites, and they were replaced by the
/// five first ones, on the disk too.)
fn find(path: &Path) -> Found {
    match std::fs::read(path) {
        Ok(bytes) => Found::Text(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Found::Missing,
        Err(e) => {
            // (said once for each file: the profile is read again at every change)
            static SAID: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
            let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
            if !said.iter().any(|p| p == path) {
                said.push(path.to_path_buf());
                eprintln!("Sanaga: {} could not be read ({e}): it is left as it is, and not written during this run", path.display());
            }
            Found::Unreadable
        }
    }
}

/// The text of a file read for showing only (a profile's name, its look):
/// None when it is not there or cannot be read.
fn read_text(path: &Path) -> Option<String> {
    match find(path) {
        Found::Text(t) => Some(t),
        Found::Missing | Found::Unreadable => None,
    }
}

/// A page that renames itself again and again (a counter, "(3) Messages")
/// has its title written to the history this often at most.
const TITLE_PAUSE: std::time::Duration = std::time::Duration::from_secs(20);

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
            history: crate::history::History::in_memory(),
            bookmarks: default_bookmarks(&Settings::default().language),
            reading: Vec::new(),
            site_zoom: Default::default(),
            downloads: Vec::new(),
            first_run: false,
            identities: Vec::new(),
            containers: Vec::new(),
            buttons: Vec::new(),
            titles_late: Vec::new(),
            titles_since: None,
            titles_written: Vec::new(),
            unreadable: Vec::new(),
        }
    }

    /// Sanaga's data folder (the main profile lives here, others in `profils/`).
    pub fn root_dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))).unwrap_or_else(|| PathBuf::from(".")).join("sanaga")
    }

    /// Light or dark, read straight from a profile's settings before
    /// anything else is loaded: GTK must know it before it reads its theme
    /// (told later, it reads the whole theme a second time).
    pub fn peek_dark(profile: &str) -> bool {
        let text = read_text(&Self::profile_dir(profile).join("settings.txt")).unwrap_or_default();
        let value = |key: &str| text.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')));
        match value("theme") {
            Some("1") => false,
            Some("2") => true,
            _ => crate::system_look::current().map(|l| l.dark).unwrap_or(value("dark") == Some("1")),
        }
    }

    /// How a profile reaches the network (its proxy, its encrypted DNS,
    /// "HTTPS first", the warning before dangerous sites), read straight
    /// from its settings. A guest window keeps nothing of the profile — but
    /// it must not go round the proxy its owner set, nor ask in clear the
    /// names the owner has asked not to.
    pub fn peek_network(profile: &str, into: &mut Settings) {
        let text = read_text(&Self::profile_dir(profile).join("settings.txt")).unwrap_or_default();
        let value = |key: &str| text.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')));
        if let Some(v) = value("proxy") {
            into.proxy = v.to_string();
        }
        if let Some(v) = value("doh") {
            into.doh = v.to_string();
        }
        if let Some(v) = value("https_first") {
            into.https_first = v == "1";
        }
        if let Some(v) = value("safe_sites") {
            into.safe_sites = v == "1";
        }
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
        if let Some(t) = read_text(&dir.join("settings.txt")) {
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
        let mut v: Vec<String> =
            std::fs::read_dir(Self::root_dir().join("profils")).map(|d| d.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
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
        // the person's data: closed to other users (folders 0700, files 0600)
        ensure_private_dir(&Self::root_dir());
        Self::load_from(Self::profile_dir(profile), profile)
    }

    /// The text of one of this profile's files, read when the profile is
    /// loaded. None when there is nothing to read: no such file yet, or a
    /// file that cannot be read — remembered, so that this store never
    /// replaces it with the little it holds instead (see `file`).
    fn text_of(&mut self, name: &'static str) -> Option<String> {
        match find(&self.dir.as_ref()?.join(name)) {
            Found::Text(t) => Some(t),
            Found::Missing => None,
            Found::Unreadable => {
                self.unreadable.push(name);
                None
            }
        }
    }

    /// Where this profile saves the file `name`. None when nothing is kept
    /// (a window that leaves no trace), and when the file was there and
    /// could not be read as the profile was loaded: its content is not in
    /// this store, and saving would replace it with an empty list or with
    /// the settings Sanaga starts with. It is left alone until it can be
    /// read again (the profile is read again at every change, and at the
    /// next start).
    fn file(&self, name: &str) -> Option<PathBuf> {
        if self.unreadable.contains(&name) {
            return None;
        }
        Some(self.dir.as_ref()?.join(name))
    }

    /// The files of this profile that could not be read (their names:
    /// "bookmarks.tsv", "settings.txt"…): what they hold is missing from
    /// this window, and what is changed there meanwhile is not saved.
    /// (For the window to say so; nothing shows it yet.)
    #[allow(dead_code)]
    pub fn unreadable(&self) -> &[&'static str] {
        &self.unreadable
    }

    /// Read a profile from its folder.
    fn load_from(dir: PathBuf, profile: &str) -> Store {
        // (what this run is still writing is read back whole)
        flush_files();
        // (a first run: no settings yet. Settings that are there and cannot
        // be read are not a first run, and are not written again)
        let first_run = matches!(std::fs::symlink_metadata(dir.join("settings.txt")), Err(e) if e.kind() == std::io::ErrorKind::NotFound);
        ensure_private_dir(&dir);
        close_files(&dir);
        let mut s = Store {
            dir: Some(dir.clone()),
            profile: profile.to_string(),
            logins: Vec::new(),
            extensions_on: Vec::new(),
            permissions: Vec::new(),
            settings: Settings::default(),
            // (the same history for every window of the profile; the file
            // is opened by the history's own thread, not here)
            history: crate::history::History::open(&dir),
            bookmarks: Vec::new(),
            reading: Vec::new(),
            site_zoom: Default::default(),
            downloads: Vec::new(),
            first_run,
            identities: Vec::new(),
            containers: Vec::new(),
            buttons: Vec::new(),
            titles_late: Vec::new(),
            titles_since: None,
            titles_written: Vec::new(),
            unreadable: Vec::new(),
        };
        if let Some(t) = s.text_of("containers.tsv") {
            s.containers = t.lines().filter_map(|l| l.split_once('\t')).map(|(n, c)| Container { name: n.to_string(), color: c.parse().unwrap_or(1) }).collect();
        }
        if let Some(t) = s.text_of("buttons.tsv") {
            s.buttons = read_buttons(&t);
        }
        crate::site_prefs::set_all(s.text_of("sites.tsv").map(|t| t.lines().filter_map(crate::site_prefs::SitePrefs::from_line).collect()).unwrap_or_default());
        if let Some(t) = s.text_of("identity.tsv") {
            s.identities = t.lines().filter_map(|l| l.split_once('\t')).map(|(h, k)| (h.to_string(), crate::identity::Identity::from_key(k))).collect();
        }
        crate::identity::set_choices(s.identities.clone());
        if let Some(t) = s.text_of("zoom.tsv") {
            for line in t.lines() {
                if let Some((h, z)) = line.split_once('\t') {
                    if let Ok(z) = z.parse() {
                        s.site_zoom.insert(h.to_string(), z);
                    }
                }
            }
        }
        if let Some(t) = s.text_of("permissions.tsv") {
            s.permissions = t
                .lines()
                .filter_map(|l| {
                    let mut p = l.split('\t');
                    Some((p.next()?.to_string(), p.next()?.to_string(), p.next()? == "1"))
                })
                .collect();
        }
        if let Some(t) = s.text_of("logins.tsv") {
            s.logins = t.lines().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect();
        }
        match s.text_of("extensions.txt") {
            Some(t) => s.extensions_on = t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect(),
            // (the two Sanaga starts with: for a profile that has no list
            // yet, and while its list cannot be read)
            None => s.extensions_on = vec!["bloqueur-de-publicites".into(), "sans-bannieres-cookies".into()],
        }
        if let Some(t) = s.text_of("reading.tsv") {
            for line in t.lines() {
                if let Some((u, ti)) = line.split_once('\t') {
                    s.reading.push(Bookmark { url: u.into(), title: ti.into() });
                }
            }
        }
        if let Some(t) = s.text_of("settings.txt") {
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
                        "block_lists" => st.block_lists = v == "1",
                        "low_power" => st.low_power = v.parse().unwrap_or(1),
                        "tree_tabs" => st.tree_tabs = v == "1",
                        "app_icon" => st.app_icon = v.parse().unwrap_or(0),
                        "profile_emoji" => st.profile_emoji = v.to_string(),
                        "profile_name" => st.profile_name = v.to_string(),
                        "profile_color" => st.profile_color = v.parse().unwrap_or(0),
                        "drm_auto" => st.drm_auto = v == "1",
                        "tab_switcher" => st.tab_switcher = v == "1",
                        "translate_to" => st.translate_to = v.to_string(),
                        "translate_offer" => st.translate_offer = v == "1",
                        "ai_tools" => st.ai_tools = v == "1",
                        "keep_chats" => st.keep_chats = v == "1",
                        "engine_warned" => st.engine_warned = v.to_string(),
                        "translate_skip" => st.translate_skip = v.split(',').map(str::trim).filter(|h| !h.is_empty()).map(str::to_string).collect(),
                        "close_right" => st.close_right = v == "1",
                        "download_dir" => st.download_dir = v.to_string(),
                        "download_ask" => st.download_ask = v == "1",
                        "https_first" => st.https_first = v == "1",
                        "proxy" => st.proxy = v.to_string(),
                        "update_check" => st.update_check = v == "1",
                        "safe_sites" => st.safe_sites = v == "1",
                        "doh" => st.doh = v.to_string(),
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
        match s.text_of("bookmarks.tsv") {
            Some(t) => {
                for line in t.lines() {
                    if let Some((u, ti)) = line.split_once('\t') {
                        s.bookmarks.push(Bookmark { url: u.into(), title: ti.into() });
                    }
                }
            }
            // favourites that are there and cannot be read: none shown, and
            // the file is not touched (never the first favourites over them)
            None if s.unreadable.contains(&"bookmarks.tsv") => {}
            // no favourites file yet: the ones Sanaga starts with
            None => {
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
        let Some(path) = self.file("settings.txt") else { return };
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
        let extra = format!(
            "block_lists={}\ntranslate_offer={}\ntranslate_skip={}\nai_tools={}\nkeep_chats={}\nengine_warned={}\n",
            b(st.block_lists),
            b(st.translate_offer),
            clean(&st.translate_skip.join(",")),
            b(st.ai_tools),
            b(st.keep_chats),
            clean(&st.engine_warned)
        );
        let network = format!(
            "download_dir={}\ndownload_ask={}\nhttps_first={}\nproxy={}\nupdate_check={}\nsafe_sites={}\ndoh={}\n",
            clean(&st.download_dir),
            b(st.download_ask),
            b(st.https_first),
            clean(&st.proxy),
            b(st.update_check),
            b(st.safe_sites),
            clean(&st.doh)
        );
        save_soon(&path, t + &more + &extra + &network);
    }

    pub fn save_containers(&self) {
        let Some(path) = self.file("containers.tsv") else { return };
        let t: String = self.containers.iter().map(|c| format!("{}\t{}\n", clean(&c.name).replace('\t', " "), c.color)).collect();
        save_soon(&path, t);
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn save_buttons(&self) {
        let Some(path) = self.file("buttons.tsv") else { return };
        save_soon(&path, write_buttons(&self.buttons));
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Website settings (JavaScript, fonts, blocker, look) of every site.
    pub fn save_site_prefs(&self, list: &[(String, crate::site_prefs::SitePrefs)]) {
        crate::site_prefs::set_all(list.to_vec());
        let Some(path) = self.file("sites.tsv") else { return };
        let t: String = list.iter().filter(|(_, p)| !p.is_default()).map(|(h, p)| p.to_line(h) + "\n").collect();
        save_soon(&path, t);
        CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// A page was visited. (Not asked of a private tab; a window that
    /// keeps nothing has a history that ends with it.)
    pub fn add_visit(&mut self, url: &str, title: &str) {
        // (its title, if it tells one in a moment, is written at once)
        self.titles_written.retain(|(u, _)| u != url);
        self.titles_late.retain(|(u, _)| u != url);
        self.history.add_visit(url, title, now());
    }

    /// A page told its title after its visit was recorded (most do: the
    /// title of a page that draws itself comes after its load). The page
    /// is renamed in the history, and no visit is added. Written at once
    /// the first time; a page that renames itself again within moments (a
    /// counter, "(3) Messages": some do every second) has its next titles
    /// wait for `settle_history` — each would be a write to the disk, and
    /// every list of the history would be read again.
    pub fn title_visit(&mut self, url: &str, title: &str) {
        if title.is_empty() {
            return;
        }
        if let Some(late) = self.titles_late.iter_mut().find(|(u, _)| u == url) {
            late.1 = title.to_string();
            return;
        }
        let now = std::time::Instant::now();
        self.titles_written.retain(|(_, at)| now.duration_since(*at) < TITLE_PAUSE);
        if self.titles_written.iter().any(|(u, _)| u == url) {
            self.titles_late.push((url.to_string(), title.to_string()));
            self.titles_since.get_or_insert(now);
            return;
        }
        self.titles_written.push((url.to_string(), now));
        // (a page not visited yet — still loading — is not renamed: its
        // visit will carry this title when it is recorded)
        self.history.set_title(url, title);
    }

    /// Write the titles that waited (see `title_visit`). `now`: Sanaga is
    /// closing, or the profile is read again.
    pub fn settle_history(&mut self, now: bool) {
        if !self.titles_since.is_some_and(|t| now || t.elapsed() >= TITLE_PAUSE) {
            return;
        }
        self.titles_since = None;
        for (url, title) in std::mem::take(&mut self.titles_late) {
            self.history.set_title(&url, &title);
        }
    }

    /// Forget every visit (and what was kept of the history file of
    /// earlier versions: what the person clears must be gone).
    pub fn clear_history(&mut self) {
        self.titles_late.clear();
        self.titles_since = None;
        self.history.clear_since(0);
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
        let Some(path) = self.file("reading.tsv") else { return };
        let mut t = String::new();
        for b in &self.reading {
            t.push_str(&format!("{}\t{}\n", clean(&b.url), clean(&b.title)));
        }
        save_soon(&path, t);
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
        let Some(path) = self.file("zoom.tsv") else { return };
        let t: String = self.site_zoom.iter().map(|(h, z)| format!("{}\t{}\n", clean(h), z)).collect();
        save_soon(&path, t);
    }

    pub fn save_bookmarks(&self) {
        let Some(path) = self.file("bookmarks.tsv") else { return };
        let mut t = String::new();
        for b in &self.bookmarks {
            t.push_str(&format!("{}\t{}\n", clean(&b.url), clean(&b.title)));
        }
        save_soon(&path, t);
    }

    /// Read favourites and reading list again (after a sync brought newer ones).
    pub fn permission(&self, site: &str, kind: &str) -> Option<bool> {
        self.permissions.iter().find(|(s, k, _)| s == site && k == kind).map(|p| p.2)
    }

    pub fn set_permission(&mut self, site: &str, kind: &str, allow: Option<bool>) {
        self.permissions.retain(|(s, k, _)| !(s == site && k == kind));
        if let Some(a) = allow {
            self.permissions.push((site.to_string(), kind.to_string(), a));
        }
        let Some(path) = self.file("permissions.tsv") else { return };
        let t: String = self.permissions.iter().map(|(s, k, a)| format!("{}\t{}\t{}\n", clean(s), k, if *a { "1" } else { "0" })).collect();
        save_soon(&path, t);
    }

    pub fn save_logins(&self) {
        let Some(path) = self.file("logins.tsv") else { return };
        let t: String = self.logins.iter().map(|(a, b)| format!("{}\t{}\n", clean(a), clean(b))).collect();
        save_soon(&path, t);
    }

    /// How Sanaga introduces itself to sites the person chose for.
    pub fn save_identities(&self) {
        if self.dir.is_none() {
            return;
        }
        // (in use at once, saved or not)
        crate::identity::set_choices(self.identities.clone());
        let Some(path) = self.file("identity.tsv") else { return };
        let t: String = self.identities.iter().map(|(h, i)| format!("{}\t{}\n", clean(h), i.key())).collect();
        save_soon(&path, t);
    }

    pub fn save_extensions(&self) {
        let Some(path) = self.file("extensions.txt") else { return };
        save_soon(&path, self.extensions_on.join("\n"));
    }

    /// The tabs of window `slot` (0 = the first window: `session.txt`; the
    /// others: `session-N.txt`).
    fn session_file(dir: &Path, slot: usize) -> PathBuf {
        if slot == 0 {
            dir.join("session.txt")
        } else {
            dir.join(format!("session-{slot}.txt"))
        }
    }

    pub fn save_session(&self, slot: usize, urls: &[String]) {
        let Some(dir) = &self.dir else { return };
        save_soon(&Self::session_file(dir, slot), urls.join("\n"));
    }

    pub fn load_session(&self, slot: usize) -> Vec<String> {
        let Some(dir) = &self.dir else { return Vec::new() };
        flush_files();
        read_lines(&Self::session_file(dir, slot))
    }

    /// A window was closed while others stay open: its tabs are not reopened.
    pub fn forget_session(&self, slot: usize) {
        let Some(dir) = &self.dir else { return };
        // (after the saves of that file still on their way)
        late(Late::Remove(Self::session_file(dir, slot)));
    }

    /// The tabs of the other windows of the last session (their files are
    /// removed: the windows write them again under their new numbers).
    pub fn take_extra_sessions(&self) -> Vec<Vec<String>> {
        let Some(dir) = &self.dir else { return Vec::new() };
        flush_files();
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
    read_text(path).map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect()).unwrap_or_default()
}

/// The first line of `buttons.tsv` since the scripts of the person's
/// buttons are written exactly (see `write_buttons`). A file without it was
/// written by an earlier Sanaga. (Earlier versions skip this line: it has
/// no columns.)
const BUTTONS_EXACT: &str = "#2";

/// The person's buttons as saved: one per line, `label⇥symbol⇥kind⇥value`.
/// A line break in a script is written `\n`. So that a script holding the
/// two characters `\` and `n` (in a text: "a\nb") is not read back with a
/// real line break there, a `\` is doubled wherever it would be read
/// wrongly: before `n`, before another `\`, before a line break. Every
/// other `\` (`\d`, `\.`…) stays as it is, as earlier versions wrote it.
fn write_buttons(list: &[CustomButton]) -> String {
    let mut t = format!("{BUTTONS_EXACT}\n");
    for b in list {
        let mut value = String::with_capacity(b.value.len());
        let mut chars = b.value.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if matches!(chars.peek(), Some('n' | '\\' | '\n')) => value.push_str("\\\\"),
                '\n' => value.push_str("\\n"),
                '\t' => value.push_str("    "),
                c => value.push(c),
            }
        }
        t.push_str(&format!("{}\t{}\t{}\t{value}\n", b.label.replace(['\t', '\n'], " "), b.symbol.replace(['\t', '\n'], " "), b.kind));
    }
    t
}

fn read_buttons(text: &str) -> Vec<CustomButton> {
    let exact = text.lines().next() == Some(BUTTONS_EXACT);
    text.lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\t');
            let (label, symbol, kind, written) = (p.next()?, p.next()?, p.next()?.parse().ok()?, p.next()?);
            let value = if exact {
                let mut value = String::with_capacity(written.len());
                let mut chars = written.chars().peekable();
                while let Some(c) = chars.next() {
                    match (c, chars.peek()) {
                        ('\\', Some('n')) => {
                            chars.next();
                            value.push('\n');
                        }
                        ('\\', Some('\\')) => {
                            chars.next();
                            value.push('\\');
                        }
                        (c, _) => value.push(c),
                    }
                }
                value
            } else {
                // a file of before: its line breaks were written `\n`, and
                // nothing told them from a `\n` of the script itself
                written.replace("\\n", "\n")
            };
            Some(CustomButton { label: label.to_string(), symbol: symbol.to_string(), kind, value })
        })
        .collect()
}

/// Changes to the data files made by this Sanaga (any window): another window
/// of the same profile sees the number change and reads the files again.
pub static CHANGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    /// How many times this thread told the windows (for the tests: `CHANGES`
    /// is shared by all of them, which run at the same time).
    static TOLD_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times the thread of this test told the windows of a change.
#[cfg(test)]
pub fn told_here() -> u64 {
    TOLD_HERE.with(|c| c.get())
}

fn changed() {
    CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    TOLD_HERE.with(|c| c.set(c.get() + 1));
}

// (private folders and permissions: see guard.rs, testable without GTK)
pub use crate::guard::{close_files, ensure_private_dir, runtime_dir, PrivateDir};

/// Write a data file safely: into a copy first, then put in place in one go,
/// so a crash or power cut while writing never leaves a half-written file.
/// The copy is a new file only this user can read (0600, never a file or
/// link that was already there). It waits for the disk: call it from a
/// thread of its own, never from the window's (see `save_soon`).
pub fn write_atomic(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    write_file(path, data.as_ref())?;
    // (each window has its own session file: no need to tell the others)
    if !is_session(path) {
        changed();
    }
    Ok(())
}

/// Write safely, as `write_atomic`, a file that is not one of the profile's
/// data files: a file of the sync folder, what the sync remembers of that
/// folder. No window is told. (Told of each of them, every window read its
/// whole profile again at every sync round — every 30 seconds — and what
/// the person changed in that instant could be lost to the reading.)
pub fn write_quietly(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    write_file(path, data.as_ref())
}

fn is_session(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("session"))
}

/// `path` with `end` added to its name.
fn with_end(path: &Path, end: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(end);
    PathBuf::from(p)
}

/// The copy a save is first written into: `<file>.<process>-<n>.tmp`, a
/// name no other save uses — not another save of this Sanaga, not another
/// Sanaga running on the same profile. (They used to share `<file>.tmp`,
/// each removing it before creating it: one could put the other's
/// half-written copy in place of the file.)
fn copy_name(path: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    with_end(path, &format!(".{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))
}

fn write_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        sweep_copies(dir);
    }
    let mut tries = 0;
    let (mut f, tmp) = loop {
        let tmp = copy_name(path);
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp) {
            Ok(f) => break (f, tmp),
            // (left by a Sanaga that had the same process number and was
            // killed: never written into, the next name is taken)
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && tries < 16 => tries += 1,
            Err(e) => return Err(e),
        }
    };
    // (really on the disk before it takes the old file's place: after a
    // power cut the file is the old one or the new one, never an empty one)
    let done = f.write_all(data).and_then(|_| f.sync_all());
    drop(f);
    let done = done.and_then(|_| std::fs::rename(&tmp, path));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

/// Is this the name of a save's copy (`copy_name`): `<file>.<process>-<n>.tmp`?
fn is_copy_name(name: &str) -> bool {
    let numbers = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    name.strip_suffix(".tmp").and_then(|n| n.rsplit_once('.')).and_then(|(_, tail)| tail.split_once('-')).is_some_and(|(process, n)| numbers(process) && numbers(n))
}

/// Remove the copies a save left behind when it never finished (Sanaga
/// killed, the power cut): in each folder Sanaga writes in, once per run,
/// and only copies older than ten minutes — a save under way is never
/// that old. (Under their one earlier name, `<file>.tmp`, the next save
/// replaced them; each has its own name now.)
fn sweep_copies(dir: &Path) {
    static SWEPT: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    {
        let mut swept = SWEPT.lock().unwrap_or_else(|e| e.into_inner());
        if swept.iter().any(|d| d == dir) {
            return;
        }
        swept.push(dir.to_path_buf());
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if !is_copy_name(&e.file_name().to_string_lossy()) {
            continue;
        }
        // (the entry's own metadata: a link is not followed)
        let Ok(m) = e.metadata() else { continue };
        let old = m.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d > std::time::Duration::from_secs(600));
        if m.is_file() && old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Is the earlier state of this file kept when it is first replaced (see
/// `keep_backup`)? Not a window's tabs (they are written all the time), and
/// never the history: what the person removes from it must be gone.
fn keeps_backup(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    !name.starts_with("session") && name != "history.tsv"
}

/// Before a data file is replaced for the first time in a run, what it held
/// stays next to it as `<file>.bak`: one earlier state to go back to by
/// hand when a save went wrong (a list read damaged and saved so, a mistake
/// of Sanaga's). A second name for the same content where the disk can (a
/// hard link: nothing is copied), a copy otherwise; made by the saving
/// thread, never by the window's.
fn keep_backup(path: &Path) {
    static KEPT: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    {
        let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
        if kept.iter().any(|p| p == path) {
            return;
        }
        kept.push(path.to_path_buf());
    }
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return;
    }
    // (under another name first, then put in place in one go: the backup
    // of before is there until the new one is whole)
    let bak = with_end(path, ".bak");
    let tmp = copy_name(&bak);
    let made = std::fs::hard_link(path, &tmp).or_else(|_| std::fs::copy(path, &tmp).map(|_| ()));
    if made.and_then(|_| std::fs::rename(&tmp, &bak)).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

// ------------------------------------------------- saving without waiting
//
// Putting a file safely on the disk takes a few milliseconds on a fast disk
// and half a second on a slow or busy one. Sanaga saves at every tab shown,
// page visited, setting changed: done in the window's thread, each of these
// would freeze the window for that long. So the window only hands the file
// to a thread that writes, in the order asked; what is read back in the
// same run waits for it first (`flush_saves`), and so does quitting.

enum Late {
    /// Replace a file (its earlier state is kept the first time: `keep_backup`).
    Write(PathBuf, Vec<u8>),
    /// The same, with its result waited for (see `write_in_order`).
    WriteTold(PathBuf, Vec<u8>, std::sync::mpsc::Sender<std::io::Result<()>>),
    Remove(PathBuf),
    /// Answer once everything asked before is on the disk.
    Flush(std::sync::mpsc::Sender<()>),
}

fn do_late(op: Late) {
    match op {
        Late::Write(path, data) => {
            if keeps_backup(&path) {
                keep_backup(&path);
            }
            let _ = write_file(&path, &data);
        }
        Late::WriteTold(path, data, done) => {
            if keeps_backup(&path) {
                keep_backup(&path);
            }
            let _ = done.send(write_file(&path, &data));
        }
        Late::Remove(path) => {
            let _ = std::fs::remove_file(path);
        }
        Late::Flush(done) => {
            let _ = done.send(());
        }
    }
}

/// What a batch of requests comes down to: a file written or removed again
/// later in the batch need not be written now (only its last state counts).
/// Later, but before the next `Flush`: whoever asked for a flush reads the
/// files as soon as it is answered, and must find in them what was asked
/// before it. (A write followed by a flush and a second write of the same
/// file was dropped: the flush was answered with the file still as it was
/// before both.)
fn settle(batch: Vec<Late>) -> Vec<Late> {
    let path_of = |op: &Late| match op {
        Late::Write(p, _) | Late::Remove(p) => Some(p.clone()),
        // (someone waits for its answer: always done)
        Late::WriteTold(..) | Late::Flush(_) => None,
    };
    let replaced: Vec<bool> = batch
        .iter()
        .enumerate()
        .map(|(k, op)| match path_of(op) {
            Some(p) => {
                batch[k + 1..].iter().take_while(|later| !matches!(later, Late::Flush(_))).any(|later| matches!(later, Late::Write(q, _) | Late::WriteTold(q, _, _) | Late::Remove(q) if *q == p))
            }
            None => false,
        })
        .collect();
    batch.into_iter().zip(replaced).filter(|(_, r)| !r).map(|(op, _)| op).collect()
}

fn late(op: Late) {
    static SAVER: std::sync::OnceLock<Option<std::sync::Mutex<std::sync::mpsc::Sender<Late>>>> = std::sync::OnceLock::new();
    let saver = SAVER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Late>();
        std::thread::Builder::new()
            .name("sanaga-saver".into())
            .spawn(move || {
                // (asleep until something is asked: it costs nothing meanwhile)
                while let Ok(first) = rx.recv() {
                    let mut batch = vec![first];
                    while let Ok(more) = rx.try_recv() {
                        batch.push(more);
                    }
                    for op in settle(batch) {
                        do_late(op);
                    }
                }
            })
            .ok()
            .map(|_| std::sync::Mutex::new(tx))
    });
    let sent = match saver.as_ref().and_then(|s| s.lock().ok()) {
        Some(tx) => tx.send(op).map_err(|e| e.0),
        None => Err(op),
    };
    // (no thread to hand it to: done here, as before)
    if let Err(op) = sent {
        do_late(op);
    }
}

/// Save a data file a moment later, without waiting for the disk (safely:
/// see `write_atomic`). The other windows are told at once; they wait for
/// the file when they read it.
fn save_soon(path: &Path, data: impl AsRef<[u8]>) {
    if !is_session(path) {
        changed();
    }
    late(Late::Write(path.to_path_buf(), data.as_ref().to_vec()));
}

/// Write one of the window's data files from another thread (the sync):
/// after the saves already asked for it, never at the same time as one, and
/// waited for. The windows are told, as with `write_atomic`. (Not for the
/// files of the sync folder: `write_quietly`.)
pub fn write_in_order(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    late(Late::WriteTold(path.to_path_buf(), data.as_ref().to_vec(), tx));
    rx.recv().unwrap_or_else(|_| Err(std::io::Error::other("the saving thread is gone")))?;
    if !is_session(path) {
        changed();
    }
    Ok(())
}

/// Wait until every save asked so far is on the disk, and every visit in
/// the history: before Sanaga ends, before a sync round.
pub fn flush_saves() {
    flush_files();
    crate::history::flush();
}

/// Wait until every data file saved so far is on the disk: before reading
/// the files back. (Not for the history, which is not read back with them:
/// a window reading its profile again never waits for it.)
fn flush_files() {
    // (the saving thread itself has nothing to wait for)
    if std::thread::current().name() == Some("sanaga-saver") {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    late(Late::Flush(tx));
    let _ = rx.recv();
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
    [("https://wajub.com/", "Wajub"), ("https://komerxe.com/", "Komerxe"), ("https://moniwa.app/", "Moniwa"), ("https://lekalao.com/", "Lekalao"), ("https://grassfield.site/", "Grassfield")]
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

/// The groups a list of visits is cut into, newest first: the age
/// (seconds) at which each of the first four ends; the fifth has no end.
pub const DAY_GROUPS: [u64; 4] = [86400, 2 * 86400, 7 * 86400, 30 * 86400];

/// The name of the group `k` of `DAY_GROUPS`.
pub fn day_group_name(k: usize) -> &'static str {
    match k {
        0 => tr("Aujourd'hui"),
        1 => tr("Hier"),
        2 => tr("Cette semaine"),
        3 => tr("Ce mois-ci"),
        _ => tr("Plus ancien"),
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
        assert_eq!(names_in(&dir), ["settings.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The names of the files of a folder, sorted.
    fn names_in(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        names
    }

    /// A new, empty folder for a test.
    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sanaga-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn two_saves_of_one_file_never_share_their_copy() {
        let dir = temp("copies");
        let f = dir.join("settings.txt");
        // the half-written copy of another Sanaga on the same profile, under
        // the name every save used to take: it is not removed, not written
        // into, not put in place of the file
        std::fs::write(dir.join("settings.txt.tmp"), "half of anoth").unwrap();
        super::write_atomic(&f, "a=1\n").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.txt.tmp")).unwrap(), "half of anoth");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "a=1\n");
        // many saves of the same file at the same time: each is whole, none fails
        let texts: Vec<String> = (0..8).map(|k| format!("{k}").repeat(20_000)).collect();
        let threads: Vec<_> = texts
            .iter()
            .cloned()
            .map(|text| {
                let f = f.clone();
                std::thread::spawn(move || (0..40).all(|_| super::write_file(&f, text.as_bytes()).is_ok()))
            })
            .collect();
        assert!(threads.into_iter().all(|t| t.join().unwrap()));
        assert!(texts.contains(&std::fs::read_to_string(&f).unwrap()));
        assert_eq!(names_in(&dir), ["settings.txt", "settings.txt.tmp"]);
        // a copy left by a save that never finished goes once it is old;
        // one that may still be under way stays
        let aside = temp("copies-left");
        let hour_old = |name: &str| {
            std::fs::write(aside.join(name), "x").unwrap();
            std::fs::File::options().write(true).open(aside.join(name)).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600)).unwrap();
        };
        hour_old("zoom.tsv.4242-7.tmp");
        hour_old("notes.tmp");
        hour_old("zoom.tsv");
        std::fs::write(aside.join("zoom.tsv.4242-8.tmp"), "x").unwrap();
        super::write_atomic(&aside.join("reading.tsv"), "").unwrap();
        assert_eq!(names_in(&aside), ["notes.tmp", "reading.tsv", "zoom.tsv", "zoom.tsv.4242-8.tmp"]);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&aside);
    }

    #[test]
    fn favourites_with_a_damaged_byte_are_all_kept() {
        let dir = temp("damaged");
        // one byte that is not text in the second title
        let on_disk = b"https://a.example/\tA\nhttps://b.example/\tB\xff\xfe damaged\nhttps://c.example/\tC\n".to_vec();
        std::fs::write(dir.join("bookmarks.tsv"), &on_disk).unwrap();
        std::fs::write(dir.join("settings.txt"), "home=sanaga://newtab\n").unwrap();
        let mut s = super::Store::load_from(dir.clone(), "");
        let urls: Vec<&str> = s.bookmarks.iter().map(|b| b.url.as_str()).collect();
        assert_eq!(urls, ["https://a.example/", "https://b.example/", "https://c.example/"]);
        assert!(s.bookmarks[1].title.starts_with('B') && s.bookmarks[1].title.ends_with(" damaged") && s.unreadable().is_empty());
        // reading changed nothing on the disk
        super::flush_saves();
        assert_eq!(std::fs::read(dir.join("bookmarks.tsv")).unwrap(), on_disk);
        assert!(!dir.join("bookmarks.tsv.bak").exists());
        // the person adds one: saved with the others, and the file as it was
        // before this run stays next to it
        s.toggle_bookmark("https://d.example/", "D");
        s.toggle_bookmark("https://e.example/", "E");
        super::flush_saves();
        let saved = std::fs::read_to_string(dir.join("bookmarks.tsv")).unwrap();
        assert_eq!(saved.lines().count(), 5);
        assert!(saved.starts_with("https://a.example/\tA\nhttps://b.example/\tB") && saved.ends_with("https://d.example/\tD\nhttps://e.example/\tE\n"));
        assert_eq!(std::fs::read(dir.join("bookmarks.tsv.bak")).unwrap(), on_disk);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_replaced() {
        let dir = temp("unreadable");
        // files that are there and cannot be read (here: links to
        // themselves; on a real disk: a damaged block, a refusal)
        for name in ["bookmarks.tsv", "settings.txt", "reading.tsv", "extensions.txt", "zoom.tsv", "history.tsv"] {
            std::os::unix::fs::symlink(name, dir.join(name)).unwrap();
        }
        let untouched = |name: &str| std::fs::symlink_metadata(dir.join(name)).unwrap().file_type().is_symlink();
        let mut s = super::Store::load_from(dir.clone(), "");
        // nothing of them is shown — not the first favourites either — and it is known
        assert!(s.bookmarks.is_empty() && s.reading.is_empty() && s.history.count() == 0 && !s.first_run);
        assert_eq!(s.unreadable(), ["zoom.tsv", "extensions.txt", "reading.tsv", "settings.txt", "bookmarks.tsv"]);
        // whatever is changed during this run, the files stay as they are
        s.toggle_bookmark("https://a.example/", "A");
        s.toggle_reading("https://b.example/", "B");
        s.settings.compact = true;
        s.save_settings();
        s.extensions_on.clear();
        s.save_extensions();
        s.set_site_zoom("a.example", 1.5);
        s.add_visit("https://a.example/", "A");
        // (a file that could be read is saved as ever)
        s.set_permission("https://a.example", "camera", Some(true));
        super::flush_saves();
        for name in ["bookmarks.tsv", "settings.txt", "reading.tsv", "extensions.txt", "zoom.tsv", "history.tsv"] {
            assert!(untouched(name), "{name} was replaced");
        }
        assert_eq!(std::fs::read_to_string(dir.join("permissions.tsv")).unwrap(), "https://a.example\tcamera\t1\n");
        // read again once it can be: the favourites are back, and saved again
        std::fs::remove_file(dir.join("bookmarks.tsv")).unwrap();
        std::fs::write(dir.join("bookmarks.tsv"), "https://kept.example/\tKept\n").unwrap();
        let mut s = super::Store::load_from(dir.clone(), "");
        assert_eq!(s.bookmarks.len(), 1);
        s.toggle_bookmark("https://a.example/", "A");
        super::flush_saves();
        assert_eq!(std::fs::read_to_string(dir.join("bookmarks.tsv")).unwrap(), "https://kept.example/\tKept\nhttps://a.example/\tA\n");
        // the visit of this run is kept, in the history's own file
        assert_eq!(s.history.count(), 1);
        // emptying the history is done even to an earlier history file
        // that cannot be read: it goes
        s.clear_history();
        super::flush_saves();
        assert_eq!(s.history.count(), 0);
        assert!(std::fs::symlink_metadata(dir.join("history.tsv")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_profile_gets_the_first_favourites() {
        let dir = temp("new");
        let s = super::Store::load_from(dir.clone(), "");
        super::flush_saves();
        assert!(s.first_run && s.bookmarks.len() == 5 && s.unreadable().is_empty());
        assert_eq!(std::fs::read_to_string(dir.join("bookmarks.tsv")).unwrap().lines().count(), 5);
        assert!(dir.join("settings.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_state_before_a_run_is_kept_once() {
        let dir = temp("backup");
        let (f, bak) = (dir.join("reading.tsv"), dir.join("reading.tsv.bak"));
        std::fs::write(&f, "before\n").unwrap();
        super::save_soon(&f, "first\n");
        super::flush_saves();
        assert_eq!((std::fs::read_to_string(&f).unwrap().as_str(), std::fs::read_to_string(&bak).unwrap().as_str()), ("first\n", "before\n"));
        // only the state before the run: later saves leave it alone
        super::save_soon(&f, "second\n");
        super::write_in_order(&f, "third\n").unwrap();
        assert_eq!((std::fs::read_to_string(&f).unwrap().as_str(), std::fs::read_to_string(&bak).unwrap().as_str()), ("third\n", "before\n"));
        // never of the history (what is removed from it must be gone), nor of a window's tabs
        for name in ["history.tsv", "session.txt", "session-2.txt"] {
            std::fs::write(dir.join(name), "before\n").unwrap();
            super::save_soon(&dir.join(name), "after\n");
        }
        super::flush_saves();
        assert_eq!(names_in(&dir), ["history.tsv", "reading.tsv", "reading.tsv.bak", "session-2.txt", "session.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_buttons_script_is_read_back_as_written() {
        let button = |value: &str| super::CustomButton { label: "Go".into(), symbol: "G".into(), kind: 2, value: value.into() };
        let list = vec![
            // the two characters \ and n in a text, next to real line breaks
            button("alert('a\\nb');\nalert(2)"),
            button("x.replace(/\\\\/g, '/').split(/\\d+\\s*\\n/)"),
            button("a\\\nb\\\\\nc\\\\n \\"),
            button("https://a.example/"),
            button(""),
        ];
        let text = super::write_buttons(&list);
        assert_eq!(text.lines().count(), 6);
        assert_eq!(super::read_buttons(&text), list);
        // a \ that needs no mark is written as it is
        assert!(text.contains("/\\d+\\s*"));
        // a file of an earlier version: read as that version read it
        let before = "Go\tG\t2\talert(1);\\nalert('\\\\d')\nSite\tS\t0\thttps://a.example/\n";
        assert_eq!(
            super::read_buttons(before),
            vec![button("alert(1);\nalert('\\\\d')"), super::CustomButton { label: "Site".into(), symbol: "S".into(), kind: 0, value: "https://a.example/".into() }]
        );
    }

    #[test]
    fn an_empty_address_is_not_a_visit() {
        let mut s = super::Store::ephemeral();
        s.add_visit("", "");
        s.add_visit("  ", "Sans titre");
        s.add_visit("sanaga://newtab", "");
        assert_eq!(s.history.count(), 0);
        s.add_visit("https://a.test/", "A");
        assert_eq!(s.history.count(), 1);
    }

    #[test]
    fn a_category_is_restored_and_the_persons_own_stays() {
        let mut s = super::Settings::default();
        assert!(!s.differs_from_origin("appearance"));
        s.accent = 4;
        s.compact = true;
        s.blocker = false;
        s.language = "pt".into();
        s.fill_name = "Awa".into();
        s.sync_dir = "/home/awa/Sync".into();
        assert!(s.differs_from_origin("appearance") && s.differs_from_origin("privacy") && !s.differs_from_origin("performance"));
        assert!(s.restore("appearance"));
        assert_eq!((s.accent, s.compact), (0, false));
        // the other categories and what is the person's own are untouched
        assert!(!s.blocker);
        assert_eq!((s.language.as_str(), s.fill_name.as_str(), s.sync_dir.as_str()), ("pt", "Awa", "/home/awa/Sync"));
        assert!(s.restore("privacy") && s.blocker);
        assert!(!s.restore("privacy") && !s.restore("sync") && !s.restore("nowhere"));
    }

    #[test]
    fn saves_asked_in_a_row_come_down_to_the_last_state() {
        use super::{settle, Late};
        use std::path::PathBuf;
        let (a, b) = (PathBuf::from("a"), PathBuf::from("b"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let batch = vec![Late::Write(a.clone(), b"1".to_vec()), Late::Write(b.clone(), b"1".to_vec()), Late::Write(a.clone(), b"2".to_vec()), Late::Flush(tx), Late::Remove(b.clone())];
        let kept: Vec<String> = settle(batch)
            .iter()
            .map(|op| match op {
                Late::Write(p, d) => format!("write {} {}", p.display(), String::from_utf8_lossy(d)),
                Late::WriteTold(p, ..) => format!("told {}", p.display()),
                Late::Remove(p) => format!("remove {}", p.display()),
                Late::Flush(_) => "flush".to_string(),
            })
            .collect();
        // (b is written before the flush: whoever waits for it finds it so)
        assert_eq!(kept, ["write b 1", "write a 2", "flush", "remove b"]);
        // a write is done before the flush that follows it, even when the
        // same file is written again after
        let (tx, _rx) = std::sync::mpsc::channel();
        let kept = settle(vec![Late::Write(a.clone(), b"1".to_vec()), Late::Flush(tx), Late::Write(a.clone(), b"2".to_vec())]);
        assert!(matches!(&kept[..], [Late::Write(_, v1), Late::Flush(_), Late::Write(_, v2)] if v1 == b"1" && v2 == b"2"));
    }

    /// The lines of a store's history page, newest first.
    fn history_lines(s: &super::Store) -> String {
        let all = s.history.list("", super::now());
        s.history.rows(&all, 0, 100).into_iter().map(|r| format!("{} {}", r.url, r.title)).collect::<Vec<_>>().join(", ")
    }

    #[test]
    fn a_title_learnt_late_renames_its_page() {
        let mut s = super::Store::ephemeral();
        s.add_visit("https://a.test/", "");
        s.add_visit("https://b.test/", "B");
        // the late title renames, and never adds a visit
        s.title_visit("https://a.test/", "The A page");
        s.title_visit("https://not-visited.test/", "x");
        assert_eq!(history_lines(&s), "https://b.test/ B, https://a.test/ The A page");
        // a page that renames itself again within moments: its next titles
        // wait (some do it every second), and the last one is written
        s.title_visit("https://a.test/", "(1) The A page");
        s.title_visit("https://a.test/", "(2) The A page");
        s.settle_history(false);
        assert_eq!(history_lines(&s), "https://b.test/ B, https://a.test/ The A page");
        s.settle_history(true);
        assert_eq!(history_lines(&s), "https://b.test/ B, https://a.test/ (2) The A page");
        // the same page again, with a title this time: one visit still
        s.add_visit("https://b.test/", "B, renamed");
        assert_eq!(history_lines(&s), "https://b.test/ B, renamed, https://a.test/ (2) The A page");
    }

    #[test]
    fn windows_of_a_profile_share_their_history() {
        let dir = temp("shared");
        let (mut a, mut b) = (super::Store::load_from(dir.clone(), ""), super::Store::load_from(dir.clone(), ""));
        // another profile, and a window that keeps nothing: they see nothing of it
        let (other, guest) = (super::Store::load_from(dir.join("other"), ""), super::Store::ephemeral());
        let (told, seen) = (super::told_here(), b.history.version());
        a.add_visit("https://a.test/", "");
        a.title_visit("https://a.test/", "A");
        b.add_visit("https://b.test/", "B");
        assert_eq!(history_lines(&b), "https://b.test/ B, https://a.test/ A");
        assert_eq!(history_lines(&a), "https://b.test/ B, https://a.test/ A");
        assert_eq!((other.history.count(), guest.history.count()), (0, 0));
        // each window knows that the history changed — and a visit is no
        // longer a reason for the others to read their whole profile again
        assert!(b.history.version() > seen);
        assert_eq!(super::told_here(), told);
        // one window clears the history: cleared for the other
        a.clear_history();
        assert_eq!(b.history.count(), 0);
        // more visits than the earlier limit of 5,000: all are kept
        for k in 0..6000 {
            a.add_visit(&format!("https://a.test/{k}"), "x");
        }
        assert_eq!(b.history.count(), 6000);
        // a window that keeps nothing writes no file
        let mut guest = guest;
        guest.add_visit("https://guest.test/", "Guest");
        assert_eq!(guest.history.count(), 1);
        super::flush_saves();
        assert!(!names_in(&dir).iter().any(|n| n.contains("guest")));
        let bytes = std::fs::read(dir.join("history.db")).unwrap();
        assert!(!bytes.windows(10).any(|w| w == b"guest.test"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_are_on_the_disk_once_waited_for() {
        let dir = std::env::temp_dir().join(format!("sanaga-soon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("history.tsv");
        for k in 0..50 {
            super::save_soon(&f, format!("{k}\n"));
        }
        super::flush_saves();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "49\n");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(names_in(&dir), ["history.tsv"]);
        let _ = std::fs::remove_dir_all(&dir);
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
        assert!(!dir.join("session-5.txt").exists());
        assert_eq!(s.take_extra_sessions(), vec![vec!["https://b.example/".to_string(), "https://c.example/".to_string()]]);
        assert!(s.take_extra_sessions().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

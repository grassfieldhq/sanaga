//! The Sanaga window: shared state, the frame loop and the screenshot mode.

mod accounts;
mod chrome;
mod devices;
mod myframes;
mod scene;
mod recorder;
mod source;
mod menus;
mod nav;
mod plus;
mod profiles;
mod pages;
mod settings;
mod layout;
mod tabbar;
mod siteid;
mod onboarding;
mod translate;
mod mainmenu;
mod sidebar;
mod bookmarks;
mod buttons;
mod groups;
mod dialogs;
mod errors;
mod switcher;
mod media;
mod containers;
mod crashes;
mod sitepanel;
mod tools;
mod intelligence;
mod extras;
mod importer;
mod render;
mod start;
mod tabs;
mod toolbar;
mod web;
#[cfg(test)]
mod robust;

pub use web::web_engine_available;

use crate::html::{Document, Form};
use crate::i18n::{tf, tr};
use crate::net::{self, NetMsg};
use crate::store::{Engine, Store};
use crate::theme::{theme, Theme};
use eframe::egui::{self, pos2, vec2, CornerRadius, FontFamily, FontId, Frame, Key, Rect, Stroke, TextureHandle, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const WELCOME_HTML: &str = include_str!("../../assets/bienvenue.html");

/// The welcome page in the interface language.
pub fn welcome_html() -> &'static str {
    match crate::i18n::language() {
        "fr" => WELCOME_HTML,
        "es" => include_str!("../../assets/bienvenue.es.html"),
        "pt" => include_str!("../../assets/bienvenue.pt.html"),
        "de" => include_str!("../../assets/bienvenue.de.html"),
        _ => include_str!("../../assets/bienvenue.en.html"),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Internal {
    NewTab,
    History,
    Bookmarks,
    Downloads,
    Settings,
    About,
    /// Sanaga's processes and tabs: memory, processor, sleep (`sanaga://activity`).
    Activity,
    /// The crash reports kept on this computer (`sanaga://crashes`).
    Crashes,
    Passwords,
    Extensions,
    Profiles,
    ClearData,
    Sites,
    Import,
    CreateExtension,
    Onboarding,
    /// A protected site (Netflix…) opened where it plays: `sanaga://protected?u=…`.
    Protected,
}

/// Sanaga's own pages have English addresses; the French ones of earlier
/// versions (in saved sessions, settings and bookmarks) still work.
pub fn english_address(u: &str) -> String {
    const OLD: [(&str, &str); 11] = [
        ("sanaga://nouvel-onglet", "sanaga://newtab"),
        ("sanaga://historique", "sanaga://history"),
        ("sanaga://favoris", "sanaga://bookmarks"),
        ("sanaga://telechargements", "sanaga://downloads"),
        ("sanaga://parametres", "sanaga://settings"),
        ("sanaga://a-propos", "sanaga://about"),
        ("sanaga://motsdepasse", "sanaga://passwords"),
        ("sanaga://profils", "sanaga://profiles"),
        ("sanaga://effacer", "sanaga://clear"),
        ("sanaga://importer", "sanaga://import"),
        ("sanaga://bienvenue", "sanaga://welcome"),
    ];
    let t = u.trim_end_matches('/');
    OLD.iter().find(|(old, _)| *old == t).map(|(_, new)| new.to_string()).unwrap_or_else(|| u.to_string())
}

thread_local! {
    /// Windows closed while others stayed open: their tabs, last one last.
    static CLOSED_WINDOWS: std::cell::RefCell<Vec<Vec<String>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Marks the message of a page that stopped working (see `page_error`).
pub(crate) const CRASH_MARK: &str = "\u{1}crash\u{1}";

fn internal_from_url(u: &str) -> Option<Internal> {
    Some(match english_address(u).trim_end_matches('/') {
        "sanaga://newtab" | "about:blank" => Internal::NewTab,
        "sanaga://history" => Internal::History,
        "sanaga://bookmarks" => Internal::Bookmarks,
        "sanaga://downloads" => Internal::Downloads,
        "sanaga://settings" => Internal::Settings,
        // (a category of the settings: sanaga://settings/privacy)
        p if p.starts_with("sanaga://settings/") => Internal::Settings,
        "sanaga://about" => Internal::About,
        "sanaga://activity" => Internal::Activity,
        "sanaga://crashes" => Internal::Crashes,
        "sanaga://passwords" => Internal::Passwords,
        "sanaga://extensions" => Internal::Extensions,
        "sanaga://profiles" => Internal::Profiles,
        "sanaga://clear" => Internal::ClearData,
        "sanaga://sites" => Internal::Sites,
        "sanaga://import" => Internal::Import,
        "sanaga://create-extension" => Internal::CreateExtension,
        "sanaga://start" => Internal::Onboarding,
        p if p.starts_with("sanaga://protected") => Internal::Protected,
        _ => return None,
    })
}

fn internal_title(i: Internal) -> &'static str {
    match i {
        Internal::NewTab => tr("Nouvel onglet"),
        Internal::History => tr("Historique"),
        Internal::Bookmarks => tr("Favoris"),
        Internal::Downloads => tr("Téléchargements"),
        Internal::Settings => tr("Paramètres"),
        Internal::About => tr("À propos de Sanaga"),
        Internal::Activity => tr("Activité"),
        Internal::Crashes => tr("Rapports de plantage"),
        Internal::Passwords => tr("Mots de passe"),
        Internal::Extensions => tr("Extensions"),
        Internal::Profiles => tr("Profils"),
        Internal::ClearData => tr("Effacer les données"),
        Internal::Sites => tr("Paramètres des sites"),
        Internal::Import => tr("Importer depuis un autre navigateur"),
        Internal::CreateExtension => tr("Créer une extension"),
        Internal::Onboarding => tr("Bienvenue dans Sanaga"),
        Internal::Protected => tr("Contenu protégé"),
    }
}

struct Entry {
    url: String,
    title: String,
    scroll: f32,
    doc: Option<Arc<Document>>,
}

enum View {
    Internal(Internal),
    Loading,
    Doc(Arc<Document>),
    /// Shown by the WebKit engine.
    Web,
    Error(String),
}

struct Tab {
    /// When the tab was opened (the interface's clock), for its entrance
    /// in the tab bar; before any time for the tabs Sanaga starts with.
    born: f64,
    id: u64,
    private: bool,
    entries: Vec<Entry>,
    idx: usize,
    view: View,
    nav: u64,
    loading: bool,
    address: String,
    forms: Vec<Form>,
    heights: Vec<f32>,
    layout_key: (u32, u32),
    scroll_to: Option<f32>,
    target_block: Option<(usize, u8)>,
    restore_scroll: Option<f32>,
    scroll_y: f32,
    zoom: f32,
    /// Show the whole page (menus, sidebars) instead of the main content.
    full: bool,
    web: Option<crate::page::PageView>,
    web_pending: Option<String>,
    /// The person asked for `web_pending` now (not a restored or woken tab).
    web_by_person: bool,
    web_shown: bool,
    web_zoom: f32,
    /// How many times the person came back to this tab (memory saver).
    revisits: u32,
    /// Safari-style Reader: this page shown as clean, easy-to-read text.
    reader: bool,
    /// Blocks the Reader leaves out (menus, buttons, link lists), worked out once per page.
    reader_skip: Vec<bool>,
    /// Pinned tabs stay small at the left of the tab bar.
    pinned: bool,
    /// Tab group (index into `Sanaga::groups`; 0 = the main "Onglets" group).
    group: usize,
    /// Sound: muted by the person / currently playing.
    muted: bool,
    audio: bool,
    /// Picture of the page for the tab overview.
    thumb: Option<TextureHandle>,
    /// Memory saver: when the tab was last shown, and whether its page was put to sleep.
    last_active: f64,
    sleeping: bool,
    /// The person typed in a form of the page and has not sent it: the tab
    /// never sleeps (what was typed would be lost, a classic Chrome grief).
    form_dirty: bool,
    /// Light-engine page to fetch when the tab is first shown.
    lite_pending: Option<net::Request>,
    /// Container (own cookies and logins; "" = the profile's).
    container: String,
    /// The tab it was opened from (vertical tabs show it underneath).
    parent: Option<u64>,
    /// Its tabs underneath are folded away (tree tabs).
    tree_folded: bool,
    /// This tab is the phone-sized companion of another one (mobile preview
    /// side by side): a phone's identity and width, and it follows the
    /// other page's address and scrolling.
    companion: bool,
}

impl Tab {
    fn title(&self) -> String {
        match &self.view {
            View::Internal(i) => internal_title(*i).to_string(),
            View::Doc(d) if !d.title.is_empty() => d.title.clone(),
            _ => {
                let e = &self.entries[self.idx];
                if !e.title.is_empty() {
                    e.title.clone()
                } else {
                    host_of(&e.url)
                }
            }
        }
    }
    fn url(&self) -> &str {
        &self.entries[self.idx].url
    }
}

fn host_of(url: &str) -> String {
    if url.starts_with("sanaga://") {
        return "Sanaga".into(); // Sanaga's own pages
    }
    thread_local! {
        /// Site names already worked out: tabs, lists and badges ask for the
        /// same addresses at every frame, and reading an address is not free
        /// (hundreds of rows, sixty times a second).
        static KNOWN: std::cell::RefCell<HashMap<String, String>> = std::cell::RefCell::new(HashMap::new());
    }
    KNOWN.with(|known| {
        let mut known = known.borrow_mut();
        if let Some(h) = known.get(url) {
            return h.clone();
        }
        let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string())).unwrap_or_else(|| url.to_string());
        // (kept small: started again when it has grown)
        if known.len() >= 8192 {
            known.clear();
        }
        known.insert(url.to_string(), host.clone());
        host
    })
}

fn badge_letter(url: &str, title: &str) -> String {
    let h = host_of(url);
    let src = if url.starts_with("sanaga://") || h.is_empty() { title } else { &h };
    src.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "•".into())
}

fn short_engine(e: Engine) -> &'static str {
    match e {
        Engine::DuckDuckGo => "DuckDuckGo",
        Engine::Google => "Google",
        Engine::Bing => "Bing",
        Engine::Wikipedia => "Wikipédia",
    }
}

fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Turns what the person typed into an address: a URL, or a search.
pub fn input_to_url(s: &str, engine: Engine, home: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return home.to_string();
    }
    // search shortcuts, as in Chrome's site search: "w Sanaga", "yt musique"...
    if let Some((u, _)) = plus::keyword_search(t) {
        return u;
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("sanaga://") || lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file://") {
        return t.to_string();
    }
    if lower == "about:blank" {
        return "sanaga://newtab".into();
    }
    if t.starts_with('/') && std::path::Path::new(t).exists() {
        return format!("file://{t}");
    }
    let no_space = !t.contains(char::is_whitespace);
    let hostish = t.split(['/', '?', '#']).next().unwrap_or("");
    let looks_host = hostish.contains('.')
        && !hostish.ends_with('.')
        && !hostish.starts_with('.')
        && hostish.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == ':');
    if no_space && (looks_host || hostish.starts_with("localhost")) {
        let scheme = if hostish.starts_with("localhost") || hostish.starts_with("127.") { "http" } else { "https" };
        return format!("{scheme}://{t}");
    }
    engine.search_url(t)
}

enum Action {
    Navigate(String),
    OpenTab { url: String, private: bool, focus: bool },
    Submit { form: usize, button: Option<(String, String)> },
    LoadImage(String),
    ShowFullPage,
    ReaderSize(f32),
    ReaderTheme(u8),
    ExitReader,
}

enum ImgSlot {
    Loading,
    Ready(TextureHandle, Vec2, u64),
    Failed,
}

pub struct ScreenshotJob {
    pub path: std::path::PathBuf,
    pub frames: u32,
    pub requested: bool,
    pub done: Arc<AtomicBool>,
}

pub struct Sanaga {
    store: Store,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    agent: ureq::Agent,
    private_agent: ureq::Agent,
    tx: Sender<NetMsg>,
    rx: Receiver<NetMsg>,
    images: HashMap<String, ImgSlot>,
    img_bytes: usize,
    forced_images: HashSet<String>,
    frame: u64,
    menu_open: bool,
    menu_rect: Rect,
    find_open: bool,
    find_query: String,
    find_hits: Vec<usize>,
    find_pos: usize,
    find_key: (String, u64, usize),
    focus_address: bool,
    focus_find: bool,
    suggest_open: bool,
    suggest_rect: Rect,
    hover_url: Option<String>,
    history_filter: String,
    closed: Vec<String>,
    mem: (f64, u64),
    downloads_bar: bool,
    pages_loaded: u64,
    bytes_loaded: u64,
    visited: HashSet<String>,
    applied_dark: Option<(bool, u8)>,
    ctx: egui::Context,
    screenshot: Option<ScreenshotJob>,
    web_ok: bool,
    web_tx: Sender<web::WebEvent>,
    web_rx: Receiver<web::WebEvent>,
    sidebar_open: bool,
    /// Tab groups (Safari-style); 0 is the main group.
    groups: Vec<String>,
    current_group: usize,
    /// "Screen size" mode, and the tab given a mobile browser identity.
    device: Option<devices::DeviceMode>,
    device_ua: Option<(u64, String)>,
    device_touch: Option<webkit6::UserScript>,
    /// Pages sites opened with window.open, waiting to become tabs: (opener tab, new tab id, page).
    popups: std::rc::Rc<std::cell::RefCell<Vec<(u64, u64, crate::page::PageView)>>>,
    popup_ids: std::rc::Rc<std::cell::Cell<u64>>,
    /// Video recording of the simulator.
    recording: Option<recorder::Recording>,
    /// The address being typed in the simulated device's browser.
    device_url_edit: Option<String>,
    /// The address field was just opened (it takes the keyboard).
    device_url_new: bool,
    /// The simulator's drawn device: what it shows, the picture, as egui picture.
    device_frame: Option<(String, gtk::cairo::ImageSurface, TextureHandle)>,
    /// The person's own frame pictures, read once per device and orientation.
    custom_frames: std::cell::RefCell<HashMap<(String, bool), Option<std::rc::Rc<myframes::CustomFrame>>>>,
    /// Address suggestions: the list shown, the one picked with the arrows and
    /// the text typed before picking.
    suggest_items: Vec<(String, String, bool)>,
    suggest_sel: Option<usize>,
    suggest_typed: String,
    /// What was really typed in the address field, and whether the field
    /// now holds it completed with a site's name (the added part selected).
    inline_typed: String,
    inline_done: bool,
    /// The line picked with the arrows in the command palette.
    palette_sel: usize,
    /// A menu is open over a web page, drawn in the layer above it (the
    /// page area is left clear), and that layer is showing.
    live_overlay: bool,
    live_shown: bool,
    /// How far the tab overview's cards have risen into place (0 to 1).
    overview_rise: f32,
    /// Split view: two tabs shown side by side (the left one, the right
    /// one), whenever one of them is the tab in use.
    split: Option<(u64, u64)>,
    /// The two halves as drawn this frame, and whether the left one is in use.
    split_shown: Option<(Rect, Rect, bool)>,
    /// Per group: colour (index in GROUP_COLORS) and folded state.
    group_colors: Vec<u8>,
    group_collapsed: Vec<bool>,
    /// Group whose name field should take the keyboard (just created).
    group_editing: Option<usize>,
    /// Tab overview (all tabs as thumbnails).
    overview_open: bool,
    share_open: bool,
    share_rect: Rect,
    /// Privacy report: trackers kept from following you (domain, number of sites).
    privacy: Vec<(String, usize)>,
    privacy_asked: f64,
    thumb_tx: Sender<(u64, egui::ColorImage)>,
    thumb_rx: Receiver<(u64, egui::ColorImage)>,
    /// Picture of the page shown while a menu covers it (the real page view
    /// can't sit under Sanaga's menus, so it steps aside behind this picture).
    freeze: Option<(u64, TextureHandle)>,
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    live_step: usize,
    freeze_asked: Option<f64>,
    freeze_tx: Sender<(u64, egui::ColorImage)>,
    freeze_rx: Receiver<(u64, egui::ColorImage)>,
    /// Started as a web app (`--app`): one site, no session.
    pub app_mode: bool,
    fullscreen: bool,
    customize_open: bool,
    /// A short message at the bottom of the window (e.g. web app created).
    notice: Option<(String, f64)>,
    pw_prompt: Option<accounts::PwPrompt>,
    /// The saved-accounts bar the person dismissed (tab, site).
    pw_fill_dismissed: Option<(u64, String)>,
    pw_filter: String,
    /// Settings page: the category shown, the search text, and whether
    /// Sanaga is the default browser (asked once).
    settings_section: settings::Section,
    settings_filter: String,
    is_default_browser: Option<bool>,
    /// Settings: the categories seen before / after (‹ ›), and when a
    /// "clear…" button asked for confirmation.
    settings_confirm: Option<f64>,
    /// The welcome tour's step.
    onboarding_step: usize,
    /// Page translation running (tab) and the language asked; the batches
    /// already sent, and which translation the answers belong to.
    translating: Option<u64>,
    translate_batches: usize,
    translate_gen: u64,
    /// The "translate with Google or the AI?" question is shown.
    translate_ask: bool,
    /// The tab that question is about; the translation whose bar is shown.
    translate_ask_tab: u64,
    /// The number of the last request made to a model (answers to earlier
    /// ones are dropped).
    ai_run: u64,
    translation: Option<translate::Translation>,
    /// A page in another language: translating it is offered (tab, site).
    translate_offered: Option<(u64, String)>,
    /// An extension tried in one tab before it is switched on: (tab, its id, its name).
    ext_trial: Option<(u64, String, String)>,
    /// Sites whose DRM bar was dismissed; the identity chooser is open.
    drm_dismissed: HashSet<String>,
    /// The protected-site page's "always open this way" box.
    drm_always_pick: bool,
    /// The tab shown at the memory saver's last step (to count revisits).
    saver_last_tab: u64,
    identity_ask: bool,
    /// Sidebar: its search, the group being renamed in place.
    sidebar_filter: String,
    group_renaming: Option<usize>,
    /// Favourites: the one edited in place (name, address), the one just
    /// removed (to undo), the star's bubble (open, name typed), the search
    /// of the Favourites page, the row being dragged.
    bm_editing: Option<usize>,
    bm_edit: (String, String),
    bm_undo: Option<(usize, crate::store::Bookmark, f64)>,
    bm_popover: bool,
    bm_pop_title: String,
    bm_filter: String,
    bm_drag: Option<usize>,
    pw_shown: Option<(usize, String)>,
    new_profile_name: String,
    new_profile_color: u8,
    /// A guest window (`--guest`): nothing is kept.
    guest: bool,
    /// Started with `--automation`: test software (WebDriver) may open and
    /// drive pages in this window.
    automation: bool,
    /// The profile menu and the button it hangs from.
    profile_open: bool,
    profile_rect: Rect,
    /// Profile photos, loaded once (file date, picture).
    avatar_tex: std::collections::HashMap<std::path::PathBuf, (std::time::SystemTime, egui::TextureHandle)>,
    /// The profiles page: 0 all, 1 this profile customised, 2 adding one.
    profile_section: u8,
    profile_delete: Option<String>,
    web_context: Option<crate::page::Engine>,
    /// This window's number (0 = the first one) and what it asks of the
    /// window system: a new window (with these addresses), or quit everything.
    slot: usize,
    window_request: Option<Vec<String>>,
    quit_all: bool,
    /// The tabs of the other windows of the last session (opened at start).
    extra_sessions: Vec<Vec<String>>,
    /// Data changes seen (another window may change bookmarks, settings…).
    store_seen: u64,
    /// Page summary at the side, answers of the AI models, the
    /// create-an-extension form, the Ollama models found, the Claude key typed.
    summary: Option<intelligence::Summary>,
    ai_tx: std::sync::mpsc::Sender<intelligence::AiDone>,
    ai_rx: Receiver<intelligence::AiDone>,
    ext_form: intelligence::ExtForm,
    ai_models: Option<Vec<String>>,
    ai_key_input: String,
    /// The sync folder's secret phrase being typed (Settings › Sync).
    sync_phrase_input: String,
    ai_key_known: Option<bool>,
    /// When the system's look was last checked.
    look_checked: f64,
    sync_status: crate::sync::SyncStatus,
    sync_at: f64,
    perm_queue: plus::PermQueue,
    perm_current: Option<plus::PermAsk>,
    pending_clear: Option<(u32, u64)>,
    tab_search_open: bool,
    tab_search_q: String,
    clear_period: u64,
    clear_history: bool,
    clear_cookies: bool,
    clear_cache: bool,
    qr: Option<(String, TextureHandle)>,
    share_qr: bool,
    /// When the link was copied from the Share popover (its button says so).
    share_copied_at: f64,
    import_rx: Option<Receiver<importer::ImportDone>>,
    import_status: String,
    import_bookmarks: bool,
    import_history: bool,
    import_passwords: bool,
    /// Until when WebKit is considered busy (fast ticking).
    web_busy_until: f64,
    /// Tab row scrolling when there are too many tabs.
    tab_scroll: f32,
    tab_scroll_seen: (usize, usize),
    restoring: bool,
    shot_private: bool,
    /// The profile's web data folder (containers keep theirs inside).
    web_dir: Option<std::path::PathBuf>,
    /// Focus mode: the whole interface hidden, only the page.
    focus_mode: bool,
    /// Battery: when last checked, and whether the computer runs on it.
    on_battery: (f64, bool),
    /// The battery is at or below Chrome's threshold (20 %).
    battery_low: bool,
    /// The website settings panel of the page shown.
    site_panel: bool,
    /// Open this address (or a new tab: "") in a container: the chooser.
    container_ask: Option<String>,
    new_container_name: String,
    /// The » menu of the toolbar (buttons tucked away).
    overflow_open: bool,
    overflow_rect: Rect,
    /// Text of the page being edited (tab id).
    editing_text: Option<u64>,
    /// The window kept above the others.
    on_top: bool,
    /// The form adding a toolbar button (Settings › Appearance).
    button_form: buttons::ButtonForm,
    /// Video codecs: the bar was dismissed, a page said it couldn't play a
    /// video (tab id), and the install's progress.
    codec_dismissed: bool,
    codec_flag: Option<u64>,
    codec_install: Arc<std::sync::atomic::AtomicU8>,
    /// Sites waiting for "Allow downloads on …?" (Safari's question); for
    /// the light engine, the sites allowed in this window (private or not)
    /// and the downloads waiting (site, private, request).
    download_asks: Vec<String>,
    light_download_sites: Vec<(bool, String)>,
    light_download_pending: Vec<(String, bool, net::Request)>,
    /// The tab where the person started the element remover, or asked for
    /// the floating video: the page's answer is accepted from there only.
    zap_armed: Option<u64>,
    pip_armed: Option<u64>,
    /// An extension just installed from a file, switched off until the
    /// person has read it (shown on the Extensions page).
    ext_review: Option<String>,
    /// Tabs by last use (most recent first), the Ctrl+Tab switcher, and the
    /// page to give the keyboard back to once it is shown.
    recent: Vec<u64>,
    switcher: Option<switcher::Switcher>,
    switcher_refocus: bool,
    /// The ✦ menu (summarise / translate) and where its button is.
    ai_menu_open: bool,
    ai_menu_rect: Rect,
    /// Questions from pages waiting (tab, key), the one being answered, the
    /// site of the last one, the sites silenced; tabs showing an error page.
    dialogs: Vec<(u64, u64)>,
    dialog_state: dialogs::DialogState,
    last_dialog_host: String,
    silenced_sites: HashSet<String>,
    error_tabs: HashSet<u64>,
    /// Tabs showing Sanaga's error page: for what failure, on which host
    /// (its buttons are accepted from there only).
    error_pages: HashMap<u64, errors::ErrorPage>,
    /// The group whose editor is open (name, colour…), and where the group
    /// labels are in the tab bar.
    group_editor: Option<usize>,
    group_label_rects: Vec<(usize, Rect)>,
    /// The last run ended abruptly: the bar that says so. The report whose
    /// text was just copied (its button says so).
    /// Work done on other threads, handed back here (see `off_thread`); what
    /// was already asked once; the keyring is there (None: not known yet);
    /// a sync round is running.
    jobs_tx: Sender<Job>,
    jobs_rx: Receiver<Job>,
    asked_once: HashSet<&'static str>,
    keyring_there: Option<bool>,
    sync_running: bool,
    /// The new tab page's "frequently visited", and what it was worked out
    /// from (visits, favourites): worked out again only when those change.
    frequent: (u64, Vec<(String, usize, String)>),
    crash_notice: Option<crashes::CrashNotice>,
    crash_copied: Option<(std::path::PathBuf, f64)>,
}

/// What a piece of work done on another thread hands back to the window.
type Job = Box<dyn FnOnce(&mut Sanaga) + Send>;

/// Right-click menus open as soon as the button is pressed (like Chrome and
/// GTK apps): a touchpad click that moves the pointer a little, or a slow
/// press, still opens them.
pub(super) trait PressMenu {
    fn menu_on_press(&self, add: impl FnOnce(&mut Ui)) -> Option<egui::InnerResponse<()>>;
}

impl PressMenu for egui::Response {
    fn menu_on_press(&self, add: impl FnOnce(&mut Ui)) -> Option<egui::InnerResponse<()>> {
        let (pressed, held) = self.ctx.input(|i| {
            let s = egui::PointerButton::Secondary;
            (i.pointer.button_pressed(s), i.pointer.button_down(s) || i.pointer.button_released(s))
        });
        let cmd = if pressed && self.hovered() {
            Some(egui::SetOpenCommand::Bool(true))
        } else if self.clicked() {
            Some(egui::SetOpenCommand::Bool(false))
        } else {
            None
        };
        // the release of the button that opened it must not close it
        let close = if held { egui::PopupCloseBehavior::IgnoreClicks } else { egui::PopupCloseBehavior::CloseOnClickOutside };
        egui::Popup::menu(self).open_memory(cmd).at_pointer_fixed().close_behavior(close).show(add)
    }
}

/// Hover help. In Sanaga's GTK window it is a real system tooltip (it shows
/// above web pages, which an in-window bubble can't do).
pub(super) trait Tip {
    fn tip(self, text: impl Into<egui::WidgetText>) -> egui::Response;
}

impl Tip for egui::Response {
    fn tip(self, text: impl Into<egui::WidgetText>) -> egui::Response {
        let text: egui::WidgetText = text.into();
        // the words of the tooltip also name the control for screen readers
        // (Sanaga draws many of its buttons itself)
        let label = text.text().to_string();
        let role = if self.sense.senses_click() { egui::Role::Button } else { egui::Role::Label };
        self.widget_info(|| egui::WidgetInfo::labeled(role, true, &label));
        if crate::shell::active() {
            if self.hovered() && !self.dragged() {
                crate::shell::set_tooltip(label);
            }
            self
        } else {
            self.on_hover_text(text)
        }
    }
}

impl Sanaga {
    pub fn new(egui_ctx: &egui::Context, urls: Vec<String>, screenshot: Option<ScreenshotJob>) -> Self {
        crate::setup_fonts(egui_ctx);
        crate::stage("fonts read");
        egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        // `--profil NOM`: this window uses that profile's own data
        let profile = urls.iter().find_map(|u| u.strip_prefix("profil:").map(|s| s.to_string())).unwrap_or_default();
        // "fenetre:N": an extra window (number N), which starts with its own tabs
        let slot: usize = urls.iter().find_map(|u| u.strip_prefix("fenetre:").and_then(|n| n.parse().ok())).unwrap_or(0);
        // `--guest`: a window that keeps nothing (its own memory-only data)
        let guest = urls.iter().any(|u| u == "invite:");
        // `--automation`: a guest window that test software may drive
        let automation = guest && urls.iter().any(|u| u == "automation:");
        let urls: Vec<String> = urls.into_iter().filter(|u| !u.starts_with("profil:") && !u.starts_with("fenetre:") && u != "nouvelle-fenetre:" && u != "invite:" && u != "automation:").collect();
        let store = if screenshot.is_some() || guest { Store::ephemeral() } else { Store::load_profile(&profile) };
        crate::stage("data read");
        // WebKit keeps cookies and site data per profile
        let web_dir = store.data_dir().map(|d| d.join("webkit"));
        let web_dir_kept = web_dir.clone();
        crate::icons::ICON_STYLE.store(store.settings.app_icon, Ordering::Relaxed);
        let (tx, rx) = channel();
        let (web_tx, web_rx) = channel();
        let (thumb_tx, thumb_rx) = channel();
        let (freeze_tx, freeze_rx) = channel();
        let (ai_tx, ai_rx) = channel();
        let (jobs_tx, jobs_rx) = channel();
        // interface language; WebKit reads it too (pages are asked in that language)
        crate::i18n::set_language(&store.settings.language);
        if screenshot.is_none() {
            let lang = crate::i18n::language();
            unsafe {
                std::env::set_var("LANGUAGE", lang);
            }
        }
        // site icons (loaded by WebKit with their pages): kept in Sanaga's
        // folder; nothing written while taking a screenshot
        // (a guest window writes none: it keeps nothing)
        crate::favicon::setup(Store::root_dir().join("favicons"), screenshot.is_none() && !guest);
        let web_ok = screenshot.is_none() && web_engine_available();
        crate::stage("before engine");
        // The web engine itself is made after the first picture of the
        // window (see `ensure_engine`): starting it takes a moment, and the
        // window must not wait for it to show.
        let web_context: Option<crate::page::Engine> = None;
        crate::stage("before app state");
        let mut app = Sanaga {
            store,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            agent: net::agent(),
            private_agent: net::agent(),
            tx,
            rx,
            images: HashMap::new(),
            img_bytes: 0,
            forced_images: HashSet::new(),
            frame: 0,
            menu_open: false,
            menu_rect: Rect::NOTHING,
            find_open: false,
            find_query: String::new(),
            find_hits: Vec::new(),
            find_pos: 0,
            find_key: (String::new(), 0, 0),
            focus_address: false,
            focus_find: false,
            suggest_open: false,
            suggest_rect: Rect::NOTHING,
            hover_url: None,
            history_filter: String::new(),
            closed: Vec::new(),
            mem: (-10.0, 0),
            downloads_bar: false,
            pages_loaded: 0,
            bytes_loaded: 0,
            visited: HashSet::new(),
            applied_dark: None,
            ctx: egui_ctx.clone(),
            screenshot,
            web_ok,
            web_tx,
            web_rx,
            sidebar_open: false,
            groups: vec![tr("Onglets").to_string()],
            current_group: 0,
            device: None,
            device_ua: None,
            device_touch: None,
            popups: Default::default(),
            // far from the ids of ordinary tabs
            popup_ids: std::rc::Rc::new(std::cell::Cell::new(1 << 40)),
            recording: None,
            device_url_edit: None,
            device_url_new: false,
            device_frame: None,
            custom_frames: Default::default(),
            suggest_items: Vec::new(),
            suggest_sel: None,
            suggest_typed: String::new(),
            inline_typed: String::new(),
            inline_done: false,
            palette_sel: 0,
            live_overlay: false,
            live_shown: false,
            overview_rise: 0.0,
            split: None,
            split_shown: None,
            group_colors: Vec::new(),
            group_collapsed: Vec::new(),
            group_editing: None,
            overview_open: false,
            share_open: false,
            share_rect: Rect::NOTHING,
            privacy: Vec::new(),
            privacy_asked: -100.0,
            thumb_tx,
            thumb_rx,
            freeze: None,
            live_step: 0,
            freeze_asked: None,
            freeze_tx,
            freeze_rx,
            app_mode: false,
            fullscreen: false,
            customize_open: false,
            notice: None,
            pw_prompt: None,
            pw_fill_dismissed: None,
            pw_filter: String::new(),
            settings_section: settings::Section::General,
            settings_filter: String::new(),
            is_default_browser: None,
            settings_confirm: None,
            onboarding_step: 0,
            translating: None,
            translate_batches: 0,
            translate_gen: 0,
            translate_ask: false,
            translate_ask_tab: 0,
            ai_run: 0,
            translation: None,
            translate_offered: None,
            ext_trial: None,
            drm_dismissed: HashSet::new(),
            drm_always_pick: true,
            saver_last_tab: 0,
            identity_ask: false,
            sidebar_filter: String::new(),
            group_renaming: None,
            bm_editing: None,
            bm_edit: (String::new(), String::new()),
            bm_undo: None,
            bm_popover: false,
            bm_pop_title: String::new(),
            bm_filter: String::new(),
            bm_drag: None,
            pw_shown: None,
            new_profile_name: String::new(),
            new_profile_color: 0,
            guest,
            automation,
            profile_open: false,
            profile_rect: Rect::NOTHING,
            avatar_tex: Default::default(),
            profile_section: 0,
            profile_delete: None,
            web_context,
            slot,
            window_request: None,
            quit_all: false,
            extra_sessions: Vec::new(),
            store_seen: crate::store::CHANGES.load(Ordering::Relaxed),
            summary: None,
            ai_tx,
            ai_rx,
            ext_form: Default::default(),
            ai_models: None,
            ai_key_input: String::new(),
            sync_phrase_input: String::new(),
            ai_key_known: None,
            look_checked: 0.0,
            sync_status: Default::default(),
            sync_at: -100.0,
            perm_queue: Default::default(),
            perm_current: None,
            pending_clear: None,
            tab_search_open: false,
            tab_search_q: String::new(),
            clear_period: 3600,
            clear_history: true,
            clear_cookies: true,
            clear_cache: true,
            qr: None,
            share_qr: false,
            share_copied_at: -10.0,
            import_rx: None,
            import_status: String::new(),
            import_bookmarks: true,
            import_history: true,
            import_passwords: true,
            web_busy_until: 0.0,
            tab_scroll: 0.0,
            tab_scroll_seen: (usize::MAX, 0),
            restoring: false,
            jobs_tx,
            jobs_rx,
            asked_once: HashSet::new(),
            keyring_there: None,
            sync_running: false,
            frequent: (u64::MAX, Vec::new()),
            crash_notice: None,
            crash_copied: None,
            shot_private: false,
            web_dir: web_dir_kept,
            focus_mode: false,
            on_battery: (-100.0, false),
            battery_low: false,
            site_panel: false,
            container_ask: None,
            new_container_name: String::new(),
            overflow_open: false,
            overflow_rect: Rect::NOTHING,
            editing_text: None,
            on_top: false,
            button_form: Default::default(),
            codec_dismissed: false,
            codec_flag: None,
            codec_install: Default::default(),
            download_asks: Vec::new(),
            light_download_sites: Vec::new(),
            light_download_pending: Vec::new(),
            zap_armed: None,
            pip_armed: None,
            ext_review: None,
            recent: Vec::new(),
            switcher: None,
            switcher_refocus: false,
            ai_menu_open: false,
            ai_menu_rect: Rect::NOTHING,
            dialogs: Vec::new(),
            dialog_state: Default::default(),
            last_dialog_host: String::new(),
            silenced_sites: HashSet::new(),
            error_tabs: HashSet::new(),
            error_pages: HashMap::new(),
            group_editor: None,
            group_label_rects: Vec::new(),
        };
        crate::stage("app state made");
        app.visited = app.store.history.iter().map(|v| v.url.clone()).collect();
        // test aid for screenshots: SANAGA_SHOT_STATE=dark,menu,sidebar,share,tabsearch,find,private
        if app.screenshot.is_some() {
            let st = std::env::var("SANAGA_SHOT_STATE").unwrap_or_default();
            let has = |k: &str| st.split(',').any(|x| x.trim() == k);
            app.store.settings.dark = has("dark");
            app.menu_open = has("menu");
            app.sidebar_open = has("sidebar");
            app.share_open = has("share");
            app.tab_search_open = has("tabsearch");
            app.find_open = has("find");
            if has("private") {
                app.shot_private = true;
            }
        }
        // `--apercu`: start on the tab overview
        let overview = urls.iter().any(|u| u == "apercu:");
        // (a link to preview, with no Sanaga running yet: simply opened)
        let urls: Vec<String> = urls.into_iter().filter(|u| u != "apercu:").map(|u| u.strip_prefix("preview:").map(|x| x.to_string()).unwrap_or(u)).collect();
        if app.screenshot.is_some() && urls.is_empty() {
            // A fixed scene for the picture: the welcome page, a site tab and a new tab.
            app.open_tab("sanaga://welcome", false);
            app.open_tab("https://fr.wikipedia.org/", false);
            if let Some(t) = app.tabs.last_mut() {
                t.nav += 1;
                t.loading = false;
                t.view = View::Error(String::new());
                t.entries[0].title = tr("Wikipédia, l'encyclopédie libre").into();
            }
            let private = app.shot_private;
            app.open_tab("sanaga://newtab", private);
            app.focus_address = false;
            app.overview_open = overview;
            let st = std::env::var("SANAGA_SHOT_STATE").unwrap_or_default();
            let has = |k: &str| st.split(',').any(|x| x.trim() == k);
            app.menu_open = has("menu");
            app.share_open = has("share");
            app.tab_search_open = has("tabsearch");
            return app;
        }
        let mut start: Vec<String> = urls;
        // web app: one site on its own, no session
        if let Some(u) = start.iter().find_map(|u| u.strip_prefix("webapp:").map(|s| s.to_string())) {
            app.app_mode = true;
            start = vec![u];
        }
        // did the last run of this profile end without closing? (the first
        // window of a normal Sanaga asks; this run leaves its own note)
        let abrupt = if slot == 0 && !app.app_mode && app.screenshot.is_none() { app.store.data_dir().and_then(|d| crate::crash::mark_running(d)) } else { None };
        let mut lost_tabs: Vec<String> = Vec::new();
        if slot == 0 && !app.app_mode && app.screenshot.is_none() {
            // the other windows of the last session open too (or are forgotten)
            let mut extra = app.store.take_extra_sessions();
            // (after an abrupt end, tabs that don't come back by themselves
            // are offered in the bar)
            if abrupt.is_some() && !(start.is_empty() && app.store.settings.restore_tabs) {
                lost_tabs = app.store.load_session(0);
            }
            if start.is_empty() && app.store.settings.restore_tabs {
                start = app.store.load_session(0);
                if start.is_empty() && !extra.is_empty() {
                    start = extra.remove(0);
                }
                app.extra_sessions = extra;
            }
        }
        if start.is_empty() {
            // the very first start: the welcome tour
            start.push(if app.store.first_run { "sanaga://start".into() } else { app.store.settings.home.clone() });
        }
        // restored tabs don't load now: each one loads when it is first shown
        app.restoring = true;
        let mut parents: Vec<(usize, usize)> = Vec::new();
        let mut group_keys: Vec<(String, usize)> = Vec::new();
        // the tab that was shown when Sanaga closed
        let mut shown_at: Option<usize> = None;
        let first_tab = app.tabs.len();
        for line in start {
            let mut parts = line.split('\t');
            let u = parts.next().unwrap_or("").to_string();
            let group = parts.next().unwrap_or("").to_string();
            let pinned = parts.next() == Some("1");
            let saved_title = parts.next().unwrap_or("").to_string();
            let container = parts.next().unwrap_or("").to_string();
            let parent: Option<usize> = parts.next().and_then(|p| p.parse().ok());
            let was_shown = parts.next() == Some("1");
            let color: Option<u8> = parts.next().and_then(|c| c.parse().ok());
            let folded = parts.next() == Some("1");
            // (a group without a name is saved as "\u{1f}N")
            app.current_group = if group.is_empty() || group == app.groups[0] {
                0
            } else if let Some(&(_, g)) = group_keys.iter().find(|(k, _)| *k == group) {
                g
            } else {
                app.groups.push(if group.starts_with('\u{1f}') { String::new() } else { group.clone() });
                let g = app.groups.len() - 1;
                group_keys.push((group, g));
                app.sync_group_meta();
                if let Some(c) = color {
                    app.group_colors[g] = c;
                }
                app.group_collapsed[g] = folded;
                g
            };
            let (reader, u) = match u.strip_prefix("lecteur:") {
                Some(rest) => (true, rest.to_string()),
                None => (false, u),
            };
            let u = input_to_url(&u, app.store.settings.engine, &app.store.settings.home);
            let i = app.open_tab(&u, false);
            app.tabs[i].pinned = pinned;
            if app.store.containers.iter().any(|c| c.name == container) {
                app.tabs[i].container = container;
            }
            if let Some(p) = parent {
                parents.push((i, p));
            }
            if was_shown {
                shown_at = Some(i);
            }
            app.tabs[i].group = app.current_group;
            // tabs load when shown: until then they keep their saved title
            if !saved_title.is_empty() && app.tabs[i].entries[app.tabs[i].idx].title.is_empty() {
                let idx = app.tabs[i].idx;
                app.tabs[i].entries[idx].title = saved_title;
            }
            if reader {
                app.toggle_reader(i);
            }
        }
        for (i, p) in parents {
            if let Some(id) = app.tabs.get(first_tab + p).map(|t| t.id) {
                if first_tab + p != i {
                    app.tabs[i].parent = Some(id);
                }
            }
        }
        app.restoring = false;
        app.active = shown_at.filter(|&i| i < app.tabs.len()).unwrap_or(app.tabs.len() - 1);
        app.current_group = app.tabs[app.active].group;
        app.overview_open = overview;
        if slot > 0 {
            app.save_session();
        }
        if let Some(since) = abrupt {
            app.note_abrupt_end(since, lost_tabs);
        }
        // the saved accounts, from the system keyring
        app.refresh_logins();
        if app.store.settings.ai == "claude" {
            app.check_ai_key();
        }
        app
    }

    fn theme(&self) -> Theme {
        let private = self.tabs.get(self.active).map(|t| t.private).unwrap_or(false);
        theme(self.store.settings.dark, private)
    }

    /// Follow the system's look (Grassfield's accent colour and light / dark
    /// appearance) when the settings say "System".
    fn follow_system_look(&mut self) {
        if self.screenshot.is_some() {
            return;
        }
        let now = self.ctx.input(|i| i.time);
        if now - self.look_checked < 1.5 && self.look_checked > 0.0 {
            return;
        }
        self.look_checked = now;
        let look = crate::system_look::current();
        if let Some(l) = &look {
            crate::theme::set_system_accent(crate::theme::accent_index(&l.accent));
        }
        let st = &mut self.store.settings;
        st.dark = match st.theme {
            1 => false,
            2 => true,
            _ => look.map(|l| l.dark).unwrap_or(st.dark),
        };
    }

    /// The web engine of this window's profile, made at the first need
    /// (after the window's first picture): its processes, cookies and site
    /// data, the downloads, the codecs check and the blocker.
    pub(super) fn ensure_engine(&mut self) {
        if !self.web_ok || self.web_context.is_some() {
            return;
        }
        let c = match &self.web_dir {
            Some(d) => crate::page::profile_context(d),
            None => crate::page::ephemeral_engine(),
        };
        crate::stage("engine made");
        let (t, r) = (self.web_tx.clone(), self.ctx.clone());
        crate::page::follow_downloads(&c);
        crate::page::add_download_sink(self.slot, move |url, path, ok| {
            let _ = t.send(web::WebEvent::DownloadDone { url, path, ok });
            r.request_repaint();
        });
        let (t, r) = (self.web_tx.clone(), self.ctx.clone());
        crate::page::add_download_ask_sink(self.slot, move |site| {
            let _ = t.send(web::WebEvent::DownloadAsk { site });
            r.request_repaint();
        });
        // which video formats the system can play (for YouTube & co.)
        crate::codecs::check_in_background();
        // the network blocker of ads and trackers, compiled once
        crate::page::BLOCKER_ON.store(self.store.settings.blocker, Ordering::Relaxed);
        crate::page::LISTS_ON.store(self.store.settings.block_lists, Ordering::Relaxed);
        if let Some(d) = &self.web_dir {
            crate::page::prepare_filters(&d.join("filters"));
        }
        self.web_context = Some(c);
        if self.automation {
            self.allow_automation();
        }
    }

    fn apply_style(&mut self) {
        self.follow_system_look();
        crate::stage("s: system look");
        let dark = self.store.settings.dark;
        crate::theme::set_accent(self.store.settings.accent as usize);
        let accent = crate::theme::accent_in_use() as u8;
        if self.applied_dark == Some((dark, accent)) {
            return;
        }
        self.applied_dark = Some((dark, accent));
        if self.web_ok {
            // GTK follows: the web inspector, file and print dialogs, and the
            // pages that offer a dark look (`prefers-color-scheme`)
            if let Some(s) = gtk::Settings::default() {
                // (only when it changes: GTK reads its whole theme again, some 150 ms)
                if s.is_gtk_application_prefer_dark_theme() != dark {
                    s.set_gtk_application_prefer_dark_theme(dark);
                }
            }
        }
        crate::stage("s: gtk theme");
        let th = theme(dark, false);
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = th.page;
        v.window_fill = th.page;
        v.extreme_bg_color = th.omnibox;
        v.selection.bg_fill = th.accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, th.accent);
        v.hyperlink_color = th.link;
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        v.widgets.inactive.corner_radius = CornerRadius::same(8);
        v.widgets.hovered.corner_radius = CornerRadius::same(8);
        v.widgets.active.corner_radius = CornerRadius::same(8);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, th.text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, th.text);
        self.ctx.set_visuals(v);
        self.ctx.global_style_mut(|s| {
            use egui::TextStyle::*;
            s.text_styles.insert(Body, FontId::proportional(14.0));
            s.text_styles.insert(Button, FontId::proportional(14.0));
            s.text_styles.insert(Small, FontId::proportional(11.5));
            s.text_styles.insert(Heading, FontId::new(22.0, bold()));
            s.text_styles.insert(Monospace, FontId::monospace(13.0));
            s.spacing.button_padding = vec2(10.0, 5.0);
            s.spacing.interact_size.y = 26.0;
        });
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.screenshot else { return };
        job.frames += 1;
        ctx.request_repaint();
        if crate::shell::active() {
            // in the GTK window: GTK takes the picture (and Sanaga quits)
            let busy = self.tabs.iter().any(|t| t.loading) || self.images.values().any(|i| matches!(i, ImgSlot::Loading));
            if job.frames >= 120 && (!busy || job.frames >= 600) && !job.requested {
                job.requested = true;
                crate::shell::shot_and_quit(job.path.to_string_lossy().to_string());
            }
            return;
        }
        let busy = self.tabs.iter().any(|t| t.loading) || self.images.values().any(|i| matches!(i, ImgSlot::Loading));
        if job.frames >= 120 && (!busy || job.frames >= 900) && !job.requested {
            job.requested = true;
            let path = job.path.clone();
            let done = job.done.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&p.to_srgba_unmultiplied());
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match image::save_buffer(&path, &buf, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => eprintln!("Capture enregistrée : {}", path.display()),
                    Err(e) => eprintln!("Capture impossible : {e}"),
                }
                done.store(true, Ordering::SeqCst);
                c2.request_repaint();
            });
        }
        if job.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if job.frames > 1500 {
            eprintln!("Capture : délai dépassé");
            std::process::exit(1);
        }
    }
}

impl eframe::App for Sanaga {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame_ui(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
    }
}

impl Sanaga {
    /// Addresses opened from other applications while Sanaga runs: new tabs.
    pub fn open_from_outside(&mut self, urls: Vec<String>) {
        let (engine, home) = (self.store.settings.engine, self.store.settings.home.clone());
        for u in urls {
            let (reader, u) = match u.strip_prefix("lecteur:") {
                Some(rest) => (true, rest.to_string()),
                None => (false, u),
            };
            if u == "apercu:" {
                self.overview_open = true;
                continue;
            }
            // `sanaga --preview URL` (a link from another application): a preview window
            if let Some(link) = u.strip_prefix("preview:") {
                self.preview_link(&input_to_url(link, engine, &home));
                continue;
            }
            let i = self.open_tab(&input_to_url(&u, engine, &home), false);
            if reader {
                self.toggle_reader(i);
            }
        }
    }

    /// The web page shown now, if any (it gets the keyboard back).
    /// Split view: the tab shown next to the tab in use (None: no split, or
    /// the tab in use is not one of the pair, or one of them is not a web page).
    pub(super) fn split_partner(&self) -> Option<usize> {
        let (a, b) = self.split?;
        if self.device.is_some() || self.overview_open || self.focus_mode {
            return None;
        }
        let me = self.tabs.get(self.active)?;
        let other = if me.id == a { b } else if me.id == b { a } else { return None };
        let i = self.tabs.iter().position(|t| t.id == other)?;
        (matches!(me.view, View::Web) && matches!(self.tabs[i].view, View::Web)).then_some(i)
    }

    /// Mobile preview side by side: the page in use on the left, the same
    /// page as a phone gets it on the right (a phone's identity and width),
    /// following the left one's address and scrolling. Again: ends it.
    pub(super) fn toggle_mobile_side(&mut self) {
        let now = self.ctx.input(|i| i.time);
        if let Some((_, right)) = self.split {
            if let Some(i) = self.tabs.iter().position(|t| t.id == right && t.companion) {
                self.split = None;
                self.close_tab(i);
                return;
            }
        }
        let a = self.active;
        if !matches!(self.tabs[a].view, View::Web) || !self.tabs[a].url().starts_with("http") {
            self.notice = Some((tr("L'aperçu mobile côte à côte montre une page web : ouvrez-en une d'abord.").into(), now));
            return;
        }
        let (id, url, private, group, container) = (self.tabs[a].id, self.tabs[a].url().to_string(), self.tabs[a].private, self.tabs[a].group, self.tabs[a].container.clone());
        let i = self.open_tab(&url, private);
        self.focus_address = false;
        let t = &mut self.tabs[i];
        t.companion = true;
        t.parent = Some(id);
        t.group = group;
        t.container = container;
        let companion = t.id;
        // right after its page in the tab row; the page stays the tab in use
        if let Some(from) = self.tabs.iter().position(|t| t.id == id) {
            let to = (from + 1).min(self.tabs.len() - 1);
            if to != i {
                self.move_tab(i, to);
            }
        }
        if let Some(back) = self.tabs.iter().position(|t| t.id == id) {
            self.active = back;
        }
        self.split = Some((id, companion));
        self.sync_companion_scroll(id);
    }

    /// The left page of a mobile preview tells Sanaga how far it is
    /// scrolled (a share of its height), so that the phone follows.
    pub(super) fn sync_companion_scroll(&self, left: u64) {
        const JS: &str = "(function(){if(window.__sanagaScroll)return;window.__sanagaScroll=1;var last=0;function say(){var d=document.documentElement;var m=Math.max(1,d.scrollHeight-window.innerHeight);window.ipc.postMessage('scr:'+(window.scrollY/m).toFixed(4));}window.addEventListener('scroll',function(){var n=Date.now();if(n-last>40){last=n;say();}else{clearTimeout(window.__sanagaScrollT);window.__sanagaScrollT=setTimeout(say,60);}},{passive:true});})();";
        if let Some(w) = self.tabs.iter().find(|t| t.id == left).and_then(|t| t.web.as_ref()) {
            let _ = w.evaluate_own(JS);
        }
    }

    /// The companion of `tab` in a mobile preview, if it is its left page.
    fn companion_of(&self, tab: u64) -> Option<usize> {
        let (left, right) = self.split?;
        (left == tab).then(|| self.tabs.iter().position(|t| t.id == right && t.companion)).flatten()
    }

    /// Start or end the split view: the tab in use on the left, the web tab
    /// used just before it on the right.
    pub(super) fn toggle_split(&mut self) {
        if self.split.take().is_some() {
            return;
        }
        let now = self.ctx.input(|i| i.time);
        let Some(me) = self.tabs.get(self.active).filter(|t| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("La vue partagée montre deux pages web : ouvrez-en une d'abord.").into(), now));
            return;
        };
        let other = self.tabs.iter().enumerate().filter(|(i, t)| *i != self.active && matches!(t.view, View::Web) && t.private == me.private && t.group == me.group).max_by(|a, b| a.1.last_active.partial_cmp(&b.1.last_active).unwrap_or(std::cmp::Ordering::Equal));
        match other {
            Some((_, t)) => self.split = Some((me.id, t.id)),
            None => self.notice = Some((tr("Ouvrez une deuxième page web pour la vue partagée.").into(), now)),
        }
    }

    pub fn shown_page(&self) -> Option<webkit6::WebView> {
        let t = self.tabs.get(self.active)?;
        t.web.as_ref().filter(|_| t.web_shown).map(|w| w.webview())
    }

    pub fn frame_count(&self) -> u64 {
        self.frame
    }

    /// Saved state when the window closes.
    pub fn on_close(&mut self) {
        self.save_session();
    }

    /// This window closes while others stay open: its tabs are not reopened
    /// next time (as in Chrome), and it no longer receives downloads.
    pub fn close_window(&mut self) {
        // its tabs can come back: menu ⋮ › More tools › Reopen closed window
        let urls: Vec<String> = self.tabs.iter().filter(|t| !t.private && !t.entries.is_empty()).map(|t| t.url().to_string()).collect();
        if !urls.is_empty() {
            CLOSED_WINDOWS.with(|c| c.borrow_mut().push(urls));
        }
        self.store.forget_session(self.slot);
        crate::page::remove_download_sink(self.slot);
    }

    /// "profil:NAME" for windows of the same profile (nothing for the main one).
    pub fn profile_marker(&self) -> Vec<String> {
        if self.store.profile.is_empty() { Vec::new() } else { vec![format!("profil:{}", self.store.profile)] }
    }

    /// A new window was asked for (with these addresses; none = the home page).
    pub fn take_window_request(&mut self) -> Option<Vec<String>> {
        self.window_request.take()
    }

    /// Quit Sanaga (all windows) rather than close this window?
    pub fn take_quit_all(&mut self) -> bool {
        std::mem::take(&mut self.quit_all)
    }

    /// The tabs of the last session's other windows, to open them again.
    pub fn take_extra_sessions(&mut self) -> Vec<Vec<String>> {
        std::mem::take(&mut self.extra_sessions)
    }

    /// Open again the window closed last (with its tabs).
    pub(super) fn reopen_window(&mut self) {
        match CLOSED_WINDOWS.with(|c| c.borrow_mut().pop()) {
            Some(urls) => self.new_window(urls),
            None => self.notice = Some((tr("Aucune fenêtre fermée à rouvrir.").into(), self.ctx.input(|i| i.time))),
        }
    }

    /// Ask for a new window (Ctrl+N, menu, tab moved to a new window).
    pub(super) fn new_window(&mut self, mut urls: Vec<String>) {
        if !self.store.profile.is_empty() {
            urls.push(format!("profil:{}", self.store.profile));
        }
        self.window_request = Some(urls);
    }

    /// Anything that may wait — another program (the keyring, xdg-settings),
    /// a slow disk or folder, a long calculation — is done on a thread of
    /// its own, never in the window's: the window would stop answering
    /// meanwhile. `then` gets the result back in the window's thread, at
    /// one of the next frames (the tab or page it was for may be gone by
    /// then: it must look again).
    pub(super) fn off_thread<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, then: impl FnOnce(&mut Sanaga, T) + Send + 'static) {
        let (tx, ctx) = (self.jobs_tx.clone(), self.ctx.clone());
        let _ = std::thread::Builder::new().name("sanaga-work".into()).spawn(move || {
            let result = work();
            let job: Job = Box::new(move |app: &mut Sanaga| then(app, result));
            let _ = tx.send(job);
            ctx.request_repaint();
        });
    }

    /// The results of work done on other threads.
    fn finish_jobs(&mut self) {
        while let Ok(job) = self.jobs_rx.try_recv() {
            job(self);
        }
    }

    /// True the first time it is asked with this name (for what is looked
    /// up once per run).
    pub(super) fn first_time(&mut self, what: &'static str) -> bool {
        self.asked_once.insert(what)
    }

    /// Is the system's keyring there? Asked once, on a thread; taken as
    /// there until it has answered (no warning shown for nothing).
    pub(super) fn keyring_ok(&mut self) -> bool {
        if self.first_time("keyring") {
            self.off_thread(crate::keyring::available, |app, ok| app.keyring_there = Some(ok));
        }
        self.keyring_there.unwrap_or(true)
    }

    /// A short message at the bottom of the window.
    pub(super) fn say(&mut self, text: &str) {
        self.notice = Some((text.to_string(), self.ctx.input(|i| i.time)));
    }

    /// A frame ended in an error of Sanaga's own (see shell.rs): what was
    /// open over the page is closed (it may be what failed), and the person
    /// is told. The second time in a row, the tab shown is left for a new
    /// one: its own page may be what fails.
    pub fn after_error(&mut self, times: u8) {
        self.menu_open = false;
        self.suggest_open = false;
        self.share_open = false;
        self.overview_open = false;
        self.tab_search_open = false;
        self.site_panel = false;
        self.profile_open = false;
        self.overflow_open = false;
        self.ai_menu_open = false;
        self.bm_popover = false;
        self.switcher = None;
        self.find_open = false;
        if times >= 2 {
            self.open_tab("sanaga://newtab", false);
        }
        self.say(tr("Une erreur s'est produite dans Sanaga, qui continue. Un rapport a été gardé (sanaga://crashes)."));
    }

    /// Open Settings (at a category's address, or where it was left) in a
    /// new tab. The keys are for the settings — ↑ ↓ change category, Ctrl+F
    /// searches them — so the address bar lets go of them.
    pub(super) fn open_settings(&mut self, address: &str) {
        self.open_tab(address, false);
        self.focus_address = false;
        if let Some(id) = self.ctx.memory(|m| m.focused()) {
            self.ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }

    /// Quit Sanaga: every window.
    pub(super) fn quit_app(&mut self) {
        self.quit_all = true;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Another window changed the saved data (favourites, settings, history,
    /// passwords…): read it again so this window shows the same.
    fn follow_other_windows(&mut self) {
        let now = crate::store::CHANGES.load(Ordering::Relaxed);
        if now == self.store_seen || self.screenshot.is_some() {
            return;
        }
        self.store_seen = now;
        let downloads = std::mem::take(&mut self.store.downloads);
        let profile = self.store.profile.clone();
        self.store = Store::load_profile(&profile);
        self.store.downloads = downloads;
        self.store.first_run = false;
        crate::i18n::set_language(&self.store.settings.language);
        self.applied_dark = None;
        self.visited = self.store.history.iter().map(|v| v.url.clone()).collect();
    }

    /// One frame of Sanaga's interface (drawn by eframe for screenshots, by
    /// the GTK window otherwise).
    pub fn frame_ui(&mut self, ui: &mut Ui) {
        let first = self.frame == 0;
        if first {
            crate::stage("first frame begins");
        }
        self.follow_other_windows();
        if first {
            crate::stage("s: other windows");
        }
        self.frame_body(ui);
        if first {
            crate::stage("first frame laid out");
        }
        // what this window saved itself is already in its memory
        self.store_seen = crate::store::CHANGES.load(Ordering::Relaxed);
    }

    /// Test aids driven by environment variables (SANAGA_BENCH_CYCLE,
    /// SANAGA_LIVE_TEST): compiled into debug builds only.
    #[cfg(debug_assertions)]
    fn test_hooks(&mut self, ui: &mut Ui, ctx0: &egui::Context, now: f64) {
        // test aid: SANAGA_BENCH_CYCLE=n shows the next tab every 3 s, n times
        if let Some(n) = std::env::var("SANAGA_BENCH_CYCLE").ok().and_then(|v| v.parse::<u64>().ok()) {
            let step = (now / 3.0) as u64;
            if step > 0 && step <= n && !self.tabs.is_empty() {
                let want = (step as usize) % self.tabs.len();
                if self.active != want {
                    self.active = want;
                }
            }
            ctx0.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // test aid: SANAGA_LIVE_TEST="4:sidebar,7:menu,9:shot" runs actions at given
        // seconds; "shot" saves a picture of Sanaga's own window (not the screen).
        if let Some(spec) = std::env::var("SANAGA_LIVE_TEST").ok().filter(|_| self.slot == 0) {
            let steps: Vec<(f64, String)> = spec.split(',').filter_map(|x| x.split_once(':')).filter_map(|(t, a)| Some((t.parse().ok()?, a.to_string()))).collect();
            if let Some((t, a)) = steps.get(self.live_step) {
                if now >= *t {
                    self.live_step += 1;
                    crate::shell::doing(a);
                    match a.as_str() {
                        "menu" => self.menu_open = !self.menu_open,
                        "max" => crate::desktop::toggle_maximize(ui.ctx()),
                        "shot" => {
                            // test aid: pictures go to $SANAGA_SHOT_DIR (default: the temp folder)
                            let dir = std::env::var_os("SANAGA_SHOT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("sanaga-shots"));
                            let _ = std::fs::create_dir_all(&dir);
                            crate::shell::request_shot(dir.join(format!("live-{}.png", self.live_step)).to_string_lossy().to_string());
                        }
                        "quit" => self.quit_app(),
                        a if a.starts_with("click:") || a.starts_with("rclick:") || a.starts_with("move:") || a.starts_with("rclick1:") => {
                            let mut p = a.split(':');
                            let kind = p.next().unwrap_or("");
                            let x: f32 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            let y: f32 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            let pos = pos2(x, y);
                            let mut ev = vec![egui::Event::PointerMoved(pos)];
                            if kind != "move" {
                                let button = if kind.starts_with("rclick") { egui::PointerButton::Secondary } else { egui::PointerButton::Primary };
                                ev.push(egui::Event::PointerButton { pos, button, pressed: true, modifiers: Default::default() });
                                ev.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers: Default::default() });
                            }
                            // "rclick1": press and release in the same frame (a quick real click)
                            crate::shell::inject_together(kind == "rclick1", ev);
                        }
                        a if a.starts_with("rpress:") || a.starts_with("rrelease:") => {
                            let mut p = a.split(':');
                            let kind = p.next().unwrap_or("");
                            let x: f32 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            let y: f32 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            let pos = pos2(x, y);
                            crate::shell::inject(vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: kind == "rpress", modifiers: Default::default() },
                            ]);
                        }
                        a if a.starts_with("gtkclick:") || a.starts_with("gtkrclick:") => {
                            let mut p = a.split(':');
                            let kind = p.next().unwrap_or("");
                            let x: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            let y: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                            crate::shell::gtk_click(x, y, if kind == "gtkrclick" { 3 } else { 1 });
                        }
                        a if a.starts_with("type:") => crate::shell::inject(vec![egui::Event::Text(a[5..].to_string())]),
                        // test aids: open a page in a container, a child tab, a preview,
                        // install an extension file, choose the app icon
                        a if a.starts_with("incontainer:") => {
                            if let Some((name, url)) = a[12..].split_once('|') {
                                self.add_container(name);
                                self.open_in_container(url, name);
                            }
                        }
                        a if a.starts_with("childtab:") => {
                            let parent = self.tabs[self.active].id;
                            let i = self.open_tab(&a[9..], false);
                            self.tabs[i].parent = Some(parent);
                        }
                        a if a.starts_with("previewlink:") => self.preview_link(&a[12..]),
                        a if a.starts_with("installext:") => match crate::webext::install(std::path::Path::new(&a[11..])) {
                            Ok((name, id)) => {
                                self.store.extensions_on.push(id.clone());
                                self.store.save_extensions();
                                self.refresh_extensions();
                                eprintln!("live ext installed {name} {id}");
                            }
                            Err(e) => eprintln!("live ext error {e}"),
                        },
                        a if a.starts_with("appicon:") => self.set_app_icon(a[8..].parse().unwrap_or(0)),
                        "moretools" => {
                            self.menu_open = true;
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("menu-more"), true));
                        }
                        "overflow" => self.overflow_open = !self.overflow_open,
                        "aimenu" => self.ai_menu_open = !self.ai_menu_open,
                        "sharemenu" => self.share_open = !self.share_open,
                        "profilemenu" => self.profile_open = !self.profile_open,
                        "makedefault" => {
                            self.make_default_browser_pub();
                            eprintln!("live default browser: {:?}", self.is_default_browser);
                        }
                        a if a.starts_with("grouptab:") => {
                            if let Ok(i) = a[9..].parse::<usize>() {
                                if i < self.tabs.len() {
                                    self.tab_to_new_group(i);
                                }
                            }
                        }
                        a if a.starts_with("foldgroup:") => {
                            if let Ok(g) = a[10..].parse::<usize>() {
                                self.toggle_group_fold(g);
                            }
                        }
                        "launcher" => self.add_profile_launcher(),
                        a if a.starts_with("settingsfind:") => {
                            self.settings_filter = a[13..].to_string();
                            let i = self.active;
                            self.navigate(i, "sanaga://settings".into(), None, true);
                        }
                        // (for tools/lag-audit.sh: an address in this tab or a new
                        // one, a Settings category by its place, a sync now)
                        a if a.starts_with("go:") => {
                            let i = self.active;
                            self.navigate(i, a[3..].to_string(), None, true);
                        }
                        a if a.starts_with("opentab:") => {
                            self.open_tab(&a[8..], false);
                        }
                        a if a.starts_with("section:") => {
                            if let Some(s) = settings::Section::by_place(a[8..].parse().unwrap_or(0)) {
                                let i = self.active;
                                self.navigate(i, s.address(), None, true);
                            }
                        }
                        "syncnow" => self.sync_now(),
                        "panic" => panic!("live test: a panic inside a frame"),
                        a if a.starts_with("probe:") => {
                            if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
                                let _ = w.evaluate_own(&format!("try{{window.ipc.postMessage('tool:probe:'+String({}))}}catch(e){{window.ipc.postMessage('tool:probe:error '+e)}}", &a[6..]));
                            }
                        }
                        "title" => eprintln!("live title {}", self.tabs[self.active].title()),
                        "tabs" => {
                            let list: Vec<String> = self.tabs.iter().enumerate().map(|(k, t)| format!("{}{}{}", if k == self.active { "*" } else { "" }, if t.private { "[private]" } else { "" }, t.url())).collect();
                            eprintln!("live tabs {}", list.join(" | "));
                        }
                        "address" => {
                            let focused = self.ctx.memory(|m| m.has_focus(egui::Id::new("address-bar")));
                            eprintln!("live address {:?} focused={focused}", self.tabs[self.active].address);
                        }
                        a if a.starts_with("key:") => {
                            if let Some(key) = egui::Key::from_name(&a[4..]) {
                                let m = Default::default();
                                crate::shell::inject(vec![
                                    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: m },
                                    egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: m },
                                ]);
                            }
                        }
                        a if a.starts_with("swipe:") => {
                            let mut p = a[6..].split(':');
                            let dir: i32 = p.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
                            let prog: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.6);
                            crate::shell::test_swipe(dir, prog, p.next() == Some("go"));
                        }
                        "inspector-detach" | "inspector-attach" => {
                            use webkit6::prelude::*;
                            if let Some(ins) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()).and_then(|w| w.webview().inspector()) {
                                if a == "inspector-detach" {
                                    ins.detach();
                                } else {
                                    ins.attach();
                                }
                            }
                        }
                        a if a.starts_with("devpick:") => {
                            if let (Some(d), Ok(k)) = (self.device.as_mut(), a[8..].parse::<usize>()) {
                                if let Some(dev) = devices::DEVICES.get(k) {
                                    d.preset = Some(k);
                                    d.w = dev.w;
                                    d.h = dev.h;
                                    d.panel = false;
                                }
                            }
                        }
                        "dbgalloc" => {
                            crate::shell::debug_allocations();
                            let area = self.ctx.content_rect();
                            eprintln!("egui window {:?}", area);
                        }
                        "devrecord" => self.toggle_recording(),
                        "devrotate" => {
                            if let Some(d) = self.device.as_mut() {
                                d.landscape = !d.landscape;
                            }
                        }
                        a if a.starts_with("devcapture") => self.device_capture(a[10..].parse().unwrap_or(0)),
                        "info" => eprintln!("live info ppp={} size={:?} focus={}", ui.ctx().pixels_per_point(), ui.ctx().content_rect().size(), self.focus_mode),
                        other => self.run_shortcut(other),
                    }
                }
            }
            // (only while steps remain: CPU measurements need the real pace)
            if self.live_step < steps.len() {
                ctx0.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    #[cfg(not(debug_assertions))]
    fn test_hooks(&mut self, _ui: &mut Ui, _ctx0: &egui::Context, _now: f64) {}

    fn frame_body(&mut self, ui: &mut Ui) {
        self.frame += 1;
        let frame_start = std::time::Instant::now();
        if std::env::var_os("SANAGA_DEBUG_TICK").is_some() {
            eprintln!("frame t={:.2}", ui.input(|i| i.time));
        }
        self.apply_style();
        // The keyboard alone: in one of Sanaga's own pages (settings,
        // history…), the first Tab goes to the page's first control, not
        // to the window's toolbar (which comes after the page's last one).
        if self.tabs.get(self.active).is_some_and(|t| matches!(t.view, View::Internal(_))) && ui.ctx().memory(|m| m.focused().is_none()) && ui.input(|i| i.key_pressed(Key::Tab) && !i.modifiers.any()) {
            ui.ctx().memory_mut(|m| m.request_focus(egui::Id::new("page-start")));
        }
        self.finish_jobs();
        self.poll_network();
        let ctx0 = ui.ctx().clone();
        let t_pump = std::time::Instant::now();
        self.pump_web(&ctx0);
        let pump_ms = t_pump.elapsed().as_secs_f64() * 1000.0;
        self.poll_tab_extras();
        self.poll_import();
        self.poll_ai();
        crate::favicon::pump(&ctx0);
        let now = ctx0.input(|i| i.time);
        // test aids (debug builds only: not in a release build)
        self.test_hooks(ui, &ctx0, now);
        self.memory_saver_step(now);
        if self.frame == 1 {
            crate::stage("f: before steps");
        }
        // simulator video: stop with the simulator, feed the encoder
        if self.device.is_none() && self.recording.as_ref().is_some_and(|r| !r.stopping) {
            self.toggle_recording();
        }
        self.recording_step();
        self.permission_step();
        if self.screenshot.is_none() && !self.store.settings.sync_dir.trim().is_empty() && now - self.sync_at > 30.0 {
            self.sync_at = now;
            self.sync_now();
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        // the shown group follows the active tab (e.g. a link opened in a new tab)
        let g = self.tabs[self.active].group;
        if g != self.current_group && g < self.groups.len() {
            self.current_group = g;
        }
        self.shortcuts(ui);
        if self.frame == 1 {
            crate::stage("f: shortcuts");
        }
        // back / forward: touchpad swipe or mouse side buttons over a page...
        match crate::shell::take_nav() {
            -1 => self.go_history(self.active, -1),
            1 => self.go_history(self.active, 1),
            _ => {}
        }
        // ...and the mouse side buttons over Sanaga's own parts
        let (b1, b2) = ui.input(|i| (i.pointer.button_pressed(egui::PointerButton::Extra1), i.pointer.button_pressed(egui::PointerButton::Extra2)));
        if b1 {
            self.go_history(self.active, -1);
        } else if b2 {
            self.go_history(self.active, 1);
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(Key::Q)) {
            self.quit_app();
        }
        self.update_find();
        // the group shown is the one of the tab shown (Safari's tab groups)
        if let Some(t) = self.tabs.get(self.active) {
            if !t.pinned {
                self.current_group = t.group;
            }
        }
        let th = self.theme();
        let base_th = theme(self.store.settings.dark, false);
        let ctx = ui.ctx().clone();
        if self.frame % 10 == 1 {
            let title = format!("{} - Sanaga", self.tabs[self.active].title());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        self.power_step(now);
        if self.frame == 1 {
            crate::stage("f: power");
        }
        // a border around the window (Settings › Appearance): the rest inside it
        let border = self.draw_border(ui, now);
        let mut inner_ui;
        let ui: &mut Ui = if border > 0.0 {
            inner_ui = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect().shrink(border)).id_salt("in-border"));
            &mut inner_ui
        } else {
            ui
        };
        // tabs and address bar where the person put them (top by default);
        // focus mode hides them all
        if !self.focus_mode {
            self.window_parts(ui, &th, &base_th);
        }
        if self.permission_bar_visible() {
            egui::Panel::top("permbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.permission_bar(ui, &th);
            });
        }
        if self.automation {
            egui::Panel::top("automationbar").exact_size(30.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(tr("Sanaga est piloté par un logiciel de test automatisé.")).size(12.5).color(th.sub));
                });
            });
        }
        if self.extension_trial_visible() {
            egui::Panel::top("exttrial").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.extension_trial_bar(ui, &th);
            });
        }
        if self.translate_offer_visible() {
            egui::Panel::top("translateoffer").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.translate_offer_bar(ui, &th);
            });
        }
        if self.translation_bar_visible() {
            egui::Panel::top("translationbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.translation_bar(ui, &th);
            });
        }
        if self.crash_bar_visible() {
            egui::Panel::top("crashbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.crash_bar(ui, &th);
            });
        }
        if !self.download_asks.is_empty() {
            egui::Panel::top("downloadaskbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.download_ask_bar(ui, &th);
            });
        }
        self.media_step();
        if self.frame == 1 {
            crate::stage("f: bars");
        }
        if self.codec_bar_visible() {
            egui::Panel::top("codecbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.codec_bar(ui, &th);
            });
        }
        if self.drm_bar_service().is_some() {
            egui::Panel::top("drmbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.drm_bar(ui, &th);
            });
        }
        if self.password_bar_visible() {
            egui::Panel::top("pwbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.card).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.password_bar(ui, &th);
            });
        }
        if self.find_open {
            egui::Panel::top("findbar").exact_size(40.0).show_separator_line(false).frame(Frame::NONE.fill(th.toolbar)).show(ui, |ui| {
                self.find_bar(ui, &th);
            });
        }
        if self.downloads_bar && !self.store.downloads.is_empty() {
            egui::Panel::bottom("downloads").exact_size(50.0).frame(Frame::NONE.fill(th.toolbar).stroke(Stroke::new(1.0, th.border))).show(ui, |ui| {
                self.downloads_panel(ui, &th);
            });
        }
        // the sidebar slides in and out (its content keeps its width and
        // is uncovered, instead of being squeezed)
        let slide = ctx.animate_bool_with_time(egui::Id::new("sidebar-slide"), self.sidebar_open && !self.focus_mode, 0.16);
        if slide > 0.0 {
            let eased = slide * slide * (3.0 - 2.0 * slide);
            egui::Panel::left("sidebar")
                .exact_size((264.0 * eased).max(1.0))
                .show_separator_line(false)
                .frame(Frame::NONE.fill(th.page))
                .show(ui, |ui| {
                    let shown = ui.max_rect();
                    let whole = Rect::from_min_max(pos2(shown.right() - 264.0, shown.top()), shown.right_bottom());
                    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(whole));
                    inner.set_clip_rect(shown);
                    self.sidebar(&mut inner, &th);
                });
        }
        if self.summary.is_some() && !self.focus_mode {
            // (its left edge can be dragged: wider to read, narrower for the page)
            egui::Panel::right("summary").resizable(true).default_size(380.0).size_range(300.0..=640.0).frame(Frame::NONE.fill(th.sidebar)).show(ui, |ui| {
                self.summary_panel(ui, &th);
            });
        }
        // (a menu over a living page: the page area is left clear, see sync_web)
        let page_fill = if self.live_overlay { egui::Color32::TRANSPARENT } else { th.page };
        let central = egui::CentralPanel::no_frame().frame(Frame::NONE.fill(page_fill)).show(ui, |ui| {
            let r = ui.max_rect();
            if !self.focus_mode {
                ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, th.border));
            }
            self.page_area(ui, &th);
            r
        });
        let content = central.inner;
        if self.frame == 1 {
            crate::stage("f: panels and page area");
        }
        self.main_menu(&ctx, &th);
        self.share_popover(&ctx, &th);
        self.tab_search(&ctx, &th);
        self.show_notice(&ctx, content, &th);
        self.undo_bar(&ctx, content, &th);
        self.star_popover(&ctx, &th);
        self.translate_question(&ctx, &th);
        self.identity_chooser(&ctx, &th);
        self.note_recent();
        self.tab_switcher(&ctx, &th);
        self.ai_menu(&ctx, &th);
        self.group_editor_popup(&ctx, &th);
        self.dialog_sheet(&ctx, &th);
        self.site_settings_panel(&ctx, &th);
        self.container_chooser(&ctx, &th);
        self.overflow_menu(&ctx, &th);
        self.profile_menu(&ctx, &th);
        if self.frame == 1 {
            crate::stage("f: popups");
        }
        let t_sync = std::time::Instant::now();
        self.sync_web(content);
        let sync_ms = t_sync.elapsed().as_secs_f64() * 1000.0;
        if self.frame == 1 {
            crate::stage("f: pages placed");
        }

        // link preview bubble, bottom-left
        if let Some(u) = &self.hover_url {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("status")));
            let galley = painter.layout_no_wrap(u.clone(), FontId::proportional(12.0), th.text);
            let w = (galley.size().x + 16.0).min(content.width() * 0.6);
            let r = Rect::from_min_size(pos2(content.left(), content.bottom() - 24.0), vec2(w, 24.0));
            let cr = CornerRadius { nw: 0, ne: 8, sw: 0, se: 0 };
            painter.rect_filled(r, cr, th.card);
            painter.rect_stroke(r, cr, Stroke::new(1.0, th.border), egui::StrokeKind::Inside);
            painter.with_clip_rect(r.shrink2(vec2(8.0, 0.0))).galley(pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0), galley, th.text);
        }

        if self.tabs.iter().any(|t| t.loading) {
            // (with the screen's own refresh: the loading bar moves smoothly)
            ctx.request_repaint();
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            crate::desktop::end_move();
        }
        crate::desktop::resize_edges(&ctx);
        self.screenshot_step(&ctx);
        if std::env::var_os("SANAGA_DEBUG_FRAMES").is_some() {
            let ms = frame_start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("frame {:.3} size={:?} took={ms:.1}ms pump={pump_ms:.1} sync={sync_ms:.1}", ctx.input(|i| i.time), ctx.content_rect().size());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_go_only_to_their_own_site() {
        use super::accounts::origin_of;
        assert_eq!(origin_of("https://www.bank.example/login?x=1"), "https://www.bank.example");
        assert_eq!(origin_of("http://site.example:8080/a"), "http://site.example:8080");
        assert_eq!(origin_of("file:///tmp/login.html"), "file://");
        assert_ne!(origin_of("https://evil.example/"), origin_of("https://bank.example/"));
        assert_eq!(origin_of("not a url"), "");
    }

    #[test]
    fn address_bar_understands_input() {
        let e = Engine::DuckDuckGo;
        assert_eq!(input_to_url("wikipedia.org", e, "h"), "https://wikipedia.org");
        assert_eq!(input_to_url("localhost:8080/x", e, "h"), "http://localhost:8080/x");
        assert_eq!(input_to_url("https://a.b/c", e, "h"), "https://a.b/c");
        assert!(input_to_url("fleuve du Cameroun", e, "h").starts_with("https://html.duckduckgo.com/html/?q=fleuve"));
        assert!(input_to_url("bonjour", e, "h").contains("q=bonjour"));
        assert_eq!(input_to_url("  ", e, "home"), "home");
    }
}

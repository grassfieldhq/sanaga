//! How Sanaga introduces itself to each site (its "user agent"). By default it
//! is WebKit's own (close to Safari's). A few web apps refuse WebKit on Linux
//! although they work with it — they get a Chrome identity; the person can
//! choose for any site (Settings › Privacy). And sites whose videos need a
//! DRM that WebKitGTK does not have (Netflix, Spotify…) are recognised, to
//! offer opening them in an installed browser or app.

use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Identity {
    /// WebKit's own (Safari-like).
    Auto,
    Safari,
    Chrome,
    Firefox,
}

impl Identity {
    pub const ALL: [Identity; 4] = [Identity::Auto, Identity::Safari, Identity::Chrome, Identity::Firefox];

    pub fn key(self) -> &'static str {
        match self {
            Identity::Auto => "auto",
            Identity::Safari => "safari",
            Identity::Chrome => "chrome",
            Identity::Firefox => "firefox",
        }
    }

    pub fn from_key(k: &str) -> Identity {
        Identity::ALL.into_iter().find(|i| i.key() == k).unwrap_or(Identity::Auto)
    }

    pub fn label(self) -> &'static str {
        match self {
            Identity::Auto => crate::i18n::tr("Automatique"),
            Identity::Safari => "Safari",
            Identity::Chrome => "Chrome",
            Identity::Firefox => "Firefox",
        }
    }

    /// The user agent sent (None: WebKit's own).
    pub fn user_agent(self) -> Option<&'static str> {
        match self {
            Identity::Auto => None,
            Identity::Safari => Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15"),
            Identity::Chrome => Some("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"),
            Identity::Firefox => Some("Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0"),
        }
    }
}

/// Web apps that turn WebKit on Linux away but work with it.
const BUILT_IN: [(&str, Identity); 6] = [
    ("web.whatsapp.com", Identity::Chrome),
    ("teams.microsoft.com", Identity::Chrome),
    ("teams.live.com", Identity::Chrome),
    ("app.zoom.us", Identity::Chrome),
    ("zoom.us", Identity::Chrome),
    ("messenger.com", Identity::Chrome),
];

/// The person's choices (host → identity), kept by the store.
static CHOSEN: Mutex<Vec<(String, Identity)>> = Mutex::new(Vec::new());

pub fn set_choices(list: Vec<(String, Identity)>) {
    if let Ok(mut c) = CHOSEN.lock() {
        *c = list;
    }
}

fn matches(host: &str, site: &str) -> bool {
    host == site || host.ends_with(&format!(".{site}"))
}

/// The identity for a host: the person's choice, else the built-in list.
pub fn for_host(host: &str) -> Identity {
    let host = host.trim_start_matches("www.");
    if let Some(i) = CHOSEN.lock().ok().and_then(|c| c.iter().find(|(s, _)| matches(host, s)).map(|(_, i)| *i)) {
        return i;
    }
    BUILT_IN.iter().find(|(s, _)| matches(host, s)).map(|(_, i)| *i).unwrap_or(Identity::Auto)
}

// ------------------------------------------------------------------ DRM

/// Sites whose video or music needs Widevine (a DRM WebKitGTK cannot use).
const DRM_SITES: [(&str, &str); 14] = [
    ("netflix.com", "Netflix"),
    ("disneyplus.com", "Disney+"),
    ("primevideo.com", "Prime Video"),
    ("max.com", "Max"),
    ("open.spotify.com", "Spotify"),
    ("tidal.com", "Tidal"),
    ("deezer.com", "Deezer"),
    ("tv.apple.com", "Apple TV+"),
    ("music.apple.com", "Apple Music"),
    ("canalplus.com", "Canal+"),
    ("hulu.com", "Hulu"),
    ("paramountplus.com", "Paramount+"),
    ("crunchyroll.com", "Crunchyroll"),
    ("molotov.tv", "Molotov"),
];

/// The service's name when this host needs a DRM.
pub fn drm_service(host: &str) -> Option<&'static str> {
    let host = host.trim_start_matches("www.");
    DRM_SITES.iter().find(|(s, _)| matches(host, s)).map(|(_, n)| *n)
}

/// The service, when opening this address in Sanaga would only show a
/// player that can't play: a page of a DRM site (not the small players
/// other sites embed, which play previews without DRM), or a video-call
/// site when this system's web engine has no WebRTC.
pub fn protected_url(url: &str) -> Option<&'static str> {
    let u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.path().starts_with("/embed") {
        return None;
    }
    let host = u.host_str()?;
    let service = drm_service(host).or_else(|| if webrtc_missing() { call_service(host) } else { None })?;
    let staying = STAY.lock().is_ok_and(|s| s.iter().any(|x| *x == service));
    (!staying).then_some(service)
}

/// Video-call sites: they need WebRTC.
const CALL_SITES: [(&str, &str); 8] = [
    ("meet.google.com", "Google Meet"),
    ("teams.microsoft.com", "Microsoft Teams"),
    ("teams.live.com", "Microsoft Teams"),
    ("app.zoom.us", "Zoom"),
    ("meet.jit.si", "Jitsi Meet"),
    ("whereby.com", "Whereby"),
    ("webex.com", "Webex"),
    ("discord.com", "Discord"),
];

/// The service, when this host is a video-call site.
pub fn call_service(host: &str) -> Option<&'static str> {
    let host = host.trim_start_matches("www.");
    CALL_SITES.iter().find(|(s, _)| matches(host, s)).map(|(_, n)| *n)
}

/// This system's WebKitGTK was built without WebRTC (a page found no
/// `RTCPeerConnection`): video calls can't connect in Sanaga.
static NO_WEBRTC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_webrtc_missing() {
    NO_WEBRTC.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn webrtc_missing() -> bool {
    NO_WEBRTC.load(std::sync::atomic::Ordering::Relaxed)
}

/// Services the person chose to open in Sanaga anyway (until it closes).
static STAY: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

pub fn stay_here(service: &'static str) {
    if let Ok(mut s) = STAY.lock() {
        if !s.contains(&service) {
            s.push(service);
        }
    }
}

/// A way to play a DRM site: the service's own app, or a browser that has
/// Widevine.
#[derive(Clone, Debug, PartialEq)]
pub struct Handoff {
    /// "Ouvrir dans Google Chrome"
    pub label: String,
    /// "Google Chrome"
    pub name: String,
    pub program: String,
    pub app: bool,
}

/// Can this browser play Widevine-protected video? Chrome, Edge, Brave and
/// Vivaldi bring it; Firefox downloads it by itself; Chromium only when the
/// Widevine module is there.
fn has_widevine(program: &str) -> bool {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    let exists = |p: &std::path::Path| p.exists();
    match program {
        "chromium" => {
            exists(std::path::Path::new("/usr/lib/chromium/WidevineCdm"))
                || exists(std::path::Path::new("/usr/lib/chromium/libwidevinecdm.so"))
                || exists(std::path::Path::new("/usr/lib64/chromium/WidevineCdm"))
                || exists(&home.join(".config/chromium/WidevineCdm"))
        }
        _ => true,
    }
}

/// What is installed to play `service` (its app first, then browsers that
/// have Widevine).
pub fn handoffs(service: &str) -> Vec<Handoff> {
    // (looked up once per run: the bar and the page ask every frame)
    static FOUND: std::sync::OnceLock<(Option<String>, Vec<(String, &'static str)>)> = std::sync::OnceLock::new();
    let (spotify, browsers) = FOUND.get_or_init(find_handoffs);
    let mut out = Vec::new();
    if service == "Spotify" {
        if let Some(p) = spotify {
            out.push(Handoff { label: crate::i18n::tr("Ouvrir dans l'application Spotify").into(), name: "Spotify".into(), program: p.clone(), app: true });
        }
    }
    for (p, name) in browsers {
        out.push(Handoff { label: crate::i18n::tf("Ouvrir dans {}", &[name]), name: name.to_string(), program: p.clone(), app: false });
    }
    out
}

/// The programs on this computer that can play protected video: installed
/// normally (in PATH) or as a Flatpak (its launcher in flatpak's exports).
fn find_handoffs() -> (Option<String>, Vec<(String, &'static str)>) {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/share/flatpak/exports/bin"));
    }
    dirs.push("/var/lib/flatpak/exports/bin".into());
    // a program in PATH is started by its name, a Flatpak by its launcher's path
    let find = |p: &str| {
        dirs.iter().map(|d| d.join(p)).find(|f| f.is_file()).map(|f| if f.to_string_lossy().contains("/flatpak/exports/") { f.to_string_lossy().to_string() } else { p.to_string() })
    };
    let spotify = ["spotify", "spotify-launcher", "com.spotify.Client"].iter().find_map(|p| find(p));
    let mut browsers: Vec<(String, &'static str)> = Vec::new();
    for (p, name) in [
        ("google-chrome-stable", "Google Chrome"),
        ("com.google.Chrome", "Google Chrome"),
        ("chromium", "Chromium"),
        ("brave", "Brave"),
        ("com.brave.Browser", "Brave"),
        ("vivaldi-stable", "Vivaldi"),
        ("microsoft-edge-stable", "Microsoft Edge"),
        ("com.microsoft.Edge", "Microsoft Edge"),
        ("firefox", "Firefox"),
        ("org.mozilla.firefox", "Firefox"),
        ("zen-browser", "Zen"),
    ] {
        if browsers.iter().any(|(_, n)| *n == name) || !has_widevine(p) {
            continue;
        }
        if let Some(path) = find(p) {
            browsers.push((path, name));
        }
    }
    (spotify, browsers)
}

/// The command that installs a browser able to play protected video on
/// this system (shown, to copy), or None when the system is not known.
pub fn browser_install_hint() -> Option<String> {
    let has = |p: &str| std::path::Path::new(p).exists();
    let admin = "sudo";
    Some(if has("/usr/bin/pacman") {
        format!("{admin} pacman -S firefox")
    } else if has("/usr/bin/apt") {
        format!("{admin} apt install firefox")
    } else if has("/usr/bin/dnf") {
        format!("{admin} dnf install firefox")
    } else if has("/usr/bin/zypper") {
        format!("{admin} zypper install MozillaFirefox")
    } else if has("/usr/bin/flatpak") {
        "flatpak install flathub org.mozilla.firefox".to_string()
    } else {
        return None;
    })
}

/// Chrome-family browsers open the page as an app window of its own (no
/// tabs, no address bar: it looks like part of the desktop).
fn chrome_family(program: &str) -> bool {
    let p = program.rsplit('/').next().unwrap_or(program);
    ["google-chrome-stable", "chromium", "brave", "vivaldi-stable", "microsoft-edge-stable", "com.google.Chrome", "com.brave.Browser", "com.microsoft.Edge"].contains(&p)
}

/// Open the page with that browser or app.
pub fn open_with(h: &Handoff, url: &str) {
    let mut cmd = std::process::Command::new(&h.program);
    if h.app && url.contains("open.spotify.com") {
        // the app opens the same thing from its spotify: address
        let path = url.split("open.spotify.com/").nth(1).unwrap_or("").split('?').next().unwrap_or("");
        let uri = format!("spotify:{}", path.replace('/', ":"));
        cmd.arg(format!("--uri={uri}"));
    } else if !h.app && chrome_family(&h.program) {
        cmd.arg(format!("--app={url}"));
    } else if !h.app && (h.program == "firefox" || h.program.ends_with("org.mozilla.firefox")) {
        cmd.args(["--new-window", url]);
    } else if !h.app {
        cmd.arg(url);
    }
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_and_drm_sites() {
        assert_eq!(for_host("web.whatsapp.com"), Identity::Chrome);
        assert_eq!(for_host("www.example.com"), Identity::Auto);
        set_choices(vec![("example.com".into(), Identity::Firefox)]);
        assert_eq!(for_host("shop.example.com"), Identity::Firefox);
        set_choices(Vec::new());
        assert_eq!(drm_service("www.netflix.com"), Some("Netflix"));
        assert_eq!(drm_service("open.spotify.com"), Some("Spotify"));
        assert_eq!(drm_service("spotify.com"), None);
        assert_eq!(protected_url("https://www.netflix.com/browse"), Some("Netflix"));
        assert_eq!(protected_url("https://open.spotify.com/embed/track/1"), None);
        assert_eq!(protected_url("https://example.com/netflix.com"), None);
        assert_eq!(call_service("meet.google.com"), Some("Google Meet"));
        assert_eq!(protected_url("https://meet.google.com/abc-defg-hij"), None);
        set_webrtc_missing();
        assert_eq!(protected_url("https://meet.google.com/abc-defg-hij"), Some("Google Meet"));
        assert_eq!(Identity::from_key("chrome"), Identity::Chrome);
    }
}

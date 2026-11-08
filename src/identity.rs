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

/// A way to play a DRM site: the service's own app, or a browser that has
/// Widevine. (label, program, arguments before the address)
#[derive(Clone, Debug, PartialEq)]
pub struct Handoff {
    pub label: String,
    pub program: String,
    pub app: bool,
}

/// What is installed to play `service` (its app first, then browsers).
pub fn handoffs(service: &str) -> Vec<Handoff> {
    let mut out = Vec::new();
    let has = |p: &str| std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(p).is_file()));
    if service == "Spotify" {
        for p in ["spotify", "spotify-launcher"] {
            if has(p) {
                out.push(Handoff { label: crate::i18n::tr("Ouvrir dans l'application Spotify").into(), program: p.into(), app: true });
                break;
            }
        }
    }
    for (p, name) in [
        ("google-chrome-stable", "Google Chrome"),
        ("chromium", "Chromium"),
        ("brave", "Brave"),
        ("vivaldi-stable", "Vivaldi"),
        ("microsoft-edge-stable", "Microsoft Edge"),
        ("firefox", "Firefox"),
        ("zen-browser", "Zen"),
    ] {
        if has(p) {
            out.push(Handoff { label: crate::i18n::tf("Ouvrir dans {}", &[&name]), program: p.into(), app: false });
        }
    }
    out
}

/// Open the page with that browser or app.
pub fn open_with(h: &Handoff, url: &str) {
    let mut cmd = std::process::Command::new(&h.program);
    if h.app && url.contains("open.spotify.com") {
        // the app opens the same thing from its spotify: address
        let path = url.split("open.spotify.com/").nth(1).unwrap_or("").split('?').next().unwrap_or("");
        let uri = format!("spotify:{}", path.replace('/', ":"));
        cmd.arg(format!("--uri={uri}"));
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
        assert_eq!(Identity::from_key("chrome"), Identity::Chrome);
    }
}

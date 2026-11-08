//! Website settings, site by site: JavaScript, web fonts, the ad and tracker
//! blocker, and the page's look (dark mode, another font, no sticky headers,
//! copy unlocked, parts of the page removed for good). Kept by the store,
//! read by the pages as they load (`page.rs`) — like the browser identity.

use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct SitePrefs {
    /// JavaScript allowed.
    pub js: bool,
    /// The site's own fonts (off: the system's fonts, lighter and readable).
    pub fonts: bool,
    /// The ad and tracker blocker works on this site.
    pub blocker: bool,
    /// Shown dark (colours turned around).
    pub dark: bool,
    /// Headers and bars that stay stuck at the top or bottom are let go.
    pub no_sticky: bool,
    /// Text can be selected, copied and right-clicked even if the site forbids it.
    pub unlock: bool,
    /// Font used for the whole page ("" = the site's).
    pub font: String,
    /// Parts of the page removed by the person (CSS selectors).
    pub hidden: Vec<String>,
}

impl Default for SitePrefs {
    fn default() -> Self {
        SitePrefs { js: true, fonts: true, blocker: true, dark: false, no_sticky: false, unlock: false, font: String::new(), hidden: Vec::new() }
    }
}

impl SitePrefs {
    pub fn is_default(&self) -> bool {
        *self == SitePrefs::default()
    }

    /// One line of `sites.tsv`: host, then key=value pairs.
    pub fn to_line(&self, host: &str) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let hidden: Vec<String> = self.hidden.iter().map(|s| s.replace(['\t', '\n', '\u{1f}'], " ")).collect();
        format!(
            "{}\tjs={}\tfonts={}\tblocker={}\tdark={}\tnosticky={}\tunlock={}\tfont={}\thidden={}",
            host,
            b(self.js),
            b(self.fonts),
            b(self.blocker),
            b(self.dark),
            b(self.no_sticky),
            b(self.unlock),
            self.font.replace(['\t', '\n'], " "),
            hidden.join("\u{1f}")
        )
    }

    pub fn from_line(line: &str) -> Option<(String, SitePrefs)> {
        let mut parts = line.split('\t');
        let host = parts.next()?.trim().to_string();
        if host.is_empty() {
            return None;
        }
        let mut p = SitePrefs::default();
        for kv in parts {
            let Some((k, v)) = kv.split_once('=') else { continue };
            match k {
                "js" => p.js = v == "1",
                "fonts" => p.fonts = v == "1",
                "blocker" => p.blocker = v == "1",
                "dark" => p.dark = v == "1",
                "nosticky" => p.no_sticky = v == "1",
                "unlock" => p.unlock = v == "1",
                "font" => p.font = v.to_string(),
                "hidden" => p.hidden = v.split('\u{1f}').filter(|s| !s.trim().is_empty()).map(|s| s.to_string()).collect(),
                _ => {}
            }
        }
        Some((host, p))
    }

    /// The style sheet that gives the site its look (empty: nothing to add).
    pub fn css(&self) -> String {
        let mut css = String::new();
        if self.dark {
            css.push_str("html{filter:invert(0.92) hue-rotate(180deg)!important;background:#fff!important}img,video,picture,canvas,iframe,svg image,[style*=\"background-image\"]{filter:invert(1) hue-rotate(180deg)!important}\n");
        }
        if !self.font.trim().is_empty() {
            let f = self.font.replace(['"', '\\', '{', '}', ';', '<'], "");
            css.push_str(&format!("body,body *:not(code):not(pre):not(kbd):not(samp):not([class*=\"icon\"]):not([class*=\"Icon\"]):not(i){{font-family:\"{f}\",sans-serif!important}}\n"));
        }
        if self.unlock {
            css.push_str("*{-webkit-user-select:text!important;user-select:text!important}\n");
        }
        if !self.hidden.is_empty() {
            let list: Vec<String> = self.hidden.iter().map(|s| s.replace(['{', '}', '<'], "")).collect();
            css.push_str(&format!("{}{{display:none!important}}\n", list.join(",")));
        }
        css
    }

    /// What has to run in the page itself (sticky headers, copy locks).
    pub fn script(&self) -> String {
        let mut js = String::new();
        if self.unlock {
            js.push_str(UNLOCK_JS);
        }
        if self.no_sticky {
            js.push_str(NO_STICKY_JS);
        }
        js
    }
}

/// The site's own handlers that forbid copying, selecting or the context
/// menu never hear about it: Sanaga's listeners run first and stop it.
const UNLOCK_JS: &str = r#"(function(){if(window.__sanagaUnlock)return;window.__sanagaUnlock=1;
['copy','cut','paste','selectstart','contextmenu','dragstart','mousedown','mouseup','keydown'].forEach(function(t){
window.addEventListener(t,function(e){if(t==='keydown'&&!(e.ctrlKey||e.metaKey))return;e.stopImmediatePropagation();},true);});
document.oncopy=document.oncontextmenu=document.onselectstart=null;})();"#;

/// Bars stuck at the top or bottom of the window are let go (they scroll
/// away with the page); checked again as the page changes.
const NO_STICKY_JS: &str = r#"(function(){if(window.__sanagaNoSticky)return;window.__sanagaNoSticky=1;
function free(){var h=window.innerHeight;document.querySelectorAll('body *').forEach(function(el){var s=getComputedStyle(el);
if((s.position==='fixed'||s.position==='sticky')&&el.offsetHeight<h*0.4&&el.offsetWidth>window.innerWidth*0.5){
var r=el.getBoundingClientRect();if(r.top<=2||r.bottom>=h-2){el.style.setProperty('position','static','important');}}});}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',free);else free();
var n=0;var t=setInterval(function(){free();if(++n>8)clearInterval(t);},1500);})();"#;

/// The person's choices (host → prefs), kept by the store.
static PREFS: Mutex<Vec<(String, SitePrefs)>> = Mutex::new(Vec::new());

pub fn set_all(list: Vec<(String, SitePrefs)>) {
    if let Ok(mut p) = PREFS.lock() {
        *p = list;
    }
}

pub fn all() -> Vec<(String, SitePrefs)> {
    PREFS.lock().map(|p| p.clone()).unwrap_or_default()
}

/// "www.site.com" and "site.com" share their settings.
pub fn key(host: &str) -> String {
    host.trim_start_matches("www.").to_ascii_lowercase()
}

/// The settings of a host (its own, else those of the site it belongs to).
pub fn for_host(host: &str) -> SitePrefs {
    let h = key(host);
    let Ok(p) = PREFS.lock() else { return SitePrefs::default() };
    if let Some((_, s)) = p.iter().find(|(k, _)| *k == h) {
        return s.clone();
    }
    p.iter().filter(|(k, _)| h.ends_with(&format!(".{k}"))).max_by_key(|(k, _)| k.len()).map(|(_, s)| s.clone()).unwrap_or_default()
}

/// Ads and trackers blocked by the network blocker (the request never
/// leaves): advertising networks, analytics and fingerprinting services. Only
/// requests from other sites are stopped; a tracker's own site still opens.
pub const BLOCKED_DOMAINS: &[&str] = &[
    "doubleclick.net", "googlesyndication.com", "googleadservices.com", "google-analytics.com", "googletagmanager.com", "googletagservices.com",
    "adservice.google.com", "pagead2.googlesyndication.com", "imasdk.googleapis.com", "app-measurement.com", "admob.com",
    "facebook.net", "connect.facebook.net", "ads-twitter.com", "ads.linkedin.com", "analytics.tiktok.com", "ads.tiktok.com",
    "amazon-adsystem.com", "adnxs.com", "adsrvr.org", "criteo.com", "criteo.net", "taboola.com", "outbrain.com", "rubiconproject.com",
    "pubmatic.com", "openx.net", "casalemedia.com", "smartadserver.com", "teads.tv", "moatads.com", "adform.net", "yieldmo.com",
    "33across.com", "sharethrough.com", "media.net", "bidswitch.net", "contextweb.com", "indexww.com", "lijit.com", "sovrn.com",
    "quantserve.com", "scorecardresearch.com", "chartbeat.com", "chartbeat.net", "hotjar.com", "hotjar.io", "mouseflow.com",
    "fullstory.com", "crazyegg.com", "clarity.ms", "newrelic.com", "nr-data.net", "segment.io", "segment.com", "mixpanel.com",
    "amplitude.com", "heap.io", "heapanalytics.com", "kissmetrics.com", "optimizely.com", "branch.io", "appsflyer.com",
    "adjust.com", "kochava.com", "braze.com", "onesignal.com", "pushwoosh.com", "zemanta.com", "revcontent.com", "mgid.com",
    "adroll.com", "bluekai.com", "demdex.net", "omtrdc.net", "everesttech.net", "krxd.net", "exelator.com", "agkn.com",
    "rlcdn.com", "tapad.com", "liadm.com", "id5-sync.com", "crwdcntrl.net", "addthis.com", "sharethis.com", "yandex.ru/ads",
    "mc.yandex.ru", "adriver.ru", "popads.net", "propellerads.com", "adsterra.com", "exoclick.com", "trafficjunky.net",
    "juicyads.com", "zedo.com", "serving-sys.com", "flashtalking.com", "innovid.com", "spotxchange.com", "springserve.com",
    "stickyadstv.com", "tremorhub.com", "undertone.com", "vidoomy.com", "gumgum.com", "kargo.com", "nativo.com", "triplelift.com",
];

/// WebKit's content-blocker rules (JSON) for the list above.
pub fn blocker_rules() -> String {
    let mut out = String::from("[");
    for (k, d) in BLOCKED_DOMAINS.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        let (host, path) = d.split_once('/').unwrap_or((d, ""));
        // in JSON a regular expression's "\." is written "\\."
        let esc = |s: &str| s.replace('.', "\\\\.");
        out.push_str(&format!(
            "{{\"trigger\":{{\"url-filter\":\"^[a-z]+://([^/]*\\\\.)?{}[:/]{}\",\"load-type\":[\"third-party\"]}},\"action\":{{\"type\":\"block\"}}}}",
            esc(host),
            esc(path)
        ));
    }
    out.push(']');
    out
}

/// A rule set that stops the site's web fonts (the system's are used).
pub fn no_fonts_rules() -> &'static str {
    r#"[{"trigger":{"url-filter":".*","resource-type":["font"]},"action":{"type":"block"}}]"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        let p = SitePrefs { js: false, dark: true, font: "Inter".into(), hidden: vec![".ad".into(), "#top > div".into()], ..Default::default() };
        let (h, q) = SitePrefs::from_line(&p.to_line("site.example")).unwrap();
        assert_eq!(h, "site.example");
        assert_eq!(p, q);
    }

    #[test]
    fn rules_are_json_like() {
        let r = blocker_rules();
        assert!(r.starts_with('[') && r.ends_with(']'));
        assert!(r.contains("doubleclick\\\\.net"));
        assert_eq!(r.matches("\"block\"").count(), BLOCKED_DOMAINS.len());
    }

    #[test]
    fn subdomains_share_settings() {
        set_all(vec![("site.example".into(), SitePrefs { js: false, ..Default::default() })]);
        assert!(!for_host("www.site.example").js);
        assert!(!for_host("news.site.example").js);
        assert!(for_host("other.example").js);
        set_all(Vec::new());
    }
}

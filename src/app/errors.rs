//! The pages shown when a site does not load: what happened in plain words,
//! what to try, and the right buttons — try again (by itself when the
//! connection comes back), search for the site, its archived copy, go back to
//! safety (certificate). Drawn in Sanaga's look (light / dark, accent colour).

use super::*;
use crate::page::LoadError;
use webkit6::prelude::*;

fn hex(c: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The picture at the top of the page, by kind (drawn in the accent colour).
fn icon(kind: LoadError) -> &'static str {
    match kind {
        LoadError::NotFound => r#"<circle cx="30" cy="30" r="20"/><path d="M10 30h40M30 10c6 6 9 13 9 20s-3 14-9 20M30 10c-6 6-9 13-9 20s3 14 9 20"/><circle cx="48" cy="48" r="9" class="f"/><path d="M45 45.5c.6-2 4.8-2 5 .6.2 2-2.6 2.2-2.6 4.2" class="w"/><circle cx="47.4" cy="53" r=".6" class="w"/>"#,
        LoadError::Offline => r#"<path d="M8 24c12-11 32-11 44 0M15 31c8-7 22-7 30 0M22 38c5-4 11-4 16 0"/><circle cx="30" cy="45" r="2.5" class="f"/><path d="M10 10l40 40"/>"#,
        LoadError::Refused | LoadError::BlockedPort => r#"<rect x="12" y="14" width="36" height="24" rx="4"/><path d="M20 46h20M30 38v8"/><path d="M24 22l12 10M36 22l-12 10"/>"#,
        LoadError::Timeout => r#"<circle cx="30" cy="32" r="20"/><path d="M30 20v12l8 5M24 8h12"/>"#,
        LoadError::Certificate => r#"<rect x="14" y="26" width="32" height="24" rx="4"/><path d="M21 26v-6a9 9 0 0118 0v6"/><path d="M30 34v6"/><circle cx="30" cy="33" r="2.5" class="f"/>"#,
        LoadError::NoFile => r#"<path d="M18 8h17l11 11v33H18z"/><path d="M35 8v11h11"/><path d="M25 32l10 10M35 32l-10 10"/>"#,
        LoadError::Other => r#"<circle cx="30" cy="30" r="21"/><path d="M30 18v15"/><circle cx="30" cy="41" r="2.5" class="f"/>"#,
    }
}

impl Sanaga {
    /// A page did not load: Sanaga's page in its place.
    pub(super) fn show_load_error(&mut self, tab: u64, url: &str, kind: LoadError, detail: &str) {
        let Some(i) = self.tabs.iter().position(|t| t.id == tab) else { return };
        self.tabs[i].loading = false;
        self.error_tabs.insert(tab);
        // what this tab's error page may ask of Sanaga (its buttons): only
        // for this failure, while it is shown
        self.error_pages.insert(tab, (kind, host_of(url)));
        let html = self.error_html(url, kind, detail);
        if let Some(w) = &self.tabs[i].web {
            w.webview().load_alternate_html(&html, url, None);
        }
    }

    fn error_html(&self, url: &str, kind: LoadError, detail: &str) -> String {
        let th = self.theme();
        let dark = self.store.settings.dark;
        let host = {
            let h = host_of(url);
            if h.is_empty() { url.to_string() } else { h }
        };
        let (title, text, tips): (&str, String, Vec<&str>) = match kind {
            LoadError::NotFound => (
                tr("Ce site est introuvable"),
                tf("L'adresse de {} ne mène nulle part : le nom du site n'existe pas, ou il ne répond plus.", &[&format!("<b>{}</b>", esc(&host))]),
                vec![tr("Vérifiez l'orthographe de l'adresse."), tr("Cherchez le site par son nom."), tr("Si vous utilisez un VPN ou un pare-feu, vérifiez qu'il laisse passer ce site.")],
            ),
            LoadError::Offline => (
                tr("Vous n'êtes pas connecté à Internet"),
                tr("Sanaga n'arrive pas à joindre le réseau. La page se rechargera toute seule dès que la connexion revient.").to_string(),
                vec![tr("Vérifiez le Wi-Fi ou le câble réseau."), tr("Désactivez le mode avion s'il est activé."), tr("Redémarrez votre box ou votre routeur.")],
            ),
            LoadError::Refused => (
                tr("Le site refuse la connexion"),
                tf("{} est joignable, mais n'accepte pas la connexion en ce moment.", &[&format!("<b>{}</b>", esc(&host))]),
                vec![tr("Le site est peut-être en maintenance : réessayez dans un moment."), tr("Vérifiez l'adresse, et le port s'il y en a un.")],
            ),
            LoadError::Timeout => (
                tr("Le site met trop de temps à répondre"),
                tf("{} n'a pas répondu à temps. Il est peut-être surchargé, ou votre connexion est lente.", &[&format!("<b>{}</b>", esc(&host))]),
                vec![tr("Réessayez dans un instant."), tr("Vérifiez votre connexion.")],
            ),
            LoadError::Certificate => (
                tr("La connexion à ce site n'est pas privée"),
                tf("Le certificat de {} n'est pas valide. Quelqu'un pourrait lire ou modifier ce que vous échangez avec ce site (mots de passe, messages, cartes bancaires).", &[&format!("<b>{}</b>", esc(&host))]),
                vec![tr("Revenez en lieu sûr, sauf si vous savez exactement pourquoi ce site a un certificat non valide (un appareil de votre réseau, par exemple).")],
            ),
            LoadError::BlockedPort => (
                tr("Cette adresse utilise un port bloqué"),
                tr("Pour votre sécurité, les navigateurs n'ouvrent pas ce port : il sert à d'autres services (courrier, transfert de fichiers…).").to_string(),
                vec![tr("Vérifiez le numéro de port dans l'adresse.")],
            ),
            LoadError::NoFile => (tr("Ce fichier est introuvable"), tf("Le fichier {} n'existe pas (ou plus) sur cet ordinateur.", &[&format!("<b>{}</b>", esc(url.trim_start_matches("file://")))]), vec![]),
            LoadError::Other => (tr("Cette page n'a pas pu être chargée"), tf("Le chargement de {} a échoué.", &[&format!("<b>{}</b>", esc(&host))]), vec![tr("Réessayez dans un instant."), tr("Vérifiez votre connexion.")]),
        };
        let search = self.store.settings.engine.search_url(&host);
        let archive = format!("https://web.archive.org/web/{url}");
        let web = url.starts_with("http://") || url.starts_with("https://");
        let mut buttons = String::new();
        if kind == LoadError::Certificate {
            // (Sanaga's own buttons: handled by its isolated script, checked in Rust)
            buttons.push_str(&format!("<button class=\"primary\" data-sanaga-action=\"safe\">{}</button>", tr("Revenir en lieu sûr")));
        } else {
            buttons.push_str(&format!("<button class=\"primary\" onclick=\"location.reload()\">{}</button>", tr("Réessayer")));
            if web && kind != LoadError::Offline {
                buttons.push_str(&format!("<a class=\"button\" href=\"{}\">{}</a>", esc(&search), tf("Rechercher « {} »", &[&esc(&host)])));
                buttons.push_str(&format!("<a class=\"button\" href=\"{}\">{}</a>", esc(&archive), tr("Version archivée")));
            }
        }
        let tips_html: String = tips.iter().map(|t| format!("<li>{t}</li>")).collect();
        let advanced = if kind == LoadError::Certificate {
            format!(
                "<details><summary>{}</summary><p>{}</p><button class=\"danger\" data-sanaga-action=\"tls-allow\" data-sanaga-arg=\"{}\">{}</button><p class=\"code\">{}</p></details>",
                tr("Avancé"),
                tr("Continuer expose ce que vous envoyez à ce site. Sanaga ne s'en souviendra que jusqu'à sa fermeture."),
                esc(&host),
                tf("Continuer vers {} (dangereux)", &[&esc(&host)]),
                esc(detail)
            )
        } else {
            format!("<details><summary>{}</summary><p class=\"code\">{}<br>{}</p></details>", tr("Détails techniques"), esc(url), esc(detail))
        };
        let (bg, card, fg, sub, border) = (hex(th.page), hex(th.card), hex(th.text), hex(th.sub), hex(th.border));
        let accent = hex(th.accent);
        let danger = if dark { "#ff6b6b" } else { "#c0302a" };
        let auto_retry = if kind == LoadError::Offline { "window.addEventListener('online',function(){location.reload()});setInterval(function(){if(navigator.onLine)location.reload()},5000);" } else { "" };
        format!(
            r#"<!doctype html><html lang="{lang}" data-sanaga-error><head><meta charset="utf-8"><title>{title_e}</title><meta name="color-scheme" content="{scheme}"><style>
*{{box-sizing:border-box}}html,body{{margin:0;height:100%}}
body{{background:{bg};color:{fg};font:15px/1.55 Inter,"Noto Sans",system-ui,sans-serif;display:flex;align-items:center;justify-content:center;padding:32px}}
main{{max-width:560px;width:100%;animation:in .35s ease-out}}@keyframes in{{from{{opacity:0;transform:translateY(8px)}}to{{opacity:1;transform:none}}}}
.icon{{width:76px;height:76px;border-radius:22px;background:{card};display:flex;align-items:center;justify-content:center;margin-bottom:22px;box-shadow:0 1px 0 {border}}}
.icon svg{{width:44px;height:44px;fill:none;stroke:{accent};stroke-width:2.6;stroke-linecap:round;stroke-linejoin:round}}.icon .f{{fill:{accent};stroke:none}}.icon .w{{stroke:{bg};stroke-width:1.6}}
h1{{font-size:26px;line-height:1.25;margin:0 0 10px;font-weight:650;letter-spacing:-.01em}}
p.lead{{color:{sub};margin:0 0 18px;font-size:15.5px}}p.lead b{{color:{fg};font-weight:600}}
ul{{margin:0 0 24px;padding:14px 18px 14px 34px;background:{card};border-radius:14px;color:{fg}}}li{{margin:3px 0;font-size:14px}}
.actions{{display:flex;flex-wrap:wrap;gap:10px;margin-bottom:22px}}
button,.button{{font:inherit;font-size:14px;border:0;border-radius:999px;padding:9px 18px;cursor:pointer;text-decoration:none;background:{card};color:{fg};box-shadow:inset 0 0 0 1px {border}}}
button:hover,.button:hover{{filter:brightness(.97)}}button.primary{{background:{accent};color:#fff;box-shadow:none;font-weight:600}}
button.danger{{background:transparent;color:{danger};box-shadow:inset 0 0 0 1px {danger};margin:6px 0 4px}}
details{{color:{sub};font-size:13px}}summary{{cursor:pointer;user-select:none}}p.code{{font-family:ui-monospace,monospace;font-size:12px;word-break:break-all;opacity:.85}}
.retry{{display:inline-block;width:8px;height:8px;border-radius:50%;background:{accent};margin-right:8px;animation:pulse 1.4s infinite}}@keyframes pulse{{50%{{opacity:.3}}}}
</style></head><body><main>
<div class="icon"><svg viewBox="0 0 60 60">{icon}</svg></div>
<h1>{title_e}</h1><p class="lead">{offline_dot}{text}</p>
{tips_block}
<div class="actions">{buttons}</div>
{advanced}
</main><script>{auto_retry}</script></body></html>"#,
            lang = crate::i18n::language(),
            title_e = esc(title),
            scheme = if dark { "dark" } else { "light" },
            icon = icon(kind),
            offline_dot = if kind == LoadError::Offline { "<span class=\"retry\"></span>" } else { "" },
            tips_block = if tips_html.is_empty() { String::new() } else { format!("<ul>{tips_html}</ul>") },
        )
    }

    /// The person chose to go on despite the certificate (error page, Advanced).
    pub(super) fn accept_certificate(&mut self, tab: u64, host: &str) {
        if let Some(w) = self.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
            crate::page::accept_certificate(&w.webview(), host);
        }
    }
}

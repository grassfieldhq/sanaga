//! Page features found in Safari: share, print, translate, page source, web
//! inspector, full screen, web apps ("Add to Dock"), per-site zoom and the
//! privacy report (cross-site tracking prevention).

use super::*;
use crate::icons::{self, Icon};
use eframe::egui::{Align2, Color32, Margin, Sense};
use webkit2gtk::{WebViewExt, WebsiteDataManagerExt};

impl Sanaga {
    fn current_page(&self) -> Option<(String, String)> {
        let t = self.tabs.get(self.active)?;
        if t.entries.is_empty() || t.url().starts_with("sanaga://") {
            return None;
        }
        Some((t.url().to_string(), t.title()))
    }

    pub(super) fn copy_link(&self) {
        if let Some((u, _)) = self.current_page() {
            self.ctx.copy_text(u);
        }
    }

    pub(super) fn email_link(&self) {
        if let Some((u, t)) = self.current_page() {
            let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().replace('+', "%20");
            let mail = format!("mailto:?subject={}&body={}", enc(&t), enc(&u));
            let _ = std::process::Command::new("xdg-open").arg(mail).spawn();
        }
    }

    pub(super) fn print_page(&self) {
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            let _ = w.print();
        }
    }

    pub(super) fn open_inspector(&mut self) {
        if let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) {
            // docked under the page, inside Sanaga's window
            use webkit2gtk::{WebInspectorExt, WebViewExt};
            if let Some(ins) = w.webview().inspector() {
                ins.show();
                ins.attach();
            } else {
                w.open_devtools();
            }
        }
    }

    pub(super) fn view_source(&mut self) {
        if let Some((u, _)) = self.current_page() {
            self.open_tab(&format!("view-source:{u}"), self.tabs[self.active].private);
        }
    }

    /// Translate the page into French (through a translation website).
    pub(super) fn translate_page(&mut self) {
        if let Some((u, _)) = self.current_page() {
            let enc: String = url::form_urlencoded::byte_serialize(u.as_bytes()).collect();
            let a = self.active;
            self.navigate(a, format!("https://translate.google.com/translate?sl=auto&tl={}&u={enc}", crate::i18n::language()), None, true);
        }
    }

    pub(super) fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        crate::desktop::toggle_fullscreen(&self.ctx, self.fullscreen);
    }

    /// "Add to Dock": an application-menu entry that opens this site on its own.
    pub(super) fn make_web_app(&mut self) -> Option<String> {
        let (u, title) = self.current_page()?;
        let host = host_of(&u);
        let exe = std::env::current_exe().ok()?;
        let dir = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))?
            .join("applications");
        std::fs::create_dir_all(&dir).ok()?;
        let safe: String = host.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let name = if title.trim().is_empty() { host.clone() } else { title.replace(['\n', '\r'], " ") };
        let exe_q = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nComment={host} — application web Sanaga\nExec=\"{exe_q}\" --app \"{u}\"\nIcon=web-browser\nTerminal=false\nCategories=Network;WebBrowser;\nStartupWMClass=sanaga\n"
        );
        let path = dir.join(format!("sanaga-web-{safe}.desktop"));
        std::fs::write(&path, desktop).ok()?;
        Some(name)
    }

    /// Zoom remembered per site.
    pub(super) fn remember_zoom(&mut self) {
        let t = &self.tabs[self.active];
        let host = host_of(t.url());
        if t.url().starts_with("http") {
            let z = t.zoom;
            self.store.set_site_zoom(&host, z);
        }
    }

    pub(super) fn apply_site_zoom(&mut self, ti: usize) {
        let Some(t) = self.tabs.get_mut(ti) else { return };
        let host = host_of(t.url());
        t.zoom = self.store.site_zoom.get(&host).copied().unwrap_or(self.store.settings.zoom);
    }

    /// Ask WebKit which trackers it kept from following the person (privacy report).
    pub(super) fn refresh_privacy(&mut self, now: f64) {
        if now - self.privacy_asked < 15.0 {
            return;
        }
        self.privacy_asked = now;
        let Some(w) = self.tabs.iter().find_map(|t| t.web.as_ref()) else { return };
        let Some(dm) = w.webview().website_data_manager() else { return };
        let tx = self.web_tx.clone();
        dm.itp_summary(None::<&webkit2gtk::gio::Cancellable>, move |res| {
            if let Ok(list) = res {
                let mut out: Vec<(String, usize)> = list
                    .iter()
                    .filter_map(|tp| tp.domain().map(|d| (d.to_string(), tp.first_parties().len())))
                    .collect();
                out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let _ = tx.send(web::WebEvent::Privacy(out));
            }
        });
    }

    /// Share button popover (copy, mail, print, reading list, favourites, web app).
    pub(super) fn share_popover(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.share_open {
            return;
        }
        let w = 270.0;
        let pos = pos2(self.share_rect.right() - w, self.share_rect.bottom() + 6.0);
        let mut act: Option<&'static str> = None;
        let url = self.tabs[self.active].url().to_string();
        let in_list = self.store.in_reading_list(&url);
        let fav = self.store.is_bookmarked(&url);
        let web = self.tabs[self.active].web.is_some() && matches!(self.tabs[self.active].view, View::Web);
        let qr = if self.share_qr && url.starts_with("http") { self.qr_texture(&url) } else { None };
        let area = egui::Area::new(egui::Id::new("share-pop")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.border))
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(50) })
                .inner_margin(Margin::symmetric(0, 8))
                .show(ui, |ui| {
                    ui.set_width(w);
                    let mut item = |ui: &mut Ui, icon: Icon, label: &str, id: &'static str, enabled: bool| {
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), if enabled { Sense::click() } else { Sense::hover() });
                        let hover = enabled && resp.hovered();
                        if hover {
                            ui.painter().rect_filled(r.shrink2(vec2(6.0, 1.0)), CornerRadius::same(6), th.accent);
                        }
                        let col = if hover { Color32::WHITE } else if enabled { th.text } else { th.sub.gamma_multiply(0.6) };
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(15.0, 15.0)), icon, col);
                        ui.painter().text(pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), col);
                        if enabled && resp.clicked() {
                            act = Some(id);
                        }
                    };
                    item(ui, Icon::Share, tr("Copier le lien"), "copy", true);
                    item(ui, Icon::Mail, tr("Envoyer par e-mail"), "mail", true);
                    item(ui, Icon::Print, tr("Imprimer…"), "print", web);
                    item(ui, Icon::Glasses, if in_list { tr("Retirer de la liste de lecture") } else { tr("Ajouter à la liste de lecture") }, "reading", true);
                    item(ui, if fav { Icon::StarFilled } else { Icon::Star }, if fav { tr("Retirer des favoris") } else { tr("Ajouter aux favoris") }, "fav", true);
                    item(ui, Icon::Plus, tr("Créer une application web"), "webapp", true);
                    item(ui, Icon::Grid, if qr.is_some() { tr("Masquer le code QR") } else { tr("Code QR de la page") }, "qr", url.starts_with("http"));
                    if let Some(tex) = &qr {
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), vec2(180.0, 180.0))));
                            ui.label(egui::RichText::new(tr("Scannez-le avec votre téléphone")).size(11.5).color(th.sub));
                        });
                    }
                });
        });
        let share_rect = self.share_rect;
        if ctx.input(|i| i.pointer.any_pressed()) && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || share_rect.contains(p)).unwrap_or(false) {
            self.share_open = false;
        }
        let Some(id) = act else { return };
        if id == "qr" {
            self.share_qr = !self.share_qr;
            return;
        }
        self.share_open = false;
        match id {
            "copy" => self.copy_link(),
            "mail" => self.email_link(),
            "print" => self.print_page(),
            "reading" => self.toggle_reading_list(),
            "fav" => self.toggle_bookmark(),
            "webapp" => {
                if let Some(name) = self.make_web_app() {
                    self.notice = Some((tf("« {} » a été ajoutée au menu des applications.", &[&name]), ctx.input(|i| i.time)));
                }
            }
            _ => {}
        }
    }

    /// Short message shown at the bottom of the window for a few seconds.
    pub(super) fn show_notice(&mut self, ctx: &egui::Context, content: Rect, th: &Theme) {
        let Some((msg, t0)) = &self.notice else { return };
        let now = ctx.input(|i| i.time);
        if now - t0 > 4.0 {
            self.notice = None;
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("notice")));
        let g = painter.layout_no_wrap(msg.clone(), FontId::proportional(13.0), Color32::WHITE);
        let r = Rect::from_center_size(pos2(content.center().x, content.bottom() - 40.0), vec2(g.size().x + 36.0, 34.0));
        painter.rect_filled(r, CornerRadius::same(17), Color32::from_rgba_unmultiplied(30, 30, 32, 230));
        painter.galley(pos2(r.left() + 18.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let _ = th;
    }
}

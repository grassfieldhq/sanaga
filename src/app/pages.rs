//! Built-in pages: new tab, history, bookmarks, downloads, settings, about, errors.

use super::chrome::site_badge;
use super::*;
use crate::icons::{self, Icon};
use crate::store;
use eframe::egui::{Align, Align2, Color32, CursorIcon, Label, Layout, Margin, RichText, Sense, TextEdit};

pub(super) fn page_title(ui: &mut Ui, th: &Theme, title: &str) {
    ui.add_space(28.0);
    ui.label(RichText::new(title).size(26.0).family(bold()).color(th.text));
    ui.add_space(14.0);
}

pub(super) fn centered_column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let cw = (w - 64.0).min(760.0);
    let x = (w - cw) / 2.0;
    ui.horizontal(|ui| {
        ui.add_space(x);
        ui.vertical(|ui| {
            ui.set_width(cw);
            add(ui);
        });
    });
}

fn list_row(ui: &mut Ui, th: &Theme, url: &str, title: &str, meta: &str, go: &mut Option<(String, bool)>, remove: &mut Option<usize>, i: usize) {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), th.card);
    }
    let c = pos2(r.left() + 20.0, r.center().y);
    site_badge(ui.painter(), c, 9.0, url, title);
    let right = r.right() - 40.0;
    let meta_w = if meta.is_empty() { 0.0 } else { 110.0 };
    let clip = Rect::from_min_max(pos2(c.x + 20.0, r.top()), pos2(right - meta_w, r.bottom()));
    let p = ui.painter().with_clip_rect(clip);
    let t = if title.is_empty() { url } else { title };
    let g = p.text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, t, FontId::proportional(14.0), th.text);
    p.text(pos2(g.right() + 10.0, r.center().y), Align2::LEFT_CENTER, host_of(url), FontId::proportional(12.5), th.sub);
    if !meta.is_empty() {
        ui.painter().text(pos2(right - 4.0, r.center().y), Align2::RIGHT_CENTER, meta, FontId::proportional(12.0), th.sub);
    }
    let xr = Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(24.0, 24.0));
    let xresp = ui.interact(xr, ui.id().with(("rm", i, url)), Sense::click()).tip(tr("Supprimer"));
    if xresp.hovered() {
        ui.painter().circle_filled(xr.center(), 12.0, th.border);
    }
    if resp.hovered() || xresp.hovered() {
        icons::paint(ui.painter(), xr.shrink(6.0), Icon::Close, th.sub);
    }
    if xresp.clicked() {
        *remove = Some(i);
        return;
    }
    let resp = resp.tip(url).on_hover_cursor(CursorIcon::PointingHand);
    if resp.middle_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
        *go = Some((url.to_string(), true));
    } else if resp.clicked() {
        *go = Some((url.to_string(), false));
    }
}

impl Sanaga {
    pub(super) fn page_history(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut clear = false;
        let mut remove: Option<usize> = None;
        let filter = self.history_filter.to_lowercase();
        let store = &self.store;
        let hf = &mut self.history_filter;
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Historique"));
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(hf).hint_text(tr("Rechercher dans l'historique")).desired_width(320.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !store.history.is_empty() && ui.button(tr("Effacer l'historique")).clicked() {
                        clear = true;
                    }
                });
            });
            ui.add_space(12.0);
            if store.history.is_empty() {
                ui.label(RichText::new(tr("Les pages que vous visitez apparaîtront ici.")).color(th.sub));
                return;
            }
            let mut group = "";
            let mut shown = 0;
            for (i, v) in store.history.iter().enumerate().rev() {
                if !filter.is_empty() && !v.url.to_lowercase().contains(&filter) && !v.title.to_lowercase().contains(&filter) {
                    continue;
                }
                shown += 1;
                if shown > 400 {
                    ui.label(RichText::new(tr("… affinez la recherche pour voir plus de résultats")).color(th.sub));
                    break;
                }
                let g = store::day_group(v.time);
                if g != group {
                    group = g;
                    ui.add_space(10.0);
                    ui.label(RichText::new(g).size(13.0).family(bold()).color(th.accent));
                    ui.add_space(4.0);
                }
                list_row(ui, th, &v.url, &v.title, &store::relative_time(v.time), &mut go, &mut remove, i);
            }
        });
        if clear {
            self.store.clear_history();
            self.visited.clear();
        }
        if let Some(i) = remove {
            if i < self.store.history.len() {
                self.store.history.remove(i);
                self.store.rewrite_history();
            }
        }
        self.apply_go(go);
    }

    /// The system's file picker (GTK), when WebKit/GTK is running.
    pub(super) fn choose_file(&mut self, title: &str, patterns: &[&str], filter_name: &str) -> Option<std::path::PathBuf> {
        if !self.web_ok {
            self.notice = Some((tr("Le sélecteur de fichiers n'est pas disponible ici.").into(), self.ctx.input(|i| i.time)));
            return None;
        }
        crate::shell::choose_file(title, patterns, filter_name, None)
    }

    pub(super) fn page_downloads(&mut self, ui: &mut Ui, th: &Theme) {
        let store = &self.store;
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Téléchargements"));
            if store.downloads.is_empty() {
                ui.label(RichText::new(tr("Les fichiers que vous téléchargez apparaîtront ici. Ils sont enregistrés dans votre dossier Téléchargements.")).color(th.sub));
            }
            for d in store.downloads.iter().rev() {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Download, th.accent);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&d.name).size(14.0).color(th.text));
                            let s = if d.ok {
                                format!("{} · {} · {}", net::human_bytes(d.bytes), store::relative_time(d.time), d.path.display())
                            } else {
                                tf("Échec : {}", &[&d.error])
                            };
                            ui.label(RichText::new(s).size(12.0).color(th.sub));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if d.ok && ui.button(tr("Afficher dans le dossier")).clicked() {
                                if let Some(dir) = d.path.parent() {
                                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                                }
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });
    }

    /// `sanaga://activity`: what Sanaga uses right now, process by process,
    /// and the tabs with their state — any running tab can be put to sleep.
    pub(super) fn page_activity(&mut self, ui: &mut Ui, th: &Theme) {
        let list = net::processes_cached();
        // (measured every 2 s: the page follows)
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
        let total: u64 = list.iter().map(|p| p.bytes).sum();
        let cpu: f32 = list.iter().map(|p| p.cpu).sum();
        let running = self.tabs.iter().filter(|t| t.web.is_some()).count();
        let mut sleep: Option<usize> = None;
        let mut show: Option<usize> = None;
        let active = self.active;
        centered_column(ui, |ui| {
            ui.add_space(26.0);
            ui.label(RichText::new(tr("Activité")).size(26.0).family(bold()).color(th.text));
            ui.label(RichText::new(tf("{} de mémoire · {} % d'un cœur · {} page(s) en cours sur {} onglet(s)", &[&net::human_bytes(total), &format!("{cpu:.0}"), &running, &self.tabs.len()])).color(th.sub));
            ui.add_space(16.0);
            let card = |ui: &mut Ui, body: &mut dyn FnMut(&mut Ui)| {
                Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(ui);
                });
            };
            ui.label(RichText::new(tr("Processus")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            card(ui, &mut |ui| {
                let mut page = 0;
                for p in &list {
                    let name = match p.kind {
                        0 => tr("Interface de Sanaga").to_string(),
                        1 => tr("Réseau").to_string(),
                        2 => {
                            page += 1;
                            tf("Page {}", &[&page])
                        }
                        _ => continue,
                    };
                    if p.kind == 2 && p.bytes == 0 {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0} %", p.cpu)).color(if p.cpu >= 20.0 { th.accent } else { th.sub }));
                            ui.add_space(18.0);
                            ui.label(RichText::new(net::human_bytes(p.bytes)).color(th.text));
                        });
                    });
                }
                // the small helpers of the engine (sandbox, display), together
                let (other_bytes, other_cpu) = list.iter().filter(|p| p.kind == 3).fold((0u64, 0f32), |a, p| (a.0 + p.bytes, a.1 + p.cpu));
                if other_bytes > 1 << 20 || other_cpu >= 1.0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Autres (bac à sable, auxiliaires)")).color(th.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{other_cpu:.0} %")).color(th.sub));
                            ui.add_space(18.0);
                            ui.label(RichText::new(net::human_bytes(other_bytes)).color(th.text));
                        });
                    });
                }
                if list.is_empty() {
                    ui.label(RichText::new(tr("Mesure en cours…")).color(th.sub));
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Chaque page a son propre processus ; le moteur ne dit pas à quel onglet chacun appartient.")).size(12.0).color(th.sub));
            ui.add_space(18.0);
            ui.label(RichText::new(tr("Onglets")).size(15.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            card(ui, &mut |ui| {
                for (i, t) in self.tabs.iter().enumerate() {
                    let title = t.title();
                    let state = if i == active {
                        tr("affiché")
                    } else if t.web.is_some() && t.audio {
                        tr("en cours · son")
                    } else if t.web.is_some() {
                        tr("en cours")
                    } else {
                        tr("en veille")
                    };
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                        site_badge(ui.painter(), r.center(), 8.0, t.url(), &title);
                        let short: String = title.chars().take(56).collect();
                        if ui.add(Label::new(RichText::new(short).color(th.text)).sense(Sense::click()).truncate()).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            show = Some(i);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // (the tab shown, pinned tabs and unsent forms never sleep)
                            if i != active && t.web.is_some() && !t.pinned && !t.form_dirty && ui.button(tr("Mettre en veille")).clicked() {
                                sleep = Some(i);
                            }
                            ui.label(RichText::new(state).color(th.sub));
                        });
                    });
                }
            });
            ui.add_space(30.0);
        });
        if let Some(i) = sleep {
            super::plus::put_to_sleep(&mut self.tabs[i]);
        }
        if let Some(i) = show {
            self.current_group = self.tabs[i].group;
            self.active = i;
        }
    }

    pub(super) fn page_about(&mut self, ui: &mut Ui, th: &Theme) {
        let cached: usize = self.tabs.iter().flat_map(|t| t.entries.iter()).filter_map(|e| e.doc.as_ref()).map(|d| d.bytes).sum();
        let rows = [
            (tr("Mémoire utilisée"), net::human_bytes(self.mem.1)),
            (tr("Onglets ouverts"), self.tabs.len().to_string()),
            (tr("Pages gardées en mémoire"), net::human_bytes(cached as u64)),
            (tr("Images en mémoire"), net::human_bytes(self.img_bytes as u64)),
            (tr("Pages chargées depuis le démarrage"), self.pages_loaded.to_string()),
            (tr("Données téléchargées"), net::human_bytes(self.bytes_loaded)),
        ];
        let mut open_welcome = false;
        centered_column(ui, |ui| {
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                icons::logo(ui.painter(), r.center(), 34.0);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Sanaga").size(30.0).family(bold()).color(th.text));
                    ui.label(RichText::new(tf("Version {} · navigateur léger", &[&env!("CARGO_PKG_VERSION")])).color(th.sub));
                });
            });
            ui.add_space(20.0);
            Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (k, v) in rows {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(th.sub));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(v).color(th.text)));
                    });
                }
            });
            ui.add_space(16.0);
            ui.add(Label::new(RichText::new(tr("Sanaga affiche les sites avec WebKit, un moteur web mature (celui de Safari et de GNOME Web). Pour économiser davantage, le moteur Sanaga Léger (dans les Paramètres) affiche l'essentiel des pages sans JavaScript, avec très peu de mémoire. La mémoire indiquée comprend les processus du moteur.")).color(th.text)).wrap());
            ui.add_space(10.0);
            if ui.link(tr("Revoir la page de bienvenue")).clicked() {
                open_welcome = true;
            }
        });
        if open_welcome {
            self.navigate(self.active, "sanaga://welcome".into(), None, true);
        }
    }

    pub(super) fn page_error(&mut self, ui: &mut Ui, th: &Theme, msg: &str) {
        let url = self.tabs[self.active].url().to_string();
        let mut retry = false;
        let mut archive = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.18);
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 30.0, th.card);
            icons::paint(ui.painter(), r.shrink(14.0), Icon::Info, th.sub);
            ui.add_space(14.0);
            let crashed = msg.starts_with(super::CRASH_MARK);
            let msg = msg.trim_start_matches(super::CRASH_MARK);
            let title = if crashed {
                tr("Cette page a cessé de fonctionner")
            } else if msg.is_empty() {
                tr("Page non chargée")
            } else {
                tr("Impossible d'accéder à ce site")
            };
            ui.label(RichText::new(title).size(24.0).family(bold()).color(th.text));
            ui.add_space(6.0);
            ui.label(RichText::new(host_of(&url)).size(15.0).color(th.sub));
            ui.add_space(6.0);
            let m = if msg.is_empty() { tr("Cliquez sur Actualiser pour ouvrir cette page.") } else { msg };
            ui.set_max_width(560.0);
            ui.add(Label::new(RichText::new(m).size(14.0).color(th.sub)).wrap());
            ui.add_space(18.0);
            let b = egui::Button::new(RichText::new(tr("Actualiser")).color(Color32::WHITE).size(14.0)).fill(th.accent).corner_radius(CornerRadius::same(16)).min_size(vec2(120.0, 34.0));
            if ui.add(b).clicked() {
                retry = true;
            }
            // gone or not loading: the copy kept by the Internet Archive
            if !crashed && (url.starts_with("http://") || url.starts_with("https://")) {
                ui.add_space(8.0);
                if ui.add(egui::Button::new(RichText::new(tr("Version archivée")).size(13.5)).corner_radius(CornerRadius::same(16)).min_size(vec2(120.0, 30.0))).clicked() {
                    archive = true;
                }
            }
        });
        if retry {
            self.reload(self.active);
        }
        if archive {
            self.open_archive();
        }
    }
}

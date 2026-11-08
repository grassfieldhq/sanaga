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

fn card(ui: &mut Ui, th: &Theme, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title).size(15.0).family(bold()).color(th.text));
    ui.add_space(6.0);
    Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(18.0);
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

    pub(super) fn page_bookmarks(&mut self, ui: &mut Ui, th: &Theme) {
        let mut go: Option<(String, bool)> = None;
        let mut remove: Option<usize> = None;
        let mut import: Option<std::path::PathBuf> = None;
        let mut pick_file = false;
        let sources = crate::import::chrome_family_sources();
        let mut open_import = false;
        let store = &self.store;
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Favoris"));
            // import from another browser
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Importer :")).size(13.0).color(th.sub));
                for (name, path) in &sources {
                    if ui.button(tf("depuis {}", &[name])).clicked() {
                        import = Some(path.clone());
                    }
                }
                if ui.button(tr("un fichier HTML (Firefox, Safari…)")).clicked() {
                    pick_file = true;
                }
                if ui.link(tr("Importer depuis un autre navigateur (Chrome, Firefox…)")).clicked() {
                    open_import = true;
                }
            });
            ui.add_space(10.0);
            if store.bookmarks.is_empty() {
                ui.label(RichText::new(tr("Cliquez sur l'étoile de la barre d'adresse pour ajouter une page.")).color(th.sub));
            }
            for (i, b) in store.bookmarks.iter().enumerate() {
                list_row(ui, th, &b.url, &b.title, "", &mut go, &mut remove, i);
            }
        });
        if let Some(i) = remove {
            self.store.bookmarks.remove(i);
            self.store.save_bookmarks();
        }
        if open_import {
            self.navigate(self.active, "sanaga://importer".into(), None, true);
        }
        if pick_file {
            import = self.choose_file(tr("Importer des favoris (fichier HTML)"), &["*.html", "*.htm"], tr("Favoris (HTML)"));
        }
        if let Some(path) = import {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let found = if text.trim_start().starts_with('{') { crate::import::parse_chrome(&text) } else { crate::import::parse_html(&text) };
            let mut added = 0;
            for (url, title) in found {
                if !self.store.is_bookmarked(&url) {
                    let title = if title.trim().is_empty() { url.clone() } else { title };
                    self.store.bookmarks.push(crate::store::Bookmark { url, title });
                    added += 1;
                }
            }
            self.store.save_bookmarks();
            self.notice = Some((tf("Favoris importés : {}", &[&added]), ui.input(|i| i.time)));
        }
        self.apply_go(go);
    }

    /// The system's file picker (GTK), when WebKit/GTK is running.
    pub(super) fn choose_file(&mut self, title: &str, patterns: &[&str], filter_name: &str) -> Option<std::path::PathBuf> {
        use gtk::prelude::*;
        if !self.web_ok {
            self.notice = Some((tr("Le sélecteur de fichiers n'est pas disponible ici.").into(), self.ctx.input(|i| i.time)));
            return None;
        }
        let dialog = gtk::FileChooserNative::new(Some(title), None::<&gtk::Window>, gtk::FileChooserAction::Open, Some(tr("Importer")), Some(tr("Annuler")));
        let filter = gtk::FileFilter::new();
        for p in patterns {
            filter.add_pattern(p);
        }
        filter.set_name(Some(filter_name));
        dialog.add_filter(filter);
        let res = dialog.run();
        let path = if res == gtk::ResponseType::Accept { dialog.filename() } else { None };
        dialog.destroy();
        path
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

    pub(super) fn page_settings(&mut self, ui: &mut Ui, th: &Theme) {
        let mut changed = false;
        let mut clear_history = false;
        let web_ok = self.web_ok;
        let sync = self.sync_status.clone();
        let profile = if self.store.profile.is_empty() { tr("Personnel").to_string() } else { self.store.profile.clone() };
        let mut go_page: Option<&'static str> = None;
        let mut sync_now = false;
        let st = &mut self.store.settings;
        centered_column(ui, |ui| {
            page_title(ui, th, tr("Paramètres"));
            card(ui, th, "Langue · Language · Idioma · Sprache", |ui| {
                ui.horizontal(|ui| {
                    let current = crate::i18n::LANGUAGES.iter().find(|(c, _)| *c == st.language).map(|l| l.1).unwrap_or("English");
                    egui::ComboBox::from_id_salt("language").selected_text(current).width(220.0).show_ui(ui, |ui| {
                        for (code, name) in crate::i18n::LANGUAGES {
                            if ui.selectable_label(st.language == code, name).clicked() && st.language != code {
                                st.language = code.to_string();
                                changed = true;
                            }
                        }
                    });
                });
                ui.label(RichText::new(tr("Toute l'interface de Sanaga change aussitôt ; les sites sont aussi demandés dans cette langue (pour les nouveaux onglets).")).size(12.0).color(th.sub));
            });
            card(ui, th, tr("Moteur d'affichage des pages"), |ui| {
                changed |= ui.radio_value(&mut st.webkit, true, tr("WebKit — sites complets avec JavaScript, vidéos et applications (recommandé)")).changed();
                changed |= ui.radio_value(&mut st.webkit, false, tr("Sanaga Léger — pages simplifiées, très rapide et très économe en mémoire")).changed();
                let note = if web_ok {
                    tr("Le changement s'applique aux prochaines pages ouvertes.")
                } else {
                    tr("WebKit n'est pas disponible ici : Sanaga utilise le moteur léger.")
                };
                ui.label(RichText::new(note).size(12.0).color(th.sub));
            });
            card(ui, th, tr("Moteur de recherche"), |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Moteur utilisé dans la barre d'adresse"));
                    egui::ComboBox::from_id_salt("engine").selected_text(st.engine.label()).width(240.0).show_ui(ui, |ui| {
                        for e in Engine::ALL {
                            changed |= ui.selectable_value(&mut st.engine, e, e.label()).changed();
                        }
                    });
                });
                ui.label(RichText::new(tr("La version légère de DuckDuckGo fonctionne le mieux avec Sanaga.")).size(12.0).color(th.sub));
            });
            card(ui, th, tr("Démarrage"), |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Page d'accueil"));
                    changed |= ui.add(TextEdit::singleline(&mut st.home).desired_width(360.0)).changed();
                });
                ui.add_space(4.0);
                changed |= ui.checkbox(&mut st.restore_tabs, tr("Rouvrir les onglets de la dernière session")).changed();
            });
            card(ui, th, tr("Apparence"), |ui| {
                changed |= ui.checkbox(&mut st.dark, tr("Thème sombre")).changed();
                changed |= ui.checkbox(&mut st.show_bookmarks_bar, tr("Afficher la barre de favoris")).changed();
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Boutons de la barre d'outils (aussi : clic droit sur la barre d'outils)")).strong());
                changed |= super::toolbar::toolbar_checkboxes(ui, &mut st.toolbar);
                ui.horizontal(|ui| {
                    ui.label(tr("Zoom par défaut"));
                    let s = egui::Slider::new(&mut st.zoom, 0.5..=2.0).step_by(0.1).custom_formatter(|v, _| format!("{:.0} %", v * 100.0));
                    changed |= ui.add(s).changed();
                });
            });
            card(ui, th, tr("Économie de données et de mémoire"), |ui| {
                changed |= ui.checkbox(&mut st.images, tr("Charger les images")).changed();
                ui.label(RichText::new(tr("Désactivées, les images s'affichent comme un bouton : cliquez pour charger seulement celles qui vous intéressent.")).size(12.0).color(th.sub));
            });
            card(ui, th, tr("Économiseur de mémoire"), |ui| {
                ui.label(RichText::new(tr("Un onglet ne charge sa page que quand vous l'ouvrez. Ensuite, les onglets que vous n'avez pas regardés depuis un moment sont mis en veille : leur page est fermée pour libérer la mémoire et se recharge quand vous y revenez. Les onglets qui jouent du son ne sont jamais mis en veille.")).size(12.5).color(th.sub));
                ui.add_space(6.0);
                for (v, label) in [(0u8, tr("Désactivé")), (1, tr("Modéré — veille après 1 heure, 12 pages actives au plus")), (2, tr("Équilibré — veille après 20 minutes, 6 pages actives au plus (recommandé)")), (3, tr("Maximal — veille après 5 minutes, 3 pages actives au plus"))] {
                    changed |= ui.radio_value(&mut st.memory_saver, v, label).changed();
                }
            });
            card(ui, th, tr("Mots de passe et remplissage automatique"), |ui| {
                changed |= ui.checkbox(&mut st.passwords, tr("Proposer d'enregistrer et de remplir les mots de passe")).changed();
                if ui.link(tr("Gérer les mots de passe enregistrés")).clicked() {
                    go_page = Some("sanaga://motsdepasse");
                }
                if ui.link(tr("Importer depuis un autre navigateur (Chrome, Firefox…)")).clicked() {
                    go_page = Some("sanaga://importer");
                }
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Vos coordonnées, pour remplir les formulaires (menu ⋮ › Remplir avec mes coordonnées, ou Ctrl+Maj+F) :")).size(12.5).color(th.sub));
                egui::Grid::new("contact-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (label, value) in [(tr("Nom"), &mut st.fill_name), (tr("E-mail"), &mut st.fill_email), (tr("Téléphone"), &mut st.fill_phone), (tr("Adresse"), &mut st.fill_address)] {
                        ui.label(label);
                        changed |= ui.add(TextEdit::singleline(value).desired_width(360.0)).changed();
                        ui.end_row();
                    }
                });
            });
            card(ui, th, tr("Synchronisation"), |ui| {
                ui.label(RichText::new(tr("Choisissez un dossier partagé entre vos ordinateurs (Syncthing, Nextcloud, Dropbox, clé USB…). Sanaga y garde vos favoris, votre liste de lecture et vos onglets ouverts, et montre sur la page d'accueil les onglets de vos autres appareils.")).size(12.5).color(th.sub));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(tr("Dossier"));
                    changed |= ui.add(TextEdit::singleline(&mut st.sync_dir).hint_text("/home/vous/Nextcloud").desired_width(360.0)).changed();
                    if ui.button(tr("Synchroniser maintenant")).clicked() {
                        sync_now = true;
                    }
                });
                let state = if !sync.error.is_empty() {
                    sync.error.clone()
                } else if let Some(t) = sync.last {
                    tf("Dernière synchronisation {} · autres appareils : {} · cet ordinateur : {}", &[&store::relative_time(t), &sync.devices.len(), &crate::sync::device_name()])
                } else {
                    tr("Désactivée").to_string()
                };
                ui.label(RichText::new(state).size(12.0).color(th.sub));
            });
            card(ui, th, tr("Profils"), |ui| {
                ui.horizontal(|ui| {
                    ui.label(tf("Profil de cette fenêtre : {}", &[&profile]));
                    if ui.link(tr("Gérer les profils")).clicked() {
                        go_page = Some("sanaga://profils");
                    }
                });
            });
            card(ui, th, tr("Extensions"), |ui| {
                if ui.link(tr("Gérer les extensions (bloqueur de publicités, mode sombre…)")).clicked() {
                    go_page = Some("sanaga://extensions");
                }
            });
            card(ui, th, tr("Confidentialité"), |ui| {
                changed |= ui.checkbox(&mut st.itp, tr("Empêcher le suivi intersites")).changed();
                ui.label(RichText::new(tr("Les traceurs publicitaires ne peuvent plus vous suivre d'un site à l'autre (comme dans Safari). Le Rapport de confidentialité de la page d'accueil montre ceux qui ont été tenus à l'écart. S'applique aux nouveaux onglets.")).size(12.0).color(th.sub));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Avec le moteur Sanaga Léger, aucun script n'est exécuté : pas de publicités, pas de traceurs.")).size(12.0).color(th.sub));
                ui.add_space(6.0);
                if ui.button(tr("Effacer l'historique de navigation")).clicked() {
                    clear_history = true;
                }
            });
        });
        if changed {
            crate::i18n::set_language(&self.store.settings.language);
            self.store.save_settings();
        }
        if let Some(p) = go_page {
            self.navigate(self.active, p.to_string(), None, true);
        }
        if sync_now || (changed && !self.store.settings.sync_dir.trim().is_empty()) {
            self.sync_now();
        }
        if clear_history {
            self.store.clear_history();
            self.visited.clear();
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
            self.navigate(self.active, "sanaga://bienvenue".into(), None, true);
        }
    }

    pub(super) fn page_error(&mut self, ui: &mut Ui, th: &Theme, msg: &str) {
        let url = self.tabs[self.active].url().to_string();
        let mut retry = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.18);
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 30.0, th.card);
            icons::paint(ui.painter(), r.shrink(14.0), Icon::Info, th.sub);
            ui.add_space(14.0);
            let title = if msg.is_empty() { tr("Page non chargée") } else { tr("Impossible d'accéder à ce site") };
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
        });
        if retry {
            self.reload(self.active);
        }
    }
}

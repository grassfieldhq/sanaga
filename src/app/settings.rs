//! The Settings page: a list of categories on the left (like Chrome and
//! Firefox), the settings of the chosen category on the right, and a search
//! field that shows every setting matching what is typed, whatever its category.

use super::*;
use crate::icons::{self, Icon};
use crate::store;
use eframe::egui::{Align, Align2, Color32, CursorIcon, Layout, Margin, RichText, Sense, TextEdit};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Section {
    General,
    Appearance,
    Privacy,
    Passwords,
    Performance,
    Sync,
    Extensions,
}

impl Section {
    const ALL: [Section; 7] = [Section::General, Section::Appearance, Section::Privacy, Section::Passwords, Section::Performance, Section::Sync, Section::Extensions];

    fn label(self) -> &'static str {
        match self {
            Section::General => tr("Général"),
            Section::Appearance => tr("Apparence"),
            Section::Privacy => tr("Confidentialité et sécurité"),
            Section::Passwords => tr("Mots de passe et remplissage automatique"),
            Section::Performance => tr("Performances"),
            Section::Sync => tr("Synchronisation et profils"),
            Section::Extensions => tr("Extensions"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Home,
            Section::Appearance => Icon::Palette,
            Section::Privacy => Icon::Shield,
            Section::Passwords => Icon::Lock,
            Section::Performance => Icon::Leaf,
            Section::Sync => Icon::Rotate,
            Section::Extensions => Icon::Grid,
        }
    }
}

/// Is a group of settings shown? Its category is the chosen one, or, while
/// searching, its title or one of its labels contains the search.
struct Shown<'a> {
    section: Section,
    query: &'a str,
}

impl Shown<'_> {
    fn shows(&self, sec: Section, title: &str, labels: &[&str]) -> bool {
        if self.query.is_empty() {
            return sec == self.section;
        }
        let q = self.query;
        title.to_lowercase().contains(q) || sec.label().to_lowercase().contains(q) || labels.iter().any(|l| l.to_lowercase().contains(q))
    }
}

/// A titled group of settings on a rounded card.
fn group(ui: &mut Ui, th: &Theme, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title).size(14.5).family(bold()).color(th.text));
    ui.add_space(6.0);
    Frame::new().fill(th.card).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 7.0;
        add(ui);
    });
    ui.add_space(20.0);
}

fn note(ui: &mut Ui, th: &Theme, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(th.sub));
}

/// A row that opens another page (with an arrow on the right, like Chrome).
fn link_row(ui: &mut Ui, th: &Theme, label: &str, sub: &str) -> bool {
    let w = ui.available_width();
    let h = if sub.is_empty() { 30.0 } else { 42.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r.expand2(vec2(8.0, 0.0)), CornerRadius::same(8), th.omnibox_hover);
    }
    if sub.is_empty() {
        ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), th.text);
    } else {
        ui.painter().text(pos2(r.left(), r.top() + 12.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), th.text);
        ui.painter().text(pos2(r.left(), r.bottom() - 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), th.sub);
    }
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.right() - 8.0, r.center().y), vec2(12.0, 12.0)), Icon::Forward, th.sub);
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Is Sanaga the system's default web browser? (`xdg-settings`)
fn default_browser_is_sanaga() -> bool {
    std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "sanaga.desktop")
        .unwrap_or(false)
}

fn launcher_installed() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share")));
    let mut dirs: Vec<std::path::PathBuf> = data.into_iter().collect();
    dirs.extend(std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into()).split(':').map(std::path::PathBuf::from));
    dirs.iter().any(|d| d.join("applications/sanaga.desktop").is_file())
}

impl Sanaga {
    pub(super) fn page_settings(&mut self, ui: &mut Ui, th: &Theme) {
        let mut changed = false;
        let mut clear_history = false;
        let mut make_default = false;
        let web_ok = self.web_ok;
        let sync = self.sync_status.clone();
        let profile = if self.store.profile.is_empty() { tr("Personnel").to_string() } else { self.store.profile.clone() };
        let mut go_page: Option<&'static str> = None;
        let mut sync_now = false;
        if self.is_default_browser.is_none() {
            self.is_default_browser = Some(default_browser_is_sanaga());
        }
        let is_default = self.is_default_browser.unwrap_or(false);
        let mut section = self.settings_section;
        let query = self.settings_filter.trim().to_lowercase();
        let filter = &mut self.settings_filter;
        let st = &mut self.store.settings;

        let w = ui.available_width();
        let nav_w = 230.0;
        let wide = w >= 820.0;
        let content_w = if wide { (w - nav_w - 96.0).min(720.0) } else { (w - 48.0).min(720.0) };
        let left = if wide { ((w - nav_w - 32.0 - content_w) / 2.0).max(24.0) } else { (w - content_w) / 2.0 };

        ui.add_space(26.0);
        ui.horizontal_top(|ui| {
            ui.add_space(left);
            // --- the list of categories (in a menu on narrow windows) ---
            if wide {
                ui.vertical(|ui| {
                    ui.set_width(nav_w);
                    ui.label(RichText::new(tr("Paramètres")).size(24.0).family(bold()).color(th.text));
                    ui.add_space(14.0);
                    for sec in Section::ALL {
                        let sel = query.is_empty() && sec == section;
                        let (r, resp) = ui.allocate_exact_size(vec2(nav_w, 36.0), Sense::click());
                        if sel {
                            ui.painter().rect_filled(r, CornerRadius::same(18), th.accent.gamma_multiply(0.15));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(18), th.omnibox_hover);
                        }
                        let col = if sel { th.accent } else { th.text };
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(16.0, 16.0)), sec.icon(), col);
                        let clip = Rect::from_min_max(pos2(r.left() + 42.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
                        ui.painter().with_clip_rect(clip).text(pos2(clip.left(), r.center().y), Align2::LEFT_CENTER, sec.label(), FontId::proportional(13.5), col);
                        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            section = sec;
                            filter.clear();
                        }
                    }
                });
                ui.add_space(32.0);
            }
            ui.vertical(|ui| {
                ui.set_width(content_w);
                if !wide {
                    ui.label(RichText::new(tr("Paramètres")).size(24.0).family(bold()).color(th.text));
                    ui.add_space(10.0);
                }
                // search in every category
                Frame::new().fill(th.omnibox).corner_radius(CornerRadius::same(18)).inner_margin(Margin::symmetric(14, 0)).show(ui, |ui| {
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 38.0), Layout::left_to_right(Align::Center), |ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Search, th.sub);
                        ui.add_space(4.0);
                        let te = TextEdit::singleline(filter)
                            .id(egui::Id::new("settings-search"))
                            .frame(Frame::NONE)
                            .font(FontId::proportional(14.0))
                            .vertical_align(Align::Center)
                            .hint_text(RichText::new(tr("Rechercher dans les paramètres")).color(th.sub))
                            .desired_width(ui.available_width());
                        ui.add(te);
                    });
                });
                ui.add_space(18.0);
                if !wide {
                    egui::ComboBox::from_id_salt("settings-section").selected_text(section.label()).width(content_w).show_ui(ui, |ui| {
                        for sec in Section::ALL {
                            if ui.selectable_label(sec == section, sec.label()).clicked() {
                                section = sec;
                            }
                        }
                    });
                    ui.add_space(14.0);
                }
                if query.is_empty() {
                    ui.label(RichText::new(section.label()).size(20.0).family(bold()).color(th.text));
                    ui.add_space(12.0);
                }
                let show = Shown { section, query: &query };
                let mut any = false;

                // ---------------- General ----------------
                if show.shows(Section::General, tr("Navigateur par défaut"), &[]) {
                    any = true;
                    group(ui, th, tr("Navigateur par défaut"), |ui| {
                        ui.horizontal(|ui| {
                            if is_default {
                                ui.label(RichText::new(tr("Sanaga est votre navigateur par défaut.")).color(th.text));
                            } else {
                                ui.label(RichText::new(tr("Sanaga n'est pas votre navigateur par défaut.")).color(th.text));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add(egui::Button::new(RichText::new(tr("Définir par défaut")).color(Color32::WHITE)).fill(th.accent)).clicked() {
                                        make_default = true;
                                    }
                                });
                            }
                        });
                        note(ui, th, tr("Les liens ouverts depuis vos autres applications s'ouvriront dans Sanaga."));
                    });
                }
                let lang_title = "Langue · Language · Idioma · Sprache";
                if show.shows(Section::General, lang_title, &[tr("Langue")]) {
                    any = true;
                    group(ui, th, lang_title, |ui| {
                        let current = crate::i18n::LANGUAGES.iter().find(|(c, _)| *c == st.language).map(|l| l.1).unwrap_or("English");
                        egui::ComboBox::from_id_salt("language").selected_text(current).width(240.0).show_ui(ui, |ui| {
                            for (code, name) in crate::i18n::LANGUAGES {
                                if ui.selectable_label(st.language == code, name).clicked() && st.language != code {
                                    st.language = code.to_string();
                                    changed = true;
                                }
                            }
                        });
                        note(ui, th, tr("Toute l'interface de Sanaga change aussitôt ; les sites sont aussi demandés dans cette langue (pour les nouveaux onglets)."));
                    });
                }
                if show.shows(Section::General, tr("Au démarrage"), &[tr("Page d'accueil"), tr("Rouvrir les onglets de la dernière session")]) {
                    any = true;
                    group(ui, th, tr("Au démarrage"), |ui| {
                        changed |= ui.radio_value(&mut st.restore_tabs, true, tr("Rouvrir les onglets de la dernière session")).changed();
                        changed |= ui.radio_value(&mut st.restore_tabs, false, tr("Ouvrir la page d'accueil")).changed();
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(tr("Page d'accueil"));
                            changed |= ui.add(TextEdit::singleline(&mut st.home).desired_width(ui.available_width() - 8.0)).changed();
                        });
                    });
                }
                if show.shows(Section::General, tr("Moteur de recherche"), &[tr("Moteur utilisé dans la barre d'adresse")]) {
                    any = true;
                    group(ui, th, tr("Moteur de recherche"), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(tr("Moteur utilisé dans la barre d'adresse"));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                egui::ComboBox::from_id_salt("engine").selected_text(st.engine.label()).width(220.0).show_ui(ui, |ui| {
                                    for e in Engine::ALL {
                                        changed |= ui.selectable_value(&mut st.engine, e, e.label()).changed();
                                    }
                                });
                            });
                        });
                        note(ui, th, tr("La version légère de DuckDuckGo fonctionne le mieux avec Sanaga."));
                    });
                }

                // ---------------- Appearance ----------------
                if show.shows(Section::Appearance, tr("Thème"), &[tr("Thème sombre")]) {
                    any = true;
                    group(ui, th, tr("Thème"), |ui| {
                        ui.horizontal(|ui| {
                            changed |= ui.selectable_value(&mut st.dark, false, tr("Clair")).changed();
                            changed |= ui.selectable_value(&mut st.dark, true, tr("Sombre")).changed();
                        });
                    });
                }
                if show.shows(Section::Appearance, tr("Barre d'outils"), &[tr("Afficher la barre de favoris"), tr("Boutons de la barre d'outils (aussi : clic droit sur la barre d'outils)")]) {
                    any = true;
                    group(ui, th, tr("Barre d'outils"), |ui| {
                        changed |= ui.checkbox(&mut st.show_bookmarks_bar, tr("Afficher la barre de favoris")).changed();
                        ui.add_space(4.0);
                        note(ui, th, tr("Boutons de la barre d'outils (aussi : clic droit sur la barre d'outils)"));
                        changed |= super::toolbar::toolbar_checkboxes(ui, &mut st.toolbar);
                    });
                }
                if show.shows(Section::Appearance, tr("Zoom par défaut"), &[]) {
                    any = true;
                    group(ui, th, tr("Zoom par défaut"), |ui| {
                        let s = egui::Slider::new(&mut st.zoom, 0.5..=2.0).step_by(0.1).custom_formatter(|v, _| format!("{:.0} %", v * 100.0));
                        changed |= ui.add(s).changed();
                        note(ui, th, tr("Ctrl + molette ou Ctrl +/− change le zoom d'un site ; Sanaga s'en souvient."));
                    });
                }

                // ---------------- Privacy ----------------
                if show.shows(Section::Privacy, tr("Suivi"), &[tr("Empêcher le suivi intersites")]) {
                    any = true;
                    group(ui, th, tr("Suivi"), |ui| {
                        changed |= ui.checkbox(&mut st.itp, tr("Empêcher le suivi intersites")).changed();
                        note(ui, th, tr("Les traceurs publicitaires ne peuvent plus vous suivre d'un site à l'autre (comme dans Safari). Le Rapport de confidentialité de la page d'accueil montre ceux qui ont été tenus à l'écart. S'applique aux nouveaux onglets."));
                    });
                }
                if show.shows(Section::Privacy, tr("Données de navigation"), &[tr("Effacer les données"), tr("Effacer l'historique de navigation"), tr("Paramètres des sites")]) {
                    any = true;
                    group(ui, th, tr("Données de navigation"), |ui| {
                        if link_row(ui, th, tr("Effacer les données"), tr("Historique, cookies, cache…")) {
                            go_page = Some("sanaga://effacer");
                        }
                        if link_row(ui, th, tr("Paramètres des sites"), tr("Caméra, micro, notifications, zoom… site par site")) {
                            go_page = Some("sanaga://sites");
                        }
                        ui.add_space(4.0);
                        if ui.button(tr("Effacer l'historique de navigation")).clicked() {
                            clear_history = true;
                        }
                    });
                }
                if show.shows(Section::Privacy, tr("Sécurité"), &[tr("Sanaga Léger")]) {
                    any = true;
                    group(ui, th, tr("Sécurité"), |ui| {
                        note(ui, th, tr("Chaque page s'exécute isolée du reste de l'ordinateur (bac à sable). Les mots de passe restent dans le trousseau du système."));
                        note(ui, th, tr("Avec le moteur Sanaga Léger, aucun script n'est exécuté : pas de publicités, pas de traceurs."));
                    });
                }

                // ---------------- Passwords ----------------
                if show.shows(Section::Passwords, tr("Mots de passe"), &[tr("Proposer d'enregistrer et de remplir les mots de passe"), tr("Gérer les mots de passe enregistrés")]) {
                    any = true;
                    group(ui, th, tr("Mots de passe"), |ui| {
                        changed |= ui.checkbox(&mut st.passwords, tr("Proposer d'enregistrer et de remplir les mots de passe")).changed();
                        note(ui, th, tr("Quand un site n'a qu'un compte enregistré, Sanaga le remplit tout seul ; sinon, choisissez le compte dans la liste sous le champ."));
                        ui.add_space(4.0);
                        if link_row(ui, th, tr("Gérer les mots de passe enregistrés"), "") {
                            go_page = Some("sanaga://motsdepasse");
                        }
                        if link_row(ui, th, tr("Importer depuis un autre navigateur (Chrome, Firefox…)"), "") {
                            go_page = Some("sanaga://importer");
                        }
                    });
                }
                if show.shows(Section::Passwords, tr("Coordonnées"), &[tr("Nom"), tr("E-mail"), tr("Téléphone"), tr("Adresse")]) {
                    any = true;
                    group(ui, th, tr("Coordonnées"), |ui| {
                        note(ui, th, tr("Vos coordonnées, pour remplir les formulaires (menu ⋮ › Remplir avec mes coordonnées, ou Ctrl+Maj+F) :"));
                        ui.add_space(4.0);
                        egui::Grid::new("contact-grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                            let fw = (ui.available_width() - 110.0).max(200.0);
                            for (label, value) in [(tr("Nom"), &mut st.fill_name), (tr("E-mail"), &mut st.fill_email), (tr("Téléphone"), &mut st.fill_phone), (tr("Adresse"), &mut st.fill_address)] {
                                ui.label(label);
                                changed |= ui.add(TextEdit::singleline(value).desired_width(fw)).changed();
                                ui.end_row();
                            }
                        });
                    });
                }

                // ---------------- Performance ----------------
                if show.shows(Section::Performance, tr("Économiseur de mémoire"), &[]) {
                    any = true;
                    group(ui, th, tr("Économiseur de mémoire"), |ui| {
                        note(ui, th, tr("Un onglet ne charge sa page que quand vous l'ouvrez. Ensuite, les onglets que vous n'avez pas regardés depuis un moment sont mis en veille : leur page est fermée pour libérer la mémoire et se recharge quand vous y revenez. Les onglets qui jouent du son ne sont jamais mis en veille."));
                        ui.add_space(4.0);
                        for (v, label) in [(0u8, tr("Désactivé")), (1, tr("Modéré — veille après 1 heure, 12 pages actives au plus")), (2, tr("Équilibré — veille après 20 minutes, 6 pages actives au plus (recommandé)")), (3, tr("Maximal — veille après 5 minutes, 3 pages actives au plus"))] {
                            changed |= ui.radio_value(&mut st.memory_saver, v, label).changed();
                        }
                    });
                }
                if show.shows(Section::Performance, tr("Moteur d'affichage des pages"), &[tr("WebKit"), tr("Sanaga Léger")]) {
                    any = true;
                    group(ui, th, tr("Moteur d'affichage des pages"), |ui| {
                        changed |= ui.radio_value(&mut st.webkit, true, tr("WebKit — sites complets avec JavaScript, vidéos et applications (recommandé)")).changed();
                        changed |= ui.radio_value(&mut st.webkit, false, tr("Sanaga Léger — pages simplifiées, très rapide et très économe en mémoire")).changed();
                        note(ui, th, if web_ok { tr("Le changement s'applique aux prochaines pages ouvertes.") } else { tr("WebKit n'est pas disponible ici : Sanaga utilise le moteur léger.") });
                    });
                }
                if show.shows(Section::Performance, tr("Économie de données"), &[tr("Charger les images")]) {
                    any = true;
                    group(ui, th, tr("Économie de données"), |ui| {
                        changed |= ui.checkbox(&mut st.images, tr("Charger les images")).changed();
                        note(ui, th, tr("Désactivées, les images s'affichent comme un bouton : cliquez pour charger seulement celles qui vous intéressent."));
                    });
                }

                // ---------------- Sync and profiles ----------------
                if show.shows(Section::Sync, tr("Synchronisation"), &[tr("Dossier"), tr("Synchroniser maintenant")]) {
                    any = true;
                    group(ui, th, tr("Synchronisation"), |ui| {
                        note(ui, th, tr("Choisissez un dossier partagé entre vos ordinateurs (Syncthing, Nextcloud, Dropbox, clé USB…). Sanaga y garde vos favoris, votre liste de lecture et vos onglets ouverts, et montre sur la page d'accueil les onglets de vos autres appareils."));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(tr("Dossier"));
                            changed |= ui.add(TextEdit::singleline(&mut st.sync_dir).hint_text("/home/vous/Nextcloud").desired_width((ui.available_width() - 190.0).max(160.0))).changed();
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
                        note(ui, th, &state);
                    });
                }
                if show.shows(Section::Sync, tr("Profils"), &[tr("Gérer les profils")]) {
                    any = true;
                    group(ui, th, tr("Profils"), |ui| {
                        if link_row(ui, th, tr("Gérer les profils"), &tf("Profil de cette fenêtre : {}", &[&profile])) {
                            go_page = Some("sanaga://profils");
                        }
                    });
                }

                // ---------------- Extensions ----------------
                if show.shows(Section::Extensions, tr("Extensions"), &[tr("Gérer les extensions (bloqueur de publicités, mode sombre…)")]) {
                    any = true;
                    group(ui, th, tr("Extensions"), |ui| {
                        if link_row(ui, th, tr("Gérer les extensions (bloqueur de publicités, mode sombre…)"), "") {
                            go_page = Some("sanaga://extensions");
                        }
                    });
                }

                if !any {
                    ui.label(RichText::new(tr("Aucun paramètre ne correspond à votre recherche.")).color(th.sub));
                }
                ui.add_space(10.0);
                if ui.link(tr("À propos de Sanaga")).clicked() {
                    go_page = Some("sanaga://a-propos");
                }
                ui.add_space(30.0);
            });
        });
        self.settings_section = section;

        if changed {
            crate::i18n::set_language(&self.store.settings.language);
            self.store.save_settings();
        }
        if make_default {
            self.make_default_browser();
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

    /// Make Sanaga the default browser (web links and HTML files).
    fn make_default_browser(&mut self) {
        let now = self.ctx.input(|i| i.time);
        if !launcher_installed() {
            self.notice = Some((tr("Installez d'abord Sanaga (packaging/install.sh) pour qu'il apparaisse dans vos applications.").into(), now));
            return;
        }
        let ok = std::process::Command::new("xdg-settings").args(["set", "default-web-browser", "sanaga.desktop"]).status().is_ok_and(|s| s.success());
        // HTML files too
        let _ = std::process::Command::new("xdg-mime").args(["default", "sanaga.desktop", "text/html", "application/xhtml+xml"]).status();
        self.is_default_browser = Some(default_browser_is_sanaga());
        self.notice = Some((if ok { tr("Sanaga est maintenant votre navigateur par défaut.") } else { tr("Impossible de changer le navigateur par défaut.") }.into(), now));
    }
}

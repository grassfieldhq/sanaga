//! Summaries of pages and extensions written from a description (see `ai.rs`
//! for the models). The summary shows in a panel at the side of the page.

use super::*;
use crate::ai::{self, Provider};
use crate::icons::{self, Icon};
use eframe::egui::{Align, Color32, Layout, Margin, RichText, Sense, TextEdit};

/// Collects the readable text of the page (its article or main part when it
/// has one) and hands it to Sanaga.
const PAGE_TEXT_JS: &str = r#"(function(){
  var root = document.querySelector('article') || document.querySelector('main') || document.querySelector('[role=main]') || document.body;
  // the paragraphs of the text first (not menus, lists of links, references)
  var ps = Array.prototype.slice.call((root || document).querySelectorAll('p'))
    .filter(function (p) { return p.offsetParent !== null && !p.closest('nav,footer,aside,.references,.reflist,[role=navigation]'); })
    .map(function (p) { return p.innerText.trim(); })
    .filter(function (x) { return x.split(/\s+/).length >= 8; });
  var t = ps.join('\n');
  if (t.length < 600) t = (root && root.innerText) || '';
  if (t.length < 600 && document.body) t = document.body.innerText || '';
  if (window.ipc) window.ipc.postMessage('sum:' + (document.title || '').replace(/\n/g, ' ') + '\n' + t.slice(0, 80000));
})();"#;

pub(super) struct Summary {
    tab: u64,
    title: String,
    text: String,
    busy: bool,
    by: String,
    note: String,
    /// The page's text has arrived (later ones are ignored).
    sent: bool,
}

impl Summary {
    pub(super) fn is_for(&self, tab: u64) -> bool {
        self.tab == tab
    }
}

pub(super) enum AiDone {
    Summary { tab: u64, result: Result<String, String>, by: String, fallback: String },
    Extension(Result<(String, String), String>),
    Models(Vec<String>),
    /// The accounts read from the system keyring.
    Logins(Vec<(String, String)>),
    /// A batch of a page's texts, translated (from its place in the page,
    /// for the translation `generation` of that tab).
    Translation { tab: u64, from: usize, generation: u64, result: Result<Vec<String>, String> },
}

#[derive(Default)]
pub(super) struct ExtForm {
    name: String,
    description: String,
    sites: String,
    status: String,
    busy: bool,
    code: String,
}

impl Sanaga {
    /// The model chosen in Settings › Intelligence (with its key, for Claude).
    pub(super) fn ai_provider(&self) -> Provider {
        let st = &self.store.settings;
        if let Some(tool) = crate::ai_cli::CliTool::from_key(&st.ai) {
            return Provider::Cli { tool, model: st.ai_model.clone() };
        }
        match st.ai.as_str() {
            "ollama" => {
                let model = if !st.ai_model.is_empty() {
                    st.ai_model.clone()
                } else {
                    self.ai_models.as_ref().and_then(|m| m.first().cloned()).unwrap_or_else(|| "llama3.2".into())
                };
                Provider::Ollama { model }
            }
            "claude" => match crate::keyring::lookup(&self.store.profile, ai::KEY_SITE, "claude") {
                Some(key) if !key.is_empty() => Provider::Claude { model: if st.ai_model.is_empty() { ai::CLAUDE_MODEL.into() } else { st.ai_model.clone() }, key },
                _ => Provider::Local,
            },
            _ => Provider::Local,
        }
    }

    /// Summarise the page shown (toolbar button, menu).
    pub(super) fn summarize_page(&mut self) {
        let t = &self.tabs[self.active];
        let (id, title) = (t.id, t.title());
        let Some(w) = t.web.as_ref().filter(|_| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("Le résumé marche sur les pages des sites.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        let _ = w.evaluate_own(PAGE_TEXT_JS);
        self.summary = Some(Summary { tab: id, title, text: String::new(), busy: true, by: String::new(), note: String::new(), sent: false });
    }

    pub(super) fn summary_text_arrived(&mut self, tab: u64, text: String) {
        // only the text the person asked for (one answer per request)
        if !self.summary.as_ref().is_some_and(|s| s.tab == tab && s.busy && s.text.is_empty() && !s.sent) {
            return;
        }
        if let Some(s) = self.summary.as_mut() {
            s.sent = true;
        }
        let (title, body) = text.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
        let fallback = ai::local_summary(&body, 6);
        let provider = self.ai_provider();
        if let Some(s) = self.summary.as_mut() {
            if !title.trim().is_empty() {
                s.title = title.trim().to_string();
            }
        }
        if body.split_whitespace().count() < 30 {
            if let Some(s) = self.summary.as_mut() {
                s.busy = false;
                s.text = tr("Cette page contient trop peu de texte à résumer.").into();
            }
            return;
        }
        // the page's text only goes to a model with no tools and no files
        let refused = ai::page_content_allowed(&provider).err();
        if provider == Provider::Local || refused.is_some() {
            if let Some(s) = self.summary.as_mut() {
                s.busy = false;
                s.text = fallback;
                s.by = Provider::Local.label();
                s.note = refused.unwrap_or_default();
            }
            return;
        }
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let by = provider.label();
        std::thread::Builder::new()
            .name("sanaga-summary".into())
            .spawn(move || {
                let (system, prompt) = ai::summary_prompt(&title, &body);
                let result = ai::ask(&provider, &system, &prompt, 900);
                let _ = tx.send(AiDone::Summary { tab, result, by, fallback });
                repaint.request_repaint();
            })
            .ok();
    }

    /// Answers of the models, when they come.
    pub(super) fn poll_ai(&mut self) {
        while let Ok(done) = self.ai_rx.try_recv() {
            match done {
                AiDone::Summary { tab, result, by, fallback } => {
                    let Some(s) = self.summary.as_mut().filter(|s| s.tab == tab) else { continue };
                    s.busy = false;
                    match result {
                        Ok(t) => {
                            s.text = t.trim().to_string();
                            s.by = by;
                        }
                        Err(e) => {
                            // the model failed: Sanaga's own summary instead
                            s.text = fallback;
                            s.by = Provider::Local.label();
                            s.note = e;
                        }
                    }
                }
                AiDone::Extension(r) => {
                    self.ext_form.busy = false;
                    match r {
                        Ok((file, text)) => {
                            let dir = crate::extensions::prepare_folder();
                            let mut path = dir.join(&file);
                            let mut n = 2;
                            while path.exists() {
                                path = dir.join(file.replace(".js", &format!("-{n}.js")));
                                n += 1;
                            }
                            match crate::store::write_atomic(&path, &text) {
                                Ok(()) => {
                                    let id = format!("fichier:{}", path.file_name().unwrap_or_default().to_string_lossy());
                                    if !self.store.extensions_on.contains(&id) {
                                        self.store.extensions_on.push(id);
                                        self.store.save_extensions();
                                    }
                                    self.ext_form.status = tf("« {} » est créée et activée : elle s'applique aux onglets que vous ouvrez maintenant.", &[&self.ext_form.name]);
                                    self.ext_form.code = text;
                                }
                                Err(e) => self.ext_form.status = e.to_string(),
                            }
                        }
                        Err(e) => self.ext_form.status = e,
                    }
                }
                AiDone::Models(m) => self.ai_models = Some(m),
                AiDone::Translation { tab, from, generation, result } => self.translation_done(tab, from, generation, result),
                AiDone::Logins(list) => {
                    // the keyring is the reference; an empty answer (a locked
                    // or missing keyring) leaves the list as it was
                    if !list.is_empty() && list != self.store.logins {
                        self.store.logins = list;
                        self.store.save_logins();
                    }
                }
            }
        }
    }

    /// Read the saved accounts from the system keyring (in the background).
    pub(super) fn refresh_logins(&mut self) {
        if !self.web_ok || self.store.data_dir().is_none() {
            return;
        }
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let profile = self.store.profile.clone();
        std::thread::spawn(move || {
            if let Some(list) = crate::keyring::list(&profile) {
                let _ = tx.send(AiDone::Logins(list));
                repaint.request_repaint();
            }
        });
    }

    /// Look for the models installed in Ollama (in the background).
    pub(super) fn refresh_ollama_models(&mut self) {
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        self.ai_models = Some(Vec::new());
        std::thread::spawn(move || {
            let _ = tx.send(AiDone::Models(ai::ollama_models()));
            repaint.request_repaint();
        });
    }

    /// The summary, at the side of the page.
    pub(super) fn summary_panel(&mut self, ui: &mut Ui, th: &Theme) {
        let mut close = false;
        let mut again = false;
        let mut copy = false;
        let active_id = self.tabs[self.active].id;
        let Some(s) = self.summary.as_ref() else { return };
        Frame::new().inner_margin(Margin::symmetric(18, 14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Sparkle, th.accent);
                ui.label(RichText::new(tr("Résumé")).size(17.0).family(bold()).color(th.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if super::chrome::icon_button_small(ui, Icon::Close, th).tip(tr("Fermer")).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new(&s.title).size(13.5).family(bold()).color(th.sub));
            ui.add_space(10.0);
            if s.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tr("Lecture de la page…")).color(th.sub));
                });
            } else {
                egui::ScrollArea::vertical().id_salt("summary-scroll").max_height(ui.available_height() - 80.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&s.text).size(14.5).color(th.text).line_height(Some(22.0))).selectable(true).wrap());
                });
                ui.add_space(12.0);
                if !s.note.is_empty() {
                    ui.label(RichText::new(&s.note).size(11.5).color(Color32::from_rgb(0xD0, 0x60, 0x30)));
                }
                ui.label(RichText::new(tf("Par : {}", &[&s.by])).size(11.5).color(th.sub));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Copier")).clicked() {
                        copy = true;
                    }
                    let label = if s.tab == active_id { tr("Résumer à nouveau") } else { tr("Résumer cette page") };
                    if ui.button(label).clicked() {
                        again = true;
                    }
                });
            }
        });
        if copy {
            if let Some(s) = &self.summary {
                self.ctx.copy_text(format!("{}\n\n{}", s.title, s.text));
            }
        }
        if close {
            self.summary = None;
        } else if again {
            self.summarize_page();
        }
    }

    /// sanaga://create-extension: describe what you want, a model writes it.
    pub(super) fn page_create_extension(&mut self, ui: &mut Ui, th: &Theme) {
        let provider_label = (!self.store.settings.ai.is_empty()).then(|| self.store.settings.ai.clone());
        let found = crate::ai_cli::installed();
        let mut use_tool: Option<crate::ai_cli::CliTool> = None;
        let mut go_settings = false;
        let mut create = false;
        super::pages::centered_column(ui, |ui| {
            super::pages::page_title(ui, th, tr("Créer une extension"));
            ui.label(RichText::new(tr("Décrivez ce que vous voulez changer sur les sites : Sanaga écrit l'extension pour vous et l'active.")).size(14.0).color(th.sub));
            ui.add_space(14.0);
            if provider_label.is_none() {
                Frame::new().fill(th.accent.gamma_multiply(0.12)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tr("Il faut un modèle d'IA : Ollama sur cet ordinateur (gratuit, rien ne sort de chez vous) ou Claude avec votre clé.")).color(th.text));
                    if !found.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(tr("Trouvés sur cet ordinateur :")).color(th.text));
                            for t in &found {
                                let b = egui::Button::new(RichText::new(tf("Utiliser {}", &[&t.name()])).color(Color32::WHITE)).fill(th.accent);
                                if ui.add(b).clicked() {
                                    use_tool = Some(*t);
                                }
                            }
                        });
                    }
                    if ui.link(tr("Choisir dans Paramètres › Intelligence")).clicked() {
                        go_settings = true;
                    }
                });
                ui.add_space(12.0);
            }
            let f = &mut self.ext_form;
            ui.label(RichText::new(tr("Nom")).family(bold()).color(th.text));
            ui.add(TextEdit::singleline(&mut f.name).hint_text(tr("Ex. : YouTube sans distractions")).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Ce qu'elle doit faire")).family(bold()).color(th.text));
            ui.add(TextEdit::multiline(&mut f.description).hint_text(tr("Ex. : masquer les Shorts et les vidéos recommandées à droite, et agrandir le lecteur.")).desired_rows(5).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Sur quels sites")).family(bold()).color(th.text));
            ui.add(TextEdit::singleline(&mut f.sites).hint_text(tr("youtube.com — vide : tous les sites")).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Idées :")).size(12.5).color(th.sub));
                for (name, desc, site) in [
                    (tr("Lecture zen"), tr("Masquer les bannières de cookies, les fenêtres d'inscription et les publicités collantes."), ""),
                    (tr("YouTube calme"), tr("Masquer les Shorts, les recommandations et les commentaires."), "youtube.com"),
                    (tr("Liens dans un nouvel onglet"), tr("Ouvrir les liens vers d'autres sites dans un nouvel onglet."), ""),
                ] {
                    if ui.small_button(name).clicked() {
                        f.name = name.into();
                        f.description = desc.into();
                        f.sites = site.into();
                    }
                }
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let ready = !f.description.trim().is_empty() && !f.busy && provider_label.is_some();
                let b = egui::Button::new(RichText::new(tr("Créer l'extension")).color(Color32::WHITE)).fill(if ready { th.accent } else { th.sub }).min_size(vec2(160.0, 34.0)).corner_radius(CornerRadius::same(10));
                if ui.add_enabled(ready, b).clicked() {
                    create = true;
                }
                if f.busy {
                    ui.spinner();
                    ui.label(RichText::new(tr("Écriture de l'extension…")).color(th.sub));
                }
            });
            if !f.status.is_empty() {
                ui.add_space(10.0);
                ui.label(RichText::new(&f.status).color(th.text));
            }
            if !f.code.is_empty() {
                ui.add_space(8.0);
                egui::CollapsingHeader::new(tr("Voir le code")).show(ui, |ui| {
                    let mut code = f.code.clone();
                    ui.add(TextEdit::multiline(&mut code).code_editor().desired_width(f32::INFINITY).interactive(false));
                });
            }
            ui.add_space(10.0);
            ui.label(RichText::new(tr("Les extensions créées sont de simples fichiers dans le dossier des extensions : vous pouvez les désactiver ou les supprimer dans Extensions.")).size(12.0).color(th.sub));
        });
        if let Some(t) = use_tool {
            self.store.settings.ai = t.key().to_string();
            self.store.settings.ai_model.clear();
            self.store.save_settings();
        }
        if go_settings {
            self.settings_section = super::settings::Section::Intelligence;
            self.navigate(self.active, "sanaga://settings".into(), None, true);
        }
        if create {
            let provider = self.ai_provider();
            if provider == Provider::Local {
                self.ext_form.status = tr("Ajoutez votre clé Claude ou lancez Ollama (Paramètres › Intelligence).").into();
                return;
            }
            let f = &mut self.ext_form;
            f.busy = true;
            f.status.clear();
            f.code.clear();
            if f.name.trim().is_empty() {
                f.name = f.description.split_whitespace().take(5).collect::<Vec<_>>().join(" ");
            }
            let (name, desc, sites) = (f.name.clone(), f.description.clone(), f.sites.clone());
            let tx = self.ai_tx.clone();
            let repaint = self.ctx.clone();
            std::thread::Builder::new()
                .name("sanaga-extension".into())
                .spawn(move || {
                    let _ = tx.send(AiDone::Extension(ai::write_extension(&provider, &name, &desc, &sites)));
                    repaint.request_repaint();
                })
                .ok();
        }
    }
}

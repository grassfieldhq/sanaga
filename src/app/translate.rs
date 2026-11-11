//! Page translation, the person's choice: Google Translate (the page's
//! address goes to Google), or the AI chosen in Settings › Intelligence —
//! then the page is translated in place, a batch of its texts at a time, and
//! only those texts are sent to that AI.

use super::*;
use crate::ai::{self, Provider};
use eframe::egui::Sense;

/// Collects the page's texts (not code, fields or scripts) and sends them to
/// Sanaga in batches; `__sanagaTrApply` puts the translations in place and
/// asks for the next batch.
const TRANSLATE_JS: &str = r#"(function () {
  if (!window.ipc) return;
  var skip = /^(SCRIPT|STYLE|NOSCRIPT|CODE|PRE|TEXTAREA|INPUT|SELECT|OPTION|SVG|MATH|KBD|SAMP)$/;
  var nodes = [];
  var w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
    acceptNode: function (n) {
      var p = n.parentElement;
      if (!p || skip.test(p.tagName) || p.isContentEditable || p.closest('[translate=no],.notranslate')) return NodeFilter.FILTER_REJECT;
      var t = n.nodeValue.trim();
      if (t.length < 2 || !/[A-Za-zÀ-ÿЀ-ӿ؀-ۿ一-鿿]/.test(t)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    }
  });
  while (w.nextNode()) nodes.push(w.currentNode);
  window.__sanagaTr = { nodes: nodes, pos: 0, orig: {}, stopped: false };
  function next() {
    var s = window.__sanagaTr, out = [], size = 0, from = s.pos;
    if (s.stopped) return;
    while (s.pos < s.nodes.length && out.length < 80 && size < 5000) {
      var t = s.nodes[s.pos].nodeValue.trim();
      out.push(t); size += t.length; s.pos++;
    }
    window.ipc.postMessage(out.length ? 'tr:' + from + '\n' + JSON.stringify(out) : 'tr:done');
  }
  // The model's answer goes into the page as plain text only (a text node's
  // data: never read as HTML), from Sanaga's own world.
  window.__sanagaTrApply = function (from, list) {
    var s = window.__sanagaTr;
    if (!s || !Array.isArray(list)) return;
    for (var i = 0; i < list.length; i++) {
      var n = s.nodes[from + i];
      if (!n || n.nodeType !== 3 || typeof list[i] !== 'string' || !list[i]) continue;
      var v = n.data, lead = v.match(/^\s*/)[0], trail = v.match(/\s*$/)[0];
      // (the page's own words are kept, to show them again)
      if (!(from + i in s.orig)) s.orig[from + i] = v;
      n.data = lead + list[i] + trail;
    }
    next();
  };
  // Stop asking for more; and put the page's own words back.
  window.__sanagaTrStop = function () { if (window.__sanagaTr) window.__sanagaTr.stopped = true; };
  window.__sanagaTrOriginal = function () {
    var s = window.__sanagaTr;
    if (!s) return;
    s.stopped = true;
    for (var k in s.orig) { var n = s.nodes[k]; if (n && n.nodeType === 3) n.data = s.orig[k]; }
    s.orig = {};
  };
  next();
})();"#;

/// At most this many batches of a page are translated (80 texts or 5000
/// characters each): a very long page stops there.
const MAX_BATCHES: usize = 40;

/// A batch as Sanaga's page script makes it: where it starts, and its texts.
/// Anything else (too many texts, too long, a strange start) is refused.
pub(super) fn read_batch(msg: &str) -> Option<(usize, Vec<String>)> {
    let (from, json) = msg.split_once('\n')?;
    let from: usize = from.trim().parse().ok().filter(|f| *f <= 1_000_000)?;
    let texts = parse_string_array(json);
    let size: usize = texts.iter().map(|t| t.chars().count()).sum();
    (!texts.is_empty() && texts.len() <= 80 && size <= 6000).then_some((from, texts))
}

/// Languages pages can be translated into: code, name for the model, name
/// shown (in the language itself when the interface's fonts can draw it).
pub(super) const LANGUAGES: [(&str, &str, &str); 20] = [
    ("fr", "French", "Français"),
    ("en", "English", "English"),
    ("es", "Spanish", "Español"),
    ("pt", "Portuguese", "Português"),
    ("de", "German", "Deutsch"),
    ("it", "Italian", "Italiano"),
    ("nl", "Dutch", "Nederlands"),
    ("pl", "Polish", "Polski"),
    ("tr", "Turkish", "Türkçe"),
    ("sw", "Swahili", "Kiswahili"),
    ("ha", "Hausa", "Hausa"),
    ("yo", "Yoruba", "Yorùbá"),
    ("ln", "Lingala", "Lingála"),
    ("wo", "Wolof", "Wolof"),
    ("ar", "Arabic", "Arabic — العربية"),
    ("ru", "Russian", "Русский"),
    ("zh-CN", "Simplified Chinese", "Chinese (中文)"),
    ("ja", "Japanese", "Japanese (日本語)"),
    ("ko", "Korean", "Korean (한국어)"),
    ("hi", "Hindi", "Hindi (हिन्दी)"),
];

/// The language chosen (Settings › Intelligence), else the interface's.
pub(super) fn target(chosen: &str) -> (&'static str, &'static str, &'static str) {
    let code = if chosen.is_empty() { crate::i18n::language() } else { chosen };
    LANGUAGES.iter().copied().find(|l| l.0 == code).unwrap_or(LANGUAGES[1])
}

/// A JSON array of strings, read leniently from a model's answer.
pub(super) fn parse_string_array(text: &str) -> Vec<String> {
    let Some(start) = text.find('[') else { return Vec::new() };
    let mut out = Vec::new();
    let mut rest = &text[start + 1..];
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') || rest.is_empty() {
            break;
        }
        if let Some(r) = rest.strip_prefix(',') {
            rest = r;
            continue;
        }
        let Some(r) = rest.strip_prefix('"') else { break };
        match crate::ai::read_json_string_end(r) {
            Some((s, used)) => {
                out.push(s);
                rest = &r[used..];
            }
            None => break,
        }
    }
    out
}

/// Where a page's translation is, for the bar under the toolbar.
#[derive(Clone, PartialEq)]
pub(super) enum TrState {
    Running,
    Done,
    /// Stopped before the end (by the person, a very long page, an error): why.
    Stopped(String),
    /// The page shows its own words again.
    Original,
}

/// The translation of one tab's page: what the bar says and offers.
pub(super) struct Translation {
    pub(super) tab: u64,
    /// The language's name as shown, and the model's.
    lang: String,
    by: String,
    state: TrState,
    /// Parts of the page already translated.
    parts: usize,
}

impl Sanaga {
    /// Menu ⋮ › Translate page: Google or the AI, as chosen (asked once).
    pub(super) fn translate_page(&mut self) {
        match self.store.settings.translate_with.as_str() {
            "google" => self.translate_with_google(),
            "ai" => self.translate_with_ai(),
            _ => {
                self.translate_ask = true;
                // (the question is about this tab's page)
                self.translate_ask_tab = self.tabs.get(self.active).map(|t| t.id).unwrap_or(0);
            }
        }
    }

    /// The bar of the page's translation: shown on its tab until closed.
    pub(super) fn translation_bar_visible(&self) -> bool {
        !self.focus_mode && self.translation.as_ref().is_some_and(|t| self.tabs.get(self.active).is_some_and(|a| a.id == t.tab))
    }

    /// While it runs: what is being done, and "Stop". Afterwards: what was
    /// done and by which model, and "Show original" (the page's own words
    /// come back at once, without loading it again).
    pub(super) fn translation_bar(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(t) = self.translation.as_ref() else { return };
        let (state, lang, by, parts) = (t.state.clone(), t.lang.clone(), t.by.clone(), t.parts);
        let mut act: Option<&str> = None;
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            if state == TrState::Running {
                ui.add(egui::Spinner::new().size(14.0).color(th.accent));
            } else {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, crate::icons::Icon::Translate, th.accent);
            }
            ui.add_space(6.0);
            let text = match &state {
                TrState::Running if parts == 0 => tf("Traduction en {} avec {}…", &[&lang, &by]),
                TrState::Running => tf("Traduction en {} avec {}… ({} parties faites)", &[&lang, &by, &parts]),
                TrState::Done => tf("Page traduite en {} par {}.", &[&lang, &by]),
                TrState::Stopped(why) if why.is_empty() => tr("Traduction arrêtée.").to_string(),
                TrState::Stopped(why) => why.clone(),
                TrState::Original => tr("La page est de nouveau dans sa langue.").to_string(),
            };
            ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(th.text)).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if state != TrState::Running && super::chrome::icon_button_small(ui, crate::icons::Icon::Close, th).tip(tr("Fermer")).clicked() {
                    act = Some("close");
                }
                match state {
                    TrState::Running => {
                        if ui.button(tr("Arrêter")).clicked() {
                            act = Some("stop");
                        }
                    }
                    TrState::Original => {
                        if ui.button(tr("Traduire à nouveau")).clicked() {
                            act = Some("again");
                        }
                    }
                    _ => {
                        if ui.button(tr("Afficher l'original")).clicked() {
                            act = Some("original");
                        }
                    }
                }
            });
        });
        let tab = self.translation.as_ref().map(|t| t.tab).unwrap_or(0);
        let run = |app: &Sanaga, js: &str| {
            if let Some(w) = app.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
                let _ = w.evaluate_own(js);
            }
        };
        match act {
            Some("stop") => {
                run(self, "window.__sanagaTrStop && window.__sanagaTrStop();");
                self.stop_translation(tab);
                self.translation_state(tab, TrState::Stopped(String::new()));
            }
            Some("original") => {
                run(self, "window.__sanagaTrOriginal && window.__sanagaTrOriginal();");
                self.stop_translation(tab);
                self.translation_state(tab, TrState::Original);
            }
            Some("again") => {
                self.translation = None;
                self.translate_with_ai();
            }
            Some("close") => self.translation = None,
            _ => {}
        }
    }

    /// The translation of this tab's page is now in this state (its bar follows).
    fn translation_state(&mut self, tab: u64, state: TrState) {
        if let Some(t) = self.translation.as_mut().filter(|t| t.tab == tab) {
            t.state = state;
        }
    }

    fn translate_with_google(&mut self) {
        if let Some((u, _)) = self.current_page() {
            let enc: String = url::form_urlencoded::byte_serialize(u.as_bytes()).collect();
            let a = self.active;
            self.navigate(a, format!("https://translate.google.com/translate?sl=auto&tl={}&u={enc}", target(&self.store.settings.translate_to).0), None, true);
        }
    }

    fn translate_with_ai(&mut self) {
        let provider = self.ai_provider();
        if provider == Provider::Local {
            self.notice = Some((tr("Choisissez d'abord une IA dans Paramètres › Intelligence.").into(), self.ctx.input(|i| i.time)));
            return;
        }
        // the page's text only goes to a model with no tools and no files
        if let Err(e) = ai::page_content_allowed(&provider) {
            self.notice = Some((e, self.ctx.input(|i| i.time)));
            return;
        }
        let t = &self.tabs[self.active];
        let Some(w) = t.web.as_ref().filter(|_| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("La traduction marche sur les pages des sites.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        let _ = w.evaluate_own(TRANSLATE_JS);
        let id = t.id;
        self.translating = Some(id);
        self.translate_batches = 0;
        self.translate_gen += 1;
        // (what is being done stays in view, with a way to stop it)
        self.translation = Some(Translation { tab: id, lang: target(&self.store.settings.translate_to).2.to_string(), by: provider.label(), state: TrState::Running, parts: 0 });
    }

    /// The tab left its page (or closed): its translation stops, and the
    /// answers still on their way are dropped.
    pub(super) fn stop_translation(&mut self, tab: u64) {
        if self.translating == Some(tab) {
            self.translating = None;
            self.translate_batches = 0;
            self.translate_gen += 1;
        }
    }

    /// A batch of texts from the page (`from` + JSON array), or the end.
    pub(super) fn translation_batch(&mut self, tab: u64, msg: &str) {
        if self.translating != Some(tab) {
            return;
        }
        if msg == "done" {
            self.translating = None;
            self.translation_state(tab, TrState::Done);
            return;
        }
        let Some((from, texts)) = read_batch(msg) else { return };
        if self.translate_batches >= MAX_BATCHES {
            self.stop_translation(tab);
            self.translation_state(tab, TrState::Stopped(tr("La traduction s'arrête ici : la page est très longue.").into()));
            return;
        }
        self.translate_batches += 1;
        let provider = self.ai_provider();
        if let Err(e) = ai::page_content_allowed(&provider) {
            self.stop_translation(tab);
            self.translation_state(tab, TrState::Stopped(e));
            return;
        }
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let lang = target(&self.store.settings.translate_to).1;
        let generation = self.translate_gen;
        std::thread::Builder::new()
            .name("sanaga-translate".into())
            .spawn(move || {
                let system = format!(
                    "You translate web page texts into {lang}. You get a JSON array of strings. Answer with a JSON array of exactly the same length, \
                     each string translated (keep names, numbers, URLs and punctuation; keep a string as is when it is already in {lang}). \
                     Answer with the JSON array only."
                );
                let prompt = ai::json_array(&texts);
                let out = match ai::ask(&provider, &system, &prompt, 8000) {
                    Ok(answer) => {
                        let mut list = parse_string_array(&answer);
                        // a list of the wrong length: keep the originals where unsure
                        list.resize(texts.len(), String::new());
                        Ok(list)
                    }
                    Err(e) => Err(e),
                };
                let _ = tx.send(super::intelligence::AiDone::Translation { tab, from, generation, result: out });
                repaint.request_repaint();
            })
            .ok();
    }

    /// Put a translated batch in the page (it then sends the next one), as
    /// text only, from Sanaga's world; an answer for a page the tab has left
    /// is dropped.
    pub(super) fn translation_done(&mut self, tab: u64, from: usize, generation: u64, result: Result<Vec<String>, String>) {
        if self.translating != Some(tab) || generation != self.translate_gen {
            return;
        }
        match result {
            Ok(list) => {
                if let Some(w) = self.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
                    let _ = w.evaluate_own(&format!("window.__sanagaTrApply && window.__sanagaTrApply({from}, {});", ai::json_array(&list)));
                }
                if let Some(t) = self.translation.as_mut().filter(|t| t.tab == tab) {
                    t.parts += 1;
                }
            }
            Err(e) => {
                // (what the model said stays readable, with the way back)
                self.translating = None;
                self.translation_state(tab, TrState::Stopped(e));
            }
        }
    }

    /// The first time: Google or the AI? (and remember it)
    /// The ✦ button's menu: summarise the page, or translate it (into the
    /// language chosen right here or in Settings › Intelligence).
    pub(super) fn ai_menu(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.ai_menu_open {
            return;
        }
        let w = 320.0;
        let (pos, pivot, _) = super::layout::popover_place(ctx, self.ai_menu_rect, (self.ai_menu_rect.right() - w).max(8.0), 6.0);
        let mut act: Option<&str> = None;
        let dark = self.store.settings.dark;
        let lang = target(&self.store.settings.translate_to);
        let engine = match self.store.settings.translate_with.as_str() {
            "google" => "Google Traduction".to_string(),
            "ai" => self.ai_provider().label().to_string(),
            _ => String::new(),
        };
        let mut chosen = self.store.settings.translate_to.clone();
        let area = egui::Area::new(egui::Id::new("ai-menu")).order(egui::Order::Foreground).pivot(pivot).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(14))
                .shadow(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: egui::Color32::from_black_alpha(if dark { 110 } else { 45 }) })
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(w - 12.0);
                    let mut row = |ui: &mut Ui, icon: crate::icons::Icon, title: &str, sub: &str, keys: &str, key: &'static str| {
                        let (r, resp) = ui.allocate_exact_size(vec2(w - 12.0, 48.0), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{title}. {sub}")));
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(9), th.accent.gamma_multiply(0.14));
                        }
                        crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(16.0, 16.0)), icon, th.accent);
                        ui.painter().text(pos2(r.left() + 40.0, r.top() + 16.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(14.0), th.text);
                        // (the keys on the right, as in the main menu; the
                        // explanation never runs past the menu's edge)
                        ui.painter().text(pos2(r.right() - 10.0, r.top() + 16.0), egui::Align2::RIGHT_CENTER, keys, FontId::proportional(12.0), th.sub);
                        let clip = Rect::from_min_max(pos2(r.left() + 40.0, r.top()), pos2(r.right() - 8.0, r.bottom()));
                        ui.painter().with_clip_rect(clip).text(pos2(r.left() + 40.0, r.top() + 33.0), egui::Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act = Some(key);
                        }
                    };
                    row(ui, crate::icons::Icon::Sparkle, tr("Résumer la page"), tr("Les points essentiels, à côté de la page"), tr("Ctrl+Maj+Y"), "summary");
                    let sub = if engine.is_empty() { tf("En {}", &[&lang.2]) } else { tf("En {} · avec {}", &[&lang.2, &engine]) };
                    row(ui, crate::icons::Icon::Translate, tr("Traduire la page"), &sub, "", "translate");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(tr("Traduire vers")).size(12.5).color(th.sub));
                        let label = |code: &str| if code.is_empty() { tf("Langue de Sanaga ({})", &[&target("").2]) } else { target(code).2.to_string() };
                        egui::ComboBox::from_id_salt("ai-menu-lang").selected_text(label(&chosen)).width(160.0).show_ui(ui, |ui| {
                            if ui.selectable_label(chosen.is_empty(), label("")).clicked() {
                                chosen.clear();
                            }
                            for (code, _, name) in LANGUAGES {
                                if ui.selectable_label(chosen == code, name).clicked() {
                                    chosen = code.to_string();
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                });
        });
        if chosen != self.store.settings.translate_to {
            self.store.settings.translate_to = chosen;
            self.store.save_settings();
        }
        let anchor = self.ai_menu_rect;
        let combo_open = egui::Popup::is_any_open(ctx);
        let outside = ctx.input(|i| i.pointer.any_pressed()) && !combo_open && !ctx.pointer_interact_pos().map(|p| area.response.rect.contains(p) || anchor.contains(p)).unwrap_or(false);
        if outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.ai_menu_open = false;
        }
        match act {
            Some("summary") => {
                self.ai_menu_open = false;
                self.summarize_page();
            }
            Some("translate") => {
                self.ai_menu_open = false;
                self.translate_page();
            }
            _ => {}
        }
    }

    pub(super) fn translate_question(&mut self, ctx: &egui::Context, th: &Theme) {
        if !self.translate_ask {
            return;
        }
        // (it is about one tab's page: it goes when that page is left)
        let about_this_page = self.tabs.get(self.active).is_some_and(|t| t.id == self.translate_ask_tab && matches!(t.view, View::Web));
        if !about_this_page {
            self.translate_ask = false;
            return;
        }
        let mut pick: Option<&'static str> = None;
        let mut close = false;
        let screen = ctx.content_rect();
        // no model chosen yet: the first choice leads to choosing one,
        // instead of being kept and failing each time
        let no_ai = self.ai_provider() == Provider::Local;
        let (ai_label, ai_sub) = match self.ai_provider() {
            Provider::Local => (tr("Choisir une IA…").to_string(), tr("Claude, Ollama ou un assistant installé : dans Paramètres › Intelligence")),
            p => (tf("Avec {} — la page est traduite sur place", &[&p.label()]), tr("Seul le texte de la page est envoyé à cette IA")),
        };
        egui::Area::new(egui::Id::new("translate-ask")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - 210.0, screen.top() + 120.0)).show(ctx, |ui| {
            Frame::new()
                .fill(th.page)
                .corner_radius(CornerRadius::same(16))
                .stroke(Stroke::new(1.0, th.hairline.gamma_multiply(0.6)))
                .shadow(egui::Shadow { offset: [0, 10], blur: 34, spread: 0, color: egui::Color32::from_black_alpha(60) })
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(420.0);
                    ui.label(egui::RichText::new(tr("Traduire la page avec…")).size(17.0).family(bold()).color(th.text));
                    ui.add_space(12.0);
                    let mut option = |ui: &mut Ui, key: &'static str, title: &str, sub: &str| {
                        let (r, resp) = ui.allocate_exact_size(vec2(420.0, 52.0), Sense::click());
                        ui.painter().rect_filled(r, CornerRadius::same(10), if resp.hovered() { th.accent.gamma_multiply(0.12) } else { th.card });
                        ui.painter().text(pos2(r.left() + 14.0, r.top() + 17.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(14.0), th.text);
                        ui.painter().text(pos2(r.left() + 14.0, r.top() + 36.0), egui::Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pick = Some(key);
                        }
                        ui.add_space(8.0);
                    };
                    option(ui, if no_ai { "choose" } else { "ai" }, &ai_label, ai_sub);
                    option(ui, "google", "Google Traduction", tr("L'adresse de la page est envoyée à Google"));
                    ui.label(egui::RichText::new(tr("Votre choix est gardé ; il se change dans Paramètres › Intelligence.")).size(11.5).color(th.sub));
                    ui.add_space(6.0);
                    if ui.button(tr("Annuler")).clicked() {
                        close = true;
                    }
                });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if pick == Some("choose") {
            self.translate_ask = false;
            self.open_settings(&super::settings::Section::Intelligence.address());
        } else if let Some(k) = pick {
            self.translate_ask = false;
            self.store.settings.translate_with = k.to_string();
            self.store.save_settings();
            self.translate_page();
        } else if close {
            self.translate_ask = false;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_batches_are_bounded() {
        assert_eq!(super::read_batch("3\n[\"Hello\", \"World\"]"), Some((3, vec!["Hello".to_string(), "World".to_string()])));
        assert_eq!(super::read_batch("x\n[\"a\"]"), None);
        assert_eq!(super::read_batch("0\n[]"), None);
        assert_eq!(super::read_batch("no batch"), None);
        let many = format!("0\n[{}]", vec!["\"a\""; 81].join(","));
        assert_eq!(super::read_batch(&many), None);
        let long = format!("0\n[\"{}\"]", "a".repeat(6001));
        assert_eq!(super::read_batch(&long), None);
    }

    #[test]
    fn reads_translated_lists() {
        let a = super::parse_string_array("Voici :\n[\"Bonjour\", \"le \\\"monde\\\"\" ,\"é\"]\nfin");
        assert_eq!(a, vec!["Bonjour", "le \"monde\"", "é"]);
        assert!(super::parse_string_array("rien").is_empty());
    }
}

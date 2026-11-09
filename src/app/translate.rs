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
  window.__sanagaTr = { nodes: nodes, pos: 0 };
  function next() {
    var s = window.__sanagaTr, out = [], size = 0, from = s.pos;
    while (s.pos < s.nodes.length && out.length < 80 && size < 5000) {
      var t = s.nodes[s.pos].nodeValue.trim();
      out.push(t); size += t.length; s.pos++;
    }
    window.ipc.postMessage(out.length ? 'tr:' + from + '\n' + JSON.stringify(out) : 'tr:done');
  }
  window.__sanagaTrApply = function (from, list) {
    var s = window.__sanagaTr;
    for (var i = 0; i < list.length; i++) {
      var n = s.nodes[from + i];
      if (!n || typeof list[i] !== 'string' || !list[i]) continue;
      var v = n.nodeValue, lead = v.match(/^\s*/)[0], trail = v.match(/\s*$/)[0];
      n.nodeValue = lead + list[i] + trail;
    }
    next();
  };
  next();
})();"#;

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

impl Sanaga {
    /// Menu ⋮ › Translate page: Google or the AI, as chosen (asked once).
    pub(super) fn translate_page(&mut self) {
        match self.store.settings.translate_with.as_str() {
            "google" => self.translate_with_google(),
            "ai" => self.translate_with_ai(),
            _ => self.translate_ask = true,
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
        if self.ai_provider() == Provider::Local {
            self.notice = Some((tr("Choisissez d'abord une IA dans Paramètres › Intelligence.").into(), self.ctx.input(|i| i.time)));
            return;
        }
        let t = &self.tabs[self.active];
        let Some(w) = t.web.as_ref().filter(|_| matches!(t.view, View::Web)) else {
            self.notice = Some((tr("La traduction marche sur les pages des sites.").into(), self.ctx.input(|i| i.time)));
            return;
        };
        let _ = w.evaluate_script(TRANSLATE_JS);
        self.translating = Some(t.id);
        self.notice = Some((tr("Traduction de la page…").into(), self.ctx.input(|i| i.time)));
    }

    /// A batch of texts from the page (`from` + JSON array), or the end.
    pub(super) fn translation_batch(&mut self, tab: u64, msg: &str) {
        if self.translating != Some(tab) {
            return;
        }
        if msg == "done" {
            self.translating = None;
            self.notice = Some((tr("Page traduite.").into(), self.ctx.input(|i| i.time)));
            return;
        }
        let Some((from, json)) = msg.split_once('\n') else { return };
        let from: usize = from.parse().unwrap_or(0);
        let texts = parse_string_array(json);
        let provider = self.ai_provider();
        let tx = self.ai_tx.clone();
        let repaint = self.ctx.clone();
        let lang = target(&self.store.settings.translate_to).1;
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
                let _ = tx.send(super::intelligence::AiDone::Translation { tab, from, result: out });
                repaint.request_repaint();
            })
            .ok();
    }

    /// Put a translated batch in the page (it then sends the next one).
    pub(super) fn translation_done(&mut self, tab: u64, from: usize, result: Result<Vec<String>, String>) {
        match result {
            Ok(list) => {
                if let Some(w) = self.tabs.iter().find(|t| t.id == tab).and_then(|t| t.web.as_ref()) {
                    let _ = w.evaluate_script(&format!("window.__sanagaTrApply && window.__sanagaTrApply({from}, {});", ai::json_array(&list)));
                }
            }
            Err(e) => {
                self.translating = None;
                self.notice = Some((e, self.ctx.input(|i| i.time)));
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
        let w = 300.0;
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
                    let mut row = |ui: &mut Ui, icon: crate::icons::Icon, title: &str, sub: &str, key: &'static str| {
                        let (r, resp) = ui.allocate_exact_size(vec2(w - 12.0, 48.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(9), th.accent.gamma_multiply(0.14));
                        }
                        crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(16.0, 16.0)), icon, th.accent);
                        ui.painter().text(pos2(r.left() + 40.0, r.top() + 16.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(14.0), th.text);
                        ui.painter().text(pos2(r.left() + 40.0, r.top() + 33.0), egui::Align2::LEFT_CENTER, sub, FontId::proportional(11.5), th.sub);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act = Some(key);
                        }
                    };
                    row(ui, crate::icons::Icon::Sparkle, tr("Résumer la page"), tr("Les points essentiels, à côté de la page — Ctrl+Maj+Y"), "summary");
                    let sub = if engine.is_empty() { tf("En {}", &[&lang.2]) } else { tf("En {} · avec {}", &[&lang.2, &engine]) };
                    row(ui, crate::icons::Icon::Mail, tr("Traduire la page"), &sub, "translate");
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
        let mut pick: Option<&'static str> = None;
        let mut close = false;
        let screen = ctx.content_rect();
        let ai_label = match self.ai_provider() {
            Provider::Local => tr("Une IA (à choisir dans Paramètres › Intelligence)").to_string(),
            p => tf("Avec {} — la page est traduite sur place", &[&p.label()]),
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
                    option(ui, "ai", &ai_label, tr("Seul le texte de la page est envoyé à cette IA"));
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
        if let Some(k) = pick {
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
    fn reads_translated_lists() {
        let a = super::parse_string_array("Voici :\n[\"Bonjour\", \"le \\\"monde\\\"\" ,\"é\"]\nfin");
        assert_eq!(a, vec!["Bonjour", "le \"monde\"", "é"]);
        assert!(super::parse_string_array("rien").is_empty());
    }
}

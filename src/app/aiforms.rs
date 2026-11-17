//! The assistant and forms: it can read which fields the page shown asks
//! for, and propose values for them. Nothing is typed into a page before
//! the person has seen each value next to the field's name and accepted.
//!
//! What it never touches: password fields, card numbers and codes, hidden
//! and file fields, buttons — they are not even listed to the model — and
//! it never sends a form: the person does that themselves.
//!
//! Once a field is filled the page can read it, like anything typed there:
//! the question the person is asked says so.

use super::assistant::ToolAsk;
use super::*;
use crate::chat::{Call, ToolResult};
use serde_json::{json, Value};

/// Lists the fields of the page that can be filled (as JSON), and keeps
/// them, in Sanaga's own world, for `FILL_JS`. A field is known afterwards
/// by its place in that list.
const FORM_JS: &str = r#"(function(){
  var KINDS = ['text','email','tel','url','number','search','date','time','datetime-local','month','week','checkbox','radio',''];
  function secret(el){ return /(^|\s)(cc-|current-password|new-password|one-time-code)/.test((el.getAttribute('autocomplete') || '').toLowerCase()); }
  function label(el){
    var t = '';
    if (el.labels && el.labels.length) t = el.labels[0].innerText || '';
    if (!t) t = el.getAttribute('aria-label') || '';
    if (!t && el.getAttribute('aria-labelledby')) { var l = document.getElementById(el.getAttribute('aria-labelledby').split(' ')[0]); if (l) t = l.innerText || ''; }
    if (!t) t = el.placeholder || el.name || el.id || '';
    return t.replace(/\s+/g, ' ').trim().slice(0, 120);
  }
  var kept = [], out = [];
  var all = document.querySelectorAll('input, textarea, select');
  for (var i = 0; i < all.length && kept.length < 80; i++) {
    var el = all[i], tag = el.tagName, type = (el.getAttribute('type') || '').toLowerCase();
    if (el.disabled || el.readOnly || el.offsetParent === null || secret(el)) continue;
    if (tag === 'INPUT' && KINDS.indexOf(type) < 0) continue;
    var item = { n: kept.length, label: label(el), kind: tag === 'INPUT' ? (type || 'text') : tag.toLowerCase() };
    if (tag === 'SELECT') {
      item.options = [];
      for (var k = 0; k < el.options.length && k < 40; k++) item.options.push(el.options[k].text.replace(/\s+/g, ' ').trim().slice(0, 60));
      item.value = el.selectedIndex >= 0 ? item.options[el.selectedIndex] || '' : '';
    } else if (type === 'checkbox' || type === 'radio') {
      item.value = el.checked ? 'yes' : 'no';
      if (type === 'radio') item.choice = (el.value || '').slice(0, 60);
    } else {
      item.value = (el.value || '').slice(0, 80);
    }
    kept.push(el);
    out.push(item);
  }
  window.__sanagaForm = kept;
  return JSON.stringify(out);
})()"#;

/// Fills the fields given (their place in the list `FORM_JS` kept, and a
/// value); answers how many were filled. A field that has become a
/// password field, or left the page, since it was listed is passed over.
const FILL_JS: &str = r#"(function(list){
  var KINDS = ['text','email','tel','url','number','search','date','time','datetime-local','month','week','checkbox','radio',''];
  function secret(el){ return /(^|\s)(cc-|current-password|new-password|one-time-code)/.test((el.getAttribute('autocomplete') || '').toLowerCase()); }
  var fields = window.__sanagaForm || [], done = 0;
  function put(el, v){
    var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    var d = Object.getOwnPropertyDescriptor(proto, 'value');
    if (d && d.set) d.set.call(el, v); else el.value = v;
  }
  list.forEach(function(item){
    var el = fields[item.n];
    if (!el || !el.isConnected || el.disabled || el.readOnly || secret(el)) return;
    var tag = el.tagName, type = (el.getAttribute('type') || '').toLowerCase(), v = String(item.value);
    if (tag === 'INPUT' && KINDS.indexOf(type) < 0) return;
    if (tag === 'SELECT') {
      var want = v.trim().toLowerCase(), hit = -1;
      for (var k = 0; k < el.options.length; k++) { var o = el.options[k]; if (o.value.toLowerCase() === want || o.text.replace(/\s+/g, ' ').trim().toLowerCase() === want) { hit = k; break; } }
      if (hit < 0) return;
      el.selectedIndex = hit;
    } else if (type === 'checkbox' || type === 'radio') {
      var on = /^(true|yes|oui|si|sí|sim|ja|1|on)$/i.test(v.trim());
      if (type === 'radio' && !on) return;
      el.checked = on;
    } else if (tag === 'INPUT' || tag === 'TEXTAREA') {
      put(el, v);
    } else return;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    done++;
  });
  return String(done);
})"#;

/// A field of the page that can be filled.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Field {
    pub label: String,
    pub kind: String,
}

/// The fields last listed for a model: in which tab, at which address.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FormSeen {
    pub tab: u64,
    pub address: String,
    pub fields: Vec<Field>,
}

/// What the page's script listed, as fields and as the text given to the model.
pub(super) fn read_fields(json: &str) -> Option<(Vec<Field>, String)> {
    let list: Vec<Value> = serde_json::from_str(json).ok()?;
    let (mut fields, mut said) = (Vec::new(), String::new());
    for (n, item) in list.iter().enumerate().take(80) {
        let text = |k: &str| item[k].as_str().unwrap_or("").to_string();
        let (label, kind) = (text("label"), text("kind"));
        said.push_str(&format!("field {n} — {} — {kind}", if label.is_empty() { "(no label)" } else { label.as_str() }));
        if let Some(options) = item["options"].as_array() {
            let names: Vec<&str> = options.iter().filter_map(|o| o.as_str()).collect();
            said.push_str(&format!(" — choices: {}", names.join(" | ")));
        }
        if !text("choice").is_empty() {
            said.push_str(&format!(" — choice: {}", text("choice")));
        }
        if !text("value").is_empty() {
            said.push_str(&format!(" — now: {}", text("value")));
        }
        said.push('\n');
        fields.push(Field { label, kind });
    }
    Some((fields, said))
}

/// The values a model proposes: (the field's place, the value), for the
/// fields that were listed — the others are left out.
pub(super) fn proposed(call: &Call, known: usize) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for item in call.input["fields"].as_array().cloned().unwrap_or_default() {
        let Some(n) = item["n"].as_u64().map(|n| n as usize).filter(|n| *n < known) else { continue };
        let value = match &item["value"] {
            Value::String(s) => s.clone(),
            Value::Bool(b) => if *b { "yes" } else { "no" }.to_string(),
            Value::Number(x) => x.to_string(),
            _ => continue,
        };
        if !out.iter().any(|(had, _)| *had == n) {
            out.push((n, value.chars().take(4_000).collect()));
        }
    }
    out
}

fn answer(ask: ToolAsk, text: impl Into<String>, error: bool) {
    let _ = ask.reply.send(ToolResult { id: ask.call.id.clone(), name: ask.call.name.clone(), text: text.into(), error });
}

impl Sanaga {
    /// The page shown, if a form of it can be read or filled.
    fn form_page(&self) -> Option<(u64, String)> {
        let t = &self.tabs[self.active];
        (t.web.is_some() && matches!(t.view, View::Web)).then(|| (t.id, t.url().to_string()))
    }

    /// A model asks which fields the page shown has.
    pub(super) fn read_form_for_tool(&mut self, ask: ToolAsk) {
        let private = self.assistant.as_ref().is_some_and(|a| a.private);
        let t = &self.tabs[self.active];
        if t.private && !private {
            return answer(ask, "This tab is private: its page is not given.", true);
        }
        let Some((tab, address)) = self.form_page() else { return answer(ask, "The tab shown has no web page.", true) };
        let Some(w) = self.tabs[self.active].web.as_ref() else { return };
        let (tx, repaint) = (self.ai_tx.clone(), self.ctx.clone());
        let cell = std::cell::Cell::new(Some(ask));
        w.ask_own(FORM_JS, move |listed| {
            if let Some(ask) = cell.take() {
                let _ = tx.send(super::intelligence::AiDone::FormRead { tab, address, listed, ask });
                repaint.request_repaint();
            }
        });
    }

    /// The page listed its fields: kept for the filling, and said to the model.
    pub(super) fn form_read(&mut self, tab: u64, address: String, listed: Option<String>, ask: ToolAsk) {
        let Some((fields, said)) = listed.as_deref().and_then(read_fields) else { return answer(ask, "The page did not answer.", true) };
        let Some(a) = self.assistant.as_mut() else { return answer(ask, "Stopped by the person.", true) };
        if fields.is_empty() {
            a.form = None;
            return answer(ask, "This page has no field that can be filled.", false);
        }
        a.form = Some(FormSeen { tab, address, fields });
        answer(ask, format!("{said}\nPassword, payment and hidden fields are not listed and cannot be filled. Nothing is sent: the person sends the form themselves."), false);
    }

    /// What a `fill_form` call would put where, for the person to read —
    /// or why it cannot be done.
    pub(super) fn fill_summary(&self, call: &Call) -> Result<String, String> {
        let Some(seen) = self.assistant.as_ref().and_then(|a| a.form.as_ref()) else { return Err("List the fields first with read_form.".into()) };
        if self.form_page() != Some((seen.tab, seen.address.clone())) {
            return Err("The page has changed since its fields were listed: use read_form again.".into());
        }
        let values = proposed(call, seen.fields.len());
        if values.is_empty() {
            return Err("No known field in this request: use the numbers read_form gives.".into());
        }
        let mut lines = vec![tr("Remplir ces champs de la page (rien n'est envoyé) :").to_string()];
        for (n, value) in &values {
            let f = &seen.fields[*n];
            let name = if f.label.is_empty() { tf("champ n° {}", &[&(n + 1)]) } else { f.label.clone() };
            // (on one line: a value of several lines would read as several fields)
            let shown: String = value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
            lines.push(format!("• {name} ← {shown}{}", if value.chars().count() > 200 { "…" } else { "" }));
        }
        lines.push(tr("Une fois remplis, la page peut lire ces champs.").to_string());
        Ok(lines.join("\n"))
    }

    /// Fill the fields (the person accepted what `fill_summary` showed).
    pub(super) fn fill_form_for_tool(&mut self, ask: ToolAsk) {
        let Some(seen) = self.assistant.as_ref().and_then(|a| a.form.clone()) else { return answer(ask, "List the fields first with read_form.", true) };
        // (the page may have moved on while the person read the question)
        if self.form_page() != Some((seen.tab, seen.address.clone())) {
            return answer(ask, "The page has changed since its fields were listed: nothing was filled.", true);
        }
        let values = proposed(&ask.call, seen.fields.len());
        let list: Vec<Value> = values.iter().map(|(n, v)| json!({"n": n, "value": v})).collect();
        let Some(w) = self.tabs[self.active].web.as_ref() else { return answer(ask, "The tab shown has no web page.", true) };
        let (tx, repaint) = (self.ai_tx.clone(), self.ctx.clone());
        let (cell, wanted) = (std::cell::Cell::new(Some(ask)), values.len());
        w.ask_own(&format!("{FILL_JS}({})", Value::Array(list)), move |done| {
            if let Some(ask) = cell.take() {
                let filled = done.and_then(|d| d.trim().parse::<usize>().ok()).unwrap_or(0);
                let _ = tx.send(super::intelligence::AiDone::FormFilled { filled, wanted, ask });
                repaint.request_repaint();
            }
        });
    }

    pub(super) fn form_filled(&mut self, filled: usize, wanted: usize, ask: ToolAsk) {
        let said = if filled == wanted {
            format!("{filled} field(s) filled. Nothing was sent: tell the person to check the form and send it themselves.")
        } else {
            format!("{filled} of {wanted} field(s) filled; the others could not be (a choice that does not exist, or a field that changed). Nothing was sent.")
        };
        answer(ask, said, filled == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pages_fields_are_said_to_the_model() {
        let (fields, said) = read_fields(
            r#"[{"n":0,"label":"Nom","kind":"text","value":""},
                {"n":1,"label":"Pays","kind":"select","options":["Cameroun","Gabon"],"value":"Gabon"},
                {"n":2,"label":"","kind":"checkbox","value":"no"},
                {"n":3,"label":"Livraison","kind":"radio","choice":"express","value":"no"}]"#,
        )
        .unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[1], Field { label: "Pays".into(), kind: "select".into() });
        assert_eq!(said.lines().next(), Some("field 0 — Nom — text"));
        assert!(said.contains("field 1 — Pays — select — choices: Cameroun | Gabon — now: Gabon"));
        assert!(said.contains("field 2 — (no label) — checkbox — now: no"));
        assert!(said.contains("field 3 — Livraison — radio — choice: express"));
        assert_eq!(read_fields("[]").map(|f| f.0.len()), Some(0));
        assert!(read_fields("undefined").is_none());
        crate::mutate::run(&[br#"[{"n":0,"label":"Nom","kind":"select","options":["a",1,null],"value":"x","choice":"y"}]"#], 5_000, |d| {
            let _ = read_fields(&String::from_utf8_lossy(d));
        });
    }

    #[test]
    fn only_listed_fields_get_a_value() {
        let call = |input: Value| Call { id: "1".into(), name: "fill_form".into(), input };
        let got = proposed(
            &call(
                json!({"fields": [{"n": 0, "value": "Ada"}, {"n": 2, "value": true}, {"n": 1, "value": 3}, {"n": 9, "value": "x"}, {"n": 0, "value": "twice"}, {"n": -1, "value": "x"}, {"value": "x"}, {"n": 1}, "x"]}),
            ),
            3,
        );
        assert_eq!(got, vec![(0, "Ada".to_string()), (2, "yes".to_string()), (1, "3".to_string())]);
        assert!(proposed(&call(json!({"fields": "all"})), 3).is_empty());
        assert!(proposed(&call(json!({})), 3).is_empty());
        assert!(proposed(&call(json!({"fields": [{"n": 0, "value": "x"}]})), 0).is_empty());
    }

    #[test]
    fn the_scripts_never_list_nor_fill_secrets() {
        // (what they read and write is tried in the real window, with
        // tests/form.html; here: the kinds they take are the same, and no
        // secret kind is among them)
        for js in [FORM_JS, FILL_JS] {
            assert!(js.contains("'text','email','tel','url','number','search','date','time','datetime-local','month','week','checkbox','radio',''"));
            for kind in ["'password'", "'hidden'", "'file'", "'submit'"] {
                assert!(!js.contains(kind), "{kind}");
            }
            assert!(js.contains("cc-|current-password|new-password|one-time-code"));
            assert!(!js.contains("submit()") && !js.contains(".click("));
        }
    }
}

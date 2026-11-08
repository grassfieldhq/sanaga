//! Sanaga's light page engine: turns HTML into a flat list of simple blocks
//! (headings, paragraphs, lists, images, form fields). No JavaScript, no CSS
//! layout — which is what keeps pages fast and memory use small.

use std::collections::HashMap;
use url::Url;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    /// Index into `Document::links`.
    pub link: Option<u32>,
}

#[derive(Clone, Debug)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    ListItem { depth: u8, marker: String, spans: Vec<Span> },
    Quote(Vec<Span>),
    Pre(String),
    Rule,
    Image { src: String, alt: String, link: Option<u32>, width: Option<f32> },
    Input { form: usize, field: usize },
    Button { form: usize, label: String, name: String, value: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Text,
    Password,
    Hidden,
    Checkbox,
    TextArea,
    Select(Vec<(String, String)>),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub value: String,
    pub checked: bool,
    pub placeholder: String,
}

#[derive(Clone, Debug)]
pub struct Form {
    pub action: String,
    pub post: bool,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub url: String,
    pub title: String,
    pub blocks: Vec<Block>,
    pub links: Vec<String>,
    pub forms: Vec<Form>,
    /// element id / anchor name -> block index
    pub anchors: HashMap<String, usize>,
    pub bytes: usize,
    /// True when only the main content was kept (menus and sidebars left out).
    pub trimmed: bool,
}

impl Document {
    pub fn plain_text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }
}

// ---------------------------------------------------------------- tokenizer

#[derive(Debug)]
enum Token {
    Start { name: String, attrs: Vec<(String, String)>, self_closing: bool },
    End(String),
    Text(String),
}

fn find_ci(hay: &str, from: usize, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || from >= h.len() {
        return None;
    }
    let mut i = from;
    while i + n.len() <= h.len() {
        if h[i..i + n.len()].eq_ignore_ascii_case(n) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn tokenize(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        // flush text
        let flush = |out: &mut Vec<Token>, a: usize, z: usize| {
            if z > a {
                out.push(Token::Text(src[a..z].to_string()));
            }
        };
        if src[i..].starts_with("<!--") {
            flush(&mut out, text_start, i);
            let end = src[i + 4..].find("-->").map(|e| i + 4 + e + 3).unwrap_or(b.len());
            i = end;
            text_start = i;
            continue;
        }
        if i + 1 < b.len() && (b[i + 1] == b'!' || b[i + 1] == b'?') {
            flush(&mut out, text_start, i);
            let end = src[i..].find('>').map(|e| i + e + 1).unwrap_or(b.len());
            i = end;
            text_start = i;
            continue;
        }
        let closing = i + 1 < b.len() && b[i + 1] == b'/';
        let name_start = if closing { i + 2 } else { i + 1 };
        if name_start >= b.len() || !b[name_start].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        flush(&mut out, text_start, i);
        let mut j = name_start;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'-' || b[j] == b':') {
            j += 1;
        }
        let name = src[name_start..j].to_ascii_lowercase();
        // attributes
        let mut attrs = Vec::new();
        let mut self_closing = false;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            if b[j] == b'/' {
                if j + 1 < b.len() && b[j + 1] == b'>' {
                    self_closing = true;
                    j += 2;
                    break;
                }
                j += 1;
                continue;
            }
            let an = j;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'=' && b[j] != b'>' && !(b[j] == b'/' && j + 1 < b.len() && b[j + 1] == b'>') {
                j += 1;
            }
            let aname = src[an..j].to_ascii_lowercase();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut val = String::new();
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    let vs = j + 1;
                    let mut k = vs;
                    while k < b.len() && b[k] != q {
                        k += 1;
                    }
                    val = decode_entities(&src[vs..k.min(b.len())]);
                    j = (k + 1).min(b.len());
                } else {
                    let vs = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    val = decode_entities(&src[vs..j]);
                }
            }
            if !aname.is_empty() {
                attrs.push((aname, val));
            }
        }
        i = j;
        text_start = i;
        if closing {
            out.push(Token::End(name));
            continue;
        }
        let raw = matches!(name.as_str(), "script" | "style" | "textarea" | "title" | "xmp");
        out.push(Token::Start { name: name.clone(), attrs, self_closing });
        if raw && !self_closing {
            let end = find_ci(src, i, &format!("</{name}")).unwrap_or(b.len());
            if end > i {
                out.push(Token::Text(src[i..end].to_string()));
            }
            let close = src[end..].find('>').map(|e| end + e + 1).unwrap_or(b.len());
            out.push(Token::End(name));
            i = close;
            text_start = i;
        }
    }
    if text_start < b.len() {
        out.push(Token::Text(src[text_start..].to_string()));
    }
    out
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace() || c == '<').map(|e| e + 1);
        let (ent, has_semi) = match end {
            Some(e) if e <= 32 && rest.as_bytes().get(e) == Some(&b';') => (&rest[1..e], true),
            Some(e) if e <= 32 => (&rest[1..e], false),
            None if rest.len() <= 32 => (&rest[1..], false),
            _ => ("", false),
        };
        let decoded: Option<String> = if let Some(num) = ent.strip_prefix('#') {
            let n = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            n.and_then(char::from_u32).map(|c| c.to_string())
        } else {
            named_entity(ent).map(|c| c.to_string())
        };
        match decoded {
            Some(d) => {
                out.push_str(&d);
                rest = &rest[1 + ent.len() + usize::from(has_semi)..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn named_entity(n: &str) -> Option<&'static str> {
    Some(match n {
        "amp" => "&", "lt" => "<", "gt" => ">", "quot" => "\"", "apos" => "'",
        "nbsp" => "\u{a0}", "copy" => "©", "reg" => "®", "trade" => "™",
        "mdash" => "—", "ndash" => "–", "hellip" => "…", "middot" => "·", "bull" => "•",
        "laquo" => "«", "raquo" => "»", "lsquo" => "‘", "rsquo" => "’", "ldquo" => "“", "rdquo" => "”",
        "sbquo" => "‚", "bdquo" => "„", "deg" => "°", "euro" => "€", "pound" => "£", "yen" => "¥", "cent" => "¢",
        "times" => "×", "divide" => "÷", "plusmn" => "±", "para" => "¶", "sect" => "§", "frac12" => "½",
        "frac14" => "¼", "frac34" => "¾", "sup2" => "²", "sup3" => "³", "micro" => "µ", "shy" => "",
        "zwj" => "", "zwnj" => "", "thinsp" => " ", "ensp" => " ", "emsp" => " ", "larr" => "←", "rarr" => "→",
        "uarr" => "↑", "darr" => "↓", "harr" => "↔", "hearts" => "♥", "check" => "✓", "dagger" => "†",
        "aacute" => "á", "agrave" => "à", "acirc" => "â", "auml" => "ä", "atilde" => "ã", "aring" => "å", "aelig" => "æ",
        "ccedil" => "ç", "eacute" => "é", "egrave" => "è", "ecirc" => "ê", "euml" => "ë",
        "iacute" => "í", "igrave" => "ì", "icirc" => "î", "iuml" => "ï", "ntilde" => "ñ",
        "oacute" => "ó", "ograve" => "ò", "ocirc" => "ô", "ouml" => "ö", "otilde" => "õ", "oslash" => "ø", "oelig" => "œ",
        "uacute" => "ú", "ugrave" => "ù", "ucirc" => "û", "uuml" => "ü", "yacute" => "ý", "yuml" => "ÿ", "szlig" => "ß",
        "Aacute" => "Á", "Agrave" => "À", "Acirc" => "Â", "Auml" => "Ä", "Ccedil" => "Ç", "Eacute" => "É",
        "Egrave" => "È", "Ecirc" => "Ê", "Euml" => "Ë", "Icirc" => "Î", "Iuml" => "Ï", "Ocirc" => "Ô", "Ouml" => "Ö",
        "OElig" => "Œ", "Ucirc" => "Û", "Ugrave" => "Ù", "Uuml" => "Ü", "Ntilde" => "Ñ",
        _ => return None,
    })
}

// ---------------------------------------------------------------- builder

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];
const SKIP: &[&str] = &[
    "script", "style", "head", "svg", "template", "iframe", "canvas", "object", "math", "map", "dialog",
];
const BLOCKISH: &[&str] = &[
    "p", "div", "section", "article", "header", "footer", "nav", "main", "aside", "figure", "figcaption",
    "table", "tbody", "thead", "tfoot", "tr", "dl", "dt", "dd", "address", "details", "summary", "center",
    "fieldset", "legend", "form", "caption", "ul", "ol", "menu", "body", "html", "hgroup", "search",
];

struct Builder {
    base: Option<Url>,
    doc: Document,
    spans: Vec<Span>,
    bold: u32,
    italic: u32,
    code: u32,
    links: Vec<Option<u32>>,
    pre: u32,
    pre_buf: String,
    heading: u8,
    quote: u32,
    lists: Vec<(bool, u32)>,
    li_marker: Option<String>,
    in_li: u32,
    skip: Option<(String, u32)>,
    in_title: bool,
    form: Option<usize>,
    /// inside a real <form> tag (not a form made up for stray fields)
    in_form_tag: bool,
    select: Option<(usize, usize)>,
    option: Option<(String, String)>,
    textarea: Option<(usize, usize)>,
    button: Option<(usize, String, String, String)>,
    last_space: bool,
    cell_count: u32,
    in_cell: u32,
    tables: Vec<u32>,
    main: Option<(String, u32)>,
    main_range: Option<(usize, Option<usize>)>,
}

impl Builder {
    fn resolve(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() || href.starts_with("javascript:") {
            return None;
        }
        match &self.base {
            Some(b) => b.join(href).ok().map(|u| u.to_string()),
            None => Url::parse(href).ok().map(|u| u.to_string()),
        }
    }

    fn cur_link(&self) -> Option<u32> {
        self.links.last().copied().flatten()
    }

    fn push_text(&mut self, raw: &str) {
        if self.in_title {
            self.doc.title.push_str(&decode_entities(raw));
            return;
        }
        if let Some((f, fi)) = self.textarea {
            self.doc.forms[f].fields[fi].value.push_str(&decode_entities(raw));
            return;
        }
        if self.option.is_some() {
            let t = decode_entities(raw);
            if let Some((_, l)) = self.option.as_mut() {
                l.push_str(&t);
            }
            return;
        }
        if self.select.is_some() {
            return;
        }
        if let Some((_, _, _, l)) = self.button.as_mut() {
            l.push_str(&decode_entities(raw));
            return;
        }
        if self.pre > 0 {
            self.pre_buf.push_str(&decode_entities(raw));
            return;
        }
        let text = decode_entities(raw);
        let mut norm = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch.is_whitespace() && ch != '\u{a0}' {
                if !self.last_space {
                    norm.push(' ');
                    self.last_space = true;
                }
            } else {
                norm.push(ch);
                self.last_space = false;
            }
        }
        if norm.is_empty() || (norm == " " && self.spans.is_empty()) {
            return;
        }
        if self.spans.is_empty() {
            norm = norm.trim_start().to_string();
            if norm.is_empty() {
                return;
            }
        }
        let span = Span {
            text: String::new(),
            bold: self.bold > 0 || self.heading > 0,
            italic: self.italic > 0,
            code: self.code > 0,
            link: self.cur_link(),
        };
        if let Some(last) = self.spans.last_mut() {
            if last.bold == span.bold && last.italic == span.italic && last.code == span.code && last.link == span.link {
                last.text.push_str(&norm);
                return;
            }
        }
        self.spans.push(Span { text: norm, ..span });
    }

    fn flush(&mut self) {
        self.last_space = true;
        if self.spans.is_empty() {
            return;
        }
        let mut spans = std::mem::take(&mut self.spans);
        if let Some(last) = spans.last_mut() {
            let t = last.text.trim_end().to_string();
            last.text = t;
        }
        spans.retain(|s| !s.text.is_empty());
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            return;
        }
        let block = if self.heading > 0 {
            Block::Heading(self.heading, spans)
        } else if self.in_li > 0 {
            let marker = self.li_marker.take().unwrap_or_default();
            Block::ListItem { depth: self.lists.len().clamp(1, 6) as u8, marker, spans }
        } else if self.quote > 0 {
            Block::Quote(spans)
        } else {
            Block::Para(spans)
        };
        self.doc.blocks.push(block);
    }

    fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
        attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn hidden(attrs: &[(String, String)]) -> bool {
        for (k, v) in attrs {
            match k.as_str() {
                "hidden" => return true,
                "aria-hidden" if v.eq_ignore_ascii_case("true") => return true,
                "style" => {
                    let s: String = v.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
                    if s.contains("display:none") || s.contains("visibility:hidden") {
                        return true;
                    }
                }
                "class" => {
                    for c in v.split_whitespace() {
                        // screen-reader-only text and drop-down menus that are folded by default
                        if matches!(
                            c,
                            "sr-only" | "visually-hidden" | "screen-reader-text" | "mw-jump-link" | "hidden" | "noprint" | "dropdown-menu"
                                | "dropdown-content" | "vector-dropdown-content" | "mw-portlet-lang" | "skip-link"
                        ) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn ensure_form(&mut self) -> usize {
        if let Some(f) = self.form {
            return f;
        }
        let action = self.doc.url.clone();
        self.doc.forms.push(Form { action, post: false, fields: Vec::new() });
        let f = self.doc.forms.len() - 1;
        self.form = Some(f);
        f
    }

    fn start(&mut self, name: &str, attrs: Vec<(String, String)>, self_closing: bool) {
        if let Some((tag, depth)) = &mut self.skip {
            if tag == name && !VOID.contains(&name) && !self_closing {
                *depth += 1;
            }
            return;
        }
        if name == "title" && !self_closing {
            if self.doc.title.is_empty() {
                self.in_title = true;
            } else {
                self.skip = Some(("title".into(), 1));
            }
            return;
        }
        if name == "base" {
            if let Some(h) = Self::attr(&attrs, "href") {
                if let Some(u) = self.resolve(h) {
                    self.base = Url::parse(&u).ok();
                }
            }
            return;
        }
        if SKIP.contains(&name) || (Self::hidden(&attrs) && name != "input") {
            if !VOID.contains(&name) && !self_closing {
                self.skip = Some((name.to_string(), 1));
            }
            return;
        }
        if let Some((tag, depth)) = &mut self.main {
            if tag == name && !self_closing {
                *depth += 1;
            }
        } else if self.main_range.is_none() && !self_closing && (name == "main" || Self::attr(&attrs, "role") == Some("main")) {
            self.flush();
            self.main = Some((name.to_string(), 1));
            self.main_range = Some((self.doc.blocks.len(), None));
        }
        if let Some(id) = Self::attr(&attrs, "id").or_else(|| if name == "a" { Self::attr(&attrs, "name") } else { None }) {
            let idx = self.doc.blocks.len();
            self.doc.anchors.entry(id.to_string()).or_insert(idx);
        }
        match name {
            "b" | "strong" | "th" => self.bold += 1,
            "i" | "em" | "cite" | "var" | "dfn" => self.italic += 1,
            "code" | "kbd" | "samp" | "tt" => self.code += 1,
            "a" => {
                let link = Self::attr(&attrs, "href").and_then(|h| self.resolve(h)).map(|u| {
                    self.doc.links.push(u);
                    (self.doc.links.len() - 1) as u32
                });
                if self_closing {
                    return;
                }
                self.links.push(link);
                return;
            }
            _ => {}
        }
        match name {
            "br" => {
                if self.pre > 0 {
                    self.pre_buf.push('\n');
                } else if let Some(last) = self.spans.last_mut() {
                    last.text.push('\n');
                    self.last_space = true;
                }
            }
            "hr" => {
                self.flush();
                self.doc.blocks.push(Block::Rule);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.heading = name.as_bytes()[1] - b'0';
            }
            "pre" | "listing" | "plaintext" => {
                self.flush();
                self.pre += 1;
            }
            "blockquote" => {
                self.flush();
                self.quote += 1;
            }
            "ul" | "ol" | "menu" => {
                self.flush();
                let start = Self::attr(&attrs, "start").and_then(|s| s.parse().ok()).unwrap_or(1);
                self.lists.push((name == "ol", start));
            }
            "li" => {
                self.flush();
                self.in_li += 1;
                let marker = match self.lists.last_mut() {
                    Some((true, n)) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    Some((false, _)) => match self.lists.len() {
                        1 => "•".into(),
                        2 => "◦".into(),
                        _ => "▪".into(),
                    },
                    None => "•".into(),
                };
                self.li_marker = Some(marker);
            }
            "tr" => {
                self.flush();
                self.cell_count = 0;
                self.in_cell = self.tables.last().copied().unwrap_or(0);
            }
            "table" => {
                self.flush();
                self.cell_count = 0;
                self.tables.push(self.in_cell);
            }
            "td" | "th" => {
                if self.cell_count > 0 && !self.spans.is_empty() {
                    self.push_sep();
                }
                self.cell_count += 1;
                self.in_cell = self.tables.last().copied().unwrap_or(0) + 1;
            }
            "img" => {
                let src = Self::attr(&attrs, "src")
                    .filter(|s| !s.starts_with("data:"))
                    .or_else(|| Self::attr(&attrs, "data-src"))
                    .and_then(|s| self.resolve(s));
                let width = Self::attr(&attrs, "width").and_then(|w| w.trim_end_matches("px").parse::<f32>().ok());
                let alt = Self::attr(&attrs, "alt").unwrap_or("").to_string();
                if let Some(src) = src {
                    if width.map(|w| w > 4.0).unwrap_or(true) {
                        self.flush();
                        let link = self.cur_link();
                        self.doc.blocks.push(Block::Image { src, alt, link, width });
                    }
                }
            }
            "form" => {
                self.flush();
                let action = Self::attr(&attrs, "action").and_then(|a| self.resolve(a)).unwrap_or_else(|| self.doc.url.clone());
                let post = Self::attr(&attrs, "method").map(|m| m.eq_ignore_ascii_case("post")).unwrap_or(false);
                self.doc.forms.push(Form { action, post, fields: Vec::new() });
                self.form = Some(self.doc.forms.len() - 1);
                self.in_form_tag = true;
            }
            "input" => {
                let ty = Self::attr(&attrs, "type").unwrap_or("text").to_ascii_lowercase();
                let fname = Self::attr(&attrs, "name").unwrap_or("").to_string();
                let value = Self::attr(&attrs, "value").unwrap_or("").to_string();
                let f = self.ensure_form();
                match ty.as_str() {
                    "submit" | "image" | "button" => {
                        if ty == "button" {
                            return;
                        }
                        self.flush();
                        let label = if value.is_empty() { Self::attr(&attrs, "alt").unwrap_or("Envoyer").to_string() } else { value.clone() };
                        self.doc.blocks.push(Block::Button { form: f, label, name: fname, value });
                    }
                    "reset" | "file" => {}
                    _ if fname.is_empty() => {}
                    _ => {
                        let kind = match ty.as_str() {
                            "hidden" => FieldKind::Hidden,
                            "password" => FieldKind::Password,
                            "checkbox" | "radio" => FieldKind::Checkbox,
                            _ => FieldKind::Text,
                        };
                        let hidden_attr = Self::hidden(&attrs);
                        let checked = Self::attr(&attrs, "checked").is_some();
                        let kind = if hidden_attr && kind != FieldKind::Hidden { FieldKind::Hidden } else { kind };
                        if ty == "radio" && !checked {
                            return;
                        }
                        let kind = if ty == "radio" { FieldKind::Hidden } else { kind };
                        let is_hidden = kind == FieldKind::Hidden;
                        let placeholder = Self::attr(&attrs, "placeholder").or_else(|| Self::attr(&attrs, "aria-label")).unwrap_or("").to_string();
                        let value = if ty == "checkbox" && value.is_empty() { "on".to_string() } else { value };
                        self.doc.forms[f].fields.push(Field { name: fname, kind, value, checked, placeholder });
                        if !is_hidden {
                            self.flush();
                            let fi = self.doc.forms[f].fields.len() - 1;
                            self.doc.blocks.push(Block::Input { form: f, field: fi });
                        }
                    }
                }
            }
            "textarea" => {
                let f = self.ensure_form();
                let fname = Self::attr(&attrs, "name").unwrap_or("").to_string();
                let placeholder = Self::attr(&attrs, "placeholder").unwrap_or("").to_string();
                self.doc.forms[f].fields.push(Field { name: fname, kind: FieldKind::TextArea, value: String::new(), checked: false, placeholder });
                let fi = self.doc.forms[f].fields.len() - 1;
                self.flush();
                self.doc.blocks.push(Block::Input { form: f, field: fi });
                self.textarea = Some((f, fi));
            }
            "select" => {
                let f = self.ensure_form();
                let fname = Self::attr(&attrs, "name").unwrap_or("").to_string();
                self.doc.forms[f].fields.push(Field { name: fname, kind: FieldKind::Select(Vec::new()), value: String::new(), checked: false, placeholder: String::new() });
                let fi = self.doc.forms[f].fields.len() - 1;
                self.flush();
                self.doc.blocks.push(Block::Input { form: f, field: fi });
                self.select = Some((f, fi));
            }
            "option" => {
                if let Some((f, fi)) = self.select {
                    self.finish_option();
                    let v = Self::attr(&attrs, "value").map(|s| s.to_string());
                    if Self::attr(&attrs, "selected").is_some() {
                        self.doc.forms[f].fields[fi].checked = true; // marks "has explicit selection"
                        self.doc.forms[f].fields[fi].placeholder = v.clone().unwrap_or_else(|| "\u{0}".into());
                    }
                    self.option = Some((v.unwrap_or_else(|| "\u{0}".into()), String::new()));
                }
            }
            "button" => {
                let ty = Self::attr(&attrs, "type").unwrap_or("submit").to_ascii_lowercase();
                // a button outside any form only works with JavaScript: leave it out
                if ty == "submit" && self.in_form_tag {
                    let f = self.ensure_form();
                    self.flush();
                    let fname = Self::attr(&attrs, "name").unwrap_or("").to_string();
                    let value = Self::attr(&attrs, "value").unwrap_or("").to_string();
                    self.button = Some((f, fname, value, String::new()));
                } else {
                    self.skip = Some(("button".into(), 1));
                }
            }
            _ => {
                if BLOCKISH.contains(&name) {
                    self.block_break(name);
                }
            }
        }
    }

    /// Block-level tags start a new paragraph, except inside table cells
    /// (old table layouts) where they just separate words.
    fn block_break(&mut self, name: &str) {
        if self.in_cell > 0 && !matches!(name, "table" | "tbody" | "thead" | "tfoot" | "ul" | "ol" | "form") {
            if !self.last_space && !self.spans.is_empty() {
                self.push_text(" ");
            }
        } else {
            self.flush();
        }
    }

    fn push_sep(&mut self) {
        if self.spans.last().map(|s| s.text == "  ·  ").unwrap_or(true) {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            let t = last.text.trim_end().to_string();
            last.text = t;
        }
        self.spans.push(Span { text: "  ·  ".into(), ..Default::default() });
        self.last_space = true;
    }

    fn finish_option(&mut self) {
        if let (Some((f, fi)), Some((v, l))) = (self.select, self.option.take()) {
            let label = l.split_whitespace().collect::<Vec<_>>().join(" ");
            let value = if v == "\u{0}" { label.clone() } else { v };
            let field = &mut self.doc.forms[f].fields[fi];
            if field.placeholder == "\u{0}" {
                field.placeholder = value.clone();
            }
            if let FieldKind::Select(opts) = &mut field.kind {
                opts.push((value, label));
            }
        }
    }

    fn end(&mut self, name: &str) {
        if let Some((tag, depth)) = &mut self.skip {
            if tag == name {
                *depth -= 1;
                if *depth == 0 {
                    self.skip = None;
                }
            }
            return;
        }
        if let Some((tag, depth)) = &mut self.main {
            if tag == name {
                *depth -= 1;
                if *depth == 0 {
                    self.main = None;
                    self.flush();
                    if let Some((_, end)) = &mut self.main_range {
                        *end = Some(self.doc.blocks.len());
                    }
                }
            }
        }
        match name {
            "title" => self.in_title = false,
            "b" | "strong" => self.bold = self.bold.saturating_sub(1),
            "th" => {
                self.bold = self.bold.saturating_sub(1);
                self.in_cell = self.tables.last().copied().unwrap_or(0);
            }
            "td" => self.in_cell = self.tables.last().copied().unwrap_or(0),
            "i" | "em" | "cite" | "var" | "dfn" => self.italic = self.italic.saturating_sub(1),
            "code" | "kbd" | "samp" | "tt" => self.code = self.code.saturating_sub(1),
            "a" => {
                self.links.pop();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.heading = 0;
            }
            "pre" | "listing" | "plaintext" => {
                self.pre = self.pre.saturating_sub(1);
                if self.pre == 0 {
                    let text = std::mem::take(&mut self.pre_buf);
                    let text = text.strip_prefix('\n').unwrap_or(&text).trim_end().to_string();
                    if !text.is_empty() {
                        self.flush();
                        self.doc.blocks.push(Block::Pre(text));
                    }
                }
            }
            "blockquote" => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            "ul" | "ol" | "menu" => {
                self.flush();
                self.lists.pop();
            }
            "li" => {
                self.flush();
                self.in_li = self.in_li.saturating_sub(1);
                self.li_marker = None;
            }
            "form" => {
                self.flush();
                self.form = None;
                self.in_form_tag = false;
            }
            "textarea" => self.textarea = None,
            "option" => self.finish_option(),
            "select" => {
                self.finish_option();
                if let Some((f, fi)) = self.select.take() {
                    let field = &mut self.doc.forms[f].fields[fi];
                    if let FieldKind::Select(opts) = &field.kind {
                        field.value = if field.checked && field.placeholder != "\u{0}" {
                            field.placeholder.clone()
                        } else {
                            opts.first().map(|o| o.0.clone()).unwrap_or_default()
                        };
                    }
                    field.checked = false;
                    field.placeholder.clear();
                }
            }
            "button" => {
                if let Some((f, n, v, l)) = self.button.take() {
                    let label = l.split_whitespace().collect::<Vec<_>>().join(" ");
                    let label = if label.is_empty() { "Envoyer".to_string() } else { label };
                    self.doc.blocks.push(Block::Button { form: f, label, name: n, value: v });
                }
            }
            "table" => {
                self.flush();
                self.cell_count = 0;
                self.in_cell = self.tables.pop().unwrap_or(0);
            }
            _ => {
                if BLOCKISH.contains(&name) {
                    self.block_break(name);
                }
            }
        }
    }
}

/// Parse an HTML page into Sanaga's light document (the whole page).
pub fn parse(src: &str, url: &str) -> Document {
    parse_page(src, url, false)
}

/// Parse an HTML page. With `main_only`, keep just the page's main content when it marks one.
pub fn parse_page(src: &str, url: &str, main_only: bool) -> Document {
    let base = Url::parse(url).ok();
    let mut b = Builder {
        base,
        doc: Document { url: url.to_string(), bytes: src.len(), ..Default::default() },
        spans: Vec::new(),
        bold: 0,
        italic: 0,
        code: 0,
        links: Vec::new(),
        pre: 0,
        pre_buf: String::new(),
        heading: 0,
        quote: 0,
        lists: Vec::new(),
        li_marker: None,
        in_li: 0,
        skip: None,
        in_title: false,
        form: None,
        in_form_tag: false,
        select: None,
        option: None,
        textarea: None,
        button: None,
        last_space: true,
        cell_count: 0,
        in_cell: 0,
        tables: Vec::new(),
        main: None,
        main_range: None,
    };
    let mut head_done = false;
    for tok in tokenize(src) {
        match tok {
            Token::Start { name, attrs, self_closing } => {
                // <head> content other than <title>/<base> is skipped; we don't
                // put "head" in the skip list for the start tag because pages
                // often omit </head>.
                if name == "head" {
                    continue;
                }
                if !head_done && (name == "meta" || name == "link") {
                    continue;
                }
                if name == "body" {
                    head_done = true;
                }
                b.start(&name, attrs, self_closing);
                if self_closing && !VOID.contains(&name.as_str()) {
                    b.end(&name);
                }
            }
            Token::End(name) => {
                if name == "head" {
                    head_done = true;
                    continue;
                }
                b.end(&name);
            }
            Token::Text(t) => {
                if b.skip.is_none() {
                    b.push_text(&t);
                }
            }
        }
    }
    b.flush();
    if b.pre > 0 {
        let text = std::mem::take(&mut b.pre_buf);
        b.doc.blocks.push(Block::Pre(text));
    }
    if main_only {
        if let Some((start, end)) = b.main_range {
            let end = end.unwrap_or(b.doc.blocks.len()).min(b.doc.blocks.len());
            if end > start + 2 && end - start < b.doc.blocks.len() {
                b.doc.blocks.truncate(end);
                b.doc.blocks.drain(..start);
                b.doc.anchors.retain(|_, v| *v >= start && *v <= end);
                for v in b.doc.anchors.values_mut() {
                    *v -= start;
                }
                b.doc.trimmed = true;
            }
        }
    }
    let title = b.doc.title.split_whitespace().collect::<Vec<_>>().join(" ");
    b.doc.title = title;
    b.doc
}

/// A plain-text document (text/plain responses, or "view source").
pub fn plain(text: &str, url: &str) -> Document {
    Document {
        url: url.to_string(),
        title: url.rsplit('/').find(|s| !s.is_empty()).unwrap_or(url).to_string(),
        blocks: vec![Block::Pre(text.to_string())],
        bytes: text.len(),
        ..Default::default()
    }
}

/// A document showing a single image.
pub fn image_doc(url: &str, bytes: usize) -> Document {
    Document {
        url: url.to_string(),
        title: url.rsplit('/').find(|s| !s.is_empty()).unwrap_or(url).to_string(),
        blocks: vec![Block::Image { src: url.to_string(), alt: String::new(), link: None, width: None }],
        bytes,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let d = parse(
            "<html><head><title>Salut &amp; bonjour</title><style>p{}</style></head><body><h1>Titre</h1><p>Un <b>gras</b> et <a href='/x'>lien</a>.</p><ul><li>a<li>b</ul><script>var x='<p>';</script><form action=/s><input name=q><button>Go</button></form></body></html>",
            "https://ex.com/a/b",
        );
        assert_eq!(d.title, "Salut & bonjour");
        assert!(matches!(&d.blocks[0], Block::Heading(1, _)));
        assert_eq!(d.links[0], "https://ex.com/x");
        assert_eq!(d.forms[0].action, "https://ex.com/s");
        let n_li = d.blocks.iter().filter(|b| matches!(b, Block::ListItem { .. })).count();
        assert_eq!(n_li, 2);
        assert!(d.blocks.iter().any(|b| matches!(b, Block::Button { .. })));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn table_rows_stay_on_one_line() {
        let d = parse("<table><tr><td>1.</td><td><center><a href=x></a></center></td><td><a href=/t>Titre</a></td></tr><tr><td colspan=2></td><td>12 points</td></tr></table>", "https://ex.com/");
        let texts: Vec<String> = d.blocks.iter().filter_map(|b| if let Block::Para(s) = b { Some(Document::plain_text(s)) } else { None }).collect();
        assert_eq!(texts, vec!["1.  ·  Titre".to_string(), "12 points".to_string()]);
    }

    #[test]
    fn main_content_only() {
        let d = parse_page("<nav><ul><li><a href=/a>Menu</a></ul></nav><main><h1>T</h1><p>a</p><p>b</p><p id=z>c</p></main><footer>pied</footer>", "https://ex.com/", true);
        assert!(d.trimmed);
        assert!(matches!(d.blocks[0], Block::Heading(1, _)));
        assert_eq!(d.blocks.len(), 4);
        assert_eq!(d.anchors.get("z"), Some(&3));
    }

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a&amp;b &eacute;t&eacute; &#233; &#x41; &nope; &"), "a&b été é A &nope; &");
    }
}

#[cfg(test)]
mod robustness {
    /// Odd pages must never stop Sanaga: any alphabet, broken tags,
    /// entities cut short, unclosed comments, attributes without values.
    #[test]
    fn odd_pages_do_not_panic() {
        let samples = [
            "<p>هذا النطاق مخصص للاستخدام</p><p dir=rtl>עברית</p>",
            "<h1>中文標題</h1><p>日本語のテキスト 👍🏽 &amp &#x1F600; &#99999999; &notanentity; &",
            "<div title=ä data-é>é<a href=ü>ü</a><img alt=👍 src=x.png>",
            "<p>unclosed <b>bold <i>italic <!-- comment never closed",
            "<<<>>><p<<p>></p>&#;&#x;&#xZZ;</",
            "<table><tr><td>ç<td>ğ<tr><td>ŧ</table><ul><li>ł<li>ø</ul>",
            "<script>var s = '</p>é';</script><style>p{}</style><p>après",
            "&",
            "",
        ];
        for s in samples {
            let _ = super::parse(s, "https://example.com/");
        }
    }
}

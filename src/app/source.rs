//! "View page source" (Ctrl+U), like modern browsers: colours, line numbers,
//! line wrapping, and links to the source of linked files.

/// Marks a page to show from HTML made by Sanaga (instead of an address).
pub(super) const HTML_MARK: &str = "\u{1}html:";

#[derive(Clone, Copy, PartialEq)]
enum K {
    Text,
    Tag,
    Attr,
    Val,
    Link,
    Comment,
    Doctype,
    Entity,
}

fn class(k: K) -> &'static str {
    match k {
        K::Text => "",
        K::Tag => "t",
        K::Attr => "a",
        K::Val => "v",
        K::Link => "v",
        K::Comment => "c",
        K::Doctype => "d",
        K::Entity => "e",
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Cut HTML source into coloured pieces (kind, text, link target).
fn tokens(src: &str, base: &str) -> Vec<(K, String, Option<String>)> {
    let mut out: Vec<(K, String, Option<String>)> = Vec::new();
    let push = |out: &mut Vec<(K, String, Option<String>)>, k: K, s: &str, link: Option<String>| {
        if s.is_empty() {
            return;
        }
        if link.is_none() {
            if let Some(last) = out.last_mut() {
                if last.0 == k && last.2.is_none() {
                    last.1.push_str(s);
                    return;
                }
            }
        }
        out.push((k, s.to_string(), link));
    };
    let resolve = |v: &str| -> Option<String> {
        let v = v.trim();
        if v.is_empty() || v.starts_with('#') || v.starts_with("javascript:") || v.starts_with("data:") || v.starts_with("mailto:") {
            return None;
        }
        url::Url::parse(base).ok()?.join(v).ok().map(|u| u.to_string()).filter(|u| u.starts_with("http"))
    };
    let b = src.as_bytes();
    let mut i = 0;
    let mut raw_until: Option<&'static str> = None; // inside <script> / <style>
    while i < b.len() {
        if let Some(end) = raw_until {
            let rest = &src[i..];
            let stop = rest.to_ascii_lowercase().find(end).unwrap_or(rest.len());
            push(&mut out, K::Text, &rest[..stop], None);
            i += stop;
            raw_until = None;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
            push(&mut out, K::Comment, &rest[..end], None);
            i += end;
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
            push(&mut out, K::Doctype, &rest[..end], None);
            i += end;
        } else if rest.starts_with('<') && rest[1..].chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '/') {
            // a tag: name, then attributes
            let end = {
                // the closing '>' outside quotes
                let mut q: Option<char> = None;
                let mut e = rest.len();
                for (k, c) in rest.char_indices().skip(1) {
                    match q {
                        Some(qc) if c == qc => q = None,
                        Some(_) => {}
                        None if c == '"' || c == '\'' => q = Some(c),
                        None if c == '>' => {
                            e = k + 1;
                            break;
                        }
                        _ => {}
                    }
                }
                e
            };
            let tag = &rest[..end];
            let name_end = tag[1..].find(|c: char| c.is_whitespace() || c == '>' || (c == '/' && tag.len() > 2)).map(|n| n + 1).unwrap_or(tag.len());
            let name_end = match tag.strip_prefix("</") {
                Some(closing) => closing.find(|c: char| c.is_whitespace() || c == '>').map(|n| n + 2).unwrap_or(tag.len()),
                None => name_end,
            };
            push(&mut out, K::Tag, &tag[..name_end], None);
            let name = tag[1..name_end].trim_start_matches('/').to_ascii_lowercase();
            let mut j = name_end;
            let mut last_attr = String::new();
            while j < tag.len() {
                // (a whole character: names and values may be in any alphabet)
                let c = tag[j..].chars().next().unwrap_or(' ');
                if c.is_whitespace() {
                    let k = tag[j..].find(|c: char| !c.is_whitespace()).map(|n| j + n).unwrap_or(tag.len());
                    push(&mut out, K::Text, &tag[j..k], None);
                    j = k;
                } else if c == '>' || c == '/' {
                    push(&mut out, K::Tag, &tag[j..j + 1], None);
                    j += 1;
                } else if c == '=' {
                    push(&mut out, K::Text, "=", None);
                    j += 1;
                    let (v, k) = match tag[j..].chars().next() {
                        Some(qc @ ('"' | '\'')) => {
                            let e = tag[j + 1..].find(qc).map(|n| j + 1 + n + 1).unwrap_or(tag.len());
                            (&tag[j..e], e)
                        }
                        _ => {
                            let e = tag[j..].find(|c: char| c.is_whitespace() || c == '>').map(|n| j + n).unwrap_or(tag.len());
                            (&tag[j..e], e)
                        }
                    };
                    let inner = v.trim_matches(|c| c == '"' || c == '\'');
                    let link = if ["href", "src", "action", "data-src", "poster"].contains(&last_attr.as_str()) { resolve(inner) } else { None };
                    push(&mut out, if link.is_some() { K::Link } else { K::Val }, v, link);
                    j = k;
                } else {
                    let k = tag[j..].find(|c: char| c.is_whitespace() || c == '=' || c == '>').map(|n| j + n).unwrap_or(tag.len());
                    let k = if k == j { j + c.len_utf8() } else { k };
                    last_attr = tag[j..k].to_ascii_lowercase();
                    push(&mut out, K::Attr, &tag[j..k], None);
                    j = k;
                }
            }
            if !tag.starts_with("</") && !tag.ends_with("/>") {
                raw_until = match name.as_str() {
                    "script" => Some("</script"),
                    "style" => Some("</style"),
                    _ => None,
                };
            }
            i += end;
        } else if let Some(after) = rest.strip_prefix('&') {
            let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).map(|n| n + 1).unwrap_or(1);
            if rest[end..].starts_with(';') && end > 1 {
                push(&mut out, K::Entity, &rest[..end + 1], None);
                i += end + 1;
            } else {
                push(&mut out, K::Text, "&", None);
                i += 1;
            }
        } else {
            // (the first character may be any letter, of any width)
            let first = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            let end = rest[first..].find(['<', '&']).map(|n| n + first).unwrap_or(rest.len());
            push(&mut out, K::Text, &rest[..end], None);
            i += end;
        }
    }
    out
}

/// The whole "source" page.
pub(super) fn page(url: &str, src: &str) -> String {
    let looks_html = src.trim_start().starts_with('<');
    let toks = if looks_html { tokens(src, url) } else { vec![(K::Text, src.to_string(), None)] };
    // one line per <span class="l">, pieces re-opened on each line
    let mut body = String::with_capacity(src.len() * 2);
    let mut lines = 1usize;
    body.push_str("<span class=\"l\">");
    for (k, text, link) in &toks {
        let cls = class(*k);
        let open = |b: &mut String| {
            if let Some(l) = link {
                b.push_str(&format!("<a class=\"{cls}\" href=\"#\" data-u=\"{}\">", esc(l)));
            } else if !cls.is_empty() {
                b.push_str(&format!("<span class=\"{cls}\">"));
            }
        };
        let close = |b: &mut String| {
            if link.is_some() {
                b.push_str("</a>");
            } else if !cls.is_empty() {
                b.push_str("</span>");
            }
        };
        for (n, part) in text.split('\n').enumerate() {
            if n > 0 {
                body.push_str("</span><span class=\"l\">");
                lines += 1;
            }
            if !part.is_empty() {
                open(&mut body);
                body.push_str(&esc(part.trim_end_matches('\r')));
                close(&mut body);
            }
        }
    }
    body.push_str("</span>");
    let size = if src.len() >= 1024 * 1024 { format!("{:.1} Mo", src.len() as f64 / 1048576.0) } else { format!("{:.1} Ko", src.len() as f64 / 1024.0) };
    let title = esc(&format!("view-source:{url}"));
    let wrap = crate::i18n::tr("Retour à la ligne");
    let info = crate::i18n::tf("{} lignes · {}", &[&lines, &size]);
    let heading = esc(crate::i18n::tr("Code source de"));
    let gutter = (lines.to_string().len() as f32 * 0.62 + 1.6).max(2.8);
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title>
<style>
:root {{ color-scheme: light dark; --bg:#fff; --fg:#1f1f1f; --num:#9aa0a6; --bar:#f6f7f9; --line:#e3e5e8;
  --t:#881280; --a:#994500; --v:#1a1aa6; --c:#6a737d; --d:#5f6368; --e:#c80000; --link:#1a1aa6; }}
@media (prefers-color-scheme: dark) {{ :root {{ --bg:#1e1f22; --fg:#e3e3e3; --num:#6f737a; --bar:#26282c; --line:#3a3d42;
  --t:#5db0d7; --a:#9bbbdc; --v:#f29766; --c:#8a919a; --d:#a0a4ab; --e:#f28b82; --link:#f29766; }} }}
html,body {{ margin:0; background:var(--bg); color:var(--fg); }}
header {{ position:sticky; top:0; z-index:2; display:flex; gap:18px; align-items:center; padding:8px 16px;
  background:var(--bar); border-bottom:1px solid var(--line); font:13px system-ui, sans-serif; }}
header .u {{ flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
header .n {{ color:var(--num); }}
label {{ cursor:pointer; user-select:none; }}
pre {{ margin:0; padding:8px 0 40px 0; font:13px/1.55 ui-monospace, "DejaVu Sans Mono", "Noto Sans Mono", monospace; counter-reset:ln; white-space:pre; tab-size:4; }}
body.wrap pre {{ white-space:pre-wrap; word-break:break-all; }}
.l {{ display:block; padding-left:{gutter}em; padding-right:16px; position:relative; min-height:1.55em; }}
.l::before {{ counter-increment:ln; content:counter(ln); position:absolute; left:0; width:{gutter_n}em; text-align:right; color:var(--num); user-select:none; }}
.l:hover {{ background:rgba(127,127,127,0.08); }}
.t {{ color:var(--t); }} .a {{ color:var(--a); }} .v {{ color:var(--v); }} .c {{ color:var(--c); font-style:italic; }} .d {{ color:var(--d); }} .e {{ color:var(--e); }}
a.v {{ color:var(--link); text-decoration:underline; text-decoration-color:rgba(127,127,127,0.5); cursor:pointer; }}
</style></head>
<body class="wrap"><header><span class="u">{heading} <b>{url_e}</b></span><span class="n">{info}</span>
<label><input type="checkbox" id="wrap" checked> {wrap}</label></header>
<pre>{body}</pre>
<script>
document.getElementById('wrap').addEventListener('change', function (e) {{ document.body.classList.toggle('wrap', e.target.checked); }});
document.addEventListener('click', function (e) {{
  var a = e.target.closest('a[data-u]'); if (!a) return; e.preventDefault();
  if (window.ipc) window.ipc.postMessage('vs:' + a.getAttribute('data-u'));
}});
</script></body></html>"#,
        gutter_n = gutter - 0.9,
        url_e = esc(url),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn colours_tags_attributes_and_links() {
        let p = super::page("https://ex.com/a/", "<!doctype html>\n<a href=\"b.html\" class=\"x\">Hi &amp; bye</a>\n<!-- note -->");
        assert!(p.contains("<span class=\"d\">&lt;!doctype html&gt;</span>"));
        assert!(p.contains("<span class=\"t\">&lt;a</span>"));
        assert!(p.contains("<span class=\"a\">href</span>"));
        assert!(p.contains("data-u=\"https://ex.com/a/b.html\""));
        assert!(p.contains("<span class=\"e\">&amp;amp;</span>"));
        assert!(p.contains("<span class=\"c\">&lt;!-- note --&gt;</span>"));
        assert_eq!(p.matches("<span class=\"l\">").count(), 3);
    }

    #[test]
    fn script_content_is_not_read_as_tags() {
        let p = super::page("https://ex.com/", "<script>if (a<b) x();</script>");
        assert!(p.contains("if (a&lt;b) x();"));
        assert!(!p.contains("<span class=\"t\">&lt;b"));
    }
}

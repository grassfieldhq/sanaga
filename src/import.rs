//! Import favourites from other browsers: the Chrome family (Chrome, Chromium,
//! Brave, Edge, Vivaldi) directly, and any browser through an exported
//! bookmarks HTML file (Firefox, Safari...).

use std::path::PathBuf;

/// Chrome-family bookmark files found on this computer: (browser name, file).
pub fn chrome_family_sources() -> Vec<(String, PathBuf)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Vec::new() };
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    [
        ("Google Chrome", "google-chrome"),
        ("Chromium", "chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Microsoft Edge", "microsoft-edge"),
        ("Vivaldi", "vivaldi"),
    ]
    .iter()
    .map(|(n, d)| (n.to_string(), config.join(d).join("Default").join("Bookmarks")))
    .filter(|(_, p)| p.is_file())
    .collect()
}

// ---- a tiny JSON reader, enough for bookmark files ----

struct J<'a> {
    b: &'a [u8],
    i: usize,
    out: Vec<(String, String)>,
}

impl J<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut s = Vec::new();
        while self.i < self.b.len() {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(s).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => s.push(b'\n'),
                        b't' => s.push(b'\t'),
                        b'r' | b'b' | b'f' => {}
                        b'u' => {
                            let hex = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let mut cp = u32::from_str_radix(hex, 16).ok()?;
                            // surrogate pair
                            if (0xD800..0xDC00).contains(&cp) && self.b.get(self.i..self.i + 2) == Some(b"\\u") {
                                let lo = u32::from_str_radix(std::str::from_utf8(self.b.get(self.i + 2..self.i + 6)?).ok()?, 16).ok()?;
                                self.i += 6;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            s.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        other => s.push(other),
                    }
                }
                c => s.push(c),
            }
        }
        None
    }

    /// Reads any value; objects with `"type": "url"` are collected.
    fn value(&mut self, depth: u32) -> Option<()> {
        if depth > 200 {
            return None;
        }
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let (mut ty, mut url, mut name) = (String::new(), String::new(), String::new());
                loop {
                    self.ws();
                    if self.b.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        break;
                    }
                    let key = self.string()?;
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    self.ws();
                    if self.b.get(self.i) == Some(&b'"') {
                        let v = self.string()?;
                        match key.as_str() {
                            "type" => ty = v,
                            "url" => url = v,
                            "name" => name = v,
                            _ => {}
                        }
                    } else {
                        self.value(depth + 1)?;
                    }
                    self.ws();
                    if self.b.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
                if ty == "url" && !url.is_empty() {
                    self.out.push((url, name));
                }
            }
            b'[' => {
                self.i += 1;
                loop {
                    self.ws();
                    if self.b.get(self.i) == Some(&b']') {
                        self.i += 1;
                        break;
                    }
                    self.value(depth + 1)?;
                    self.ws();
                    if self.b.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.string()?;
            }
            _ => {
                while self.i < self.b.len() && !matches!(self.b[self.i], b',' | b'}' | b']') {
                    self.i += 1;
                }
            }
        }
        Some(())
    }
}

/// (address, title) of every bookmark in a Chrome-family `Bookmarks` file.
pub fn parse_chrome(text: &str) -> Vec<(String, String)> {
    let mut j = J { b: text.as_bytes(), i: 0, out: Vec::new() };
    let _ = j.value(0);
    j.out
}

/// (address, title) of every link in an exported bookmarks HTML file.
pub fn parse_html(text: &str) -> Vec<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<a ") {
        let start = pos + i;
        let Some(end_tag) = lower[start..].find('>').map(|e| start + e) else { break };
        let tag = &text[start..end_tag];
        let tag_l = &lower[start..end_tag];
        let href = tag_l.find("href=").and_then(|h| {
            let rest = &tag[h + 5..];
            let q = rest.chars().next()?;
            if q == '"' || q == '\'' {
                rest[1..].find(q).map(|e| rest[1..1 + e].to_string())
            } else {
                Some(rest.split_whitespace().next().unwrap_or("").to_string())
            }
        });
        let close = lower[end_tag..].find("</a>").map(|e| end_tag + e).unwrap_or(end_tag + 1);
        let title = crate::html::decode_entities(text.get(end_tag + 1..close).unwrap_or("").trim());
        if let Some(h) = href {
            let h = crate::html::decode_entities(&h);
            if h.starts_with("http://") || h.starts_with("https://") {
                out.push((h, title));
            }
        }
        pos = close.max(start + 3);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_file() {
        let t = r#"{"roots":{"bookmark_bar":{"children":[{"date_added":"1","meta_info":{"x":"y"},"name":"Wiki é","type":"url","url":"https://fr.wikipedia.org/"},{"children":[{"name":"HN","type":"url","url":"https://news.ycombinator.com/"}],"name":"Dossier","type":"folder"}],"name":"Barre","type":"folder"}},"version":1}"#;
        let v = parse_chrome(t);
        assert_eq!(v, vec![("https://fr.wikipedia.org/".into(), "Wiki é".into()), ("https://news.ycombinator.com/".into(), "HN".into())]);
    }

    #[test]
    fn html_file() {
        let t = r#"<DL><p><DT><A HREF="https://a.org/" ADD_DATE="1">A &amp; B</A><DT><A HREF="place:x">no</A></DL>"#;
        assert_eq!(parse_html(t), vec![("https://a.org/".into(), "A & B".into())]);
    }
}

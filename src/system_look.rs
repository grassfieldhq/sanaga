//! The look the person chose for the whole system: on Grassfield, the accent
//! colour and the light / dark appearance of Settings › Appearance
//! (`~/.config/grassfield/grassfield.json`). Sanaga follows them when its own
//! settings say "System" (the default), like every app of the system.

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub struct SystemLook {
    /// The accent's name ("multicolor" = Grassfield green, "orange", "blue"…).
    pub accent: String,
    /// Dark appearance now?
    pub dark: bool,
}

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("grassfield").join("grassfield.json"))
}

thread_local! {
    static CACHE: std::cell::RefCell<Option<(SystemTime, u32, Option<SystemLook>)>> = const { std::cell::RefCell::new(None) };
}

/// The system's look (read again only when the file changed, or each hour
/// for the automatic appearance); None when the system is not Grassfield.
pub fn current() -> Option<SystemLook> {
    let path = config_file()?;
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let hour = local_hour();
    if let Some(hit) = CACHE.with(|c| c.borrow().as_ref().filter(|(m, h, _)| *m == modified && *h == hour).map(|(_, _, l)| l.clone())) {
        return hit;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let look = parse(&text, hour);
    CACHE.with(|c| *c.borrow_mut() = Some((modified, hour, look.clone())));
    look
}

/// Reads the accent and the appearance from Grassfield's settings text.
pub fn parse(text: &str, hour: u32) -> Option<SystemLook> {
    // "accent": { "color": "orange", ... }
    // (the word "accent" also appears as a value, e.g. "folderColor": "accent":
    // only the key followed by an object counts)
    let accent = text.match_indices("\"accent\"").find_map(|(i, m)| {
        let rest = text[i + m.len()..].trim_start().strip_prefix(':')?.trim_start();
        let obj = rest.strip_prefix('{')?;
        let end = obj.find('}').unwrap_or(obj.len());
        crate::ai::json_string(&obj[..end], "color")
    });
    // "appearance": "light" | "dark" | "auto" (top level); default dark, as Grassfield
    let appearance = top_level_string(text, "appearance").unwrap_or_else(|| "dark".into());
    let dark = match appearance.as_str() {
        "light" => false,
        "auto" => !(7..19).contains(&hour),
        _ => true,
    };
    Some(SystemLook { accent: accent.unwrap_or_else(|| "multicolor".into()), dark })
}

/// A string value of `key` directly in the top object (not inside another).
fn top_level_string(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                b'"' => {
                    if depth == 1 && text[i..].starts_with(&pat) {
                        let after = text[i + pat.len()..].trim_start();
                        if let Some(v) = after.strip_prefix(':') {
                            let v = v.trim_start();
                            if v.starts_with('"') {
                                return crate::ai::json_string(&format!("{{{pat}:{v}"), key);
                            }
                        }
                    }
                    in_str = true;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn local_hour() -> u32 {
    // the local hour, from the system clock and the time zone offset GLib knows
    gtk::glib::DateTime::now_local().map(|d| d.hour() as u32).unwrap_or(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_grassfield_settings() {
        let text = r#"{ "icons": {"mode": "default", "folderColor": "accent", "appearance": "x"}, "accent": { "color": "orange" }, "appearance": "light", "z": 1 }"#;
        assert_eq!(parse(text, 12), Some(SystemLook { accent: "orange".into(), dark: false }));
        let auto = r#"{ "appearance": "auto" }"#;
        assert!(parse(auto, 22).unwrap().dark);
        assert!(!parse(auto, 10).unwrap().dark);
        assert_eq!(parse("{}", 12).unwrap().accent, "multicolor");
    }
}

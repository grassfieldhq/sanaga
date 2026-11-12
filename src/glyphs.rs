//! Every character of the interface's texts must be in the fonts Sanaga
//! carries: one that is not is drawn as an empty box (▯) on a computer
//! without extra fonts — it happened with "⋯" and "⋮". The texts checked:
//! the translations (all languages) and the welcome pages.

use std::collections::BTreeSet;

fn u16_at(d: &[u8], at: usize) -> usize {
    u16::from_be_bytes([d[at], d[at + 1]]) as usize
}

fn u32_at(d: &[u8], at: usize) -> usize {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as usize
}

/// The characters a font file has a drawing for (its `cmap` table, in the
/// two forms fonts use: 4, for the first 65 536 characters, and 12).
fn characters(font: &[u8], into: &mut BTreeSet<u32>) {
    let tables = u16_at(font, 4);
    let Some(cmap) = (0..tables).map(|i| 12 + 16 * i).find(|&t| &font[t..t + 4] == b"cmap").map(|t| u32_at(font, t + 8)) else { return };
    for i in 0..u16_at(font, cmap + 2) {
        let o = cmap + u32_at(font, cmap + 8 + 8 * i);
        match u16_at(font, o) {
            4 => {
                let twice = u16_at(font, o + 6);
                let (ends, starts, deltas, ranges) = (o + 14, o + 16 + twice, o + 16 + 2 * twice, o + 16 + 3 * twice);
                for k in 0..twice / 2 {
                    let (start, end) = (u16_at(font, starts + 2 * k), u16_at(font, ends + 2 * k));
                    let (delta, range) = (u16_at(font, deltas + 2 * k), u16_at(font, ranges + 2 * k));
                    for c in start..=end.min(0xFFFE) {
                        let glyph = if range == 0 {
                            (c + delta) & 0xFFFF
                        } else {
                            match u16_at(font, ranges + 2 * k + range + 2 * (c - start)) {
                                0 => 0,
                                g => (g + delta) & 0xFFFF,
                            }
                        };
                        if glyph != 0 {
                            into.insert(c as u32);
                        }
                    }
                }
            }
            12 => {
                for k in 0..u32_at(font, o + 12) {
                    let (a, b) = (u32_at(font, o + 16 + 12 * k), u32_at(font, o + 20 + 12 * k));
                    into.extend(a as u32..=b as u32);
                }
            }
            _ => {}
        }
    }
}

#[test]
fn the_fonts_carried_can_draw_every_text() {
    let mut have = BTreeSet::new();
    characters(include_bytes!("../assets/Inter-Regular.ttf"), &mut have);
    characters(include_bytes!("../assets/NotoSans-Regular.ttf"), &mut have);
    // (the toolkit's own: a light sans-serif, and signs and emoji in black and white)
    for data in eframe::egui::FontDefinitions::default().font_data.values() {
        characters((*data.font).as_ref(), &mut have);
    }
    assert!(have.contains(&('é' as u32)) && have.contains(&('★' as u32)), "the fonts were not read as they should");
    let texts = [
        ("assets/i18n/translations.txt", include_str!("../assets/i18n/translations.txt")),
        ("assets/bienvenue.html", include_str!("../assets/bienvenue.html")),
        ("assets/bienvenue.en.html", include_str!("../assets/bienvenue.en.html")),
        ("assets/bienvenue.es.html", include_str!("../assets/bienvenue.es.html")),
        ("assets/bienvenue.pt.html", include_str!("../assets/bienvenue.pt.html")),
        ("assets/bienvenue.de.html", include_str!("../assets/bienvenue.de.html")),
    ];
    let mut missing = Vec::new();
    for (file, text) in texts {
        for (n, line) in text.lines().enumerate() {
            // (the mark between a text and its translations is not shown)
            for c in line.chars().filter(|c| !c.is_ascii() && *c != '⟂' && !c.is_whitespace() && !have.contains(&(*c as u32))) {
                missing.push(format!("{file}:{}: {c} (U+{:04X})", n + 1, c as u32));
            }
        }
    }
    assert!(missing.is_empty(), "characters no carried font can draw:\n{}", missing.join("\n"));
}

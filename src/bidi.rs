//! Right-to-left sentences (Arabic, Hebrew...) in Sanaga's own interface:
//! tab titles, favourites, the history, suggestions.
//!
//! The interface's text engine draws each word of such a script properly —
//! letters joined, from right to left inside the word — but it places the
//! pieces of a line one after the other from left to right, in the order
//! they are written in memory: an Arabic sentence came out with its words
//! in the wrong order. (Pages are not concerned: the web engine draws them.)
//!
//! This module gives the engine the pieces in the order they are *seen*
//! (the Unicode bidirectional algorithm), so that what it draws from left
//! to right reads right. It knows how the engine cuts a line into pieces:
//! by font. A run of letters of one right-to-left script comes from one
//! font and is one piece, which the engine itself draws from right to left;
//! everything else (spaces, Latin letters, digits, punctuation) comes from
//! the interface's own font and is drawn from left to right.
//!
//! Only for text that is shown. Text being typed keeps its order in memory:
//! the caret and the selection go by it.

use std::borrow::Cow;
use unicode_bidi::BidiInfo;

/// The font a right-to-left character comes from, as far as it matters
/// here: characters of the same script, next to one another, are drawn as
/// one piece by the text engine. 0: not a right-to-left script.
fn script(c: char) -> u32 {
    match c as u32 {
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => 1,                                                       // Hebrew
        0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => 2, // Arabic
        0x0700..=0x074F | 0x0860..=0x086F => 3,                                                       // Syriac
        0x0780..=0x07BF => 4,                                                                         // Thaana
        0x07C0..=0x07FF => 5,                                                                         // N'Ko
        0x0800..=0x083F => 6,                                                                         // Samaritan
        0x0840..=0x085F => 7,                                                                         // Mandaic
        // (the older and rarer ones, by block: Phoenician, Adlam...)
        n @ (0x10800..=0x10FFF | 0x1E800..=0x1EFFF) => 8 + (n >> 7),
        _ => 0,
    }
}

/// Has this text anything written from right to left? (Asked first: nearly
/// all text has not, and is then left as it is.)
pub fn has_rtl(text: &str) -> bool {
    // (nothing below U+0590 is: most text is settled on its bytes alone)
    text.bytes().any(|b| b >= 0xD6) && text.chars().any(|c| script(c) != 0)
}

/// The marks that only say in which direction to read: they are followed,
/// and not drawn.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// A bracket seen from the other side: in a right-to-left phrase an opening
/// bracket is drawn as a closing one.
fn mirrored(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        c => c,
    }
}

/// The characters of a text in the order they are seen, from the left, each
/// with where it is in the text (its byte); and whether the text as a whole
/// is read from right to left (its first letter is: it is then set against
/// the right). Line ends are left out.
pub fn seen(text: &str) -> (Vec<(char, usize)>, bool) {
    let info = BidiInfo::new(text, None);
    let mut out: Vec<(char, usize)> = Vec::with_capacity(text.len());
    let rtl = info.paragraphs.first().is_some_and(|p| p.level.is_rtl());
    for para in &info.paragraphs {
        let (levels, runs) = info.visual_runs(para, para.range.clone());
        for run in runs {
            let piece = text[run.clone()].char_indices().map(|(i, c)| (c, run.start + i)).filter(|(c, _)| !is_mark(*c) && *c != '\n' && *c != '\r');
            if levels[run.start].is_rtl() {
                // (a bracket of the interface's own font is turned round;
                // one of a right-to-left font is turned by the engine)
                out.extend(piece.rev().map(|(c, i)| (if script(c) == 0 { mirrored(c) } else { c }, i)));
            } else {
                out.extend(piece);
            }
        }
    }
    (out, rtl)
}

/// Characters in the order they are seen, as to hand to the text engine as
/// one piece: each run of one right-to-left script is drawn by the engine
/// from its last character to its first — given the other way round, it
/// comes out as seen.
pub fn piece(seen: impl Iterator<Item = char>) -> String {
    let seen: Vec<char> = seen.collect();
    let mut out = String::with_capacity(seen.len() * 2);
    let mut k = 0;
    while k < seen.len() {
        let s = script(seen[k]);
        let mut end = k + 1;
        if s != 0 {
            while end < seen.len() && script(seen[end]) == s {
                end += 1;
            }
            out.extend(seen[k..end].iter().rev());
        } else {
            out.push(seen[k]);
        }
        k = end;
    }
    out
}

/// The text as to hand to the interface's text engine so that it reads
/// right: unchanged when nothing in it is written from right to left.
pub fn visual(text: &str) -> Cow<'_, str> {
    if !has_rtl(text) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(piece(seen(text).0.into_iter().map(|(c, _)| c)))
}

/// The text to draw in `room`: as `visual`, and when it does not fit, cut
/// where reading stops — its end as it is written, wherever that is on the
/// screen — and ended with "…". `width` measures a text as it would be drawn.
pub fn fit(text: &str, room: f32, width: &mut dyn FnMut(&str) -> f32) -> String {
    let whole = visual(text);
    if width(&whole) <= room {
        return whole.into_owned();
    }
    // (the longest beginning that fits with its "…": looked for by halves)
    let ends: Vec<usize> = text.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
    let cut = |n: usize| -> String {
        let kept = if n == 0 { "" } else { text[..ends[n - 1]].trim_end() };
        visual(&format!("{kept}…")).into_owned()
    };
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if width(&cut(mid)) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    cut(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is seen, from the left, when the engine draws `given`: each run
    /// of one right-to-left script from its last character to its first,
    /// the rest as it comes.
    fn drawn(given: &str) -> String {
        let chars: Vec<char> = given.chars().collect();
        let mut out = String::new();
        let mut k = 0;
        while k < chars.len() {
            let s = script(chars[k]);
            let mut end = k + 1;
            while s != 0 && end < chars.len() && script(chars[end]) == s {
                end += 1;
            }
            if s != 0 {
                out.extend(chars[k..end].iter().rev());
            } else {
                out.push(chars[k]);
            }
            k = end;
        }
        out
    }

    /// Written backwards: how a right-to-left word looks from the left.
    fn back(word: &str) -> String {
        word.chars().rev().collect()
    }

    #[test]
    fn text_without_right_to_left_is_left_alone() {
        for plain in ["", "Sanaga — a browser", "日本語 हिन्दी ខ្មែរ", "naïve café ÿ", "😀 emoji"] {
            assert!(!has_rtl(plain), "{plain}");
            assert!(matches!(visual(plain), Cow::Borrowed(_)), "{plain}");
        }
    }

    #[test]
    fn the_words_of_a_sentence_are_seen_from_right_to_left() {
        // (two Arabic words: the first one written is seen on the right)
        let (hello, world) = ("مرحبا", "بالعالم");
        let given = visual(&format!("{hello} {world}")).into_owned();
        assert_eq!(given, format!("{world} {hello}"));
        assert_eq!(drawn(&given), format!("{} {}", back(world), back(hello)));
        // Hebrew the same
        let (shalom, olam) = ("שלום", "עולם");
        assert_eq!(visual(&format!("{shalom} {olam}")), format!("{olam} {shalom}"));
    }

    #[test]
    fn latin_words_and_numbers_keep_their_own_order_inside() {
        let (a, b) = ("مرحبا", "بالعالم");
        // a right-to-left sentence with a name and a number in it
        let seen = drawn(&visual(&format!("{a} Sanaga 2026 {b}")));
        assert_eq!(seen, format!("{} Sanaga 2026 {}", back(b), back(a)));
        // a left-to-right sentence with two Arabic words in it: they stay
        // where they are, and read from right to left between themselves
        let seen = drawn(&visual(&format!("Title: {a} {b} (draft)")));
        assert_eq!(seen, format!("Title: {} {} (draft)", back(b), back(a)));
    }

    #[test]
    fn digits_of_the_script_read_from_left_to_right() {
        // "year 2024" in Arabic, with Arabic-Indic digits and no space
        // before them: one piece for the engine, digits not turned round
        let (year, digits) = ("عام", "٢٠٢٤");
        assert_eq!(drawn(&visual(&format!("{year}{digits}"))), format!("{digits}{}", back(year)));
        assert_eq!(drawn(&visual(&format!("{year} {digits}"))), format!("{digits} {}", back(year)));
    }

    #[test]
    fn brackets_face_the_right_way_and_marks_are_not_drawn() {
        let w = "שלום";
        // (in a right-to-left line the bracket that opens is on the right)
        assert_eq!(drawn(&visual(&format!("{w} (abc)"))), format!("(abc) {}", back(w)));
        assert_eq!(drawn(&visual(&format!("\u{200F}{w}\u{200E}!"))), format!("!{}", back(w)));
    }

    #[test]
    fn a_word_in_two_styles_is_seen_in_two_pieces_in_the_right_order() {
        // "שלום" with its last two letters in bold: seen from the left, the
        // bold half comes first; each half, handed to the engine, reads right
        let word = "שלום";
        let (chars, rtl) = seen(word);
        assert!(rtl);
        let bold_from = word.char_indices().nth(2).unwrap().0;
        let left: Vec<char> = chars.iter().filter(|(_, i)| *i >= bold_from).map(|(c, _)| *c).collect();
        let right: Vec<char> = chars.iter().filter(|(_, i)| *i < bold_from).map(|(c, _)| *c).collect();
        // (the pieces are in the order they are seen: bold first)
        assert!(chars[0].1 >= bold_from && chars[3].1 < bold_from);
        let both = format!("{}{}", drawn(&piece(left.into_iter())), drawn(&piece(right.into_iter())));
        assert_eq!(both, back(word));
        // a left-to-right text says so
        assert!(!seen("Page שלום").1);
    }

    #[test]
    fn a_title_too_long_loses_its_end_not_its_beginning() {
        let words = ["אחת", "שתיים", "שלוש", "ארבע"];
        let title = words.join(" ");
        // (each character one unit wide)
        let mut width = |t: &str| t.chars().count() as f32;
        assert_eq!(fit(&title, 100.0, &mut width), visual(&title));
        let cut = fit(&title, 12.0, &mut width);
        assert!(cut.chars().count() <= 12, "{cut}");
        // the first word written is still there, on the right; the "…" is
        // where reading stops, on the left
        let seen = drawn(&cut);
        assert!(seen.ends_with(&back(words[0])), "{seen}");
        assert!(seen.starts_with('…'), "{seen}");
        assert!(!seen.contains(&back(words[3])), "{seen}");
        // a left-to-right title that ends in Hebrew keeps its "…" on the right
        let seen = drawn(&fit(&format!("Page {title}"), 14.0, &mut width));
        assert!(seen.starts_with("Page ") && seen.ends_with('…'), "{seen}");
        // no room at all: the "…" alone
        assert_eq!(fit(&title, 1.0, &mut width), "…");
    }
}

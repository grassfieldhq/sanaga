//! What the person types and what pages, models and files hand to the
//! interface must never make it panic. Each function that cuts or reads
//! such text is fed its usual inputs, every cut of them and thousands of
//! damaged versions (see mutate.rs and docs/FUZZING.md).

use crate::mutate::run;

fn text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).to_string()
}

#[test]
fn page_source_view_survives_any_page() {
    let seeds: [&[u8]; 3] = [
        b"<!doctype html><!-- c --><html lang=en><head><title>T &amp; U</title><script src='/a.js'>var x='</p>';</script><style>p{}</style></head><body class=\"a b\" data-x><a href=/x?y=1#z>link</a><img src=i.png alt>&nbsp;&#233;&bogus;<br/></body></html>",
        "<p title=é data-ü=\"ß\">İstanbul ẞ 👍🏽 <a href='//exemple.fr/é'>é</a></p><textarea>x</textarea><".as_bytes(),
        b"<svg viewBox='0 0 1 1'><path d=\"M0 0\"/></svg><a href=javascript:x>j</a><a href=\"mailto:a@b\">m</a><a href=data:x>d</a>",
    ];
    run(&seeds, 20_000, |d| {
        let page = super::source::page("https://example.com/a/b", &text(d));
        assert!(!page.is_empty());
    });
}

#[test]
fn find_in_page_cuts_text_on_characters() {
    // (lowercasing can change how many bytes a letter takes: İ, ẞ, K)
    let seeds: [&[u8]; 3] = ["The quick brown fox — İstanbul, STRAẞE, \u{212a}elvin; ǅ and ﬁ".as_bytes(), "ĞğİıŞş ΑΣ ας 中文 👍🏽".as_bytes(), b"aAaAaA"];
    for q in ["a", "i", "ss", "k", "ς", "fox", "\u{69}\u{307}", "ß", "中", ""] {
        run(&seeds, 5_000, |d| {
            let t = text(d);
            let parts = super::render::split_find(&t, q);
            // nothing lost, nothing added
            assert_eq!(parts.iter().map(|p| p.0).collect::<String>(), t);
        });
    }
}

#[test]
fn what_is_typed_in_the_address_bar_is_always_understood() {
    let seeds: [&[u8]; 6] = [b"12*(3+4)-2^10/5%", b"=15% * 80,5", "2×3÷4".as_bytes(), b"w Sanaga river", b"exemple.fr/chemin?q=1#a", "localhost:8080 http://[::1]/ file:///tmp é".as_bytes()];
    run(&seeds, 20_000, |d| {
        let t = text(d);
        let _ = super::plus::calculate(&t);
        let _ = super::plus::keyword_search(&t);
        let u = super::input_to_url(&t, crate::store::Engine::DuckDuckGo, "sanaga://newtab");
        assert!(!u.is_empty());
        let _ = super::host_of(&t);
        let _ = super::badge_letter(&t, &t);
        let _ = super::english_address(&t);
        let _ = super::accounts::origin_of(&t);
        let _ = super::toolbar::display_url(&t, false);
        let _ = super::toolbar::display_url(&t, true);
    });
    // numbers the calculator shows
    for v in [0.0, -0.0, 1e300, -1e-300, f64::MAX, 0.1 + 0.2, 1e15 + 0.5] {
        let _ = super::plus::format_number(v);
    }
}

#[test]
fn a_models_answer_is_read_whatever_it_holds() {
    let seeds: [&[u8]; 3] = [r#"Here: ["one", "two \"quoted\"", "trois é\n", "😀"] done"#.as_bytes(), br#"["a",,"b" , "c\\"]"#, "[\"é\", \"中\"".as_bytes()];
    run(&seeds, 20_000, |d| {
        let _ = super::translate::parse_string_array(&text(d));
    });
}

#[test]
fn saved_settings_are_read_whatever_the_file_holds() {
    let seeds: [&[u8]; 3] = [b"exemple.fr\tjs=0,fonts=1,dark=1,font=Inter,zap=.ad|#x\n", b"// @name x\n// @match *://youtube.com/*\n// @include *://*.youtube.com/*\nalert(1)", br##"{"accent": {"color": "#1a73e8"}, "appearance": "dark", "auto": {"from": 19}}"##];
    run(&seeds, 20_000, |d| {
        let t = text(d);
        for line in t.lines() {
            let _ = crate::site_prefs::SitePrefs::from_line(line);
        }
        let _ = crate::extensions::matches_in(&t);
        for hour in [0, 12, 23] {
            let _ = crate::system_look::parse(&t, hour);
        }
    });
}

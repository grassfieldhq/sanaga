//! Sanaga colours, in the spirit of Safari: quiet greys, one blue accent.

use eframe::egui::Color32;

#[derive(Clone, Copy)]
pub struct Theme {
    /// Tab bar background (and inactive tabs).
    pub strip: Color32,
    pub tab_active: Color32,
    pub tab_hover: Color32,
    pub toolbar: Color32,
    /// Address field.
    pub omnibox: Color32,
    pub omnibox_hover: Color32,
    pub text: Color32,
    pub sub: Color32,
    pub accent: Color32,
    pub link: Color32,
    pub visited: Color32,
    pub page: Color32,
    pub border: Color32,
    pub highlight: Color32,
    pub card: Color32,
    pub code_bg: Color32,
    /// Start page background.
    pub ntp_bg: Color32,
    /// Sidebar background.
    pub sidebar: Color32,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn theme(dark: bool, private: bool) -> Theme {
    if dark || private {
        let mut t = Theme {
            strip: c(0x1E, 0x1E, 0x20),
            tab_active: c(0x2C, 0x2C, 0x2F),
            tab_hover: c(0x26, 0x26, 0x29),
            toolbar: c(0x2C, 0x2C, 0x2F),
            omnibox: c(0x1C, 0x1C, 0x1E),
            omnibox_hover: c(0x22, 0x22, 0x25),
            text: c(0xF2, 0xF2, 0xF7),
            sub: c(0x98, 0x98, 0x9F),
            accent: c(0x0A, 0x84, 0xFF),
            link: c(0x4D, 0xA3, 0xFF),
            visited: c(0xC4, 0x9B, 0xFF),
            page: c(0x1E, 0x1E, 0x20),
            border: c(0x3A, 0x3A, 0x3D),
            highlight: c(0x8A, 0x6D, 0x10),
            card: c(0x2C, 0x2C, 0x2F),
            code_bg: c(0x26, 0x26, 0x29),
            ntp_bg: c(0x1C, 0x1C, 0x1E),
            sidebar: c(0x24, 0x24, 0x27),
        };
        if private {
            // Private tabs: a deeper night tone and a lighter field, like a lit-up search box.
            t.toolbar = c(0x22, 0x21, 0x2A);
            t.tab_active = c(0x22, 0x21, 0x2A);
            t.strip = c(0x17, 0x16, 0x1D);
            t.omnibox = c(0x3A, 0x38, 0x47);
            t.omnibox_hover = c(0x44, 0x42, 0x52);
            t.ntp_bg = c(0x1A, 0x19, 0x21);
            t.page = c(0x1A, 0x19, 0x21);
            t.card = c(0x2A, 0x29, 0x34);
            t.accent = c(0x9D, 0x8C, 0xFF);
        }
        t
    } else {
        Theme {
            strip: c(0xE3, 0xE3, 0xE7),
            tab_active: c(0xF7, 0xF7, 0xF8),
            tab_hover: c(0xEC, 0xEC, 0xEF),
            toolbar: c(0xF7, 0xF7, 0xF8),
            omnibox: c(0xE6, 0xE6, 0xEA),
            omnibox_hover: c(0xDE, 0xDE, 0xE3),
            text: c(0x1D, 0x1D, 0x1F),
            sub: c(0x6E, 0x6E, 0x73),
            accent: c(0x0A, 0x7A, 0xFF),
            link: c(0x00, 0x66, 0xCC),
            visited: c(0x7D, 0x3C, 0xB5),
            page: c(0xFF, 0xFF, 0xFF),
            border: c(0xD6, 0xD6, 0xDB),
            highlight: c(0xFF, 0xE0, 0x66),
            card: c(0xF4, 0xF4, 0xF6),
            code_bg: c(0xF2, 0xF2, 0xF5),
            ntp_bg: c(0xF2, 0xF2, 0xF5),
            sidebar: c(0xEE, 0xEE, 0xF1),
        }
    }
}

pub fn badge_color(seed: &str) -> Color32 {
    const P: [Color32; 8] = [
        c(0x0E, 0x7C, 0x66),
        c(0x1A, 0x5F, 0xD0),
        c(0xD9, 0x5B, 0x1F),
        c(0x7B, 0x3F, 0xC4),
        c(0xC2, 0x2E, 0x5E),
        c(0x0F, 0x86, 0xA8),
        c(0x4F, 0x8A, 0x10),
        c(0xB0, 0x7A, 0x00),
    ];
    let h = seed.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    P[(h % 8) as usize]
}

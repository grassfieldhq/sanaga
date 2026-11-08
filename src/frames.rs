//! Device frames for the mobile simulator, drawn with cairo: the body with its
//! metal rim, bezels, rounded screen corners, camera island / notch / hole,
//! status bar, home bar, side buttons, laptop base or monitor stand.
//! The same picture is shown over the page (screen left see-through) and used
//! around screenshots.

use gtk::cairo::{Context, Format, ImageSurface, LinearGradient, Operator};
use std::f64::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    /// Pill-shaped camera island (recent iPhones).
    Island,
    /// Wide notch (iPhone X to 14).
    Notch,
    /// Thick bezels and a round home button (iPhone SE / 8).
    Classic,
    /// Thin bezels, small round camera in the screen (most Android phones).
    Punch,
    /// Curved screen edges, sensors in thin top/bottom bezels.
    Curved,
    Tablet,
    Laptop,
    Monitor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Finish {
    Black,
    Silver,
    Titanium,
    Blue,
    White,
}

pub const FINISHES: [(Finish, &str, [u8; 3]); 5] = [
    (Finish::Black, "Noir", [0x2B, 0x2C, 0x30]),
    (Finish::Silver, "Argent", [0xD6, 0xD8, 0xDC]),
    (Finish::Titanium, "Titane", [0xB8, 0xAE, 0x9E]),
    (Finish::Blue, "Bleu", [0x3E, 0x5A, 0x7A]),
    (Finish::White, "Blanc", [0xF2, 0xF2, 0xF0]),
];

/// Sizes in CSS pixels. `body_*` is the device itself; the picture adds `pad`
/// around it (shadow, side buttons) and `below` under it (laptop base, stand).
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub body_w: f32,
    pub body_h: f32,
    /// Screen inside the body: x, y, w, h.
    pub screen: (f32, f32, f32, f32),
    /// Status bar at the top of the screen (the page starts under it).
    pub status: f32,
    pub pad: f32,
    pub below: f32,
    body_r: f32,
    pub screen_r: f32,
    rim: f32,
    /// The device's browser bars above / below the page (see `Browser`).
    pub bar_top: f32,
    pub bar_bottom: f32,
}

/// The browser shown inside the device, as on the real one: Safari-like on
/// Apple devices, Chrome-like on Android and computers (drawn bars, no logos).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Browser {
    None,
    SafariPhone,
    SafariTablet,
    ChromePhone,
    ChromeTablet,
    ChromeDesktop,
}

/// What a click on the drawn browser does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hot {
    Back,
    Forward,
    Reload,
    Address,
}

/// The drawn browser's height above and below the page (CSS pixels).
fn bar_sizes(browser: Browser, style: Style, landscape: bool) -> (f32, f32) {
    let home_area = matches!(style, Style::Island | Style::Notch);
    match (browser, landscape) {
        (Browser::None, _) => (0.0, 0.0),
        (Browser::SafariPhone, false) => (0.0, 52.0 + 44.0 + if home_area { 26.0 } else { 0.0 }),
        (Browser::SafariPhone, true) => (46.0, if home_area { 18.0 } else { 0.0 }),
        (Browser::SafariTablet, _) => (50.0, 0.0),
        (Browser::ChromePhone, false) => (56.0, 0.0),
        (Browser::ChromePhone, true) => (52.0, 0.0),
        (Browser::ChromeTablet, _) => (38.0 + 48.0, 0.0),
        (Browser::ChromeDesktop, _) => (34.0 + 40.0, 0.0),
    }
}

/// Geometry with the device's browser bars.
pub fn geom_browser(style: Style, w: f32, h: f32, landscape: bool, browser: Browser) -> Geom {
    let mut g = geom(style, w, h, landscape);
    let (t, b) = bar_sizes(browser, style, landscape);
    g.bar_top = t;
    g.bar_bottom = b;
    g
}

impl Geom {
    pub fn width(&self) -> f32 {
        self.body_w + 2.0 * self.pad
    }
    pub fn height(&self) -> f32 {
        self.body_h + 2.0 * self.pad + self.below
    }
    /// The page's place in the picture: x, y, w, h.
    pub fn page(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.screen;
        let top = self.status + self.bar_top;
        (self.pad + x, self.pad + y + top, w, (h - top - self.bar_bottom).max(1.0))
    }
}

fn is_phone(s: Style) -> bool {
    matches!(s, Style::Island | Style::Notch | Style::Classic | Style::Punch | Style::Curved)
}

/// `w`, `h`: screen size as shown (landscape already swapped).
pub fn geom(style: Style, w: f32, h: f32, landscape: bool) -> Geom {
    // bezel left, top, right, bottom; body radius; screen radius; rim; status bar
    let (l, t, r, b, body_r, screen_r, rim, status) = match style {
        Style::Island => (13.0, 13.0, 13.0, 13.0, 68.0, 55.0, 4.0, 54.0),
        Style::Notch => (15.0, 15.0, 15.0, 15.0, 62.0, 47.0, 4.0, 47.0),
        Style::Classic => (22.0, 96.0, 22.0, 96.0, 62.0, 0.0, 3.0, 20.0),
        Style::Punch => (10.0, 10.0, 10.0, 10.0, 46.0, 36.0, 3.0, 30.0),
        Style::Curved => (7.0, 34.0, 7.0, 34.0, 48.0, 30.0, 3.0, 24.0),
        Style::Tablet => (22.0, 22.0, 22.0, 22.0, 40.0, 18.0, 3.0, 24.0),
        Style::Laptop => (16.0, 22.0, 16.0, 28.0, 16.0, 4.0, 2.0, 0.0),
        Style::Monitor => (14.0, 14.0, 14.0, 34.0, 12.0, 0.0, 2.0, 0.0),
    };
    // turned a quarter: what was at the top is now on the left
    let (l, t, r, b, status) = if landscape && (is_phone(style) || style == Style::Tablet) { (t, r, b, l, if style == Style::Tablet { status } else { 0.0 }) } else { (l, t, r, b, status) };
    let below = match style {
        Style::Laptop => 22.0,
        Style::Monitor => 130.0,
        _ => 0.0,
    };
    let pad = match style {
        Style::Laptop => 40.0f32.max(w * 0.08),
        Style::Monitor => 20.0,
        _ => 18.0,
    };
    Geom { body_w: w + l + r, body_h: h + t + b, screen: (l, t, w, h), status, pad, below, body_r, screen_r, rim, bar_top: 0.0, bar_bottom: 0.0 }
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

fn rgb(c: [u8; 3], k: f64) -> (f64, f64, f64) {
    ((c[0] as f64 / 255.0 * k).min(1.0), (c[1] as f64 / 255.0 * k).min(1.0), (c[2] as f64 / 255.0 * k).min(1.0))
}

fn finish_color(f: Finish) -> [u8; 3] {
    FINISHES.iter().find(|x| x.0 == f).map(|x| x.2).unwrap_or([0x2B, 0x2C, 0x30])
}

/// What the drawn browser shows.
pub struct Page<'a> {
    pub browser: Browser,
    pub domain: &'a str,
    pub title: &'a str,
    pub secure: bool,
}

/// Draw the frame (page area left see-through). `s` = picture pixels per CSS pixel.
#[allow(clippy::too_many_arguments)]
pub fn render(style: Style, finish: Finish, w: f32, h: f32, landscape: bool, s: f32, time: &str, dark_status: bool, page: &Page) -> Option<ImageSurface> {
    let g = geom_browser(style, w, h, landscape, page.browser);
    let (pw, ph) = ((g.width() * s).ceil() as i32, (g.height() * s).ceil() as i32);
    let surf = ImageSurface::create(Format::ARgb32, pw.max(1), ph.max(1)).ok()?;
    {
        let cr = Context::new(&surf).ok()?;
        cr.scale(s as f64, s as f64);
        draw(&cr, style, finish, &g, landscape, time, dark_status);
        draw_browser(&cr, style, &g, landscape, page);
    }
    surf.flush();
    Some(surf)
}

#[derive(Clone, Copy, PartialEq)]
enum Glyph {
    Back,
    Forward,
    Reload,
    Share,
    Book,
    Tabs,
    Dots,
    Plus,
    Sidebar,
    TextSize,
}

/// Where the drawn browser's parts are (picture CSS pixels).
struct Plan {
    /// Bar backgrounds: x, y, w, h, colour, line on top / bottom.
    bars: Vec<(f64, f64, f64, f64, (f64, f64, f64))>,
    /// Address field: x, y, w, h, radius, colour.
    pill: (f64, f64, f64, f64, f64, (f64, f64, f64)),
    /// Text in the address field: centred (Safari) or left (Chrome).
    centred: bool,
    buttons: Vec<(Glyph, Option<Hot>, f64, f64, f64, f64, (f64, f64, f64))>,
    /// Chrome tab (tablets, computers): x, y, w, h.
    tab: Option<(f64, f64, f64, f64)>,
    font: f64,
}

fn plan(style: Style, g: &Geom, landscape: bool, browser: Browser) -> Option<Plan> {
    let (ox, oy) = (g.pad as f64, g.pad as f64);
    let (sx, sy, sw, sh) = (ox + g.screen.0 as f64, oy + g.screen.1 as f64, g.screen.2 as f64, g.screen.3 as f64);
    let top = sy + g.status as f64;
    let (bt, bb) = (g.bar_top as f64, g.bar_bottom as f64);
    let safari_bar = (0.97, 0.97, 0.975);
    let safari_pill = (0.89, 0.89, 0.91);
    let blue = (0.0, 0.48, 1.0);
    let ink = (0.27, 0.28, 0.30);
    let white = (1.0, 1.0, 1.0);
    let chrome_pill = (0.945, 0.953, 0.957);
    let _ = style;
    let row = |names: &[(Glyph, Option<Hot>)], x0: f64, x1: f64, y: f64, h: f64, col| -> Vec<(Glyph, Option<Hot>, f64, f64, f64, f64, (f64, f64, f64))> {
        let n = names.len() as f64;
        let step = (x1 - x0) / n;
        names.iter().enumerate().map(|(k, (gl, hot))| (*gl, *hot, x0 + step * k as f64, y, step, h, col)).collect()
    };
    Some(match browser {
        Browser::None => return None,
        Browser::SafariPhone if !landscape => {
            let yb = sy + sh - bb;
            let pill = (sx + 12.0, yb + 7.0, sw - 24.0, 40.0, 12.0, safari_pill);
            let mut buttons = vec![(Glyph::TextSize, None, pill.0 + 4.0, pill.1, 34.0, pill.3, (0.1, 0.1, 0.12)), (Glyph::Reload, Some(Hot::Reload), pill.0 + pill.2 - 38.0, pill.1, 34.0, pill.3, (0.1, 0.1, 0.12))];
            buttons.extend(row(&[(Glyph::Back, Some(Hot::Back)), (Glyph::Forward, Some(Hot::Forward)), (Glyph::Share, None), (Glyph::Book, None), (Glyph::Tabs, None)], sx + 8.0, sx + sw - 8.0, yb + 52.0, 44.0, blue));
            Plan { bars: vec![(sx, yb, sw, bb, safari_bar)], pill, centred: true, buttons, tab: None, font: 15.0 }
        }
        Browser::SafariPhone => {
            let pw = (sw * 0.48).min(460.0);
            let pill = (sx + sw / 2.0 - pw / 2.0, top + 6.0, pw, 34.0, 10.0, safari_pill);
            let mut buttons = row(&[(Glyph::Back, Some(Hot::Back)), (Glyph::Forward, Some(Hot::Forward)), (Glyph::Book, None)], sx + 40.0, pill.0 - 12.0, top, bt, blue);
            buttons.extend(row(&[(Glyph::Share, None), (Glyph::Plus, None), (Glyph::Tabs, None)], pill.0 + pw + 12.0, sx + sw - 40.0, top, bt, blue));
            buttons.push((Glyph::Reload, Some(Hot::Reload), pill.0 + pw - 34.0, pill.1, 30.0, pill.3, (0.1, 0.1, 0.12)));
            Plan { bars: vec![(sx, top, sw, bt, safari_bar)], pill, centred: true, buttons, tab: None, font: 14.0 }
        }
        Browser::SafariTablet => {
            let pw = (sw * 0.42).min(520.0);
            let pill = (sx + sw / 2.0 - pw / 2.0, top + 8.0, pw, 34.0, 10.0, safari_pill);
            let mut buttons = row(&[(Glyph::Sidebar, None), (Glyph::Back, Some(Hot::Back)), (Glyph::Forward, Some(Hot::Forward))], sx + 10.0, sx + 150.0, top, bt, blue);
            buttons.extend(row(&[(Glyph::Share, None), (Glyph::Plus, None), (Glyph::Tabs, None)], sx + sw - 150.0, sx + sw - 10.0, top, bt, blue));
            buttons.push((Glyph::Reload, Some(Hot::Reload), pill.0 + pw - 34.0, pill.1, 30.0, pill.3, (0.1, 0.1, 0.12)));
            Plan { bars: vec![(sx, top, sw, bt, safari_bar)], pill, centred: true, buttons, tab: None, font: 14.0 }
        }
        Browser::ChromePhone => {
            let pill = (sx + 12.0, top + 8.0, sw - 12.0 - 88.0, 40.0, 20.0, chrome_pill);
            let buttons = row(&[(Glyph::Tabs, None), (Glyph::Dots, None)], sx + sw - 86.0, sx + sw - 4.0, top, bt, ink);
            Plan { bars: vec![(sx, top, sw, bt, white)], pill, centred: false, buttons, tab: None, font: 15.0 }
        }
        Browser::ChromeTablet | Browser::ChromeDesktop => {
            let (strip, bar) = if browser == Browser::ChromeTablet { (38.0, 48.0) } else { (34.0, 40.0) };
            let ty = top + strip;
            let left = row(&[(Glyph::Back, Some(Hot::Back)), (Glyph::Forward, Some(Hot::Forward)), (Glyph::Reload, Some(Hot::Reload))], sx + 6.0, sx + 6.0 + 3.0 * 34.0, ty, bar, ink);
            let px = sx + 6.0 + 3.0 * 34.0 + 6.0;
            let pill = (px, ty + (bar - 32.0) / 2.0, sw - (px - sx) - 48.0, 32.0, 16.0, chrome_pill);
            let mut buttons = left;
            buttons.push((Glyph::Dots, None, sx + sw - 42.0, ty, 38.0, bar, ink));
            let tw = (sw * 0.28).clamp(140.0, 260.0);
            buttons.push((Glyph::Plus, None, sx + 8.0 + tw + 4.0, top + 6.0, 30.0, strip - 6.0, ink));
            Plan {
                bars: vec![(sx, top, sw, strip, (0.87, 0.88, 0.90)), (sx, ty, sw, bar, white)],
                pill,
                centred: false,
                buttons,
                tab: Some((sx + 8.0, top + 6.0, tw, strip - 6.0)),
                font: 13.0,
            }
        }
    })
}

/// The click areas of the drawn browser (picture CSS pixels: x, y, w, h).
pub fn hotspots(style: Style, w: f32, h: f32, landscape: bool, browser: Browser) -> Vec<(Hot, f32, f32, f32, f32)> {
    let g = geom_browser(style, w, h, landscape, browser);
    let Some(p) = plan(style, &g, landscape, browser) else { return Vec::new() };
    let mut out: Vec<(Hot, f32, f32, f32, f32)> = vec![(Hot::Address, p.pill.0 as f32, p.pill.1 as f32, p.pill.2 as f32 - 40.0, p.pill.3 as f32)];
    for (_, hot, x, y, bw, bh, _) in p.buttons {
        if let Some(hot) = hot {
            out.push((hot, x as f32, y as f32, bw as f32, bh as f32));
        }
    }
    // the more precise areas (buttons inside the address field) first
    out.reverse();
    out
}

fn glyph(cr: &Context, gl: Glyph, cx: f64, cy: f64, col: (f64, f64, f64)) {
    cr.set_source_rgb(col.0, col.1, col.2);
    cr.set_line_width(2.0);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    cr.new_path();
    match gl {
        Glyph::Back | Glyph::Forward => {
            let d = if gl == Glyph::Back { -1.0 } else { 1.0 };
            cr.move_to(cx - d * 4.0, cy - 8.0);
            cr.line_to(cx + d * 4.0, cy);
            cr.line_to(cx - d * 4.0, cy + 8.0);
            let _ = cr.stroke();
        }
        Glyph::Reload => {
            cr.arc(cx, cy, 7.0, -0.3, 1.5 * PI);
            let _ = cr.stroke();
            cr.move_to(cx + 7.5, cy - 7.5);
            cr.line_to(cx + 7.5, cy - 1.5);
            cr.line_to(cx + 1.5, cy - 1.5);
            let _ = cr.stroke();
        }
        Glyph::Share => {
            rounded(cr, cx - 7.0, cy - 3.0, 14.0, 13.0, 2.0);
            let _ = cr.stroke();
            cr.move_to(cx, cy + 3.0);
            cr.line_to(cx, cy - 11.0);
            cr.move_to(cx - 4.0, cy - 7.0);
            cr.line_to(cx, cy - 11.0);
            cr.line_to(cx + 4.0, cy - 7.0);
            let _ = cr.stroke();
        }
        Glyph::Book => {
            cr.move_to(cx, cy - 6.0);
            cr.curve_to(cx - 4.0, cy - 9.0, cx - 9.0, cy - 9.0, cx - 10.0, cy - 8.0);
            cr.line_to(cx - 10.0, cy + 7.0);
            cr.curve_to(cx - 7.0, cy + 6.0, cx - 3.0, cy + 6.0, cx, cy + 8.0);
            cr.curve_to(cx + 3.0, cy + 6.0, cx + 7.0, cy + 6.0, cx + 10.0, cy + 7.0);
            cr.line_to(cx + 10.0, cy - 8.0);
            cr.curve_to(cx + 9.0, cy - 9.0, cx + 4.0, cy - 9.0, cx, cy - 6.0);
            cr.line_to(cx, cy + 8.0);
            let _ = cr.stroke();
        }
        Glyph::Tabs => {
            rounded(cr, cx - 8.0, cy - 8.0, 16.0, 16.0, 3.0);
            let _ = cr.stroke();
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
            cr.set_font_size(10.0);
            cr.move_to(cx - 3.0, cy + 3.5);
            let _ = cr.show_text("1");
        }
        Glyph::Dots => {
            for k in [-6.0, 0.0, 6.0] {
                cr.arc(cx, cy + k, 1.8, 0.0, 2.0 * PI);
                let _ = cr.fill();
            }
        }
        Glyph::Plus => {
            cr.move_to(cx - 7.0, cy);
            cr.line_to(cx + 7.0, cy);
            cr.move_to(cx, cy - 7.0);
            cr.line_to(cx, cy + 7.0);
            let _ = cr.stroke();
        }
        Glyph::Sidebar => {
            rounded(cr, cx - 10.0, cy - 8.0, 20.0, 16.0, 3.0);
            cr.move_to(cx - 3.0, cy - 8.0);
            cr.line_to(cx - 3.0, cy + 8.0);
            let _ = cr.stroke();
        }
        Glyph::TextSize => {
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
            cr.set_font_size(11.0);
            cr.move_to(cx - 9.0, cy + 5.0);
            let _ = cr.show_text("a");
            cr.set_font_size(16.0);
            cr.move_to(cx - 2.0, cy + 6.0);
            let _ = cr.show_text("A");
        }
    }
}

/// Short text that fits `max` wide (with "…").
fn fit_text(cr: &Context, text: &str, max: f64) -> String {
    let width = |t: &str| cr.text_extents(t).map(|e| e.x_advance()).unwrap_or(0.0);
    if width(text) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() && width(&format!("{}…", chars.iter().collect::<String>())) > max {
        chars.pop();
    }
    format!("{}…", chars.iter().collect::<String>())
}

fn draw_browser(cr: &Context, style: Style, g: &Geom, landscape: bool, page: &Page) {
    let Some(p) = plan(style, g, landscape, page.browser) else { return };
    let (ox, oy) = (g.pad as f64, g.pad as f64);
    let (sx, sy, sw, sh) = (ox + g.screen.0 as f64, oy + g.screen.1 as f64, g.screen.2 as f64, g.screen.3 as f64);
    let _ = cr.save();
    // inside the screen's rounded shape
    rounded(cr, sx, sy, sw, sh, g.screen_r as f64);
    cr.clip();
    for (x, y, w, h, c) in &p.bars {
        cr.set_source_rgb(c.0, c.1, c.2);
        cr.rectangle(*x, *y, *w, *h);
        let _ = cr.fill();
        // thin line between bar and page
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.12);
        let line_y = if *y > sy + sh / 2.0 { *y } else { *y + *h - 0.5 };
        cr.rectangle(*x, line_y, *w, 0.6);
        let _ = cr.fill();
    }
    if let Some((x, y, w, h)) = p.tab {
        // the open tab (Chrome)
        rounded(cr, x, y, w, h + 10.0, 8.0);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.fill();
        cr.set_source_rgb(0.2, 0.2, 0.22);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
        cr.set_font_size(12.0);
        let title = if page.title.trim().is_empty() { page.domain } else { page.title };
        let t = fit_text(cr, title, w - 46.0);
        cr.arc(x + 16.0, y + h / 2.0, 6.0, 0.0, 2.0 * PI);
        cr.set_source_rgb(0.75, 0.77, 0.8);
        let _ = cr.fill();
        cr.set_source_rgb(0.2, 0.2, 0.22);
        cr.move_to(x + 28.0, y + h / 2.0 + 4.0);
        let _ = cr.show_text(&t);
        cr.set_line_width(1.4);
        cr.move_to(x + w - 16.0, y + h / 2.0 - 4.0);
        cr.line_to(x + w - 8.0, y + h / 2.0 + 4.0);
        cr.move_to(x + w - 8.0, y + h / 2.0 - 4.0);
        cr.line_to(x + w - 16.0, y + h / 2.0 + 4.0);
        let _ = cr.stroke();
    }
    // address field
    let (px, py, pw, ph, pr, pc) = p.pill;
    rounded(cr, px, py, pw, ph, pr);
    cr.set_source_rgb(pc.0, pc.1, pc.2);
    let _ = cr.fill();
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
    cr.set_font_size(p.font);
    let room = pw - if p.centred { 90.0 } else { 44.0 };
    let text = fit_text(cr, page.domain, room);
    let tw = cr.text_extents(&text).map(|e| e.x_advance()).unwrap_or(0.0);
    let lock_w = if page.secure { 14.0 } else { 0.0 };
    let tx = if p.centred { px + pw / 2.0 - (tw + lock_w) / 2.0 + lock_w } else { px + 16.0 + lock_w };
    let ty = py + ph / 2.0 + p.font * 0.35;
    if page.secure {
        // small padlock
        let lx = tx - lock_w + 1.0;
        let ly = py + ph / 2.0;
        cr.set_source_rgb(0.35, 0.36, 0.38);
        rounded(cr, lx, ly - 2.0, 8.0, 7.0, 1.5);
        let _ = cr.fill();
        cr.set_line_width(1.4);
        cr.new_path();
        cr.arc(lx + 4.0, ly - 2.0, 2.8, PI, 2.0 * PI);
        let _ = cr.stroke();
    }
    cr.set_source_rgb(0.08, 0.08, 0.1);
    cr.move_to(tx, ty);
    let _ = cr.show_text(&text);
    for (gl, _, x, y, w, h, col) in &p.buttons {
        glyph(cr, *gl, x + w / 2.0, y + h / 2.0, *col);
    }
    let _ = cr.restore();
}

/// The page is a rectangle but the screen has rounded corners: the page's
/// square corners can stick out past the device's own rounded body. Paint
/// those bits with the backdrop's colour (`top`, `bottom`: the backdrop at the
/// height of the screen's top and bottom), under the rest of the picture.
#[allow(clippy::too_many_arguments)]
pub fn cover_corners(surf: &ImageSurface, style: Style, w: f32, h: f32, landscape: bool, s: f32, browser: Browser, top: [u8; 3], bottom: [u8; 3]) {
    let g = geom_browser(style, w, h, landscape, browser);
    let Ok(cr) = Context::new(surf) else { return };
    cr.scale(s as f64, s as f64);
    let (ox, oy) = (g.pad as f64, g.pad as f64);
    let (sx, sy, sw, sh) = (ox + g.screen.0 as f64, oy + g.screen.1 as f64, g.screen.2 as f64, g.screen.3 as f64);
    let r = g.screen_r as f64;
    if r <= 0.0 {
        return;
    }
    // only where the page itself reaches the screen's corners (not under a
    // status bar or a browser bar, which follow the rounded screen)
    let mut corners = Vec::new();
    if g.bar_bottom <= 0.0 {
        corners.push((sx, sy + sh - r, bottom));
        corners.push((sx + sw - r, sy + sh - r, bottom));
    }
    if g.status <= 0.0 && g.bar_top <= 0.0 {
        corners.push((sx, sy, top));
        corners.push((sx + sw - r, sy, top));
    }
    cr.set_operator(Operator::DestOver);
    for (x, y, c) in corners {
        let _ = cr.save();
        cr.rectangle(x, y, r, r);
        cr.clip();
        cr.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
        cr.rectangle(x, y, r, r);
        rounded(&cr, ox, oy, g.body_w as f64, g.body_h as f64, g.body_r as f64);
        cr.set_source_rgb(c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0);
        let _ = cr.fill();
        let _ = cr.restore();
    }
    surf.flush();
}

fn draw(cr: &Context, style: Style, finish: Finish, g: &Geom, landscape: bool, time: &str, dark_status: bool) {
    let (ox, oy) = (g.pad as f64, g.pad as f64);
    let (bw, bh) = (g.body_w as f64, g.body_h as f64);
    let base = finish_color(finish);
    let (sx, sy, sw, sh) = (ox + g.screen.0 as f64, oy + g.screen.1 as f64, g.screen.2 as f64, g.screen.3 as f64);
    let body_r = g.body_r as f64;
    let screen_r = g.screen_r as f64;
    let rim = g.rim as f64;

    // soft shadow under the device
    for k in 0..8 {
        let e = k as f64 * 1.6;
        rounded(cr, ox - e, oy + 6.0 - e * 0.4, bw + 2.0 * e, bh + 2.0 * e, body_r + e);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.035);
        let _ = cr.fill();
    }

    // laptop base / monitor stand (behind)
    match style {
        Style::Laptop => {
            let top = oy + bh - 2.0;
            let over = g.pad as f64 * 0.85;
            cr.new_path();
            cr.move_to(ox - over * 0.55, top);
            cr.line_to(ox + bw + over * 0.55, top);
            cr.line_to(ox + bw + over, top + g.below as f64 - 6.0);
            cr.curve_to(ox + bw + over, top + g.below as f64, ox + bw + over - 6.0, top + g.below as f64, ox + bw + over - 12.0, top + g.below as f64);
            cr.line_to(ox - over + 12.0, top + g.below as f64);
            cr.curve_to(ox - over + 6.0, top + g.below as f64, ox - over, top + g.below as f64, ox - over, top + g.below as f64 - 6.0);
            cr.close_path();
            let grad = LinearGradient::new(0.0, top, 0.0, top + g.below as f64);
            let (r1, g1, b1) = rgb(base, 1.15);
            let (r2, g2, b2) = rgb(base, 0.7);
            grad.add_color_stop_rgb(0.0, r1, g1, b1);
            grad.add_color_stop_rgb(1.0, r2, g2, b2);
            let _ = cr.set_source(&grad);
            let _ = cr.fill();
            // opening notch at the front
            rounded(cr, ox + bw / 2.0 - 60.0, top, 120.0, 7.0, 3.5);
            let (r, gg, b) = rgb(base, 0.6);
            cr.set_source_rgb(r, gg, b);
            let _ = cr.fill();
        }
        Style::Monitor => {
            let top = oy + bh;
            let (r1, g1, b1) = rgb(base, 0.95);
            rounded(cr, ox + bw / 2.0 - 40.0, top - 4.0, 80.0, g.below as f64 - 18.0, 4.0);
            cr.set_source_rgb(r1, g1, b1);
            let _ = cr.fill();
            rounded(cr, ox + bw / 2.0 - 140.0, top + g.below as f64 - 26.0, 280.0, 14.0, 7.0);
            let (r2, g2, b2) = rgb(base, 0.8);
            cr.set_source_rgb(r2, g2, b2);
            let _ = cr.fill();
        }
        _ => {}
    }

    // side buttons (phones, tablets)
    if is_phone(style) || style == Style::Tablet {
        let (r, gg, b) = rgb(base, 0.85);
        cr.set_source_rgb(r, gg, b);
        let bt = 4.0;
        let along = |f: f64, len: f64| -> (f64, f64) { (f, len) };
        let buttons: Vec<(bool, f64, f64)> = if style == Style::Tablet {
            vec![(false, 0.12, 0.06)]
        } else {
            vec![(true, 0.17, 0.05), (true, 0.25, 0.09), (true, 0.36, 0.09), (false, 0.27, 0.13)]
        };
        for (left_side, f, len) in buttons {
            let (f, len) = along(f, len);
            if !landscape {
                let y = oy + bh * f;
                let x = if left_side { ox - bt + 1.0 } else { ox + bw - 1.0 };
                rounded(cr, x, y, bt, bh * len, 1.5);
            } else {
                // turned: the left side is now on top
                let x = ox + bw * (1.0 - f - len);
                let y = if left_side { oy - bt + 1.0 } else { oy + bh - 1.0 };
                rounded(cr, x, y, bw * len, bt, 1.5);
            }
            let _ = cr.fill();
        }
    }

    // metal rim (ring)
    cr.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
    rounded(cr, ox, oy, bw, bh, body_r);
    rounded(cr, ox + rim, oy + rim, bw - 2.0 * rim, bh - 2.0 * rim, (body_r - rim).max(0.0));
    let grad = if landscape { LinearGradient::new(0.0, oy, 0.0, oy + bh) } else { LinearGradient::new(ox, 0.0, ox + bw, 0.0) };
    let (r0, g0, b0) = rgb(base, 0.72);
    let (r1, g1, b1) = rgb(base, 1.25);
    grad.add_color_stop_rgb(0.0, r0, g0, b0);
    grad.add_color_stop_rgb(0.08, r1, g1, b1);
    grad.add_color_stop_rgb(0.5, r0 * 1.1, g0 * 1.1, b0 * 1.1);
    grad.add_color_stop_rgb(0.92, r1, g1, b1);
    grad.add_color_stop_rgb(1.0, r0, g0, b0);
    let _ = cr.set_source(&grad);
    let _ = cr.fill();

    // bezel (ring around the screen)
    let white_face = finish == Finish::White && matches!(style, Style::Classic | Style::Tablet);
    rounded(cr, ox + rim, oy + rim, bw - 2.0 * rim, bh - 2.0 * rim, (body_r - rim).max(0.0));
    rounded(cr, sx, sy, sw, sh, screen_r);
    if white_face {
        cr.set_source_rgb(0.97, 0.97, 0.96);
    } else {
        cr.set_source_rgb(0.04, 0.04, 0.045);
    }
    let _ = cr.fill();
    cr.set_fill_rule(gtk::cairo::FillRule::Winding);
    // the screen stays see-through (the shadow went under it too)
    cr.set_operator(Operator::Clear);
    rounded(cr, sx, sy, sw, sh, screen_r);
    let _ = cr.fill();
    cr.set_operator(Operator::Over);
    let dot = if white_face { (0.25, 0.25, 0.27) } else { (0.12, 0.13, 0.15) };

    // status bar (inside the screen, above the page)
    if g.status > 0.0 {
        let st = g.status as f64;
        let _ = cr.save();
        rounded(cr, sx, sy, sw, sh, screen_r);
        cr.clip();
        let (bg, fg) = if dark_status { ((0.0, 0.0, 0.0), (1.0, 1.0, 1.0)) } else { ((1.0, 1.0, 1.0), (0.0, 0.0, 0.0)) };
        cr.set_source_rgb(bg.0, bg.1, bg.2);
        cr.rectangle(sx, sy, sw, st);
        let _ = cr.fill();
        cr.set_source_rgb(fg.0, fg.1, fg.2);
        let ios = matches!(style, Style::Island | Style::Notch | Style::Classic) || style == Style::Tablet;
        let font = if ios { 15.0 } else { 13.0 };
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(font);
        let cy = sy + st / 2.0 + if matches!(style, Style::Island | Style::Notch) { 4.0 } else { 0.0 };
        let left_x = match style {
            Style::Island | Style::Notch => sx + sw * 0.17,
            _ => sx + 16.0,
        };
        let ext = cr.text_extents(time).ok();
        let tw = ext.map(|e| e.width()).unwrap_or(30.0);
        let tx = if matches!(style, Style::Island | Style::Notch) { left_x - tw / 2.0 } else { left_x };
        cr.move_to(tx, cy + font * 0.36);
        let _ = cr.show_text(time);
        // right: signal, wifi, battery
        let right = if matches!(style, Style::Island | Style::Notch) { sx + sw * 0.83 + 34.0 } else { sx + sw - 14.0 };
        let bx = right - 25.0;
        rounded(cr, bx, cy - 6.0, 24.0, 12.0, 3.5);
        cr.set_line_width(1.2);
        let _ = cr.stroke();
        rounded(cr, bx + 2.0, cy - 4.0, 17.0, 8.0, 2.0);
        let _ = cr.fill();
        rounded(cr, bx + 24.6, cy - 2.0, 2.0, 4.0, 1.0);
        let _ = cr.fill();
        let wx = bx - 13.0;
        for (k, rr) in [(0, 9.0), (1, 6.0), (2, 3.0)] {
            if k < 2 {
                cr.new_path();
                cr.arc(wx, cy + 5.0, rr, 1.25 * PI, 1.75 * PI);
                cr.set_line_width(2.0);
                let _ = cr.stroke();
            } else {
                cr.arc(wx, cy + 4.0, 1.8, 0.0, 2.0 * PI);
                let _ = cr.fill();
            }
        }
        let gx = wx - 28.0;
        for k in 0..4 {
            let hh = 3.5 + k as f64 * 2.5;
            rounded(cr, gx + k as f64 * 4.5, cy + 5.0 - hh, 3.0, hh, 1.0);
            let _ = cr.fill();
        }
        let _ = cr.restore();
    }

    // camera island / notch / hole, sensors
    let black = (0.0, 0.0, 0.0);
    match style {
        Style::Island => {
            let (w, h) = (126.0, 37.0);
            if !landscape {
                rounded(cr, sx + sw / 2.0 - w / 2.0, sy + 11.0, w, h, h / 2.0);
            } else {
                rounded(cr, sx + 11.0, sy + sh / 2.0 - w / 2.0, h, w, h / 2.0);
            }
            cr.set_source_rgb(black.0, black.1, black.2);
            let _ = cr.fill();
        }
        Style::Notch => {
            let (w, h) = (162.0, 32.0);
            cr.new_path();
            if !landscape {
                let (x0, y0) = (sx + sw / 2.0 - w / 2.0, sy);
                cr.move_to(x0 - 6.0, y0);
                cr.arc_negative(x0 - 6.0, y0 + 6.0, 6.0, -PI / 2.0, 0.0);
                cr.line_to(x0, y0 + h - 20.0);
                cr.arc_negative(x0 + 20.0, y0 + h - 20.0, 20.0, PI, PI / 2.0);
                cr.line_to(x0 + w - 20.0, y0 + h);
                cr.arc_negative(x0 + w - 20.0, y0 + h - 20.0, 20.0, PI / 2.0, 0.0);
                cr.line_to(x0 + w, y0 + 6.0);
                cr.arc_negative(x0 + w + 6.0, y0 + 6.0, 6.0, PI, PI * 1.5);
            } else {
                let (x0, y0) = (sx, sy + sh / 2.0 - w / 2.0);
                cr.rectangle(x0, y0, h - 16.0, w);
                let _ = cr.fill();
                rounded(cr, x0, y0, h, w, 18.0);
            }
            cr.close_path();
            cr.set_source_rgb(black.0, black.1, black.2);
            let _ = cr.fill();
        }
        Style::Punch => {
            let (cx, cy) = if !landscape { (sx + sw / 2.0, sy + g.status as f64 / 2.0 + 1.0) } else { (sx + 16.0, sy + sh / 2.0) };
            cr.arc(cx, cy, 6.5, 0.0, 2.0 * PI);
            cr.set_source_rgb(black.0, black.1, black.2);
            let _ = cr.fill();
        }
        Style::Curved | Style::Classic | Style::Tablet | Style::Laptop | Style::Monitor => {
            // sensors in the bezel above the screen
            let (cx, cy) = if landscape && (is_phone(style) || style == Style::Tablet) { (ox + g.screen.0 as f64 / 2.0, oy + bh / 2.0) } else { (ox + bw / 2.0, oy + g.screen.1 as f64 / 2.0) };
            cr.set_source_rgb(dot.0, dot.1, dot.2);
            if matches!(style, Style::Classic | Style::Curved) {
                let (lw, lh) = if landscape { (6.0, 54.0) } else { (54.0, 6.0) };
                rounded(cr, cx - lw / 2.0, cy - lh / 2.0, lw, lh, 3.0);
                let _ = cr.fill();
                let (dx, dy) = if landscape { (0.0, -46.0) } else { (-46.0, 0.0) };
                cr.arc(cx + dx, cy + dy, 5.0, 0.0, 2.0 * PI);
                let _ = cr.fill();
            } else {
                cr.arc(cx, cy, 3.5, 0.0, 2.0 * PI);
                let _ = cr.fill();
            }
            if style == Style::Classic {
                let (hx, hy) = if landscape { (ox + bw - g.screen.0 as f64 / 2.0, oy + bh / 2.0) } else { (ox + bw / 2.0, oy + bh - (bh - g.screen.1 as f64 - g.screen.3 as f64) / 2.0) };
                cr.arc(hx, hy, 27.0, 0.0, 2.0 * PI);
                cr.set_line_width(2.0);
                let ring = if white_face { (0.78, 0.78, 0.8) } else { (0.22, 0.23, 0.25) };
                cr.set_source_rgb(ring.0, ring.1, ring.2);
                let _ = cr.stroke();
            }
        }
    }

    // home bar over the bottom of the screen (gesture phones)
    if matches!(style, Style::Island | Style::Notch | Style::Punch) {
        let (w, h) = if matches!(style, Style::Punch) { (108.0, 4.0) } else { (140.0, 5.0) };
        rounded(cr, sx + sw / 2.0 - w / 2.0, sy + sh - 9.0 - h, w, h, h / 2.0);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.78);
        let _ = cr.fill();
    }
    cr.set_operator(Operator::Over);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_is_left_clear_and_body_is_drawn() {
        let page = Page { browser: Browser::SafariPhone, domain: "example.com", title: "Example", secure: true };
        let s = render(Style::Island, Finish::Titanium, 393.0, 852.0, false, 1.0, "12:30", false, &page).expect("frame");
        let g = geom_browser(Style::Island, 393.0, 852.0, false, Browser::SafariPhone);
        assert_eq!((s.width(), s.height()), (g.width().ceil() as i32, g.height().ceil() as i32));
        let mut s = s;
        let stride = s.stride() as usize;
        let data = s.data().expect("pixels");
        let px = |x: f32, y: f32| data[(y as usize) * stride + (x as usize) * 4 + 3];
        let (pxx, py, pw, ph) = g.page();
        assert_eq!(px(pxx + pw / 2.0, py + ph / 2.0), 0, "page area must be see-through");
        assert_eq!(px(g.pad + 6.0, g.pad + g.body_h / 2.0), 255, "bezel must be opaque");
        assert!(g.page().1 > g.pad + g.screen.1, "status bar above the page");
        // Safari's bar under the page is drawn (opaque)
        let below = g.page().1 + g.page().3 + 20.0;
        assert_eq!(px(pxx + pw / 2.0, below), 255, "browser bar must be drawn");
        assert!(g.page().3 < 852.0 - 54.0 - 90.0, "the page is smaller than the screen, as on the device");
    }

    #[test]
    fn browsers_have_clickable_buttons() {
        for b in [Browser::SafariPhone, Browser::SafariTablet, Browser::ChromePhone, Browser::ChromeTablet, Browser::ChromeDesktop] {
            let hot = hotspots(Style::Punch, 412.0, 915.0, false, b);
            assert!(hot.iter().any(|h| h.0 == Hot::Address), "{b:?}");
        }
        let safari = hotspots(Style::Island, 393.0, 852.0, false, Browser::SafariPhone);
        assert!(safari.iter().any(|h| h.0 == Hot::Back) && safari.iter().any(|h| h.0 == Hot::Reload));
        assert!(hotspots(Style::Island, 393.0, 852.0, false, Browser::None).is_empty());
    }

    #[test]
    fn landscape_turns_the_bezels() {
        let p = geom(Style::Classic, 375.0, 667.0, false);
        let l = geom(Style::Classic, 667.0, 375.0, true);
        assert_eq!(p.screen.1, l.screen.0);
        assert_eq!(l.status, 0.0);
    }
}

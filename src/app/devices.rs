//! Mobile simulator, built into Sanaga (Ctrl+Shift+M or the phone button):
//! the page is shown as on a phone, a tablet or a computer — the device's
//! screen size inside its drawn frame (frames.rs), portrait or landscape,
//! mobile browser identity (iOS / Android), touch — with screenshots. Devices
//! are picked in a side panel (search, categories, silhouettes).

use super::*;
use super::myframes::{self, CustomFrame};
use crate::frames::{self, Finish, Style};
use eframe::egui::{Align2, Color32, RichText, ScrollArea, Sense};
use webkit6::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Phone,
    Tablet,
    Computer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Os {
    Ios,
    Android,
    Desktop,
}

pub(super) struct Device {
    pub brand: &'static str,
    pub name: &'static str,
    /// Screen size in CSS pixels (portrait).
    pub w: f32,
    pub h: f32,
    pub kind: Kind,
    pub os: Os,
    pub style: Style,
}

const fn d(brand: &'static str, name: &'static str, w: f32, h: f32, kind: Kind, os: Os, style: Style) -> Device {
    Device { brand, name, w, h, kind, os, style }
}

use Kind::*;
use Os::*;

/// Screen sizes (CSS pixels) of common devices.
pub(super) const DEVICES: &[Device] = &[
    d("Apple", "iPhone 5 / SE (1re gén.)", 320.0, 568.0, Phone, Ios, Style::Classic),
    d("Apple", "iPhone 6 / 7 / 8", 375.0, 667.0, Phone, Ios, Style::Classic),
    d("Apple", "iPhone 6 Plus / 7 Plus / 8 Plus", 414.0, 736.0, Phone, Ios, Style::Classic),
    d("Apple", "iPhone SE (2e et 3e gén.)", 375.0, 667.0, Phone, Ios, Style::Classic),
    d("Apple", "iPhone X / XS", 375.0, 812.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone XS Max", 414.0, 896.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone XR", 414.0, 896.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 11", 414.0, 896.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 11 Pro", 375.0, 812.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 11 Pro Max", 414.0, 896.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 12 mini", 375.0, 812.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 12 / 12 Pro", 390.0, 844.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 12 Pro Max", 428.0, 926.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 13 mini", 375.0, 812.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 13 / 13 Pro", 390.0, 844.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 13 Pro Max", 428.0, 926.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 14", 390.0, 844.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 14 Plus", 428.0, 926.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 14 Pro", 393.0, 852.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 14 Pro Max", 430.0, 932.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 15 / 15 Pro", 393.0, 852.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 15 Plus / 15 Pro Max", 430.0, 932.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 16", 393.0, 852.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 16 Plus", 430.0, 932.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 16 Pro", 402.0, 874.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 16 Pro Max", 440.0, 956.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 16e", 390.0, 844.0, Phone, Ios, Style::Notch),
    d("Apple", "iPhone 17", 402.0, 874.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone Air", 420.0, 912.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 17 Pro", 402.0, 874.0, Phone, Ios, Style::Island),
    d("Apple", "iPhone 17 Pro Max", 440.0, 956.0, Phone, Ios, Style::Island),
    d("Google", "Pixel 3", 393.0, 786.0, Phone, Android, Style::Curved),
    d("Google", "Pixel 3 XL", 393.0, 786.0, Phone, Android, Style::Notch),
    d("Google", "Pixel 4", 353.0, 745.0, Phone, Android, Style::Curved),
    d("Google", "Pixel 4a", 393.0, 851.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 5", 393.0, 851.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 6 / 6a", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 6 Pro / 7 Pro", 412.0, 892.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 7 / 7a", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 8 / 8a", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 8 Pro", 448.0, 998.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 9 / 10", 412.0, 923.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 9 Pro", 410.0, 914.0, Phone, Android, Style::Punch),
    d("Google", "Pixel 9 Pro XL", 414.0, 921.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S8 / S8+ / S9", 360.0, 740.0, Phone, Android, Style::Curved),
    d("Samsung", "Galaxy S10 / S10e", 360.0, 760.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S10+", 412.0, 869.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S20 / S21", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S20+ / S21+ / S22+", 384.0, 854.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S20 Ultra", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S21 Ultra", 384.0, 854.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S22 / S23 / S24 / S25", 360.0, 780.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S22 Ultra / S23 Ultra / S24 Ultra", 384.0, 824.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S23+ / S24+", 384.0, 832.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy S25 Ultra", 412.0, 891.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy Note 10", 360.0, 718.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy Note 20", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy Note 20 Ultra", 412.0, 883.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy A12 / A13 / A14 / A15", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy A32 / A33 / A34", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy A51 / A71", 412.0, 914.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy A52 / A53", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy A54 / A55", 384.0, 854.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy Z Fold (plié)", 344.0, 882.0, Phone, Android, Style::Punch),
    d("Samsung", "Galaxy Z Flip", 412.0, 919.0, Phone, Android, Style::Punch),
    d("Autres", "Tecno Spark / Pop", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Autres", "Tecno Camon / Pova", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Infinix Hot / Note", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Infinix Smart", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Autres", "itel A70 / S23", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Autres", "Xiaomi 12 / 13 / 14", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Redmi Note 10 / 11 / 13", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Redmi Note 12", 393.0, 851.0, Phone, Android, Style::Punch),
    d("Autres", "Redmi 12C / 13C", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Autres", "Poco X5 / F5", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "OnePlus 9 / 10", 412.0, 919.0, Phone, Android, Style::Punch),
    d("Autres", "OnePlus 11 / 12", 412.0, 919.0, Phone, Android, Style::Punch),
    d("Autres", "OnePlus Nord", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Autres", "Nothing Phone (1) / (2)", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Autres", "Huawei P30 / P40", 360.0, 780.0, Phone, Android, Style::Punch),
    d("Autres", "Huawei Mate 20 Pro", 360.0, 780.0, Phone, Android, Style::Notch),
    d("Autres", "Oppo Reno 8 / 10", 360.0, 800.0, Phone, Android, Style::Punch),
    d("Autres", "Oppo A78 / A98", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Vivo Y36 / V27", 393.0, 873.0, Phone, Android, Style::Punch),
    d("Autres", "Motorola Edge", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Autres", "Moto G (récents)", 412.0, 915.0, Phone, Android, Style::Punch),
    d("Autres", "Sony Xperia 10", 360.0, 840.0, Phone, Android, Style::Curved),
    d("Autres", "Sony Xperia 1", 384.0, 854.0, Phone, Android, Style::Curved),
    d("Autres", "Nokia G / X", 393.0, 851.0, Phone, Android, Style::Punch),
    d("Tablettes", "iPad Mini (6e gén.)", 744.0, 1133.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad (9e gén.)", 810.0, 1080.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad (10e gén.)", 820.0, 1180.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad Air 11 pouces", 820.0, 1180.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad Air 13 pouces", 1024.0, 1366.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad Pro 11 pouces", 834.0, 1194.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad Pro 12,9 pouces", 1024.0, 1366.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "iPad Pro 13 pouces", 1032.0, 1376.0, Tablet, Ios, Style::Tablet),
    d("Tablettes", "Galaxy Tab S7 / S8 / S9", 800.0, 1280.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Galaxy Tab A8 / A9+", 800.0, 1280.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Galaxy Tab A7 Lite", 600.0, 960.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Galaxy Z Fold (déplié)", 690.0, 829.0, Tablet, Android, Style::Punch),
    d("Tablettes", "Lenovo Tab P11", 800.0, 1280.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Xiaomi Pad 6", 800.0, 1280.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Amazon Fire HD 10", 800.0, 1280.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Surface Pro", 912.0, 1368.0, Tablet, Desktop, Style::Tablet),
    d("Tablettes", "Surface Duo", 540.0, 720.0, Tablet, Android, Style::Tablet),
    d("Tablettes", "Écran connecté (Nest Hub)", 600.0, 1024.0, Tablet, Android, Style::Tablet),
    d("Ordinateurs", "Chromebook", 1366.0, 768.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Petit portable", 1280.0, 800.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Portable 1366 × 768", 1366.0, 768.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Portable Windows courant", 1536.0, 864.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Portable 15 pouces", 1440.0, 900.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "MacBook Air 13 pouces", 1470.0, 956.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "MacBook Pro 14 pouces", 1512.0, 982.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "MacBook Pro 16 pouces", 1728.0, 1117.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Portable Full HD", 1920.0, 1080.0, Computer, Desktop, Style::Laptop),
    d("Ordinateurs", "Écran Full HD", 1920.0, 1080.0, Computer, Desktop, Style::Monitor),
    d("Ordinateurs", "Écran 2K", 2560.0, 1440.0, Computer, Desktop, Style::Monitor),
    d("Ordinateurs", "iMac 24 pouces", 2240.0, 1260.0, Computer, Desktop, Style::Monitor),
    d("Ordinateurs", "Écran 4K", 3840.0, 2160.0, Computer, Desktop, Style::Monitor),
];

const IOS_PHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const IOS_TABLET_UA: &str = "Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const ANDROID_PHONE_UA: &str = "Mozilla/5.0 (Linux; Android 14; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36";
const ANDROID_TABLET_UA: &str = "Mozilla/5.0 (Linux; Android 14; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// Makes pages see a touch screen (many sites switch their menus on this).
const TOUCH_JS: &str = "try{Object.defineProperty(Navigator.prototype,'maxTouchPoints',{get:function(){return 5;},configurable:true});if(!('ontouchstart' in window))window.ontouchstart=null;}catch(e){}";

/// Device categories shown in the panel.
const CATEGORIES: [&str; 5] = ["Tous", "iPhone", "Android", "Tablettes", "Ordinateurs"];

#[derive(Clone)]
pub(super) struct DeviceMode {
    /// Index in DEVICES, or None for a free size.
    pub preset: Option<usize>,
    /// Screen size in CSS pixels (as chosen: portrait for presets).
    pub w: f32,
    pub h: f32,
    pub landscape: bool,
    /// Display scale; 0 = fit the window.
    pub zoom: f32,
    pub frame: bool,
    pub finish: Finish,
    pub touch: bool,
    /// The device list on the left.
    pub panel: bool,
    pub search: String,
    pub category: usize,
    /// The studio's backdrop (index in scene::BACKDROPS).
    pub background: usize,
    /// Show the device's own browser (Safari / Chrome bars) around the page.
    pub browser_ui: bool,
}

/// Where things go in simulator mode (points).
pub(super) struct DevLayout {
    /// The whole drawn device (with its shadow, base, stand).
    pub frame: Rect,
    /// The page.
    pub page: Rect,
    pub scale: f32,
    pub style: Option<Style>,
    /// The person's own frame picture, when there is one for this device.
    pub custom: Option<std::rc::Rc<CustomFrame>>,
    /// The whole studio (its backdrop shows around the device).
    pub area: Rect,
}

impl DeviceMode {
    pub(super) fn device(&self) -> Option<&'static Device> {
        self.preset.and_then(|i| DEVICES.get(i))
    }

    /// Screen size as shown (landscape swaps it).
    pub fn screen(&self) -> Vec2 {
        if self.landscape {
            vec2(self.h, self.w)
        } else {
            vec2(self.w, self.h)
        }
    }

    fn style(&self) -> Option<Style> {
        if self.frame {
            self.device().map(|d| d.style)
        } else {
            None
        }
    }

    /// The browser this device would show: Safari on Apple devices, Chrome on
    /// Android and computers.
    pub(super) fn browser(&self) -> frames::Browser {
        use frames::Browser as B;
        let Some(dev) = self.device().filter(|_| self.browser_ui && self.frame) else { return B::None };
        match (dev.os, dev.kind) {
            (Ios, Phone) => B::SafariPhone,
            (Ios, _) => B::SafariTablet,
            (Android, Phone) => B::ChromePhone,
            (Android, _) => B::ChromeTablet,
            _ => B::ChromeDesktop,
        }
    }

    /// Mobile browser identity for this device (None: Sanaga's own).
    fn user_agent(&self) -> Option<&'static str> {
        let dev = self.device()?;
        match (dev.os, dev.kind) {
            (Ios, Phone) => Some(IOS_PHONE_UA),
            (Ios, _) => Some(IOS_TABLET_UA),
            (Android, Phone) => Some(ANDROID_PHONE_UA),
            (Android, _) => Some(ANDROID_TABLET_UA),
            _ => None,
        }
    }

    pub(super) fn label(&self) -> String {
        match self.device() {
            Some(d) => tr(d.name).to_string(),
            None => tr("Taille libre").to_string(),
        }
    }
}

pub(super) fn now_hm() -> String {
    gtk::glib::DateTime::now_local().ok().and_then(|t| t.format("%H:%M").ok()).map(|s| s.to_string()).unwrap_or_else(|| "12:00".into())
}

/// A cairo picture as an egui picture.
fn surface_to_color_image(s: &mut gtk::cairo::ImageSurface) -> Option<egui::ColorImage> {
    let (w, h, stride) = (s.width() as usize, s.height() as usize, s.stride() as usize);
    let data = s.data().ok()?;
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for c in data[y * stride..y * stride + w * 4].as_chunks::<4>().0 {
            px.extend_from_slice(&[c[2], c[1], c[0], c[3]]);
        }
    }
    Some(egui::ColorImage::from_rgba_premultiplied([w, h], &px))
}

impl Sanaga {
    pub(super) fn toggle_devices(&mut self) {
        if self.device.is_some() {
            self.device = None;
        } else {
            let k = DEVICES.iter().position(|d| d.name == "iPhone 16").unwrap_or(0);
            self.device = Some(DeviceMode {
                preset: Some(k),
                w: DEVICES[k].w,
                h: DEVICES[k].h,
                landscape: false,
                zoom: 0.0,
                frame: true,
                finish: Finish::Titanium,
                touch: true,
                panel: true,
                search: String::new(),
                category: 0,
                background: 0,
                browser_ui: true,
            });
        }
    }

    pub(super) fn device_layout(&self, content: Rect) -> Option<DevLayout> {
        let d = self.device.as_ref()?;
        let t = self.tabs.get(self.active)?;
        if !matches!(t.view, View::Web) {
            return None;
        }
        let screen = d.screen();
        let style = d.style();
        let custom = if d.frame { d.device().and_then(|dev| self.custom_frame(dev.name, d.landscape)) } else { None };
        let g = if custom.is_some() { None } else { style.map(|s| frames::geom_browser(s, screen.x, screen.y, d.landscape, d.browser())) };
        // the person's picture: its see-through screen is the device's screen size
        let k = custom.as_ref().map(|c| c.hole.2 as f32 / screen.x).unwrap_or(1.0);
        let (pic_w, pic_h) = match (&custom, g) {
            (Some(c), _) => (c.img.width() as f32 / k, c.img.height() as f32 / k),
            (None, Some(g)) => (g.width(), g.height()),
            _ => (screen.x, screen.y),
        };
        // the studio: tool bar and device list on the left, name bar above
        let (_, _, room) = super::scene::scene_parts(content, d.panel);
        let fit = (room.width() / pic_w).min(room.height() / pic_h).clamp(0.05, 1.0);
        let scale = if d.zoom > 0.0 { d.zoom.min(fit) } else { fit };
        let size = vec2(pic_w, pic_h) * scale;
        let top = room.top() + ((room.height() - size.y) / 2.0).max(0.0) * 0.35;
        let frame = Rect::from_min_size(pos2(room.center().x - size.x / 2.0, top), size);
        let page = match (&custom, g) {
            (Some(c), _) => Rect::from_min_size(frame.min + vec2(c.hole.0 as f32, c.hole.1 as f32) / k * scale, screen * scale),
            (None, Some(g)) => {
                let (x, y, w, h) = g.page();
                Rect::from_min_size(frame.min + vec2(x, y) * scale, vec2(w, h) * scale)
            }
            _ => frame,
        };
        Some(DevLayout { frame, page, scale, style, custom, area: content })
    }

    /// The person's frame picture for a device (read once, kept).
    pub(super) fn custom_frame(&self, name: &str, landscape: bool) -> Option<std::rc::Rc<CustomFrame>> {
        let key = (name.to_string(), landscape);
        if let Some(c) = self.custom_frames.borrow().get(&key) {
            return c.clone();
        }
        let c = myframes::load(name, landscape).map(std::rc::Rc::new);
        self.custom_frames.borrow_mut().insert(key, c.clone());
        c
    }

    /// Where the page goes in simulator mode (None: normal page area).
    pub(super) fn device_viewport(&self, content: Rect) -> Option<Rect> {
        self.device_layout(content).map(|x| x.page)
    }

    /// Display scale of the simulated screen (page zoom follows it).
    pub(super) fn device_scale(&self, content: Rect) -> f32 {
        self.device_layout(content).map(|x| x.scale).unwrap_or(1.0)
    }

    /// The shown page for the device's browser: site name, title, secure.
    pub(super) fn device_page_info(&self) -> (String, String, bool) {
        let Some(t) = self.tabs.get(self.active) else { return Default::default() };
        let url = t.url().to_string();
        let domain = url::Url::parse(&url).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string())).unwrap_or_else(|| url.clone());
        (domain, t.title(), url.starts_with("https://"))
    }

    /// The drawn device for this layout (made again only when it changes).
    fn device_frame_picture(&mut self, lay: &DevLayout) -> Option<(gtk::cairo::ImageSurface, egui::TextureHandle)> {
        let d = self.device.as_ref()?;
        let ppp = self.ctx.pixels_per_point();
        let s = lay.scale * ppp;
        let screen = d.screen();
        let time = now_hm();
        let (pw, ph) = ((lay.frame.width() * ppp).round() as u32, (lay.frame.height() * ppp).round() as u32);
        // the backdrop's colour at the top and the bottom of the page (it
        // fills the page's square corners that stick out of the device)
        let (_, bg_top, bg_bottom) = super::scene::BACKDROPS[d.background.min(super::scene::BACKDROPS.len() - 1)];
        let at = |y: f32| {
            let t = ((y - lay.area.top()) / lay.area.height().max(1.0)).clamp(0.0, 1.0);
            [0, 1, 2].map(|i| (bg_top[i] as f32 + (bg_bottom[i] as f32 - bg_top[i] as f32) * t).round() as u8)
        };
        let (c_top, c_bottom) = (at(lay.page.top()), at(lay.page.bottom()));
        // what the device's own browser shows: the site's name and title
        let browser = d.browser();
        let (domain, title, secure) = self.device_page_info();
        let key = match (&lay.custom, lay.style) {
            (Some(c), _) => format!("custom{}{}{pw}x{ph}{c_top:?}{c_bottom:?}", c.path.display(), d.landscape),
            (None, Some(style)) => format!("{style:?}{:?}{}x{}{}{:.4}{time}{c_top:?}{c_bottom:?}{browser:?}{domain}{title}{secure}", d.finish, screen.x, screen.y, d.landscape, s),
            _ => return None,
        };
        if let Some((k, surf, tex)) = &self.device_frame {
            if *k == key {
                return Some((surf.clone(), tex.clone()));
            }
        }
        let mut surf = match (&lay.custom, lay.style) {
            (Some(c), _) => {
                let surf = myframes::to_surface(&c.img, pw, ph)?;
                myframes::cover_corners(&surf, c, pw as f64 / c.img.width().max(1) as f64, c_top, c_bottom);
                surf
            }
            (None, Some(style)) => {
                let page = frames::Page { browser, domain: &domain, title: &title, secure };
                let surf = frames::render(style, d.finish, screen.x, screen.y, d.landscape, s, &time, false, &page)?;
                frames::cover_corners(&surf, style, screen.x, screen.y, d.landscape, s, browser, c_top, c_bottom);
                surf
            }
            _ => return None,
        };
        let img = surface_to_color_image(&mut surf)?;
        let tex = self.ctx.load_texture("device-frame", img, egui::TextureOptions::LINEAR);
        self.device_frame = Some((key, surf.clone(), tex.clone()));
        Some((surf, tex))
    }

    /// The drawn device over the page (the GTK window shows it above the page
    /// so the screen's rounded corners and camera cover the site, as on the real device).
    pub(super) fn device_overlay(&mut self, content: Rect, hide: bool) {
        let lay = if hide { None } else { self.device_layout(content) };
        let pic = lay.as_ref().and_then(|l| self.device_frame_picture(l).map(|p| (l.frame, p.0)));
        let zf = self.ctx.zoom_factor();
        crate::shell::set_device_frame(pic.map(|(r, surf)| ([(r.left() * zf).round() as i32, (r.top() * zf).round() as i32], surf)));
    }

    /// The identity the shown tab should have: mobile user agent / touch.
    fn wanted_identity(&self) -> Option<String> {
        let d = self.device.as_ref()?;
        let ua = d.user_agent();
        let touch = d.touch && d.device().is_some_and(|x| x.kind != Computer);
        (ua.is_some() || touch).then(|| format!("{}|{}", ua.unwrap_or(""), touch))
    }

    pub(super) fn device_identity_stale(&self) -> bool {
        let Some(t) = self.tabs.get(self.active) else { return false };
        match self.wanted_identity() {
            None => self.device_ua.is_some(),
            Some(w) => t.web.is_some() && self.device_ua.as_ref() != Some(&(t.id, w)),
        }
    }

    /// Give the shown tab its device identity (and give the previous tab its
    /// own back); the changed pages reload.
    pub(super) fn apply_device_identity(&mut self) {
        let want = self.wanted_identity();
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let old = self.device_ua.take();
        let touch_script = webkit6::UserScript::new(TOUCH_JS, webkit6::UserContentInjectedFrames::AllFrames, webkit6::UserScriptInjectionTime::Start, &[], &[]);
        for t in &self.tabs {
            let Some(w) = &t.web else { continue };
            let wk = w.webview();
            let is_old = old.as_ref().map(|o| o.0) == Some(t.id);
            let is_new = Some(t.id) == active_id && want.is_some();
            if is_old && (!is_new || old.as_ref().map(|o| &o.1) != want.as_ref()) {
                if let Some(st) = WebViewExt::settings(&wk) {
                    st.set_user_agent(None);
                }
                if let (Some(ucm), Some(s)) = (wk.user_content_manager(), &self.device_touch) {
                    ucm.remove_script(s);
                }
                if !is_new {
                    let _ = w.reload();
                }
            }
            if is_new {
                let key = want.clone().unwrap_or_default();
                let (ua, touch) = key.split_once('|').unwrap_or(("", "false"));
                if let Some(st) = WebViewExt::settings(&wk) {
                    st.set_user_agent(if ua.is_empty() { None } else { Some(ua) });
                }
                if touch == "true" {
                    if let Some(ucm) = wk.user_content_manager() {
                        ucm.add_script(&touch_script);
                    }
                }
                let _ = w.reload();
                self.device_ua = Some((t.id, key));
            }
        }
        self.device_touch = Some(touch_script);
    }

    /// The device list, on the left: search, categories, devices by brand.
    pub(super) fn device_panel(&mut self, ui: &mut Ui, th: &Theme) {
        let Some(mut d) = self.device.clone() else { return };
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Appareils")).size(15.0).family(bold()).color(th.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.add(egui::Button::new("×").frame(false)).tip(tr("Masquer la liste")).clicked() {
                    d.panel = false;
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.add(egui::TextEdit::singleline(&mut d.search).hint_text(tr("Rechercher un appareil")).desired_width(ui.available_width() - 12.0));
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(10.0);
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (k, c) in CATEGORIES.iter().enumerate() {
                if ui.selectable_label(d.category == k, tr(c)).clicked() {
                    d.category = k;
                }
            }
        });
        ui.add_space(6.0);
        let q = d.search.trim().to_lowercase();
        let mut pick: Option<usize> = None;
        ScrollArea::vertical().id_salt("device-list").auto_shrink(false).max_height(ui.available_height() - 126.0).show(ui, |ui| {
            let mut last_brand = "";
            for (k, dev) in DEVICES.iter().enumerate() {
                let in_cat = match d.category {
                    1 => dev.os == Ios && dev.kind == Phone,
                    2 => dev.os == Android && dev.kind == Phone,
                    3 => dev.kind == Tablet,
                    4 => dev.kind == Computer,
                    _ => true,
                };
                if !in_cat || (!q.is_empty() && !dev.name.to_lowercase().contains(&q) && !dev.brand.to_lowercase().contains(&q)) {
                    continue;
                }
                if dev.brand != last_brand {
                    last_brand = dev.brand;
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.label(RichText::new(tr(dev.brand).to_uppercase()).size(11.0).family(bold()).color(th.sub));
                    });
                }
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, 50.0), Sense::click());
                let row = r.shrink2(vec2(8.0, 2.0));
                let on = d.preset == Some(k);
                if on {
                    ui.painter().rect_filled(row, CornerRadius::same(8), th.accent.gamma_multiply(0.16));
                } else if resp.hovered() {
                    ui.painter().rect_filled(row, CornerRadius::same(8), th.omnibox_hover);
                }
                paint_silhouette(ui.painter(), Rect::from_center_size(pos2(row.left() + 22.0, row.center().y), vec2(30.0, 40.0)), dev, if on { th.accent } else { th.sub });
                let text = ui.painter().with_clip_rect(row.shrink(2.0));
                text.text(pos2(row.left() + 44.0, row.center().y - 8.0), Align2::LEFT_CENTER, tr(dev.name), FontId::proportional(13.0), th.text);
                text.text(pos2(row.left() + 44.0, row.center().y + 9.0), Align2::LEFT_CENTER, format!("{} × {}", dev.w, dev.h), FontId::proportional(11.5), th.sub);
                if resp.clicked() {
                    pick = Some(k);
                }
            }
        });
        if let Some(k) = pick {
            d.preset = Some(k);
            d.w = DEVICES[k].w;
            d.h = DEVICES[k].h;
        }
        // the person's own frame pictures
        ui.separator();
        let mut reload_frames = false;
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if ui.button(tr("Mes cadres…")).tip(tr("Ajoutez vos propres images de cadre : PNG avec l'écran transparent, nommé comme l'appareil (ex. « iPhone 16.png »).")).clicked() {
                myframes::open_folder();
                reload_frames = true;
            }
            if ui.button(tr("Actualiser")).tip(tr("Relire le dossier des cadres")).clicked() {
                reload_frames = true;
            }
        });
        if reload_frames {
            self.custom_frames.borrow_mut().clear();
            self.device_frame = None;
        }
        // free size at the bottom
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let on = d.preset.is_none();
            if ui.selectable_label(on, tr("Taille libre")).clicked() {
                d.preset = None;
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut w = d.w;
            let mut h = d.h;
            let rw = ui.add(egui::DragValue::new(&mut w).range(200.0..=3840.0).speed(2.0).suffix(" px"));
            ui.label("×");
            let rh = ui.add(egui::DragValue::new(&mut h).range(200.0..=2560.0).speed(2.0).suffix(" px"));
            if rw.changed() || rh.changed() {
                d.w = w;
                d.h = h;
                d.preset = None;
            }
        });
        self.device = Some(d);
    }

    /// The drawn device (under the page's own picture of it, see device_overlay).
    pub(super) fn paint_device_frame(&mut self, ui: &Ui, area: Rect, th: &Theme) {
        let Some(lay) = self.device_layout(area) else { return };
        match self.device_frame_picture(&lay) {
            Some((_, tex)) => {
                egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), lay.frame.size())).paint_at(ui, lay.frame);
            }
            None => {
                let p = ui.painter();
                p.rect_filled(lay.page.expand(1.0).translate(vec2(0.0, 3.0)), CornerRadius::same(4), Color32::from_black_alpha(30));
                p.rect_stroke(lay.page.expand(1.0), CornerRadius::same(2), Stroke::new(1.0, th.border), egui::StrokeKind::Outside);
            }
        }
    }

    /// Screenshot: 0 with the device, 1 the screen only, 2 the whole page.
    pub(super) fn device_capture(&mut self, what: u8) {
        let Some(d) = self.device.clone() else { return };
        let Some(w) = self.tabs.get(self.active).and_then(|t| t.web.as_ref()) else { return };
        let region = if what == 2 { webkit6::SnapshotRegion::FullDocument } else { webkit6::SnapshotRegion::Visible };
        let tx = self.web_tx.clone();
        let ctx = self.ctx.clone();
        let style = if what == 0 && d.frame { d.device().map(|x| x.style) } else { None };
        let custom = if what == 0 && d.frame { d.device().and_then(|dev| self.custom_frame(dev.name, d.landscape)) } else { None };
        let (screen, finish, landscape) = (d.screen(), d.finish, d.landscape);
        let name = d.label();
        let time = now_hm();
        let browser = d.browser();
        let (domain, title, secure) = self.device_page_info();
        w.webview().snapshot(region, webkit6::SnapshotOptions::NONE, None::<&webkit6::gio::Cancellable>, move |res| {
            let msg = match res {
                Ok(page) => {
                    let stamp = crate::store::now();
                    let file = crate::net::unique_path(&crate::net::download_dir(), &format!("Sanaga {} {}.png", name.replace(['/', '\\'], "-"), stamp));
                    let look = frames::Page { browser, domain: &domain, title: &title, secure };
                    match save_capture(&page, style, custom.as_deref(), finish, screen, landscape, &time, &look, &file) {
                        Ok(()) => tf("Capture enregistrée : {}", &[&file.display()]),
                        Err(e) => tf("Capture impossible : {}", &[&e]),
                    }
                }
                Err(e) => tf("Capture impossible : {}", &[&e]),
            };
            let _ = tx.send(super::web::WebEvent::Notice(msg));
            ctx.request_repaint();
        });
    }
}

/// The page picture, inside the drawn device when `style` is given.
#[allow(clippy::too_many_arguments)]
fn save_capture(page: &gtk::gdk::Texture, style: Option<Style>, custom: Option<&CustomFrame>, finish: Finish, screen: Vec2, landscape: bool, time: &str, look: &frames::Page, file: &std::path::Path) -> Result<(), String> {
    let out = compose_capture(page, style, custom, finish, screen, landscape, time, look)?;
    let mut f = std::fs::File::create(file).map_err(|e| e.to_string())?;
    out.write_to_png(&mut f).map_err(|e| e.to_string())
}

/// The page picture inside the drawn device (screenshots, video frames).
#[allow(clippy::too_many_arguments)]
pub(super) fn compose_capture(page: &gtk::gdk::Texture, style: Option<Style>, custom: Option<&CustomFrame>, finish: Finish, screen: Vec2, landscape: bool, time: &str, look: &frames::Page) -> Result<gtk::cairo::ImageSurface, String> {
    use gtk::cairo::{Context, Format, ImageSurface};
    let page_img = crate::page::texture_to_surface(page).ok_or_else(|| "image".to_string())?;
    let out = match (custom, style) {
        (Some(c), _) => {
            // the person's picture, sized so its screen matches the page
            let k = page_img.width() as f32 / c.hole.2 as f32;
            let (w, h) = ((c.img.width() as f32 * k).round() as u32, (c.img.height() as f32 * k).round() as u32);
            let frame = myframes::to_surface(&c.img, w, h).ok_or("frame")?;
            let out = ImageSurface::create(Format::ARgb32, w as i32, h as i32).map_err(|e| e.to_string())?;
            {
                let cr = Context::new(&out).map_err(|e| e.to_string())?;
                cr.set_source_surface(&page_img, (c.hole.0 as f32 * k) as f64, (c.hole.1 as f32 * k) as f64).map_err(|e| e.to_string())?;
                cr.paint().map_err(|e| e.to_string())?;
                cr.set_source_surface(&frame, 0.0, 0.0).map_err(|e| e.to_string())?;
                cr.paint().map_err(|e| e.to_string())?;
            }
            out
        }
        (None, None) => page_img,
        (None, Some(style)) => {
            let g = frames::geom_browser(style, screen.x, screen.y, landscape, look.browser);
            let (_, _, pw, _) = g.page();
            let s = page_img.width() as f32 / pw;
            let frame = frames::render(style, finish, screen.x, screen.y, landscape, s, time, false, look).ok_or("frame")?;
            let out = ImageSurface::create(Format::ARgb32, frame.width(), frame.height()).map_err(|e| e.to_string())?;
            {
                let cr = Context::new(&out).map_err(|e| e.to_string())?;
                let (x, y, _, _) = g.page();
                cr.set_source_surface(&page_img, (x * s) as f64, (y * s) as f64).map_err(|e| e.to_string())?;
                cr.paint().map_err(|e| e.to_string())?;
                cr.set_source_surface(&frame, 0.0, 0.0).map_err(|e| e.to_string())?;
                cr.paint().map_err(|e| e.to_string())?;
            }
            out
        }
    };
    Ok(out)
}

/// A small outline of the device for the list.
fn paint_silhouette(p: &egui::Painter, area: Rect, dev: &Device, col: Color32) {
    let ratio = dev.w / dev.h;
    let (w, h) = if ratio < 1.0 { (area.height() * ratio, area.height()) } else { (area.width(), area.width() / ratio) };
    let (w, h) = (w.min(area.width()), h.min(area.height()));
    let body = Rect::from_center_size(area.center(), vec2(w, h));
    let stroke = Stroke::new(1.6, col);
    match dev.style {
        Style::Laptop => {
            let scr = Rect::from_min_max(body.min, pos2(body.max.x, body.max.y - 4.0));
            p.rect_stroke(scr.shrink2(vec2(3.0, 0.0)), CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            p.line_segment([pos2(body.left(), body.bottom() - 2.0), pos2(body.right(), body.bottom() - 2.0)], Stroke::new(2.2, col));
        }
        Style::Monitor => {
            let scr = Rect::from_min_max(body.min, pos2(body.max.x, body.max.y - 6.0));
            p.rect_stroke(scr, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            p.line_segment([pos2(body.center().x, scr.bottom()), pos2(body.center().x, body.bottom())], stroke);
            p.line_segment([pos2(body.center().x - 6.0, body.bottom()), pos2(body.center().x + 6.0, body.bottom())], stroke);
        }
        _ => {
            let rad = match dev.style {
                Style::Tablet => 3,
                Style::Classic => 4,
                _ => 6,
            };
            p.rect_stroke(body, CornerRadius::same(rad), stroke, egui::StrokeKind::Middle);
            let top = pos2(body.center().x, body.top() + 3.5);
            match dev.style {
                Style::Island => {
                    p.rect_filled(Rect::from_center_size(top, vec2(7.0, 2.6)), CornerRadius::same(1), col);
                }
                Style::Notch => {
                    p.rect_filled(Rect::from_center_size(pos2(top.x, body.top() + 2.0), vec2(9.0, 2.6)), CornerRadius::same(1), col);
                }
                Style::Classic => {
                    p.circle_stroke(pos2(body.center().x, body.bottom() - 3.5), 1.8, Stroke::new(1.0, col));
                }
                _ => {
                    p.circle_filled(top, 1.3, col);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_sensible() {
        assert!(DEVICES.len() >= 100);
        assert!(DEVICES.iter().all(|d| d.w >= 300.0 && d.h >= 560.0 && d.w < 4000.0));
    }
}

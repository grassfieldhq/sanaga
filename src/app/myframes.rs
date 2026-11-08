//! The person's own device frames for the mobile simulator: PNG / WebP
//! pictures with a see-through screen, put in Sanaga's "cadres" folder and
//! named like the device ("iPhone 16.png"). Sanaga finds the screen by itself.

use std::path::PathBuf;

pub(super) struct CustomFrame {
    pub img: image::RgbaImage,
    /// The see-through screen: x, y, w, h (pixels).
    pub hole: (u32, u32, u32, u32),
    /// Rounded corners of that screen (pixels).
    pub corner: u32,
    pub path: PathBuf,
}

/// How rounded the screen's bottom-left corner is: walk in from the corner
/// along the diagonal until the picture becomes see-through.
pub(super) fn corner_radius(img: &image::RgbaImage, hole: (u32, u32, u32, u32)) -> u32 {
    let (x0, y0, w, h) = hole;
    let bottom = y0 + h - 1;
    let mut k = 0;
    while k < w.min(h) / 3 && img.get_pixel(x0 + k, bottom - k).0[3] >= 40 {
        k += 1;
    }
    // on a circle of radius r, the diagonal leaves the corner after r·(1 − 1/√2)
    (k as f32 / (1.0 - std::f32::consts::FRAC_1_SQRT_2)).round() as u32
}

pub(super) fn frames_dir() -> PathBuf {
    crate::store::Store::root_dir().join("cadres")
}

/// File name form of a device name: "iPhone 15 / 15 Pro" -> "iphone-15-15-pro".
pub(super) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The see-through screen of a frame picture: the transparent area around its centre.
pub(super) fn find_hole(img: &image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    if w < 8 || h < 8 {
        return None;
    }
    let (cx, cy) = (w / 2, h / 2);
    let see_through = |x: u32, y: u32| img.get_pixel(x, y).0[3] < 40;
    if !see_through(cx, cy) {
        return None;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (cx, cx, cy, cy);
    while x0 > 0 && see_through(x0 - 1, cy) {
        x0 -= 1;
    }
    while x1 + 1 < w && see_through(x1 + 1, cy) {
        x1 += 1;
    }
    while y0 > 0 && see_through(cx, y0 - 1) {
        y0 -= 1;
    }
    while y1 + 1 < h && see_through(cx, y1 + 1) {
        y1 += 1;
    }
    let (hw, hh) = (x1 - x0 + 1, y1 - y0 + 1);
    (hw > w / 4 && hh > h / 4).then_some((x0, y0, hw, hh))
}

/// The person's frame for this device, if any (turned a quarter for landscape).
pub(super) fn load(name: &str, landscape: bool) -> Option<CustomFrame> {
    let want = slug(name);
    let path = std::fs::read_dir(frames_dir()).ok()?.flatten().map(|e| e.path()).find(|p| {
        let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
        (ext == "png" || ext == "webp") && p.file_stem().and_then(|s| s.to_str()).map(slug).as_deref() == Some(want.as_str())
    })?;
    let img = image::open(&path).ok()?.to_rgba8();
    let img = if landscape { image::imageops::rotate270(&img) } else { img };
    let hole = find_hole(&img)?;
    let corner = corner_radius(&img, hole);
    Some(CustomFrame { img, hole, corner, path })
}

/// A picture resized to `w` × `h`, as a cairo picture (premultiplied).
pub(super) fn to_surface(img: &image::RgbaImage, w: u32, h: u32) -> Option<gtk::cairo::ImageSurface> {
    let img = image::imageops::resize(img, w.max(1), h.max(1), image::imageops::FilterType::Triangle);
    let mut surf = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w.max(1) as i32, h.max(1) as i32).ok()?;
    let stride = surf.stride() as usize;
    {
        let mut data = surf.data().ok()?;
        for (y, row) in img.rows().enumerate() {
            for (x, p) in row.enumerate() {
                let [r, g, b, a] = p.0;
                let m = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
                let i = y * stride + x * 4;
                data[i..i + 4].copy_from_slice(&[m(b), m(g), m(r), a]);
            }
        }
    }
    surf.mark_dirty();
    Some(surf)
}

/// Paint the backdrop colour (`top`, `bottom`) under the picture in the corners
/// of its screen, outside the screen's curve: the page's square corners must
/// not show past a rounded device. `k` = picture pixels per frame pixel.
pub(super) fn cover_corners(surf: &gtk::cairo::ImageSurface, c: &CustomFrame, k: f64, top: [u8; 3], bottom: [u8; 3]) {
    let Ok(cr) = gtk::cairo::Context::new(surf) else { return };
    let (x, y, w, h) = (c.hole.0 as f64 * k, c.hole.1 as f64 * k, c.hole.2 as f64 * k, c.hole.3 as f64 * k);
    let r = (c.corner as f64 * k).min(w / 2.0).min(h / 2.0);
    if r < 1.0 {
        return;
    }
    cr.set_operator(gtk::cairo::Operator::DestOver);
    for (cx, cy, col) in [(x, y, top), (x + w - r, y, top), (x, y + h - r, bottom), (x + w - r, y + h - r, bottom)] {
        let _ = cr.save();
        cr.rectangle(cx, cy, r, r);
        cr.clip();
        cr.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
        cr.rectangle(cx, cy, r, r);
        // the screen's rounded rectangle
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
        cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        cr.close_path();
        cr.set_source_rgb(col[0] as f64 / 255.0, col[1] as f64 / 255.0, col[2] as f64 / 255.0);
        let _ = cr.fill();
        let _ = cr.restore();
    }
    surf.flush();
}

/// Open the frames folder (made on first use, with a short how-to).
pub(super) fn open_folder() {
    let dir = frames_dir();
    let _ = std::fs::create_dir_all(&dir);
    let howto = dir.join("LISEZ-MOI.txt");
    if !howto.exists() {
        let text = crate::i18n::tr("Cadres d'appareils pour le simulateur mobile de Sanaga.\n\nDéposez ici une image PNG (ou WebP) par appareil : l'appareil avec son écran transparent.\nNommez le fichier comme l'appareil dans la liste, par exemple « iPhone 16.png » ou « Galaxy S20 Ultra.png ».\nSanaga trouve l'écran tout seul et y place la page. Utilisez des images que vous avez le droit d'utiliser.\nCliquez ensuite sur « Actualiser » dans la liste des appareils.\n");
        let _ = std::fs::write(&howto, text);
    }
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_screen_detection() {
        assert_eq!(slug("iPhone 15 / 15 Pro"), "iphone-15-15-pro");
        assert_eq!(slug("Galaxy S20 Ultra.png".trim_end_matches(".png")), "galaxy-s20-ultra");
        let mut img = image::RgbaImage::from_pixel(100, 200, image::Rgba([20, 20, 20, 255]));
        for y in 20..180 {
            for x in 10..90 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
        assert_eq!(find_hole(&img), Some((10, 20, 80, 160)));
        let full = image::RgbaImage::from_pixel(50, 50, image::Rgba([1, 1, 1, 255]));
        assert_eq!(find_hole(&full), None);
    }
}

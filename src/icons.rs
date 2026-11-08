//! Toolbar icons drawn with simple shapes (no image files, no icon font).

use eframe::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Icon {
    Back,
    Forward,
    Reload,
    Stop,
    Home,
    Star,
    StarFilled,
    Menu,
    Close,
    Plus,
    Lock,
    Info,
    Search,
    Download,
    Private,
    Up,
    Down,
    Sidebar,
    Leaf,
    Reader,
    Glasses,
    Share,
    Mail,
    Print,
    Grid,
    Speaker,
    SpeakerMuted,
    Shield,
    Phone,
    Camera,
    Rotate,
    Palette,
    Touch,
    Record,
}

pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 2.0; // half size
    let st = Stroke::new((s * 0.16).max(1.4), color);
    match icon {
        Icon::Back | Icon::Forward => {
            let dir = if icon == Icon::Back { -1.0 } else { 1.0 };
            let tip = pos2(c.x + dir * s * 0.62, c.y);
            p.line_segment([pos2(c.x - dir * s * 0.62, c.y), tip], st);
            p.line_segment([tip, pos2(tip.x - dir * s * 0.5, c.y - s * 0.5)], st);
            p.line_segment([tip, pos2(tip.x - dir * s * 0.5, c.y + s * 0.5)], st);
        }
        Icon::Up | Icon::Down => {
            let dir = if icon == Icon::Up { -1.0 } else { 1.0 };
            let a = pos2(c.x - s * 0.5, c.y - dir * s * 0.25);
            let b = pos2(c.x, c.y + dir * s * 0.25);
            let d = pos2(c.x + s * 0.5, c.y - dir * s * 0.25);
            p.line_segment([a, b], st);
            p.line_segment([b, d], st);
        }
        Icon::Reload => {
            let r = s * 0.58;
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = -PI * 0.35 + (i as f32 / 24.0) * PI * 1.65;
                    pos2(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            let end = pts[0];
            p.add(Shape::line(pts, st));
            // arrow head at the start of the arc
            let head = s * 0.38;
            p.add(Shape::convex_polygon(
                vec![pos2(end.x + head * 0.55, end.y - head * 0.9), pos2(end.x + head * 0.55, end.y + head * 0.35), pos2(end.x - head * 0.55, end.y - head * 0.2)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop | Icon::Close => {
            let k = if icon == Icon::Close { 0.42 } else { 0.5 };
            p.line_segment([pos2(c.x - s * k, c.y - s * k), pos2(c.x + s * k, c.y + s * k)], st);
            p.line_segment([pos2(c.x + s * k, c.y - s * k), pos2(c.x - s * k, c.y + s * k)], st);
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - s * 0.55, c.y), pos2(c.x + s * 0.55, c.y)], st);
            p.line_segment([pos2(c.x, c.y - s * 0.55), pos2(c.x, c.y + s * 0.55)], st);
        }
        Icon::Home => {
            let roof = [pos2(c.x - s * 0.7, c.y - s * 0.02), pos2(c.x, c.y - s * 0.68), pos2(c.x + s * 0.7, c.y - s * 0.02)];
            p.add(Shape::line(roof.to_vec(), st));
            let body = Rect::from_min_max(pos2(c.x - s * 0.48, c.y - s * 0.15), pos2(c.x + s * 0.48, c.y + s * 0.62));
            p.add(Shape::line(vec![body.left_top(), body.left_bottom(), body.right_bottom(), body.right_top()], st));
        }
        Icon::Star | Icon::StarFilled => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { s * 0.7 } else { s * 0.3 };
                    let a = -PI / 2.0 + i as f32 * PI / 5.0;
                    pos2(c.x + r * a.cos(), c.y + 0.06 * s + r * a.sin())
                })
                .collect();
            if icon == Icon::StarFilled {
                // concave: fill with a fan of triangles
                for i in 0..10 {
                    p.add(Shape::convex_polygon(vec![pos2(c.x, c.y + 0.06 * s), pts[i], pts[(i + 1) % 10]], color, Stroke::NONE));
                }
            } else {
                p.add(Shape::closed_line(pts, st));
            }
        }
        Icon::Menu => {
            for dy in [-0.5, 0.0, 0.5] {
                p.circle_filled(pos2(c.x, c.y + dy * s), s * 0.12, color);
            }
        }
        Icon::Lock => {
            let body = Rect::from_center_size(pos2(c.x, c.y + s * 0.22), vec2(s * 1.1, s * 0.85));
            p.rect_filled(body, 2.0, color);
            let r = s * 0.32;
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = PI + (i as f32 / 12.0) * PI;
                    pos2(c.x + r * a.cos(), body.top() - s * 0.08 + r * a.sin())
                })
                .collect();
            let mut pts = pts;
            pts.insert(0, pos2(c.x - r, body.top()));
            pts.push(pos2(c.x + r, body.top()));
            p.add(Shape::line(pts, Stroke::new(st.width * 0.85, color)));
        }
        Icon::Share => {
            // a tray with an arrow going up
            let w = st.width * 0.85;
            p.line_segment([pos2(c.x, c.y - s * 0.75), pos2(c.x, c.y + s * 0.25)], Stroke::new(w, color));
            p.line_segment([pos2(c.x - s * 0.32, c.y - s * 0.45), pos2(c.x, c.y - s * 0.77)], Stroke::new(w, color));
            p.line_segment([pos2(c.x + s * 0.32, c.y - s * 0.45), pos2(c.x, c.y - s * 0.77)], Stroke::new(w, color));
            p.add(Shape::line(
                vec![pos2(c.x - s * 0.3, c.y - s * 0.15), pos2(c.x - s * 0.6, c.y - s * 0.15), pos2(c.x - s * 0.6, c.y + s * 0.75), pos2(c.x + s * 0.6, c.y + s * 0.75), pos2(c.x + s * 0.6, c.y - s * 0.15), pos2(c.x + s * 0.3, c.y - s * 0.15)],
                Stroke::new(w, color),
            ));
        }
        Icon::Mail => {
            let r = Rect::from_center_size(c, vec2(s * 1.5, s * 1.05));
            let w = st.width * 0.85;
            p.rect_stroke(r, 2.0, Stroke::new(w, color), egui::StrokeKind::Middle);
            p.line_segment([r.left_top(), pos2(c.x, c.y + s * 0.05)], Stroke::new(w, color));
            p.line_segment([r.right_top(), pos2(c.x, c.y + s * 0.05)], Stroke::new(w, color));
        }
        Icon::Print => {
            let w = st.width * 0.85;
            let body = Rect::from_min_max(pos2(c.x - s * 0.75, c.y - s * 0.25), pos2(c.x + s * 0.75, c.y + s * 0.35));
            p.rect_stroke(body, 2.0, Stroke::new(w, color), egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(pos2(c.x - s * 0.45, c.y - s * 0.75), pos2(c.x + s * 0.45, c.y - s * 0.25)), 1.0, Stroke::new(w, color), egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(pos2(c.x - s * 0.45, c.y + s * 0.1), pos2(c.x + s * 0.45, c.y + s * 0.75)), 1.0, Stroke::new(w, color), egui::StrokeKind::Middle);
        }
        Icon::Phone => {
            let w = st.width * 0.85;
            let body = Rect::from_center_size(c, vec2(s * 1.05, s * 1.75));
            p.rect_stroke(body, s * 0.22, Stroke::new(w, color), egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x - s * 0.16, body.top() + s * 0.24), pos2(c.x + s * 0.16, body.top() + s * 0.24)], Stroke::new(w, color));
        }
        Icon::Camera => {
            let w = st.width * 0.85;
            let body = Rect::from_min_max(pos2(c.x - s * 0.8, c.y - s * 0.42), pos2(c.x + s * 0.8, c.y + s * 0.6));
            p.rect_stroke(body, 2.5, Stroke::new(w, color), egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(pos2(c.x - s * 0.3, c.y - s * 0.62), pos2(c.x + s * 0.3, c.y - s * 0.42)), 1.0, color);
            p.circle_stroke(pos2(c.x, c.y + s * 0.08), s * 0.3, Stroke::new(w, color));
        }
        Icon::Palette => {
            let w = st.width * 0.85;
            p.circle_stroke(c, s * 0.8, Stroke::new(w, color));
            for (dx, dy) in [(-0.35, -0.3), (0.05, -0.48), (0.42, -0.2), (-0.42, 0.18)] {
                p.circle_filled(pos2(c.x + dx * s, c.y + dy * s), s * 0.13, color);
            }
            p.circle_filled(pos2(c.x + 0.3 * s, c.y + 0.38 * s), s * 0.2, color);
        }
        Icon::Record => {
            p.circle_stroke(c, s * 0.82, Stroke::new(st.width * 0.85, color));
            p.circle_filled(c, s * 0.42, color);
        }
        Icon::Touch => {
            let w = st.width * 0.85;
            p.circle_stroke(pos2(c.x, c.y - s * 0.35), s * 0.42, Stroke::new(w * 0.7, color));
            p.circle_filled(pos2(c.x, c.y - s * 0.35), s * 0.16, color);
            p.line_segment([pos2(c.x, c.y - s * 0.2), pos2(c.x, c.y + s * 0.8)], Stroke::new(w * 1.4, color));
        }
        Icon::Rotate => {
            let w = st.width * 0.85;
            let r = s * 0.62;
            let n = 14;
            let pts: Vec<_> = (0..=n).map(|k| {
                let a = std::f32::consts::PI * (0.15 + 1.45 * k as f32 / n as f32);
                pos2(c.x + r * a.cos(), c.y - r * a.sin())
            }).collect();
            p.add(Shape::line(pts.clone(), Stroke::new(w, color)));
            let tip = pts[0];
            p.line_segment([tip, pos2(tip.x - s * 0.05, tip.y - s * 0.42)], Stroke::new(w, color));
            p.line_segment([tip, pos2(tip.x - s * 0.42, tip.y + s * 0.02)], Stroke::new(w, color));
        }
        Icon::Grid => {
            let q = s * 0.36;
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let r = Rect::from_center_size(pos2(c.x + dx * s * 0.42, c.y + dy * s * 0.42), vec2(q * 2.0, q * 2.0));
                p.rect_stroke(r, 2.0, Stroke::new(st.width * 0.8, color), egui::StrokeKind::Middle);
            }
        }
        Icon::Speaker | Icon::SpeakerMuted => {
            let w = st.width * 0.85;
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - s * 0.7, c.y - s * 0.25), pos2(c.x - s * 0.35, c.y - s * 0.25), pos2(c.x + s * 0.05, c.y - s * 0.65), pos2(c.x + s * 0.05, c.y + s * 0.65), pos2(c.x - s * 0.35, c.y + s * 0.25), pos2(c.x - s * 0.7, c.y + s * 0.25)],
                color,
                Stroke::NONE,
            ));
            if icon == Icon::Speaker {
                for k in [0.35, 0.65] {
                    let pts: Vec<Pos2> = (0..=8).map(|i| {
                        let a = -0.8 + i as f32 / 8.0 * 1.6;
                        pos2(c.x + s * 0.05 + s * k * a.cos(), c.y + s * k * a.sin())
                    }).collect();
                    p.add(Shape::line(pts, Stroke::new(w, color)));
                }
            } else {
                p.line_segment([pos2(c.x + s * 0.3, c.y - s * 0.3), pos2(c.x + s * 0.8, c.y + s * 0.3)], Stroke::new(w, color));
                p.line_segment([pos2(c.x + s * 0.8, c.y - s * 0.3), pos2(c.x + s * 0.3, c.y + s * 0.3)], Stroke::new(w, color));
            }
        }
        Icon::Shield => {
            let pts = vec![
                pos2(c.x, c.y - s * 0.8),
                pos2(c.x + s * 0.65, c.y - s * 0.55),
                pos2(c.x + s * 0.6, c.y + s * 0.1),
                pos2(c.x, c.y + s * 0.8),
                pos2(c.x - s * 0.6, c.y + s * 0.1),
                pos2(c.x - s * 0.65, c.y - s * 0.55),
            ];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Reader => {
            // a page with lines of text
            let r = Rect::from_center_size(c, vec2(s * 1.25, s * 1.5));
            p.rect_stroke(r, 2.0, Stroke::new(st.width * 0.8, color), egui::StrokeKind::Middle);
            for k in 0..4 {
                let y = r.top() + r.height() * (0.25 + k as f32 * 0.17);
                let x1 = if k == 3 { r.left() + r.width() * 0.55 } else { r.right() - s * 0.22 };
                p.line_segment([pos2(r.left() + s * 0.22, y), pos2(x1, y)], Stroke::new(st.width * 0.7, color));
            }
        }
        Icon::Glasses => {
            // reading glasses: the reading list
            let w = st.width * 0.9;
            p.circle_stroke(pos2(c.x - s * 0.38, c.y + s * 0.1), s * 0.3, Stroke::new(w, color));
            p.circle_stroke(pos2(c.x + s * 0.38, c.y + s * 0.1), s * 0.3, Stroke::new(w, color));
            p.line_segment([pos2(c.x - s * 0.1, c.y + s * 0.02), pos2(c.x + s * 0.1, c.y + s * 0.02)], Stroke::new(w, color));
            p.line_segment([pos2(c.x - s * 0.68, c.y + s * 0.05), pos2(c.x - s * 0.8, c.y - s * 0.25)], Stroke::new(w, color));
            p.line_segment([pos2(c.x + s * 0.68, c.y + s * 0.05), pos2(c.x + s * 0.8, c.y - s * 0.25)], Stroke::new(w, color));
        }
        Icon::Sidebar => {
            let r = Rect::from_center_size(c, vec2(s * 1.5, s * 1.2));
            p.rect_stroke(r, 3.0, Stroke::new(st.width * 0.85, color), egui::StrokeKind::Middle);
            let x = r.left() + r.width() * 0.36;
            p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(st.width * 0.85, color));
            for k in 0..3 {
                let y = r.top() + r.height() * (0.3 + k as f32 * 0.17);
                p.line_segment([pos2(r.left() + s * 0.15, y), pos2(x - s * 0.15, y)], Stroke::new(st.width * 0.6, color));
            }
        }
        Icon::Leaf => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0 * PI;
                    pos2(c.x - s * 0.6 + t / PI * s * 1.2, c.y + s * 0.6 - t / PI * s * 1.2 - t.sin() * s * 0.35)
                })
                .chain((0..=16).map(|i| {
                    let t = PI - i as f32 / 16.0 * PI;
                    pos2(c.x - s * 0.6 + t / PI * s * 1.2, c.y + s * 0.6 - t / PI * s * 1.2 + t.sin() * s * 0.35)
                }))
                .collect();
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Info => {
            p.circle_stroke(c, s * 0.68, Stroke::new(st.width * 0.8, color));
            p.line_segment([pos2(c.x, c.y - s * 0.05), pos2(c.x, c.y + s * 0.36)], st);
            p.circle_filled(pos2(c.x, c.y - s * 0.3), s * 0.09, color);
        }
        Icon::Search => {
            let cc = pos2(c.x - s * 0.15, c.y - s * 0.15);
            p.circle_stroke(cc, s * 0.42, st);
            p.line_segment([pos2(cc.x + s * 0.3, cc.y + s * 0.3), pos2(c.x + s * 0.62, c.y + s * 0.62)], st);
        }
        Icon::Download => {
            p.line_segment([pos2(c.x, c.y - s * 0.65), pos2(c.x, c.y + s * 0.2)], st);
            p.line_segment([pos2(c.x - s * 0.38, c.y - s * 0.15), pos2(c.x, c.y + s * 0.22)], st);
            p.line_segment([pos2(c.x + s * 0.38, c.y - s * 0.15), pos2(c.x, c.y + s * 0.22)], st);
            p.line_segment([pos2(c.x - s * 0.6, c.y + s * 0.62), pos2(c.x + s * 0.6, c.y + s * 0.62)], st);
        }
        Icon::Private => {
            // a hat and glasses
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - s * 0.45, c.y - s * 0.05), pos2(c.x - s * 0.3, c.y - s * 0.62), pos2(c.x + s * 0.3, c.y - s * 0.62), pos2(c.x + s * 0.45, c.y - s * 0.05)],
                color,
                Stroke::NONE,
            ));
            p.line_segment([pos2(c.x - s * 0.8, c.y - s * 0.05), pos2(c.x + s * 0.8, c.y - s * 0.05)], st);
            p.circle_stroke(pos2(c.x - s * 0.35, c.y + s * 0.4), s * 0.22, st);
            p.circle_stroke(pos2(c.x + s * 0.35, c.y + s * 0.4), s * 0.22, st);
        }
    }
}

/// The Sanaga logo: a round badge with a river flowing through it.
pub fn logo(p: &Painter, center: Pos2, radius: f32) {
    let deep = Color32::from_rgb(0x0B, 0x6E, 0x5C);
    let mid = Color32::from_rgb(0x12, 0x9C, 0x7E);
    let water = Color32::from_rgb(0x4F, 0xC3, 0xE8);
    p.circle_filled(center, radius, deep);
    p.circle_filled(pos2(center.x - radius * 0.12, center.y - radius * 0.12), radius * 0.86, mid);
    for (k, col, w) in [(-0.22, water, 0.2), (0.22, Color32::WHITE, 0.13)] {
        let pts: Vec<Pos2> = (0..=40)
            .map(|i| {
                let t = i as f32 / 40.0;
                let x = center.x - radius * 0.78 + t * radius * 1.56;
                let y = center.y + radius * k + (t * PI * 2.0).sin() * radius * 0.2;
                pos2(x, y)
            })
            .filter(|q| q.distance(center) < radius * 0.93)
            .collect();
        p.add(Shape::line(pts, Stroke::new(radius * w, col)));
    }
}

/// RGBA pixels for the window icon (same badge).
pub fn window_icon(size: u32) -> egui::IconData {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let r = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - r;
            let dy = y as f32 + 0.5 - r;
            let d = (dx * dx + dy * dy).sqrt();
            if d > r - 0.5 {
                continue;
            }
            let t = x as f32 / size as f32;
            let wave = |k: f32| r + r * k + ((t - 0.0) * PI * 2.0).sin() * r * 0.2;
            let mut col = if d < r * 0.86 { [0x12, 0x9C, 0x7E] } else { [0x0B, 0x6E, 0x5C] };
            let yy = y as f32 + 0.5;
            if (yy - wave(-0.22)).abs() < r * 0.1 && d < r * 0.93 {
                col = [0x4F, 0xC3, 0xE8];
            }
            if (yy - wave(0.22)).abs() < r * 0.07 && d < r * 0.93 {
                col = [0xFF, 0xFF, 0xFF];
            }
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&col);
            rgba[i + 3] = 255;
        }
    }
    egui::IconData { rgba, width: size, height: size }
}

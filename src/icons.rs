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
    Sparkle,
    Puzzle,
    Pencil,
    More,
    Folder,
    /// Two letters of two alphabets: translate.
    Translate,
    /// Two sheets, one over the other: copy.
    Copy,
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
        Icon::Translate => {
            let w = Stroke::new(st.width * 0.85, color);
            // "A" (bottom left)
            let (ax, ab, at) = (c.x - s * 0.42, c.y + s * 0.82, c.y - s * 0.12);
            p.line_segment([pos2(ax - s * 0.42, ab), pos2(ax, at)], w);
            p.line_segment([pos2(ax, at), pos2(ax + s * 0.42, ab)], w);
            p.line_segment([pos2(ax - s * 0.24, ab - s * 0.34), pos2(ax + s * 0.24, ab - s * 0.34)], w);
            // "文" (top right): a bar, its stroke above, two strokes crossing below
            let (kx, kt) = (c.x + s * 0.42, c.y - s * 0.55);
            p.line_segment([pos2(kx - s * 0.48, kt), pos2(kx + s * 0.48, kt)], w);
            p.line_segment([pos2(kx, kt - s * 0.26), pos2(kx, kt)], w);
            p.line_segment([pos2(kx + s * 0.3, kt), pos2(kx - s * 0.4, kt + s * 0.82)], w);
            p.line_segment([pos2(kx - s * 0.24, kt + s * 0.2), pos2(kx + s * 0.44, kt + s * 0.82)], w);
        }
        Icon::Copy => {
            let w = Stroke::new(st.width * 0.85, color);
            let back = Rect::from_min_max(pos2(c.x - s * 0.72, c.y - s * 0.8), pos2(c.x + s * 0.3, c.y + s * 0.38));
            let front = Rect::from_min_max(pos2(c.x - s * 0.28, c.y - s * 0.36), pos2(c.x + s * 0.74, c.y + s * 0.82));
            // (the sheet behind: only what shows past the one in front)
            p.line_segment([pos2(front.left(), back.top()), back.left_top()], w);
            p.line_segment([back.left_top(), back.left_bottom()], w);
            p.line_segment([back.left_bottom(), pos2(front.left(), back.bottom())], w);
            p.line_segment([pos2(front.left(), back.top()), back.right_top()], w);
            p.line_segment([back.right_top(), pos2(back.right(), front.top())], w);
            p.rect_stroke(front, 2.0, w, egui::StrokeKind::Middle);
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
        Icon::Sparkle => {
            // a four-pointed star and a small one: "made for you" (summary, AI)
            let star = |cx: f32, cy: f32, r: f32| {
                // two thin diamonds (each one convex) make the four points
                let k = r * 0.3;
                p.add(Shape::convex_polygon(vec![pos2(cx, cy - r), pos2(cx + k, cy), pos2(cx, cy + r), pos2(cx - k, cy)], color, Stroke::NONE));
                p.add(Shape::convex_polygon(vec![pos2(cx - r, cy), pos2(cx, cy - k), pos2(cx + r, cy), pos2(cx, cy + k)], color, Stroke::NONE));
            };
            star(c.x - s * 0.15, c.y + s * 0.12, s * 0.78);
            star(c.x + s * 0.62, c.y - s * 0.6, s * 0.32);
        }
        Icon::Pencil => {
            let w = Stroke::new(st.width * 0.9, color);
            let a = pos2(c.x - s * 0.7, c.y + s * 0.7);
            let b = pos2(c.x + s * 0.45, c.y - s * 0.45);
            p.line_segment([a, b], Stroke::new(st.width * 2.2, color));
            p.line_segment([pos2(b.x + s * 0.12, b.y - s * 0.12), pos2(b.x + s * 0.3, b.y - s * 0.3)], Stroke::new(st.width * 2.2, color));
            p.line_segment([pos2(a.x - s * 0.08, a.y + s * 0.08), pos2(a.x + s * 0.25, a.y + s * 0.02)], w);
        }
        Icon::More => {
            for k in [-1.0f32, 0.0, 1.0] {
                p.circle_filled(pos2(c.x + k * s * 0.62, c.y), s * 0.16, color);
            }
        }
        Icon::Folder => {
            let w = Stroke::new(st.width * 0.9, color);
            let r = Rect::from_center_size(pos2(c.x, c.y + s * 0.08), vec2(s * 1.7, s * 1.25));
            p.rect_stroke(r, s * 0.18, w, egui::StrokeKind::Middle);
            p.line_segment([pos2(r.left(), r.top() + s * 0.32), pos2(r.right(), r.top() + s * 0.32)], w);
        }
        Icon::Puzzle => {
            let w = Stroke::new(st.width * 0.9, color);
            let r = Rect::from_center_size(pos2(c.x - s * 0.08, c.y + s * 0.08), vec2(s * 1.3, s * 1.3));
            p.rect_stroke(r, s * 0.18, w, egui::StrokeKind::Middle);
            p.circle_filled(pos2(r.center().x, r.top()), s * 0.24, color);
            p.circle_filled(pos2(r.right(), r.center().y), s * 0.24, color);
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

/// Sanaga's logo, in the family of Grassfield's apps: a rounded square in
/// Grassfield green (lighter at the top), with the Sanaga river flowing across.
/// The icon's colours the person can choose (Settings › Appearance): name,
/// top and bottom of the square, the upper wave.
pub const ICON_STYLES: [(&str, [u8; 3], [u8; 3], [u8; 3]); 5] = [
    ("Prairie", [0x5F, 0xB8, 0x66], [0x3B, 0x87, 0x44], [0xD8, 0xF5, 0xDA]),
    ("Océan", [0x5A, 0xA2, 0xF0], [0x2A, 0x5D, 0xB0], [0xD6, 0xE8, 0xFD]),
    ("Crépuscule", [0xF6, 0xA3, 0x5C], [0xD2, 0x53, 0x2A], [0xFD, 0xE3, 0xCF]),
    ("Nuit", [0x4A, 0x51, 0x60], [0x1C, 0x1F, 0x26], [0xC9, 0xD1, 0xDD]),
    ("Lavande", [0xB3, 0x9D, 0xFB], [0x6D, 0x3F, 0xD9], [0xE9, 0xE1, 0xFE]),
];

/// The icon style in use (index in `ICON_STYLES`).
pub static ICON_STYLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn style() -> (&'static str, [u8; 3], [u8; 3], [u8; 3]) {
    ICON_STYLES[(ICON_STYLE.load(std::sync::atomic::Ordering::Relaxed) as usize).min(ICON_STYLES.len() - 1)]
}

fn f3(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32, c[1] as f32, c[2] as f32]
}

/// The logo as an SVG picture in a style, with an optional badge (an emoji,
/// for a profile's own launcher).
pub fn logo_svg(style_index: usize, badge: &str) -> String {
    let (_, top, bottom, wave) = ICON_STYLES[style_index.min(ICON_STYLES.len() - 1)];
    let hex = |c: [u8; 3]| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
    let pts = |k: f32| (0..=48).map(|i| {
        let t = 0.14 + 0.72 * i as f32 / 48.0;
        format!("{:.2},{:.2}", 8.96 + t * 238.08, 8.96 + wave_y(t, k) * 238.08)
    }).collect::<Vec<_>>().join(" ");
    let badge_svg = if badge.trim().is_empty() {
        String::new()
    } else {
        let b = badge.replace('&', "&amp;").replace('<', "&lt;");
        format!("<circle cx=\"196\" cy=\"196\" r=\"54\" fill=\"#FFFFFF\" stroke=\"{}\" stroke-width=\"6\"/><text x=\"196\" y=\"214\" font-size=\"60\" text-anchor=\"middle\">{b}</text>", hex(bottom))
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"256\" height=\"256\" viewBox=\"0 0 256 256\"><defs><linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient></defs>\
<rect x=\"8.96\" y=\"8.96\" width=\"238.08\" height=\"238.08\" rx=\"54.76\" fill=\"url(#g)\"/>\
<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"20.24\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\
<polyline points=\"{}\" fill=\"none\" stroke=\"#FFFFFF\" stroke-width=\"20.24\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>{badge_svg}</svg>\n",
        hex(top),
        hex(bottom),
        pts(WAVES[0].0),
        hex(wave),
        pts(WAVES[1].0)
    )
}

/// The river's two waves: (height from the middle, thickness), as parts of the
/// logo's size; the upper one light mint, the lower one white.
const WAVES: [(f32, f32, [u8; 3]); 2] = [(-0.11, 0.085, [0xD8, 0xF5, 0xDA]), (0.13, 0.085, [0xFF, 0xFF, 0xFF])];

fn wave_y(t: f32, k: f32) -> f32 {
    // t: 0..1 across the logo; result: height as a part of the size
    0.5 + k + (t * PI * 2.0 - PI * 0.35).sin() * 0.075
}

/// The logo drawn in Sanaga's interface (`radius` = half its size).
pub fn logo(p: &Painter, center: Pos2, radius: f32) {
    let size = radius * 2.0;
    let rect = Rect::from_center_size(center, vec2(size, size));
    let corner = (size * 0.23).min(255.0) as u8;
    // the green, lighter at the top: a gradient mesh clipped by rounded corners
    p.rect_filled(rect, egui::CornerRadius::same(corner), lerp_col(0.5));
    let inner = rect.shrink2(vec2(0.0, size * 0.12));
    let mut mesh = egui::Mesh::default();
    let top = lerp_col(0.0);
    let bottom = lerp_col(1.0);
    let band = Rect::from_min_max(pos2(rect.left() + size * 0.0, inner.top()), pos2(rect.right(), inner.bottom()));
    mesh.colored_vertex(band.left_top(), top);
    mesh.colored_vertex(band.right_top(), top);
    mesh.colored_vertex(band.right_bottom(), bottom);
    mesh.colored_vertex(band.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.rect_filled(Rect::from_min_max(rect.left_top(), pos2(rect.right(), inner.top() + 1.0)), egui::CornerRadius { nw: corner, ne: corner, sw: 0, se: 0 }, top);
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), inner.bottom() - 1.0), rect.right_bottom()), egui::CornerRadius { nw: 0, ne: 0, sw: corner, se: corner }, bottom);
    p.add(Shape::mesh(mesh));
    let (_, _, _, wave) = style();
    for (k, w, col) in [(WAVES[0].0, WAVES[0].1, wave), WAVES[1]] {
        let pts: Vec<Pos2> = (0..=48)
            .map(|i| {
                let t = 0.14 + 0.72 * i as f32 / 48.0;
                pos2(rect.left() + t * size, rect.top() + wave_y(t, k) * size)
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(size * w, Color32::from_rgb(col[0], col[1], col[2]))));
        // round ends
        for t in [0.14f32, 0.86] {
            p.circle_filled(pos2(rect.left() + t * size, rect.top() + wave_y(t, k) * size), size * w / 2.0, Color32::from_rgb(col[0], col[1], col[2]));
        }
    }
}

fn lerp_col(t: f32) -> Color32 {
    let (_, top, bottom, _) = style();
    let (top, bottom) = (f3(top), f3(bottom));
    let c = |i: usize| (top[i] + (bottom[i] - top[i]) * t) as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

/// RGBA pixels for the window and launcher icon (same logo, with a little
/// margin like the other Grassfield icons), drawn with smooth edges.
pub fn window_icon(size: u32) -> egui::IconData {
    let (_, top, bottom, wave) = style();
    let (logo_top, logo_bottom) = (f3(top), f3(bottom));
    let waves = [(WAVES[0].0, WAVES[0].1, wave), WAVES[1]];
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let margin = s * 0.035;
    let box_size = s - 2.0 * margin;
    let radius = box_size * 0.23;
    const SS: u32 = 4; // samples per side, for smooth edges
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // inside the rounded square?
                    let (lx, ly) = (px - margin, py - margin);
                    if lx < 0.0 || ly < 0.0 || lx > box_size || ly > box_size {
                        continue;
                    }
                    let cx = lx.clamp(radius, box_size - radius);
                    let cy = ly.clamp(radius, box_size - radius);
                    if (lx - cx).powi(2) + (ly - cy).powi(2) > radius * radius {
                        continue;
                    }
                    let (u, v) = (lx / box_size, ly / box_size);
                    let mut col = [0f32; 3];
                    for (i, c) in col.iter_mut().enumerate() {
                        *c = logo_top[i] + (logo_bottom[i] - logo_top[i]) * v;
                    }
                    for (k, w, wc) in waves {
                        if (0.14..=0.86).contains(&u) || ((u - 0.14).abs() * box_size).hypot((v - wave_y(0.14, k)) * box_size) < w * box_size / 2.0 || ((u - 0.86).abs() * box_size).hypot((v - wave_y(0.86, k)) * box_size) < w * box_size / 2.0 {
                            let uu = u.clamp(0.14, 0.86);
                            let d = (v - wave_y(uu, k)).abs();
                            let end = ((u - uu) * box_size).hypot(d * box_size);
                            if (uu == u && d < w / 2.0) || (uu != u && end < w * box_size / 2.0) {
                                col = [wc[0] as f32, wc[1] as f32, wc[2] as f32];
                            }
                        }
                    }
                    acc[0] += col[0];
                    acc[1] += col[1];
                    acc[2] += col[2];
                    acc[3] += 1.0;
                }
            }
            if acc[3] > 0.0 {
                let i = ((y * size + x) * 4) as usize;
                for k in 0..3 {
                    rgba[i + k] = (acc[k] / acc[3]) as u8;
                }
                rgba[i + 3] = (255.0 * acc[3] / (SS * SS) as f32) as u8;
            }
        }
    }
    egui::IconData { rgba, width: size, height: size }
}

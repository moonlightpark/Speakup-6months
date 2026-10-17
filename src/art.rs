//! Vector illustrations painted with egui shapes: gradients, the mascot ("Sunny", a speech
//! bubble that talks), the six-month journey, icon badges and the soft window backdrop.
//! Everything is drawn in code, so it is crisp at any scale and needs no image files.

use crate::widgets::{GOOD, WARN};
use egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Ui, Vec2, epaint::Vertex, pos2, vec2};
use std::f32::consts::{PI, TAU};

pub const VIOLET: Color32 = Color32::from_rgb(139, 92, 246);
pub const PINK: Color32 = Color32::from_rgb(236, 72, 153);
pub const CYAN: Color32 = Color32::from_rgb(34, 211, 238);
pub const INDIGO_DEEP: Color32 = Color32::from_rgb(30, 27, 75);

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Outline of a rounded rectangle (convex), clockwise.
fn rounded_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -PI / 2.0),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), PI / 2.0),
        (pos2(rect.left() + r, rect.top() + r), PI),
    ];
    let mut pts = Vec::with_capacity(4 * 9);
    for (c, a0) in corners {
        for i in 0..=8 {
            let a = a0 + PI / 2.0 * i as f32 / 8.0;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// Convex polygon filled with a diagonal gradient (`from` at the top-left, `to` at the bottom-right).
fn gradient_poly(pts: &[Pos2], bounds: Rect, from: Color32, to: Color32) -> Shape {
    let mut mesh = Mesh::default();
    let diag = bounds.size().length_sq().max(1.0);
    let color_at = |p: Pos2| lerp_color(from, to, (p - bounds.min).dot(bounds.size()) / diag);
    let c = bounds.center();
    mesh.vertices.push(Vertex { pos: c, uv: egui::epaint::WHITE_UV, color: color_at(c) });
    for p in pts {
        mesh.vertices.push(Vertex { pos: *p, uv: egui::epaint::WHITE_UV, color: color_at(*p) });
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    Shape::mesh(mesh)
}

pub fn gradient_rect(p: &Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    p.add(gradient_rect_shape(rect, radius, from, to));
}

pub fn gradient_rect_shape(rect: Rect, radius: f32, from: Color32, to: Color32) -> Shape {
    gradient_poly(&rounded_outline(rect, radius), rect, from, to)
}

/// A blurred-looking glow: concentric translucent circles.
pub fn glow(p: &Painter, c: Pos2, r: f32, color: Color32) {
    for i in 0..10 {
        let t = i as f32 / 10.0;
        p.circle_filled(c, r * (1.0 - t * 0.85), color.gamma_multiply(0.035 + t * 0.02));
    }
}

// ---------------------------------------------------------------- mascot

#[derive(Clone, Copy, PartialEq)]
pub enum Mood {
    Happy,
    Talking,
    Cheer,
}

/// "Sunny": a round speech-bubble character. `t` (seconds) animates blinking and the waves.
pub fn mascot(p: &Painter, c: Pos2, size: f32, mood: Mood, t: f32) {
    let r = size / 2.0;
    let bob = (t * 2.2).sin() * r * 0.04;
    let c = c + vec2(0.0, bob);
    // Shadow
    p.add(Shape::ellipse_filled(c + vec2(0.0, r * 1.12 - bob), vec2(r * 0.7, r * 0.12), Color32::from_black_alpha(70)));
    // Bubble tail
    let tail = vec![c + vec2(-r * 0.55, r * 0.55), c + vec2(-r * 0.95, r * 1.0), c + vec2(-r * 0.12, r * 0.82)];
    p.add(Shape::convex_polygon(tail, Color32::from_rgb(98, 112, 255), Stroke::NONE));
    // Body with gradient
    let body = Rect::from_center_size(c, Vec2::splat(size));
    let pts: Vec<Pos2> = (0..48).map(|i| c + Vec2::angled(TAU * i as f32 / 48.0) * r).collect();
    p.add(gradient_poly(&pts, body, Color32::from_rgb(120, 160, 255), VIOLET));
    // Shine
    p.add(Shape::ellipse_filled(c + vec2(-r * 0.38, -r * 0.48), vec2(r * 0.26, r * 0.13), Color32::from_white_alpha(70)));
    // Eyes (blink every ~3.5 s)
    let blink = (t % 3.5) > 3.35;
    let ink = Color32::from_rgb(25, 26, 60);
    for dx in [-0.3, 0.3] {
        let e = c + vec2(r * dx, -r * 0.08);
        if blink || mood == Mood::Cheer {
            let pts: Vec<Pos2> = (0..=8).map(|i| e + vec2((i as f32 / 8.0 - 0.5) * r * 0.24, -((i as f32 / 8.0) * PI).sin() * r * 0.09)).collect();
            p.add(Shape::line(pts, Stroke::new(r * 0.06, ink)));
        } else {
            p.add(Shape::ellipse_filled(e, vec2(r * 0.08, r * 0.11), ink));
            p.circle_filled(e + vec2(r * 0.03, -r * 0.04), r * 0.03, Color32::WHITE);
        }
    }
    // Cheeks
    for dx in [-0.5, 0.5] {
        p.add(Shape::ellipse_filled(c + vec2(r * dx, r * 0.14), vec2(r * 0.11, r * 0.06), PINK.gamma_multiply(0.55)));
    }
    // Mouth
    let m = c + vec2(0.0, r * 0.2);
    match mood {
        Mood::Talking => {
            let open = 0.06 + (t * 9.0).sin().abs() * 0.1;
            p.add(Shape::ellipse_filled(m + vec2(0.0, r * 0.04), vec2(r * 0.13, r * open), ink));
        }
        _ => {
            let pts: Vec<Pos2> = (0..=10).map(|i| m + vec2((i as f32 / 10.0 - 0.5) * r * 0.36, ((i as f32 / 10.0) * PI).sin() * r * 0.14)).collect();
            p.add(Shape::line(pts, Stroke::new(r * 0.06, ink)));
        }
    }
    // Sound waves
    if mood != Mood::Happy {
        for i in 0..3 {
            let phase = ((t * 1.6 - i as f32 * 0.25).rem_euclid(1.0)) * 0.6 + 0.4;
            let rr = r * (1.25 + i as f32 * 0.22);
            let pts: Vec<Pos2> = (0..=10).map(|k| c + Vec2::angled(-0.55 + 1.1 * k as f32 / 10.0) * rr).collect();
            p.add(Shape::line(pts, Stroke::new(r * 0.07, CYAN.gamma_multiply(phase))));
        }
    }
    if mood == Mood::Cheer {
        sparkle(p, c + vec2(-r * 1.1, -r * 0.9), r * 0.22, WARN);
        sparkle(p, c + vec2(r * 1.15, -r * 0.6), r * 0.16, PINK);
        sparkle(p, c + vec2(r * 0.9, r * 0.9), r * 0.12, CYAN);
    }
}

pub fn sparkle(p: &Painter, c: Pos2, r: f32, color: Color32) {
    let pts: Vec<Pos2> = (0..8).map(|i| c + Vec2::angled(TAU * i as f32 / 8.0 - PI / 2.0) * if i % 2 == 0 { r } else { r * 0.32 }).collect();
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// Allocates space and draws the mascot, animating.
pub fn mascot_widget(ui: &mut Ui, size: f32, mood: Mood) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size * 1.6), egui::Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    mascot(ui.painter(), rect.center(), size, mood, t);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

// ---------------------------------------------------------------- scenes

/// The six-month path as rolling hills with a flag at the end and a marker at `progress`.
pub fn journey(rect: Rect, progress: f32) -> Shape {
    let mut out: Vec<Shape> = Vec::new();
    let base = rect.bottom();
    let hill = |x0: f32, w: f32, h: f32, color: Color32| {
        let pts: Vec<Pos2> = (0..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                pos2(x0 + w * t, base - (t * PI).sin() * h)
            })
            .collect();
        Shape::convex_polygon(pts, color, Stroke::NONE)
    };
    out.push(hill(rect.left(), rect.width() * 0.65, rect.height() * 0.55, Color32::from_rgba_unmultiplied(120, 140, 255, 30)));
    out.push(hill(rect.left() + rect.width() * 0.4, rect.width() * 0.6, rect.height() * 0.8, Color32::from_rgba_unmultiplied(160, 120, 255, 34)));
    // Path
    let path_at = |t: f32| pos2(rect.left() + 20.0 + (rect.width() - 60.0) * t, base - 18.0 - (t * PI * 1.0).sin() * rect.height() * 0.28 - t * rect.height() * 0.3);
    let pts: Vec<Pos2> = (0..=40).map(|i| path_at(i as f32 / 40.0)).collect();
    out.extend(Shape::dashed_line(&pts, Stroke::new(2.0, Color32::from_white_alpha(70)), 6.0, 5.0));
    let done: Vec<Pos2> = (0..=40).map(|i| path_at(progress.clamp(0.0, 1.0) * i as f32 / 40.0)).collect();
    out.push(Shape::line(done, Stroke::new(3.5, CYAN)));
    // Month dots
    for m in 0..=6 {
        let q = path_at(m as f32 / 6.0);
        let reached = progress * 6.0 >= m as f32;
        out.push(Shape::circle_filled(q, 4.5, if reached { CYAN } else { Color32::from_white_alpha(60) }));
    }
    // Flag
    let f = path_at(1.0);
    out.push(Shape::line_segment([f, f - vec2(0.0, 30.0)], Stroke::new(2.5, Color32::WHITE)));
    out.push(Shape::convex_polygon(vec![f - vec2(0.0, 30.0), f - vec2(-20.0, 24.0), f - vec2(0.0, 18.0)], WARN, Stroke::NONE));
    // Marker
    let me = path_at(progress.clamp(0.0, 1.0));
    out.push(Shape::circle_filled(me, 9.0, Color32::WHITE));
    out.push(Shape::circle_filled(me, 6.0, PINK));
    Shape::Vec(out)
}

/// Onboarding banner: gradient card, mascot greeting, title and tagline.
pub fn onboarding_hero(ui: &mut Ui) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 230.0), egui::Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "말하고, 연습하고, 활용하세요."));
    let p = ui.painter_at(rect);
    gradient_rect(&p, rect, 24.0, Color32::from_rgb(52, 84, 220), Color32::from_rgb(120, 60, 200));
    // Kept away from the rounded corners (the painter clips to the square rect).
    glow(&p, rect.right_top() + vec2(-200.0, 90.0), 120.0, PINK);
    glow(&p, rect.left_bottom() + vec2(220.0, -40.0), 110.0, CYAN);
    let t = ui.input(|i| i.time) as f32;
    for (i, (x, y)) in [(0.62, 0.2), (0.9, 0.75), (0.55, 0.85), (0.97, 0.15)].iter().enumerate() {
        sparkle(&p, pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y), 5.0 + (t * 2.0 + i as f32).sin().abs() * 3.0, Color32::from_white_alpha(150));
    }
    mascot(&p, pos2(rect.right() - 130.0, rect.center().y - 6.0), 110.0, Mood::Talking, t);
    // Speech bubble "Hi!"
    let b = Rect::from_min_size(pos2(rect.right() - 290.0, rect.top() + 26.0), vec2(96.0, 40.0));
    p.rect_filled(b, 14.0, Color32::WHITE);
    p.add(Shape::convex_polygon(vec![b.right_bottom() - vec2(26.0, 2.0), b.right_bottom() + vec2(4.0, 12.0), b.right_bottom() - vec2(10.0, 2.0)], Color32::WHITE, Stroke::NONE));
    p.text(b.center(), egui::Align2::CENTER_CENTER, "Hi there!", egui::FontId::proportional(17.0), INDIGO_DEEP);
    let left = rect.left() + 34.0;
    p.text(pos2(left, rect.top() + 58.0), egui::Align2::LEFT_CENTER, "말하고, 연습하고, 활용하세요.", egui::FontId::proportional(32.0), Color32::WHITE);
    p.text(pos2(left, rect.top() + 100.0), egui::Align2::LEFT_CENTER, "6개월 뒤 영어로 듣고 말하는 것만을 목표로 하는 음성 학습 앱", egui::FontId::proportional(16.0), Color32::from_rgb(220, 226, 255));
    let mut x = left;
    for (txt, col) in [("🎧 듣기", CYAN), ("🎤 말하기", PINK), ("🔁 반복", GOOD), ("⚡ 스피드", WARN)] {
        let galley = p.layout_no_wrap(txt.to_string(), egui::FontId::proportional(15.0), Color32::WHITE);
        let pill = Rect::from_min_size(pos2(x, rect.top() + 140.0), galley.size() + vec2(24.0, 12.0));
        p.rect_filled(pill, 16.0, col.gamma_multiply(0.35));
        p.rect_stroke(pill, 16.0, Stroke::new(1.0, col.gamma_multiply(0.8)), egui::StrokeKind::Inside);
        p.galley(pill.min + vec2(12.0, 6.0), galley, Color32::WHITE);
        x = pill.right() + 10.0;
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

/// A rounded gradient square with a symbol in it (page titles, logo).
pub fn icon_badge(p: &Painter, rect: Rect, symbol: &str, from: Color32, to: Color32) {
    p.rect_filled(rect.translate(vec2(0.0, 3.0)), rect.width() * 0.3, Color32::from_black_alpha(60));
    gradient_rect(p, rect, rect.width() * 0.3, from, to);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, symbol, egui::FontId::proportional(rect.height() * 0.5), Color32::WHITE);
}

/// Badge colors per page (stable hash of the title so each page keeps its own look).
pub fn badge_colors(key: &str) -> (Color32, Color32) {
    const PAIRS: [(Color32, Color32); 6] = [
        (Color32::from_rgb(61, 123, 255), VIOLET),
        (VIOLET, PINK),
        (Color32::from_rgb(16, 185, 129), CYAN),
        (Color32::from_rgb(245, 158, 11), PINK),
        (CYAN, Color32::from_rgb(61, 123, 255)),
        (PINK, Color32::from_rgb(245, 158, 11)),
    ];
    let h = key.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    PAIRS[h as usize % PAIRS.len()]
}

/// Splits "🔬 피드백 센터" into ("🔬", "피드백 센터").
pub fn split_icon(title: &str) -> (Option<&str>, &str) {
    let mut chars = title.char_indices();
    match chars.next() {
        Some((_, c)) if !c.is_alphanumeric() && (c as u32) > 0x2000 => {
            let rest = title[c.len_utf8()..].trim_start_matches('\u{FE0F}');
            let icon_len = title.len() - rest.len();
            (Some(&title[..icon_len]), rest.trim_start())
        }
        _ => (None, title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_title_icons() {
        assert_eq!(split_icon("🔬 피드백 센터"), (Some("🔬"), "피드백 센터"));
        assert_eq!(split_icon("⚙ 설정"), (Some("⚙"), "설정"));
        assert_eq!(split_icon("레벨"), (None, "레벨"));
    }

    #[test]
    fn color_lerp_endpoints() {
        assert_eq!(lerp_color(Color32::BLACK, Color32::WHITE, 0.0), Color32::BLACK);
        assert_eq!(lerp_color(Color32::BLACK, Color32::WHITE, 1.0), Color32::WHITE);
    }
}

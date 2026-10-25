//! Vector illustrations painted with egui shapes: gradients, the mascot ("Sunny", a speech
//! bubble that talks), the six-month journey, icon badges and the soft window backdrop.
//! Everything is drawn in code, so it is crisp at any scale and needs no image files.

use crate::widgets::{GOOD, warn};
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
        sparkle(p, c + vec2(-r * 1.1, -r * 0.9), r * 0.22, warn());
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

// ---------------------------------------------------------------- celebration

/// How long the celebration moves (seconds); afterwards it rests on its last frame.
pub const CELEBRATION_SECS: f32 = 4.0;

fn ease_out_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// Small deterministic pseudo-random number in 0..1 for particle `i`, channel `k`.
fn rnd(i: u32, k: u32) -> f32 {
    let mut x = i.wrapping_mul(747_796_405).wrapping_add(k.wrapping_mul(2_891_336_453)).wrapping_add(1);
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    x ^= x >> 16;
    (x % 10_000) as f32 / 10_000.0
}

/// A filled five-pointed star (a triangle fan, since the outline is not convex).
pub fn star(p: &Painter, c: Pos2, r: f32, color: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, color);
    for i in 0..=10 {
        let rr = if i % 2 == 0 { r } else { r * 0.45 };
        mesh.colored_vertex(c + Vec2::angled(-PI / 2.0 + PI * i as f32 / 5.0) * rr, color);
    }
    for i in 1..=10u32 {
        mesh.add_triangle(0, i, i + 1);
    }
    p.add(Shape::mesh(mesh));
}

/// The "잘했어요!" scene, `t` seconds after it appeared: a light burst, confetti thrown up and
/// tumbling down, the mascot popping in to cheer and 1–3 stars for `score`. Returns the rect,
/// whose upper middle is left free for the title. Repaints only while it moves.
pub fn celebration(ui: &mut Ui, t: f32, score: f32) -> Rect {
    let w = ui.available_width().min(760.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 300.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    gradient_rect(&p, rect, 22.0, Color32::from_rgb(76, 58, 200), Color32::from_rgb(150, 50, 160));
    let c = rect.center() + vec2(0.0, 10.0);

    // Light burst: rings and rays spreading out.
    if t < 1.6 {
        for k in 0..3 {
            let tt = (t - k as f32 * 0.18) / 1.2;
            if (0.0..1.0).contains(&tt) {
                p.circle_stroke(c, 30.0 + tt * w * 0.5, Stroke::new(6.0 * (1.0 - tt), Color32::from_white_alpha((150.0 * (1.0 - tt)) as u8)));
            }
        }
    }
    let rays = (1.0 - (t / 2.5)).clamp(0.0, 1.0);
    if rays > 0.0 {
        for i in 0..16 {
            let a = TAU * i as f32 / 16.0 + t * 0.4;
            let (r0, r1) = (60.0, 60.0 + 260.0 * ease_out_back((t / 0.7).min(1.0)));
            p.line_segment([c + Vec2::angled(a) * r0, c + Vec2::angled(a) * r1], Stroke::new(3.0, Color32::from_white_alpha((40.0 * rays) as u8)));
        }
    }

    // Stars for the first-try average, popping in one after another.
    let n_stars = if score >= 90.0 { 3 } else if score >= 75.0 { 2 } else { 1 };
    for i in 0..3 {
        let pop = ease_out_back(((t - 0.5 - i as f32 * 0.25) / 0.45).max(0.0));
        if pop <= 0.0 {
            continue;
        }
        let pos = pos2(rect.center().x + (i as f32 - 1.0) * 64.0, rect.bottom() - 62.0 - if i == 1 { 14.0 } else { 0.0 });
        let lit = i < n_stars;
        let col = if lit { Color32::from_rgb(255, 214, 80) } else { Color32::from_white_alpha(45) };
        if lit {
            glow(&p, pos, 40.0 * pop, Color32::from_rgb(255, 214, 80));
        }
        star(&p, pos, 26.0 * pop, col);
    }

    // The mascot pops in on the left and keeps cheering.
    let m = ease_out_back((t / 0.6).min(1.0));
    if m > 0.0 {
        let hop = ((t * 5.0).sin().abs() * 10.0) * (1.0 - (t / CELEBRATION_SECS).min(1.0));
        mascot(&p, pos2(rect.left() + 95.0, rect.center().y + 20.0 - hop), 92.0 * m, Mood::Cheer, t);
        mascot(&p, pos2(rect.right() - 95.0, rect.center().y + 30.0 - hop * 0.7), 64.0 * m, Mood::Happy, t + 1.3);
    }

    // Confetti: thrown up from the middle, then falling and tumbling, fading at the end.
    let palette = [PINK, CYAN, warn(), GOOD, Color32::from_rgb(255, 214, 80), Color32::WHITE, VIOLET];
    let fade = (1.0 - (t - (CELEBRATION_SECS - 0.8)) / 0.8).clamp(0.0, 1.0);
    if fade > 0.0 {
        for i in 0..90u32 {
            let delay = rnd(i, 0) * 0.35;
            let tt = t - delay;
            if tt <= 0.0 {
                continue;
            }
            let angle = -PI / 2.0 + (rnd(i, 1) - 0.5) * 2.4;
            let speed = 260.0 + rnd(i, 2) * 360.0;
            let drift = (tt * (2.0 + rnd(i, 3) * 3.0)).sin() * 18.0;
            let pos = c + vec2(angle.cos() * speed * tt + drift, angle.sin() * speed * tt + 0.5 * 520.0 * tt * tt);
            if !rect.expand(20.0).contains(pos) {
                continue;
            }
            let col = palette[(i as usize) % palette.len()].gamma_multiply(fade);
            let spin = tt * (4.0 + rnd(i, 4) * 8.0);
            let size = 4.0 + rnd(i, 5) * 5.0;
            if i % 3 == 0 {
                p.circle_filled(pos, size * 0.6, col);
            } else {
                // A small spinning paper strip.
                let (dx, dy) = (Vec2::angled(spin) * size, Vec2::angled(spin + PI / 2.0) * size * 0.45 * spin.cos().abs().max(0.2));
                p.add(Shape::convex_polygon(vec![pos - dx - dy, pos + dx - dy, pos + dx + dy, pos - dx + dy], col, Stroke::NONE));
            }
        }
    }
    if t < CELEBRATION_SECS {
        ui.ctx().request_repaint();
    }
    rect
}

// ---------------------------------------------------------------- scenes

/// Seconds since the event `key` (e.g. a result count) last changed for widget `id`. The first
/// time an id is seen it is "long ago", so opening a screen does not replay an old event.
pub fn since_event(ui: &Ui, id: egui::Id, key: u64) -> f32 {
    let now = ui.input(|i| i.time);
    let (k, start) = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || (key, f64::NEG_INFINITY)));
    if k != key {
        ui.data_mut(|d| d.insert_temp(id, (key, now)));
        return 0.0;
    }
    (now - start).min(1e6) as f32
}

/// The full celebration with its headline and a line under it (as real labels, so they can be
/// read and tested), and the layout continuing below the animation.
pub fn cheer(ui: &mut Ui, t: f32, score: f32, headline: &str, sub: &str) {
    let rect = celebration(ui, t, score);
    let pop = 1.0 + 0.2 * (1.0 - (t / 0.5).min(1.0)) + 0.05 * (t * 6.0).sin() * (1.0 - (t / CELEBRATION_SECS).min(1.0));
    let top = Rect::from_center_size(rect.center_top() + vec2(0.0, 52.0), vec2(rect.width() - 200.0, 64.0));
    ui.put(top, egui::Label::new(egui::RichText::new(headline).size(34.0 * pop).strong().color(Color32::WHITE)));
    let below = Rect::from_center_size(rect.center_top() + vec2(0.0, 108.0), vec2(rect.width() - 200.0, 30.0));
    ui.put(below, egui::Label::new(egui::RichText::new(sub).size(18.0).color(Color32::from_rgb(230, 230, 255))));
    // `put` moves the layout cursor to the labels; continue below the whole animation.
    ui.advance_cursor_after_rect(rect);
}

/// How long a cheer strip stays on screen (seconds).
pub const STRIP_SECS: f32 = 2.6;

/// A short cheer for one good answer: a banner that pops open with a burst of confetti, a
/// hopping mascot and `text`, then folds away. Draws nothing once `t` is past [`STRIP_SECS`].
pub fn cheer_strip(ui: &mut Ui, t: f32, text: &str) {
    if !(0.0..STRIP_SECS).contains(&t) {
        return;
    }
    // Opens quickly, closes in the last 0.35 s.
    let open = ease_out_back((t / 0.35).min(1.0)) * ((STRIP_SECS - t) / 0.35).clamp(0.0, 1.0);
    let w = ui.available_width().min(760.0);
    let h = 76.0 * open.max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h.max(1.0)), egui::Sense::hover());
    ui.ctx().request_repaint();
    if h < 8.0 {
        return;
    }
    let p = ui.painter_at(rect.expand(30.0));
    gradient_rect(&p, rect, 16.0, Color32::from_rgb(76, 58, 200), Color32::from_rgb(150, 50, 160));
    let hop = (t * 7.0).sin().abs() * 6.0;
    mascot(&p, pos2(rect.left() + 44.0, rect.center().y - hop), 46.0 * open.min(1.0), Mood::Cheer, t);
    let c = pos2(rect.left() + 44.0, rect.center().y);
    let palette = [PINK, CYAN, warn(), GOOD, Color32::from_rgb(255, 214, 80), Color32::WHITE];
    for i in 0..28u32 {
        let tt = t - rnd(i, 0) * 0.2;
        if tt <= 0.0 {
            continue;
        }
        let a = -PI / 2.0 + (rnd(i, 1) - 0.2) * 2.2;
        let v = 180.0 + rnd(i, 2) * 260.0;
        let pos = c + vec2(a.cos().abs() * v * tt, a.sin() * v * 0.35 * tt + 160.0 * tt * tt);
        if rect.contains(pos) {
            let fade = (1.0 - t / STRIP_SECS).clamp(0.0, 1.0);
            p.circle_filled(pos, 2.5 + rnd(i, 5) * 3.0, palette[i as usize % palette.len()].gamma_multiply(fade));
        }
    }
    for k in 0..3 {
        let sp = ease_out_back(((t - 0.25 - k as f32 * 0.15) / 0.4).max(0.0));
        if sp > 0.0 {
            star(&p, pos2(rect.right() - 36.0 - k as f32 * 30.0, rect.center().y), 11.0 * sp, Color32::from_rgb(255, 214, 80));
        }
    }
    let size = 24.0 * (1.0 + 0.15 * (1.0 - (t / 0.4).min(1.0)));
    let label = Rect::from_min_max(pos2(rect.left() + 84.0, rect.top()), pos2(rect.right() - 120.0, rect.bottom()));
    ui.put(label, egui::Label::new(egui::RichText::new(text).size(size).strong().color(Color32::WHITE)));
    ui.advance_cursor_after_rect(rect);
}


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
    out.extend(Shape::dashed_line(&pts, Stroke::new(2.0_f32, Color32::from_white_alpha(70)), 6.0, 5.0));
    let done: Vec<Pos2> = (0..=40).map(|i| path_at(progress.clamp(0.0, 1.0) * i as f32 / 40.0)).collect();
    out.push(Shape::line(done, Stroke::new(3.5_f32, CYAN)));
    // Month dots
    for m in 0..=6 {
        let q = path_at(m as f32 / 6.0);
        let reached = progress * 6.0 >= m as f32;
        out.push(Shape::circle_filled(q, 4.5, if reached { CYAN } else { Color32::from_white_alpha(60) }));
    }
    // Flag
    let f = path_at(1.0);
    out.push(Shape::line_segment([f, f - vec2(0.0, 30.0)], Stroke::new(2.5_f32, Color32::WHITE)));
    out.push(Shape::convex_polygon(vec![f - vec2(0.0, 30.0), f - vec2(-20.0, 24.0), f - vec2(0.0, 18.0)], warn(), Stroke::NONE));
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
    // A phone: title on two lines, tagline wrapped, no mascot.
    let narrow = w < 600.0;
    if !narrow {
        mascot(&p, pos2(rect.right() - 130.0, rect.center().y - 6.0), 110.0, Mood::Talking, t);
        // Speech bubble "Hi!"
        let b = Rect::from_min_size(pos2(rect.right() - 290.0, rect.top() + 26.0), vec2(96.0, 40.0));
        p.rect_filled(b, 14.0, Color32::WHITE);
        p.add(Shape::convex_polygon(vec![b.right_bottom() - vec2(26.0, 2.0), b.right_bottom() + vec2(4.0, 12.0), b.right_bottom() - vec2(10.0, 2.0)], Color32::WHITE, Stroke::NONE));
        p.text(b.center(), egui::Align2::CENTER_CENTER, "Hi there!", egui::FontId::proportional(17.0), INDIGO_DEEP);
    }
    let left = rect.left() + if narrow { 20.0 } else { 34.0 };
    let title = if narrow { "말하고, 연습하고,\n활용하세요." } else { "말하고, 연습하고, 활용하세요." };
    let title = p.layout_no_wrap(title.to_string(), egui::FontId::proportional(if narrow { 24.0 } else { 32.0 }), Color32::WHITE);
    let title_h = title.size().y;
    p.galley(pos2(left, rect.top() + if narrow { 22.0 } else { 38.0 }), title, Color32::WHITE);
    let tag = p.layout("6개월 뒤 영어로 듣고 말하는 것만을 목표로 하는 음성 학습 앱".to_string(), egui::FontId::proportional(if narrow { 13.5 } else { 16.0 }), Color32::from_rgb(220, 226, 255), rect.right() - left - 20.0);
    let tag_top = rect.top() + if narrow { 30.0 + title_h } else { 90.0 };
    let tag_h = tag.size().y;
    p.galley(pos2(left, tag_top), tag, Color32::WHITE);
    let pill_top = if narrow { tag_top + tag_h + 14.0 } else { rect.top() + 140.0 };
    let mut x = left;
    for (txt, col) in [("🎧 듣기", CYAN), ("🎤 말하기", PINK), ("🔁 반복", GOOD), ("⚡ 스피드", warn())] {
        let galley = p.layout_no_wrap(txt.to_string(), egui::FontId::proportional(15.0), Color32::WHITE);
        let pill = Rect::from_min_size(pos2(x, pill_top), galley.size() + vec2(if narrow { 14.0 } else { 24.0 }, 12.0));
        p.rect_filled(pill, 16.0, col.gamma_multiply(0.35));
        p.rect_stroke(pill, 16.0, Stroke::new(1.0_f32, col.gamma_multiply(0.8)), egui::StrokeKind::Inside);
        p.galley(pill.min + vec2(if narrow { 7.0 } else { 12.0 }, 6.0), galley, Color32::WHITE);
        x = pill.right() + if narrow { 6.0 } else { 10.0 };
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

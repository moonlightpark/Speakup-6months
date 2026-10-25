//! Dark (default) and light themes in the spirit of Speak, with the Pretendard (Hangul) and Noto Sans JP
//! (kana and kanji) fonts built in (SIL Open Font License, assets/fonts) so text looks the same on
//! macOS and Windows.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

const PRETENDARD: &[u8] = include_bytes!("../assets/fonts/Pretendard-Regular.ttf");
const NOTO_JP: &[u8] = include_bytes!("../assets/fonts/NotoSansJP-Regular.otf");

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("pretendard".into(), Arc::new(FontData::from_static(PRETENDARD)));
    fonts.font_data.insert("noto-jp".into(), Arc::new(FontData::from_static(NOTO_JP)));
    // Pretendard first for Hangul and Latin, Noto Sans JP for Japanese; egui's own fonts stay
    // behind them for emoji and icons.
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "pretendard".into());
    prop.insert(1, "noto-jp".into());
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.push("pretendard".into());
    mono.push("noto-jp".into());
    // Optional extra fallback for scripts Pretendard lacks.
    if let Some(bytes) = std::env::var("SPEAKUP_FONT").ok().and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("custom".into(), Arc::new(FontData::from_owned(bytes)));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().push("custom".into());
        }
    }
    ctx.set_fonts(fonts);

    apply_visuals(ctx);
}

/// Colours of egui's own widgets for the chosen theme (설정 > 테마; see `widgets::set_light`).
/// The OS light/dark setting is not followed: the learner picks one in the app.
pub fn apply_visuals(ctx: &egui::Context) {
    let light = crate::widgets::is_light();
    let theme = if light { egui::Theme::Light } else { egui::Theme::Dark };
    ctx.set_theme(theme);
    let mut style = (*ctx.style_of(theme)).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(26.0)),
        (TextStyle::Body, FontId::proportional(16.0)),
        (TextStyle::Button, FontId::proportional(16.0)),
        (TextStyle::Small, FontId::proportional(12.5)),
        (TextStyle::Monospace, FontId::monospace(15.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    // Plain buttons come out ~38 px tall, the same as `ACTION_BUTTON`.
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style.spacing.interact_size.y = 30.0;
    // (dark, light) for each themed colour.
    let pick = |d: Color32, l: Color32| if light { l } else { d };
    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    let text = pick(Color32::from_rgb(236, 240, 248), Color32::from_rgb(28, 32, 46));
    let strong = pick(Color32::WHITE, Color32::from_rgb(12, 16, 28));
    // High-contrast text on the background.
    v.override_text_color = Some(text);
    v.widgets.noninteractive.fg_stroke.color = text;
    v.widgets.inactive.fg_stroke.color = pick(Color32::from_rgb(240, 244, 252), Color32::from_rgb(24, 28, 42));
    v.widgets.hovered.fg_stroke.color = strong;
    v.widgets.active.fg_stroke.color = strong;
    v.panel_fill = crate::widgets::bg();
    v.window_fill = pick(Color32::from_rgb(18, 23, 36), Color32::WHITE);
    v.window_corner_radius = 14.0.into();
    v.extreme_bg_color = pick(Color32::from_rgb(17, 22, 36), Color32::from_rgb(250, 251, 254));
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, crate::widgets::border());
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, pick(Color32::from_rgb(48, 60, 90), Color32::from_rgb(200, 207, 222)));
    v.selection.bg_fill = crate::widgets::ACCENT;
    v.selection.stroke.color = Color32::WHITE;
    v.hyperlink_color = pick(Color32::from_rgb(140, 180, 255), Color32::from_rgb(35, 90, 220));
    v.widgets.inactive.weak_bg_fill = pick(Color32::from_rgb(30, 38, 58), Color32::from_rgb(232, 236, 246));
    v.widgets.inactive.bg_fill = pick(Color32::from_rgb(30, 38, 58), Color32::from_rgb(232, 236, 246));
    v.widgets.hovered.weak_bg_fill = pick(Color32::from_rgb(44, 55, 84), Color32::from_rgb(218, 225, 242));
    v.widgets.hovered.bg_fill = pick(Color32::from_rgb(44, 55, 84), Color32::from_rgb(218, 225, 242));
    v.widgets.active.weak_bg_fill = pick(Color32::from_rgb(52, 66, 100), Color32::from_rgb(204, 214, 238));
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 10.0.into();
    }
    style.visuals = v;
    ctx.set_style_of(theme, style);
}

#[cfg(test)]
mod tests {
    /// Every symbol used in the UI source must exist in the loaded fonts (otherwise it shows as □).
    #[test]
    fn all_ui_symbols_have_glyphs() {
        let ctx = egui::Context::default();
        super::apply(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        let mut missing = std::collections::BTreeSet::new();
        let mut files = vec![];
        for dir in ["src", "src/screens"] {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                if e.path().extension().is_some_and(|x| x == "rs" || x == "tsv") {
                    files.push(e.path());
                }
            }
        }
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
                for c in line.chars().filter(|c| (*c as u32) >= 0x2000 && !('\u{AC00}'..='\u{D7A3}').contains(c) && !matches!(*c as u32, 0x3130..=0x318F | 0xFE0F | 0x200D)) {
                    if !ctx.fonts(|fo| fo.has_glyph(&egui::FontId::proportional(16.0), c)) {
                        missing.insert(format!("{c} U+{:04X} ({})", c as u32, f.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "missing glyphs:\n{}", missing.into_iter().collect::<Vec<_>>().join("\n"));
    }
}

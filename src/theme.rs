//! Dark theme in the spirit of Speak, with the Pretendard font built in (SIL Open Font
//! License, assets/fonts) so Hangul and symbols look the same on macOS, Windows and Linux.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

const PRETENDARD: &[u8] = include_bytes!("../assets/fonts/Pretendard-Regular.ttf");

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("pretendard".into(), Arc::new(FontData::from_static(PRETENDARD)));
    // Pretendard first for text; egui's own fonts stay behind it for emoji and icons.
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "pretendard".into());
    fonts.families.entry(FontFamily::Monospace).or_default().push("pretendard".into());
    // Optional extra fallback for scripts Pretendard lacks.
    if let Some(bytes) = std::env::var("SPEAKUP_FONT").ok().and_then(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("custom".into(), Arc::new(FontData::from_owned(bytes)));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().push("custom".into());
        }
    }
    ctx.set_fonts(fonts);

    // Always dark (the design assumes it), regardless of the OS light/dark setting.
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(26.0)),
        (TextStyle::Body, FontId::proportional(16.0)),
        (TextStyle::Button, FontId::proportional(16.0)),
        (TextStyle::Small, FontId::proportional(12.5)),
        (TextStyle::Monospace, FontId::monospace(15.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    let mut v = egui::Visuals::dark();
    // High-contrast text on the dark background.
    v.override_text_color = Some(Color32::from_rgb(236, 240, 248));
    v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(236, 240, 248);
    v.widgets.inactive.fg_stroke.color = Color32::from_rgb(240, 244, 252);
    v.widgets.hovered.fg_stroke.color = Color32::WHITE;
    v.widgets.active.fg_stroke.color = Color32::WHITE;
    v.panel_fill = crate::widgets::BG;
    v.window_fill = Color32::from_rgb(18, 23, 36);
    v.window_corner_radius = 14.0.into();
    v.extreme_bg_color = Color32::from_rgb(17, 22, 36);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, crate::widgets::BORDER);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(48, 60, 90));
    v.selection.bg_fill = crate::widgets::ACCENT;
    v.hyperlink_color = Color32::from_rgb(140, 180, 255);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 38, 58);
    v.widgets.inactive.bg_fill = Color32::from_rgb(30, 38, 58);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 55, 84);
    v.widgets.hovered.bg_fill = Color32::from_rgb(44, 55, 84);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(52, 66, 100);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 10.0.into();
    }
    style.visuals = v;
    ctx.set_style_of(egui::Theme::Dark, style);
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
                if e.path().extension().is_some_and(|x| x == "rs") {
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

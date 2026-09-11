//! The instrument's look: lacquer, brass and engraved type.
//!
//! Everything here is painted rather than loaded. There is no image asset in
//! this crate, because every surface the editor draws is also a surface it has
//! to reason about: the microphones sit at a computed point on the soundboard,
//! a sounding string lights along its own line. A bitmap would duplicate that
//! geometry and drift from it.
//!
//! The painting vocabulary is `phonix_ui::theme`; what is here is the palette
//! the shared widgets read, the display face, and the brass.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use phonix_ui::theme::{gradient_v, tracked_text, Palette, Relief};

use crate::colors::*;

pub use phonix_ui::theme::display_font;

/// The colours the shared knobs, meters and pickers read on this instrument.
pub const PALETTE: Palette = Palette {
    bg_dark: BG_DARK,
    bg_panel: BG_PANEL,
    bg_raised: BG_RAISED,
    border: BORDER,
    text_primary: TEXT_PRIMARY,
    text_dim: TEXT_DIM,
    accent: GOLD,
    warm: FELT_RED,
    ok: GOLD_BRIGHT,
    warn: GOLD,
    plate_top: BG_RAISED,
    plate_bottom: BG_PANEL,
    plate_edge: BORDER,
    well: BG_DARK,
    well_edge: CASE_EDGE,
    lamp_off: BG_DARK,
    silk: TEXT_PRIMARY,
    silk_dim: TEXT_DIM,
    pointer: Color32::from_rgb(228, 218, 198),
    pointer_shadow: Color32::from_rgb(34, 30, 24),
};

/// Register the display face. Idempotent, but call it once: `set_fonts`
/// rebuilds every atlas.
pub fn install_fonts(ctx: &egui::Context) {
    phonix_ui::theme::install_display_face(ctx, include_bytes!("../assets/NotoSerifDisplay-Regular.ttf"));
}

/// Dark, warm, and low-contrast enough that the brass reads as the bright
/// thing on the panel. Installs the palette the shared widgets read.
pub fn apply_visuals(ctx: &egui::Context) {
    PALETTE.install(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_LACQUER;
    v.window_fill = BG_LACQUER;
    v.extreme_bg_color = BG_DARK;
    v.widgets.noninteractive.bg_fill = BG_PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    v.widgets.inactive.bg_fill = BG_RAISED;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    v.widgets.hovered.bg_fill = BG_RAISED;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, GOLD.gamma_multiply(0.5));
    v.widgets.active.bg_fill = BG_RAISED;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, GOLD);
    v.selection.bg_fill = GOLD.gamma_multiply(0.25);
    v.selection.stroke = Stroke::new(1.0_f32, GOLD);
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    ctx.set_visuals(v);
}

/// Three stops, for the nameplate: dark edge, bright face, dark edge again.
/// That is what makes a flat rectangle read as rolled brass.
pub fn gradient_plate(ui: &Ui, rect: Rect) {
    let mid = Rect::from_min_max(
        Pos2::new(rect.left(), rect.center().y - rect.height() * 0.5),
        Pos2::new(rect.right(), rect.center().y),
    );
    let low = Rect::from_min_max(mid.left_bottom(), rect.right_bottom());
    gradient_v(ui, mid, PLATE_EDGE, PLATE_FACE);
    gradient_v(ui, low, PLATE_FACE, PLATE_EDGE);
}

/// A lamp: halo, body, and the small white glint that sells it as glass.
pub fn lamp(ui: &Ui, centre: Pos2, radius: f32, on: bool, tint: Color32) {
    let p = ui.painter();
    if on {
        p.circle_filled(centre, radius * 2.2, tint.gamma_multiply(0.20));
        p.circle_filled(centre, radius * 1.5, tint.gamma_multiply(0.35));
    }
    let body = if on { tint } else { BG_DARK };
    p.circle_filled(centre, radius, body);
    p.circle_stroke(centre, radius, Stroke::new(1.0_f32, BORDER));
    if on {
        p.circle_filled(
            centre - Vec2::splat(radius * 0.35),
            radius * 0.28,
            Color32::from_rgba_unmultiplied(255, 255, 255, 150),
        );
    }
}

/// A cluster heading: the display face, tracked, in gold, over a hairline.
pub fn cluster_header(ui: &Ui, rect: Rect, title: &str) {
    let baseline = Pos2::new(rect.left(), rect.top() + 7.0);
    let w = tracked_text(ui, baseline, title, display_font(ui.ctx(), 11.0), GOLD, 1.6, Relief::Flat);
    ui.painter().line_segment(
        [
            Pos2::new(rect.left(), baseline.y + 9.0),
            Pos2::new(rect.left() + w.max(40.0), baseline.y + 9.0),
        ],
        Stroke::new(1.0_f32, BORDER),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The palette has to stay dark enough for brass to be the bright thing.
    #[test]
    fn the_lacquer_is_darker_than_the_brass() {
        let lum = |c: Color32| {
            0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
        };
        assert!(lum(BG_LACQUER) < 30.0, "the lacquer is not dark");
        assert!(lum(GOLD) > 3.0 * lum(BG_LACQUER), "brass does not stand out");
        assert!(lum(LACQUER_TOP) > lum(LACQUER_BOTTOM), "the sheen is upside down");
    }

    /// The microphone colours and the meter needles are one idea; if they ever
    /// diverge the stereo metaphor breaks silently.
    #[test]
    fn the_microphones_and_the_needles_agree() {
        assert_eq!(MIC_LEFT, Color32::from_rgb(212, 175, 55));
        assert_eq!(MIC_RIGHT, Color32::from_rgb(192, 196, 204));
        assert_ne!(MIC_LEFT, MIC_RIGHT, "left and right must be tellable apart");
    }

    /// The shared knob reads the palette; on this instrument it must be the
    /// same warm black the forked copy painted.
    #[test]
    fn the_palette_is_what_the_widgets_used_to_read() {
        let ctx = egui::Context::default();
        apply_visuals(&ctx);
        let p = Palette::of(&ctx);
        assert_eq!(p.bg_dark, BG_DARK);
        assert_eq!(p.text_dim, TEXT_DIM);
        assert_eq!(p.pointer, Color32::from_rgb(228, 218, 198));
    }
}

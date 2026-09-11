//! A pair of needles, because a piano is not a modular synth.
//!
//! Two needles on one dial rather than two bars: left gold, right silver, the
//! same two colours as the microphones on the soundboard. Moving one microphone
//! and watching one needle answer is the whole point of naming them the same.

use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Ui, Vec2};

use crate::colors::*;

/// Peak decay per frame. The meter publishes instantaneous peaks 30 times a
/// second; without a fall-off the needles would flicker rather than swing.
pub const FALL: f32 = 0.92;

/// Let a held value fall towards a fresh peak.
pub fn ballistics(display: &mut f32, peak: f32) {
    *display = peak.max(*display * FALL);
}

/// Needle angle for a 0..1 level, in radians, measured from straight up.
///
/// The scale is in decibels, not amplitude: a linear dial spends most of its
/// travel in a range a piano never sits in.
fn angle(level: f32) -> f32 {
    const SWEEP: f32 = 1.05; // ±60°
    let db = 20.0 * level.max(1e-4).log10(); // -80..0
    let t = ((db + 40.0) / 40.0).clamp(0.0, 1.0);
    -SWEEP + t * 2.0 * SWEEP
}

/// Draw the dial. `l` and `r` are already-ballisticked 0..1 levels.
pub fn draw(ui: &Ui, rect: Rect, l: f32, r: f32) {
    let p = ui.painter();
    p.rect_filled(rect, 3.0, BG_DARK);
    p.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Inside);

    // The dial is inscribed in the panel: the needle sweeps to `radius`, the
    // ticks sit just inside it and the legends inside those, so nothing the
    // meter draws can land outside its own bezel.
    let pivot = Pos2::new(rect.center().x, rect.bottom() - 12.0);
    let radius = (rect.height() - 26.0).min(rect.width() * 0.42);

    // Scale: brass ticks, and a red arc where the piano is about to clip.
    for (frac, label) in [(0.0f32, "-40"), (0.25, "-30"), (0.5, "-20"), (0.75, "-10"), (1.0, "0")] {
        let a = -1.05 + frac * 2.1;
        let dir = Vec2::new(a.sin(), -a.cos());
        let hot = frac > 0.88;
        let col = if hot { FELT_RED } else { GOLD.gamma_multiply(0.6) };
        p.line_segment(
            [pivot + dir * (radius - 5.0), pivot + dir * radius],
            Stroke::new(if hot { 2.0_f32 } else { 1.0_f32 }, col),
        );
        p.text(
            pivot + dir * (radius - 14.0),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(7.0),
            TEXT_DIM,
        );
    }

    // The red zone as a filled arc segment, sampled like every other curve here.
    let hot: Vec<Pos2> = (0..=8)
        .map(|i| {
            let a = angle(0.9) + (1.05 - angle(0.9)) * i as f32 / 8.0;
            pivot + Vec2::new(a.sin(), -a.cos()) * (radius - 1.5)
        })
        .collect();
    p.add(Shape::line(hot, Stroke::new(2.0_f32, FELT_RED.gamma_multiply(0.7))));

    for (level, col) in [(l, MIC_LEFT), (r, MIC_RIGHT)] {
        let a = angle(level);
        let tip = pivot + Vec2::new(a.sin(), -a.cos()) * (radius - 3.0);
        if level > 0.9 {
            p.line_segment([pivot, tip], Stroke::new(3.0_f32, col.gamma_multiply(0.3)));
        }
        p.line_segment([pivot, tip], Stroke::new(1.5_f32, col));
    }
    p.circle_filled(pivot, 3.0, Color32::from_rgb(60, 52, 40));
    p.circle_stroke(pivot, 3.0, Stroke::new(1.0_f32, BORDER));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A needle must rise with level and never leave its arc, or it swings
    /// through the bezel.
    #[test]
    fn the_needle_rises_and_stays_on_the_dial() {
        let quiet = angle(0.001);
        let mid = angle(0.1);
        let loud = angle(1.0);
        assert!(quiet < mid && mid < loud, "the needle does not rise");
        for lvl in [0.0, 1e-6, 0.5, 1.0, 4.0] {
            let a = angle(lvl);
            assert!((-1.06..=1.06).contains(&a), "off the dial at {lvl}: {a}");
        }
    }

    /// A peak grabs the needle at once and then lets it fall — the opposite
    /// would make the loudest moment the one you cannot see.
    #[test]
    fn peaks_are_instant_and_the_fall_is_gradual() {
        let mut d = 0.0;
        ballistics(&mut d, 0.9);
        assert_eq!(d, 0.9, "the needle lagged the peak");
        ballistics(&mut d, 0.0);
        assert!(d > 0.8 && d < 0.9, "the fall is not gradual: {d}");
        for _ in 0..200 {
            ballistics(&mut d, 0.0);
        }
        assert!(d < 0.01, "the needle never settles: {d}");
    }
}

//! The piano, painted from above.
//!
//! Layers go on in the order a piano is built: case, soundboard, iron frame,
//! bridges, strings, dampers. Then the two things that move — the microphones
//! you drag, and the strings that light when they sound.
//!
//! Every position comes from [`crate::piano_geom`]; nothing here invents
//! geometry, so what the eye sees and what a drag hits are the same numbers.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

use crate::colors::*;
use crate::piano_geom as geom;
use phonix_ui::theme::{engraved, fill_polygon, gradient_v};

/// What the user did to the scene this frame.
#[derive(Default)]
pub struct SceneResult {
    /// A microphone was dragged, or reset by double-click.
    pub width_changed: Option<f32>,
}

/// Everything the scene needs to draw itself.
pub struct SceneState<'a> {
    /// 0..1, the model's listening spread.
    pub width: f32,
    /// The default `width` restores on double-click.
    pub width_default: f32,
    /// Notes currently sounding, so their strings light.
    pub active_notes: &'a [u8],
    /// Dampers lift off the strings when this is true.
    pub pedal_down: bool,
}

/// Map piano space onto a rect.
fn at(rect: Rect, uv: geom::Uv) -> Pos2 {
    Pos2::new(rect.left() + uv.0 * rect.width(), rect.top() + uv.1 * rect.height())
}

fn poly(rect: Rect, pts: &[geom::Uv]) -> Vec<Pos2> {
    pts.iter().map(|&p| at(rect, p)).collect()
}

/// The bounding box the piano is drawn in, centred in the space it is given.
///
/// A grand is much deeper than it is wide seen from above, so the box keeps a
/// fixed portrait ratio rather than stretching to whatever it is handed.
pub fn piano_box(area: Rect) -> Rect {
    const RATIO: f32 = 460.0 / 560.0;
    let h = area.height();
    let w = (h * RATIO).min(area.width());
    Rect::from_center_size(area.center(), Vec2::new(w, h))
}

/// Paint the instrument and handle the microphones.
pub fn draw(ui: &mut Ui, area: Rect, st: &SceneState<'_>) -> SceneResult {
    let mut out = SceneResult::default();
    let piano = piano_box(area);

    shadow(ui, piano);
    case(ui, piano);
    soundboard(ui, piano);
    frame(ui, piano);
    bridges(ui, piano);
    strings(ui, piano, st.active_notes);
    dampers(ui, piano, st.pedal_down);
    out.width_changed = microphones(ui, piano, st);

    out
}

/// The vertical extent of the case at a given x, by scanline.
///
/// Anything drawn in straight lines across the board — the grain, later the
/// frame — has to stop at the rim, and the rim is a curve, not the bounding box.
fn span_at_x(pts: &[Pos2], x: f32) -> Option<(f32, f32)> {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for w in pts.windows(2).chain(std::iter::once(
        [*pts.last().unwrap(), pts[0]].as_slice(),
    )) {
        let (a, b) = (w[0], w[1]);
        if (a.x <= x) == (b.x <= x) {
            continue;
        }
        let t = (x - a.x) / (b.x - a.x);
        let y = a.y + (b.y - a.y) * t;
        lo = lo.min(y);
        hi = hi.max(y);
    }
    (lo < hi).then_some((lo, hi))
}

fn shadow(ui: &Ui, r: Rect) {
    let pts: Vec<Pos2> = poly(r, &geom::case_outline())
        .into_iter()
        .map(|p| p + Vec2::new(6.0, 8.0))
        .collect();
    let ink = Color32::from_rgba_unmultiplied(0, 0, 0, 90);
    fill_polygon(ui, &pts, ink, ink);
}

/// The lacquered case, with the inset line that reads as an open lid: the eye
/// takes that second contour for the far edge of the rim, so the inside looks
/// like the inside.
fn case(ui: &Ui, r: Rect) {
    let outline = poly(r, &geom::case_outline());
    fill_polygon(ui, &outline, CASE_LACQUER, CASE_LACQUER);
    ui.painter()
        .add(Shape::closed_line(outline, Stroke::new(2.0_f32, CASE_EDGE)));
    ui.painter().add(Shape::closed_line(
        poly(r, &geom::case_inset(0.035)),
        Stroke::new(1.5_f32, CASE_INNER_RIM),
    ));
}

/// Spruce, lit from the tail, filling the case itself rather than a box around
/// it.
fn soundboard(ui: &mut Ui, r: Rect) {
    let inner = geom::case_inset(0.07);
    let pts = poly(r, &inner);
    fill_polygon(ui, &pts, WOOD_TOP, WOOD_BOTTOM);

    // Grain, running very slightly off vertical as a real board's does, and
    // ending at the rim rather than at the edge of the box.
    for i in 1..14 {
        let u = i as f32 / 14.0;
        let a = at(r, (u, 0.0));
        let b = at(r, (u + 0.035, 1.0));
        let Some((y0, y1)) = span_at_x(&pts, a.x) else { continue };
        let Some((y2, y3)) = span_at_x(&pts, b.x) else { continue };
        let top = Pos2::new(a.x, y0.max(a.y).min(y1));
        let bot = Pos2::new(b.x, y3.min(b.y).max(y2));
        ui.painter().line_segment([top, bot], Stroke::new(1.0_f32, WOOD_GRAIN));
    }

    // A vignette inside the rim: the lid is open above this board, and the
    // light falls off towards the case. Three passes of the same contour, each
    // wider and fainter, is the cheapest honest way to get it.
    for (i, (w, a)) in [(18.0_f32, 34u8), (9.0_f32, 40), (1.0_f32, 90)].into_iter().enumerate() {
        let ring = poly(r, &geom::case_inset(0.07 + i as f32 * 0.004));
        ui.painter().add(Shape::closed_line(
            ring,
            Stroke::new(w, Color32::from_rgba_unmultiplied(0, 0, 0, a)),
        ));
    }
    let _ = pts;
}

/// Pin block and struts: the gilded ironwork that takes twenty tonnes of
/// string tension.
fn frame(ui: &mut Ui, r: Rect) {
    let (v0, v1) = geom::PIN_BLOCK_V;
    // In front of the pins is the keybed, where the action sits under the keys.
    // Without it the board runs all the way to the front edge and the case
    // reads as a tray rather than as an instrument with a mechanism in it.
    let bed = Rect::from_min_max(at(r, (0.0, v1)), at(r, (1.0, 1.0)));
    ui.painter().rect_filled(bed, 0.0, KEYBED);
    ui.painter().line_segment(
        [bed.left_top(), bed.right_top()],
        Stroke::new(1.0_f32, CASE_EDGE),
    );
    let block = Rect::from_min_max(at(r, (0.03, v0)), at(r, (0.97, v1)));
    gradient_v(ui, block, FRAME_DARK, FRAME_LIGHT);
    ui.painter()
        .rect_stroke(block, 1.0, Stroke::new(1.0_f32, CASE_EDGE), egui::StrokeKind::Inside);

    for i in 0..30 {
        let u = 0.06 + i as f32 / 29.0 * 0.88;
        ui.painter().circle_filled(
            at(r, (u, (v0 + v1) * 0.5)),
            1.6,
            TUNING_PIN.gamma_multiply(0.8),
        );
    }

    for [a, b] in geom::STRUTS {
        let (p, q) = (at(r, a), at(r, b));
        let dir = (q - p).normalized();
        let n = Vec2::new(-dir.y, dir.x);
        // Gilded iron: a dark body, a lit edge on one side and a shadow on the
        // other. Flat fill reads as a green stripe over the wood.
        ui.painter().add(Shape::convex_polygon(
            vec![p + n * 2.8, p - n * 2.8, q - n * 1.8, q + n * 1.8],
            FRAME_DARK,
            Stroke::NONE,
        ));
        ui.painter().line_segment(
            [p + n * 2.4, q + n * 1.5],
            Stroke::new(1.2_f32, FRAME_LIGHT),
        );
        ui.painter().line_segment(
            [p - n * 2.6, q - n * 1.7],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 110)),
        );
    }
}

fn bridges(ui: &Ui, r: Rect) {
    ui.painter().add(Shape::line(
        poly(r, &geom::long_bridge()),
        Stroke::new(6.0_f32, BRIDGE),
    ));
    let [a, b] = geom::bass_bridge();
    ui.painter()
        .line_segment([at(r, a), at(r, b)], Stroke::new(7.0_f32, BRIDGE_BASS));
}

/// The string fan. Steel first, then the wound bass strings over the top of it,
/// which is what makes the crossing visible.
fn strings(ui: &Ui, r: Rect, active: &[u8]) {
    let paint = |n: u8, w: f32, col: Color32| {
        let [a, b] = geom::string_line(n);
        ui.painter()
            .line_segment([at(r, a), at(r, b)], Stroke::new(w, col));
    };

    // Back lengths first: they sit behind the bridge, so they go on under it.
    // Each is cut off at the rim, because a hitch pin is on the frame, and a
    // string drawn past it hangs in the air outside the case.
    let board = poly(r, &geom::case_inset(0.09));
    for n in geom::LOWEST..=geom::HIGHEST {
        let col = if geom::is_bass(n) { STRING_BASS } else { STRING_STEEL };
        let [a, b] = geom::back_length(n);
        let (pa, mut pb) = (at(r, a), at(r, b));
        let Some((y0, y1)) = span_at_x(&board, pb.x) else { continue };
        pb.y = pb.y.clamp(y0 + 2.0, y1 - 2.0);
        ui.painter()
            .line_segment([pa, pb], Stroke::new(0.7_f32, col.gamma_multiply(0.30)));
    }

    for n in (geom::BASS_TOP + 1)..=geom::HIGHEST {
        let alpha = if n % 3 == 0 { 185 } else { 140 };
        paint(n, 0.8, STRING_STEEL.gamma_multiply(alpha as f32 / 255.0));
    }
    for n in geom::LOWEST..=geom::BASS_TOP {
        paint(n, 1.5, STRING_BASS.gamma_multiply(0.9));
    }

    // A sounding string, lit along its own length: two passes, a wide dim one
    // under a narrow bright one, which is how every glow in this codebase is
    // made.
    for &n in active {
        if !(geom::LOWEST..=geom::HIGHEST).contains(&n) {
            continue;
        }
        let lit = if geom::is_bass(n) { STRING_LIT_BASS } else { STRING_LIT };
        paint(n, 3.0, lit.gamma_multiply(0.25));
        paint(n, 1.2, lit);
    }
}

/// The damper row. When the pedal is down every damper lifts off its string —
/// the pedal is a thing you can see on the instrument, not a lamp somewhere.
fn dampers(ui: &Ui, r: Rect, pedal_down: bool) {
    let lift = if pedal_down { -3.0 } else { 0.0 };
    let alpha = if pedal_down { 0.40 } else { 1.0 };
    for n in geom::LOWEST..=geom::LAST_DAMPED {
        let Some(uv) = geom::damper_at(n) else { continue };
        let c = at(r, uv) + Vec2::new(0.0, lift);
        let block = Rect::from_center_size(c, Vec2::new(4.0, 5.0));
        ui.painter()
            .rect_filled(block, 1.0, DAMPER.gamma_multiply(alpha * 0.85));
        ui.painter().line_segment(
            [block.left_bottom(), block.right_bottom()],
            Stroke::new(1.0_f32, FELT_RED.gamma_multiply(alpha)),
        );
    }
}

/// The two microphones. Dragging either one moves both, because what is being
/// set is the distance between them.
fn microphones(ui: &mut Ui, r: Rect, st: &SceneState<'_>) -> Option<f32> {
    let (lu, ru) = geom::mic_positions(st.width);
    let lp = at(r, (lu, geom::MIC_V));
    let rp = at(r, (ru, geom::MIC_V));

    // The line between them, dashed, with a tick at the centre they are
    // symmetric about.
    ui.painter().add(Shape::dashed_line(
        &[lp, rp],
        Stroke::new(1.0_f32, GOLD.gamma_multiply(0.28)),
        6.0,
        5.0,
    ));
    let mid = at(r, (geom::MIC_CENTRE_U, geom::MIC_V));
    ui.painter().line_segment(
        [mid - Vec2::new(0.0, 4.0), mid + Vec2::new(0.0, 4.0)],
        Stroke::new(1.0_f32, GOLD.gamma_multiply(0.4)),
    );

    let mut changed = None;
    for (is_left, pos, col, tag) in [
        (true, lp, MIC_LEFT, "L"),
        (false, rp, MIC_RIGHT, "R"),
    ] {
        let id = ui.id().with(("mic", is_left));
        let hit = Rect::from_center_size(pos, Vec2::splat(22.0));
        let resp: Response = ui.interact(hit, id, Sense::click_and_drag());

        if resp.double_clicked() {
            changed = Some(st.width_default);
        } else if resp.dragged() {
            let du = resp.drag_delta().x / r.width().max(1.0);
            let u = if is_left { lu + du } else { ru + du };
            changed = Some(geom::width_for_mic_u(u, is_left));
        }

        let glow = if resp.dragged() {
            0.55
        } else if resp.hovered() {
            0.32
        } else {
            0.0
        };
        if glow > 0.0 {
            ui.painter().circle_filled(pos, 14.0, col.gamma_multiply(glow));
        }
        ui.painter().circle_filled(pos, 9.0, col);
        ui.painter()
            .circle_stroke(pos, 9.0, Stroke::new(1.0_f32, Color32::from_rgb(30, 28, 24)));
        ui.painter().circle_filled(
            pos - Vec2::splat(2.5),
            2.5,
            Color32::from_rgba_unmultiplied(255, 255, 255, 170),
        );
        ui.painter().text(
            pos + Vec2::new(0.0, 16.0),
            egui::Align2::CENTER_CENTER,
            tag,
            egui::FontId::proportional(9.0),
            TEXT_DIM,
        );

        if resp.hovered() || resp.dragged() {
            let w = changed.unwrap_or(st.width);
            engraved(
                ui,
                mid - Vec2::new(0.0, 22.0),
                &format!("SPREAD {:.0}%", w * 100.0),
                egui::FontId::monospace(10.0),
                TEXT_PRIMARY,
                egui::Align2::CENTER_CENTER,
            );
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The piano keeps its shape whatever it is given: a grand seen from above
    /// is deeper than it is wide, and stretching it to fill a wide box would
    /// read as a table.
    #[test]
    fn the_piano_keeps_its_proportions() {
        let wide = piano_box(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 560.0)));
        let ratio = wide.width() / wide.height();
        assert!((ratio - 460.0 / 560.0).abs() < 0.01, "ratio drifted: {ratio}");
        assert!(wide.width() < 1200.0, "it stretched to fill");

        // Narrow enough and it must fit the width instead of overflowing.
        let narrow = piano_box(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 560.0)));
        assert!(narrow.width() <= 200.0, "it overflowed a narrow box");
    }

    #[test]
    fn the_microphones_sit_inside_the_case() {
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(460.0, 560.0));
        for i in 0..=10 {
            let (l, rr) = geom::mic_positions(i as f32 / 10.0);
            for u in [l, rr] {
                let p = at(r, (u, geom::MIC_V));
                assert!(r.contains(p), "a microphone left the case at width {i}");
            }
        }
    }
}

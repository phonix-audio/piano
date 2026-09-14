//! The effects page: the chain a patch carries, opened up.
//!
//! Which effects run and in what order is not editable here -- that is what
//! curated means, and the recipe lives with the patch. Everything inside each
//! effect is. Drawn with Piano's own widgets, on the same lacquer, in the
//! same clusters as the instrument page, and each effect shows what it does:
//! the EQ its response, the compressor its transfer, from
//! the same numbers the audio thread runs.

use egui::{Align2, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use phonix_fx::effects::parametric_eq::ParametricEq;
use phonix_fx::{ChainSpec, EffectSpec, ParamKind, ParamSpec, Registry, SlotSpec, SpecValue};

use crate::colors::{BG_DARK, BORDER, GOLD, GOLD_BRIGHT, TEXT_DIM};
use phonix_ui::preset_picker::{picker_ui, NamedPreset, PresetPickerState, PresetPickerStyle};
use phonix_ui::theme::{engraved, fill_polygon};
use phonix_ui::widgets;

use crate::theme;

/// What the page keeps between frames: the dropdown's own state, and the
/// kinds this build draws.
pub struct FxPageState {
    reverb_picker: PresetPickerState,
    registry: Registry,
}

impl Default for FxPageState {
    fn default() -> Self {
        FxPageState { reverb_picker: PresetPickerState::default(), registry: Registry::builtin() }
    }
}

/// One slot's written parameters, read against what the kind declares: an
/// id the recipe never wrote reads as the kind's default, and writing it
/// adds it.
struct SlotAccess<'a> {
    slot: &'a mut SlotSpec,
    spec: &'static EffectSpec,
    touched: bool,
}

impl SlotAccess<'_> {
    fn param(&self, id: &str) -> Option<&ParamSpec> {
        self.spec.param(id)
    }

    /// The value the effect holds for `id`; zero for an id the kind does
    /// not declare, so a panel drawn for another version of the kind shows
    /// a still knob rather than nothing at all.
    fn get(&self, id: &str) -> f32 {
        let Some(p) = self.param(id) else { return 0.0 };
        match self.slot.get(id) {
            Some(v) => v.resolve(p).0.as_f32(),
            None => p.default.as_f32(),
        }
    }

    fn get_bool(&self, id: &str) -> bool {
        let Some(p) = self.param(id) else { return false };
        match self.slot.get(id) {
            Some(v) => v.resolve(p).0.as_bool(),
            None => p.default.as_bool(),
        }
    }

    fn variant(&self, id: &str) -> &'static str {
        let Some(p) = self.param(id) else { return "" };
        match self.slot.get(id) {
            Some(v) => v.resolve(p).0.as_variant().unwrap_or(""),
            None => p.default.as_variant().unwrap_or(""),
        }
    }

    fn set(&mut self, id: &str, value: impl Into<SpecValue>) {
        let value = value.into();
        if self.slot.get(id) != Some(&value) {
            self.slot.set(id, value);
            self.touched = true;
        }
    }
}

const GAP: f32 = 20.0;
const KNOB: f32 = 40.0;
const PITCH_X: f32 = 66.0;
const PITCH_Y: f32 = KNOB + 26.0 + 12.0;
const PLOT_H: f32 = 110.0;
/// The rate the response curves are drawn at. A picture, not the audio: the
/// shape of a shelf at 90 Hz does not move between 44.1 and 96 kHz on a plot
/// that spans 20 Hz to 20 kHz.
const CURVE_SR: f32 = 48_000.0;
/// Vertical span of the EQ plot, in dB either side of unity.
const EQ_PLOT_DB: f32 = 18.0;

/// A knob over a declared float parameter: its range, curve and unit come
/// from the kind.
fn knob(ui: &mut Ui, at: Pos2, label: &str, access: &mut SlotAccess, id: &str) {
    let Some(p) = access.param(id).copied() else { return };
    let ParamKind::Float { min, max, curve } = p.kind else { return };
    let value = access.get(id);
    let to_norm = |v: f32| match curve {
        phonix_fx::Curve::Log if min > 0.0 => ((v / min).ln() / (max / min).ln()).clamp(0.0, 1.0),
        _ => ((v - min) / (max - min)).clamp(0.0, 1.0),
    };
    let from_norm = |n: f32| match curve {
        phonix_fx::Curve::Log if min > 0.0 => min * (max / min).powf(n),
        _ => min + n * (max - min),
    };
    let mut norm = to_norm(value);
    let old = norm;
    let cell = Rect::from_min_size(at, Vec2::new(widgets::KNOB_GROUP_W, KNOB + 26.0));
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
    cui.vertical_centered(|ui| {
        widgets::knob_fmt(ui, &mut norm, label, &p.unit.format(value), KNOB, GOLD);
    });
    if (norm - old).abs() > 1e-6 {
        access.set(id, from_norm(norm));
    }
}

/// The slot's own mix, drawn like any knob.
fn mix_knob(ui: &mut Ui, at: Pos2, slot: &mut SlotSpec) -> bool {
    let mut norm = slot.mix.clamp(0.0, 1.0);
    let old = norm;
    let cell = Rect::from_min_size(at, Vec2::new(widgets::KNOB_GROUP_W, KNOB + 26.0));
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
    cui.vertical_centered(|ui| {
        widgets::knob_fmt(ui, &mut norm, "Mix", &format!("{:.0}%", slot.mix * 100.0), KNOB, GOLD);
    });
    if (norm - old).abs() > 1e-6 {
        slot.mix = norm;
        return true;
    }
    false
}

/// A lamp with a word beside it, and the whole thing a switch.
fn lamp_switch(ui: &mut Ui, centre: Pos2, on: bool, label: &str, salt: impl std::hash::Hash) -> bool {
    theme::lamp(ui, centre, 4.0, on, GOLD_BRIGHT);
    ui.painter().text(
        centre + Vec2::new(10.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(9.0),
        if on { GOLD_BRIGHT } else { TEXT_DIM },
    );
    let hit = Rect::from_center_size(centre + Vec2::new(22.0, 0.0), Vec2::new(64.0, 18.0));
    ui.interact(hit, ui.id().with(salt), Sense::click()).clicked()
}

/// The dark inset every plot sits in, with a hairline at unity.
fn plot_frame(ui: &Ui, r: Rect) {
    ui.painter().rect_filled(r, 3.0, BG_DARK);
    ui.painter()
        .rect_stroke(r, 3.0, Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Inside);
}

fn caption(ui: &Ui, at: Pos2, text: &str) {
    ui.painter().text(at, Align2::LEFT_TOP, text, FontId::proportional(10.0), TEXT_DIM);
}

fn hz(v: f32) -> String {
    if v >= 1000.0 { format!("{:.1} kHz", v / 1000.0) } else { format!("{v:.0} Hz") }
}

/// The EQ's response, from the effect's own filters.
fn eq_plot(ui: &Ui, r: Rect, a: &SlotAccess) {
    plot_frame(ui, r);
    let mut eq = ParametricEq::new(CURVE_SR);
    for b in 0..4 {
        eq.set_band_freq(b, a.get(&format!("band.{b}.freq")));
        eq.set_band_gain(b, a.get(&format!("band.{b}.gain")));
        eq.set_band_q(b, a.get(&format!("band.{b}.q")));
        eq.set_band_enabled(b, a.get_bool(&format!("band.{b}.enabled")));
        let type_id = format!("band.{b}.type");
        let t = a.param(&type_id).and_then(|p| p.variant_index(a.variant(&type_id))).unwrap_or(0);
        eq.set_band_type(b, t as u8);
    }

    // Log frequency across, dB up. Ticks where an ear counts: 100, 1k, 10k.
    let (f0, f1) = (20.0f32, 20_000.0f32);
    let x_of = |f: f32| r.left() + (f / f0).log10() / (f1 / f0).log10() * r.width();
    let y_of = |d: f32| r.center().y - (d / EQ_PLOT_DB).clamp(-1.0, 1.0) * (r.height() * 0.5 - 6.0);
    for f in [100.0, 1000.0, 10_000.0] {
        let x = x_of(f);
        ui.painter().line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0_f32, BORDER.gamma_multiply(0.6)));
        ui.painter().text(Pos2::new(x + 3.0, r.bottom() - 2.0), Align2::LEFT_BOTTOM, hz(f), FontId::proportional(8.0), TEXT_DIM);
    }
    ui.painter().line_segment([Pos2::new(r.left(), y_of(0.0)), Pos2::new(r.right(), y_of(0.0))], Stroke::new(1.0_f32, BORDER));

    let n = r.width() as usize;
    let mut line = Vec::with_capacity(n + 1);
    let mut area = Vec::with_capacity(n + 3);
    area.push(Pos2::new(r.left(), y_of(0.0)));
    for i in 0..=n {
        let f = f0 * (f1 / f0).powf(i as f32 / n as f32);
        let d = 20.0 * eq.magnitude_at(f).max(1e-6).log10();
        let p = Pos2::new(x_of(f), y_of(d));
        line.push(p);
        area.push(p);
    }
    area.push(Pos2::new(r.right(), y_of(0.0)));
    fill_polygon(ui, &area, GOLD.gamma_multiply(0.18), GOLD.gamma_multiply(0.06));
    ui.painter().add(Shape::line(line, Stroke::new(1.5_f32, GOLD)));
}

/// A transfer curve, input dB across and output dB up, for the dynamics
/// slots. `out` maps an input level to the level that leaves.
fn transfer_plot(ui: &Ui, r: Rect, out: impl Fn(f32) -> f32) {
    plot_frame(ui, r);
    let span = 60.0f32;
    let x_of = |d: f32| r.left() + (d + span) / span * r.width();
    let y_of = |d: f32| r.bottom() - (d + span) / span * r.height();
    // Unity, faint: what the signal would do if the slot did nothing.
    ui.painter().line_segment([Pos2::new(x_of(-span), y_of(-span)), Pos2::new(x_of(0.0), y_of(0.0))], Stroke::new(1.0_f32, BORDER));
    for d in [-40.0, -20.0] {
        ui.painter().line_segment([Pos2::new(x_of(d), r.top()), Pos2::new(x_of(d), r.bottom())], Stroke::new(1.0_f32, BORDER.gamma_multiply(0.5)));
        ui.painter().text(Pos2::new(x_of(d) + 3.0, r.bottom() - 2.0), Align2::LEFT_BOTTOM, format!("{d:.0}"), FontId::proportional(8.0), TEXT_DIM);
    }
    let n = r.width() as usize;
    let line: Vec<Pos2> = (0..=n)
        .map(|i| {
            let din = -span + span * i as f32 / n as f32;
            Pos2::new(x_of(din), y_of(out(din).clamp(-span, 0.0)))
        })
        .collect();
    ui.painter().add(Shape::line(line, Stroke::new(1.5_f32, GOLD)));
}

/// What each band is under its automatic type, which the recipe never changes.
const BAND_NAMES: [&str; 4] = ["LOW\nSHELF", "PEAK 1", "PEAK 2", "HIGH\nSHELF"];

/// Draw the page. Returns true when anything was moved, so the caller knows
/// to publish the chain.
pub fn draw(ui: &mut Ui, r: Rect, spec: &mut ChainSpec, state: &mut FxPageState) -> bool {
    if spec.is_empty() {
        engraved(ui, r.center(), "this patch carries no effects", FontId::proportional(12.0), TEXT_DIM, Align2::CENTER_CENTER);
        return false;
    }

    let mut changed = false;
    let count = spec.len();
    let col_w = (r.width() - GAP * (count as f32 - 1.0)) / count as f32;

    for (i, slot) in spec.slots.iter_mut().enumerate() {
        let col = Rect::from_min_size(
            Pos2::new(r.left() + i as f32 * (col_w + GAP), r.top()),
            Vec2::new(col_w, r.height()),
        );
        let Some(entry) = state.registry.get(&slot.kind) else {
            theme::cluster_header(ui, Rect::from_min_size(col.left_top(), Vec2::new(col.width(), 20.0)), &slot.kind.to_uppercase());
            caption(ui, Pos2::new(col.left() + 4.0, col.top() + 30.0), "not in this build; kept as written");
            continue;
        };
        let espec = entry.spec;
        let mut y = col.top();

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(col.left(), y), Vec2::new(col.width(), 20.0)), &espec.name.to_uppercase());
        if lamp_switch(ui, Pos2::new(col.right() - 44.0, y + 10.0), slot.enabled, if slot.enabled { "ON" } else { "OFF" }, ("fx_on", i)) {
            slot.enabled = !slot.enabled;
            changed = true;
        }
        y += 30.0;

        let x0 = col.left() + 4.0;
        let plot = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(col.width() - 8.0, PLOT_H));
        let mut a = SlotAccess { slot, spec: espec, touched: false };

        match espec.kind {
            "parametric-eq" => {
                eq_plot(ui, plot, &a);
                y += PLOT_H + 12.0;
                for b in 0..4 {
                    let row_y = y + b as f32 * PITCH_Y;
                    let on_id = format!("band.{b}.enabled");
                    let on = a.get_bool(&on_id);
                    let c = Pos2::new(x0 + 8.0, row_y + 14.0);
                    theme::lamp(ui, c, 4.0, on, GOLD_BRIGHT);
                    ui.painter().text(Pos2::new(x0 + 2.0, row_y + 26.0), Align2::LEFT_TOP, BAND_NAMES[b], FontId::proportional(8.0), if on { GOLD } else { TEXT_DIM });
                    let hit = Rect::from_min_size(Pos2::new(x0, row_y), Vec2::new(48.0, KNOB + 26.0));
                    if ui.interact(hit, ui.id().with(("eq_band", b)), Sense::click()).clicked() {
                        a.set(&on_id, !on);
                    }
                    let at = |c: usize| Pos2::new(x0 + 50.0 + c as f32 * PITCH_X, row_y);
                    knob(ui, at(0), "Freq", &mut a, &format!("band.{b}.freq"));
                    knob(ui, at(1), "Gain", &mut a, &format!("band.{b}.gain"));
                    knob(ui, at(2), "Q", &mut a, &format!("band.{b}.q"));
                }
                y += 4.0 * PITCH_Y;
                caption(ui, Pos2::new(x0, y), "the shelf takes back what a modelled\nstring radiates below the soundboard");
            }
            "compressor" => {
                let (t, ratio, makeup) = (a.get("threshold"), a.get("ratio").max(1.0), a.get("makeup"));
                // Bus mode, which this slot runs in: a hard knee.
                transfer_plot(ui, plot, |din| if din > t { t + (din - t) / ratio } else { din } + makeup);
                y += PLOT_H + 12.0;
                let at = |c: usize, row: usize| Pos2::new(x0 + c as f32 * PITCH_X, y + row as f32 * PITCH_Y);
                knob(ui, at(0, 0), "Thresh", &mut a, "threshold");
                knob(ui, at(1, 0), "Ratio", &mut a, "ratio");
                knob(ui, at(2, 0), "Makeup", &mut a, "makeup");
                changed |= mix_knob(ui, at(3, 0), a.slot);
                knob(ui, at(0, 1), "Attack", &mut a, "attack");
                knob(ui, at(1, 1), "Release", &mut a, "release");
                y += 2.0 * PITCH_Y;
                caption(ui, Pos2::new(x0, y), "slow glue across the whole instrument,\nhard knee, no lookahead");
            }
            "reverb" => {
                // The type is a choice, not a quantity: a list, not a dial.
                let (variants, labels): (&[&str], &[&str]) = match a.param("type").map(|p| p.kind) {
                    Some(ParamKind::Enum { variants, labels }) => (variants, labels),
                    _ => (&[], &[]),
                };
                let names: Vec<NamedPreset> = labels.iter().map(|l| NamedPreset { name: l, category: None }).collect();
                let idx = variants.iter().position(|v| *v == a.variant("type")).unwrap_or(0);
                state.reverb_picker.current_idx = idx;
                let strip = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(col.width() - 8.0, 26.0));
                let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
                if let Some(new_idx) = picker_ui(&mut cui, &PresetPickerStyle { salt: "piano_reverb_type", accent: GOLD, dim: TEXT_DIM, name_in_combo: true }, &mut state.reverb_picker, &names) {
                    if let Some(v) = variants.get(new_idx) {
                        a.set("type", *v);
                    }
                }
                y += 40.0;
                let at = |c: usize, row: usize| Pos2::new(x0 + c as f32 * PITCH_X, y + row as f32 * PITCH_Y);
                knob(ui, at(0, 0), "Size", &mut a, "size");
                knob(ui, at(1, 0), "Decay", &mut a, "decay");
                knob(ui, at(2, 0), "Damp", &mut a, "damping");
                knob(ui, at(3, 0), "Width", &mut a, "width");
                knob(ui, at(0, 1), "Pre", &mut a, "predelay");
                changed |= mix_knob(ui, at(1, 1), a.slot);
                y += 2.0 * PITCH_Y;
                caption(ui, Pos2::new(x0, y), "the room past the microphones: the\nmodel has no walls of its own");
            }
            "stereo-imager" => {
                let at = |c: usize| Pos2::new(x0 + c as f32 * PITCH_X, y);
                knob(ui, at(0), "Width", &mut a, "width");
                knob(ui, at(1), "Mono", &mut a, "mono-freq");
                changed |= mix_knob(ui, at(2), a.slot);
                y += PITCH_Y;
                caption(ui, Pos2::new(x0, y), "where the listener sits against the\nsoundboard; the bass mono below");
            }
            _ => {
                // A kind without a bespoke panel: every float it declares, in rows of four.
                let ids: Vec<&str> = espec.params.iter().filter(|p| matches!(p.kind, ParamKind::Float { .. })).map(|p| p.id).collect();
                for (n, id) in ids.iter().enumerate() {
                    let at = Pos2::new(x0 + (n % 4) as f32 * PITCH_X, y + (n / 4) as f32 * PITCH_Y);
                    knob(ui, at, espec.param(id).map_or(id, |p| p.short), &mut a, id);
                }
            }
        }
        changed |= a.touched;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_reads_defaults_and_writes_only_what_moved() {
        let mut slot = SlotSpec::new("reverb").with("size", 0.35_f32).mix(0.22);
        let spec = Registry::builtin().get("reverb").unwrap().spec;
        let mut a = SlotAccess { slot: &mut slot, spec, touched: false };
        assert_eq!(a.get("size"), 0.35);
        assert_eq!(a.get("decay"), 0.5, "the kind's default, not zero");
        assert_eq!(a.variant("type"), "hall");
        a.set("size", 0.35_f32);
        assert!(!a.touched, "writing the same value is not an edit");
        a.set("size", 0.5_f32);
        assert!(a.touched);
        a.set("type", "cathedral");
        assert_eq!(a.variant("type"), "cathedral");
    }

    #[test]
    fn every_recipe_slot_has_a_panel_in_this_build() {
        let reg = Registry::builtin();
        for slot in piano::fx::concert_hall().slots {
            assert!(reg.contains(&slot.kind), "{} is not compiled in", slot.kind);
        }
    }

    #[test]
    fn a_knob_over_a_declared_parameter_formats_with_its_unit() {
        let spec = Registry::builtin().get("compressor").unwrap().spec;
        assert_eq!(spec.param("attack").unwrap().unit.format(0.02), "20 ms");
        assert_eq!(spec.param("threshold").unwrap().unit.format(-18.0), "-18.0 dB");
        assert_eq!(spec.param("ratio").unwrap().unit.format(2.0), "2.0:1");
    }
}

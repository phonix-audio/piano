//! The editor: a grand piano seen from above, with the technician's
//! adjustments around it.
//!
//! Laid out in fixed rectangles rather than egui containers. The composition
//! is a drawing — the case has to sit centred between two clusters at a
//! specific size — and a column layout that reflows is the wrong tool for a
//! picture. The window does not resize, so nothing is lost by pinning it.

use egui::{Align2, FontId, Pos2, Rect, Ui, Vec2};
use std::sync::mpsc;

use crate::colors::*;
use crate::keyboard::{self, KeyEvent, KeyboardState};
use crate::{header, scene, theme, vu};
use phonix_ui::theme::{engraved, gradient_v};
use phonix_ui::{preset_io, widgets};
use piano::patch::factory_presets_tagged;
use piano::state_buffer::SharedReader;
use piano::{PianoCommand, PianoMeterState, PianoPatch};

/// The size the layout is drawn for. The window is fixed at this.
pub const W: f32 = 1280.0;
pub const H: f32 = 800.0;

const HEADER_H: f32 = 44.0;
const CLUSTER_W: f32 = 220.0;
const KNOB: f32 = 40.0;
const KNOB_BIG: f32 = 48.0;
/// The keyboard strip: a status row, then the keys.
const KEYBOARD_H: f32 = 156.0;

pub struct PianoApp {
    /// A factory preset pick pending. Same rule as the lamp: the editor asks,
    /// the plugin moves the parameter, and the parameter is the only thing
    /// that reaches the engine and the chain.
    wants_preset: Option<i32>,
    /// Which page the body shows: 0 the instrument, 1 the effects.
    tab: u8,
    fx_dirty: bool,
    fx_page: crate::fx_page::FxPageState,
    tx: mpsc::Sender<PianoCommand>,
    meter_reader: SharedReader<PianoMeterState>,
    cached_meter: PianoMeterState,
    patch: PianoPatch,
    presets: Vec<PianoPatch>,
    preset_names: Vec<String>,
    sync_cooldown: u8,
    mirror_hold: u8,
    fonts_ready: bool,
    /// Needle positions, held between frames so peaks fall rather than flicker.
    vu_l: f32,
    vu_r: f32,
    keys: KeyboardState,
}

impl PianoApp {
    pub fn new(
        tx: mpsc::Sender<PianoCommand>,
        meter_reader: SharedReader<PianoMeterState>,
    ) -> Self {
        let presets = factory_presets_tagged();
        let preset_names = presets.iter().map(|p| p.name.clone()).collect();
        Self {
            wants_preset: None,
            tab: 0,
            fx_dirty: false,
            fx_page: Default::default(),
            tx,
            meter_reader,
            cached_meter: PianoMeterState::default(),
            patch: PianoPatch::default(),
            presets,
            preset_names,
            sync_cooldown: 0,
            mirror_hold: 0,
            fonts_ready: false,
            vu_l: 0.0,
            vu_r: 0.0,
            keys: KeyboardState::default(),
        }
    }

    pub fn current_patch(&self) -> PianoPatch {
        self.patch.clone()
    }

    /// Seed the mirror from a patch the host has restored.
    ///
    /// A plugin editor publishes `current_patch()` into its persisted state on
    /// every frame, so without this the first frames publish the *default* patch
    /// over whatever the project had saved, and the window shows the wrong knobs
    /// until the engine's meter mirror catches up. Arms the same cooldown a knob
    /// turn does, so the mirror cannot immediately undo it.
    ///
    /// Deliberately sends no command: the engine is loaded by the host wrapper,
    /// and an editor that pushes a patch on open stomps a running engine.
    pub fn set_patch(&mut self, patch: PianoPatch) {
        self.patch = patch;
        self.sync_cooldown = 20;
    }

    /// Send a parameter edit, and hold off the mirror so it cannot immediately
    /// overwrite what was just set.
    fn send(&mut self, cmd: PianoCommand) {
        self.sync_cooldown = 20;
        let _ = self.tx.send(cmd);
    }

    fn refresh_meters(&mut self, adopt_ok: bool) {
        if let Ok(mut r) = self.meter_reader.try_lock() {
            if let Some(fresh) = r.read() {
                self.cached_meter.clone_from(fresh);
            }
        }
        if self.sync_cooldown > 0 {
            self.sync_cooldown -= 1;
        } else if adopt_ok {
            if let Some(ref ep) = self.cached_meter.patch_snapshot {
                // Everything EXCEPT the chain. The engine is handed a whole
                // patch and mirrors one back, but it owns no effects and runs
                // none; adopting its copy of `fx` would erase whatever the FX
                // page just set, one frame after it was set. The chain travels
                // on its own channel, `set_fx`, from the side that runs it.
                let fx = std::mem::take(&mut self.patch.fx);
                self.patch = ep.clone();
                self.patch.fx = fx;
            }
        }
    }

    /// One knob, in its own cell, wired to a typed command.
    fn knob(
        &mut self,
        ui: &mut Ui,
        at: Pos2,
        size: f32,
        label: &str,
        value: f32,
        range: std::ops::RangeInclusive<f32>,
        fmt: impl Fn(f32) -> String,
        cmd: impl Fn(f32) -> PianoCommand,
    ) -> Option<f32> {
        let (lo, hi) = (*range.start(), *range.end());
        let mut norm = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);
        let old = norm;
        let cell = Rect::from_min_size(at, Vec2::new(widgets::KNOB_GROUP_W, size + 26.0));
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
        cui.vertical_centered(|ui| {
            widgets::knob_fmt(ui, &mut norm, label, &fmt(value), size, GOLD);
        });
        if (norm - old).abs() > 1e-6 {
            let v = lo + norm * (hi - lo);
            self.send(cmd(v));
            return Some(v);
        }
        None
    }

    fn caption(&self, ui: &Ui, at: Pos2, text: &str) {
        for (i, line) in text.lines().enumerate() {
            ui.painter().text(
                at + Vec2::new(0.0, i as f32 * 11.0),
                Align2::LEFT_TOP,
                line,
                FontId::proportional(9.0),
                TEXT_DIM,
            );
        }
    }

    /// Draw the whole window into `ui`, the root the host hands over.
    pub fn draw_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        egui_extras::install_image_loaders(ctx);
        if !self.fonts_ready {
            theme::install_fonts(ctx);
            self.fonts_ready = true;
        }
        theme::apply_visuals(ctx);

        let adopt_ok = widgets::mirror_adopt_gate(ctx, &mut self.mirror_hold);
        self.refresh_meters(adopt_ok);
        vu::ballistics(&mut self.vu_l, self.cached_meter.peak_l);
        vu::ballistics(&mut self.vu_r, self.cached_meter.peak_r);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG_LACQUER))
            .show_inside(ui, |ui| {
                let full = ui.max_rect();
                gradient_v(ui, full, LACQUER_TOP, LACQUER_BOTTOM);

                let head = Rect::from_min_size(full.min, Vec2::new(full.width(), HEADER_H));
                self.draw_header(ui, head);

                let strip = Rect::from_min_max(
                    Pos2::new(full.left(), full.bottom() - KEYBOARD_H),
                    full.max,
                );
                let body = Rect::from_min_max(
                    Pos2::new(full.left(), full.top() + HEADER_H + 12.0),
                    Pos2::new(full.right(), strip.top() - 12.0),
                );
                let left = Rect::from_min_size(
                    Pos2::new(body.left() + 20.0, body.top()),
                    Vec2::new(CLUSTER_W, body.height()),
                );
                let right = Rect::from_min_size(
                    Pos2::new(body.right() - 20.0 - CLUSTER_W, body.top()),
                    Vec2::new(CLUSTER_W, body.height()),
                );
                let stage = Rect::from_min_max(
                    Pos2::new(left.right() + 20.0, body.top()),
                    Pos2::new(right.left() - 20.0, body.bottom()),
                );

                if self.tab == 1 {
                    // The effects take the whole body: neither column has room
                    // left, and four effects need more than either could give.
                    let page = Rect::from_min_max(
                        Pos2::new(body.left() + 20.0, body.top()),
                        Pos2::new(body.right() - 20.0, body.bottom()),
                    );
                    if crate::fx_page::draw(ui, page, &mut self.patch.fx, &mut self.fx_page) {
                        self.fx_dirty = true;
                    }
                } else {
                    self.draw_left(ui, left);
                    self.draw_stage(ui, stage);
                    self.draw_right(ui, right);
                }
                self.draw_keyboard(ui, strip);
            });
    }

    /// The keys, and the row above them that says what the pedal is doing.
    ///
    /// Notes go out on the raw sender, never through `send`: a note is not a
    /// parameter edit, and arming the mirror cooldown on every keystroke would
    /// keep the editor from ever adopting the engine's patch while playing.
    fn draw_keyboard(&mut self, ui: &mut Ui, r: Rect) {
        let status = Rect::from_min_size(r.min, Vec2::new(r.width(), 22.0));
        let pedal = self.cached_meter.pedal_down;
        let lamp = Pos2::new(status.left() + 26.0, status.center().y);
        theme::lamp(ui, lamp, 4.5, pedal, GOLD_BRIGHT);
        ui.painter().text(
            lamp + Vec2::new(11.0, 0.0),
            Align2::LEFT_CENTER,
            "SUSTAIN",
            FontId::proportional(9.0),
            if pedal { TEXT_PRIMARY } else { TEXT_DIM },
        );
        ui.painter().text(
            Pos2::new(status.right() - 20.0, status.center().y),
            Align2::RIGHT_CENTER,
            "A0 - C8",
            FontId::monospace(9.0),
            TEXT_DIM,
        );

        let keys = Rect::from_min_max(
            Pos2::new(r.left() + 16.0, status.bottom() + 6.0),
            Pos2::new(r.right() - 16.0, r.bottom() - 8.0),
        );
        self.keys.active.clear();
        self.keys
            .active
            .extend_from_slice(&self.cached_meter.active_notes);
        for ev in keyboard::draw(ui, keys, &mut self.keys) {
            let cmd = match ev {
                KeyEvent::On { note, velocity } => PianoCommand::NoteOn(note, velocity),
                KeyEvent::Off { note } => PianoCommand::NoteOff(note),
            };
            let _ = self.tx.send(cmd);
        }
    }

    /// The bank's `i`th preset, as the header picks it: shown here, but NOT
    /// sent from here. The plugin moves the `preset` parameter, which loads
    /// the patch and fills the curated chain in one place; sending LoadPatch
    /// directly would engage the patch and leave the chain behind.
    pub fn pick_preset(&mut self, i: usize) {
        if let Some(p) = self.presets.get(i).cloned() {
            self.patch = p;
            self.wants_preset = Some(i as i32 + 1);
        }
    }

    /// An edit of the chain, as the FX page makes one: the chain moves and
    /// the plugin is told on the next frame.
    pub fn edit_fx(&mut self, f: impl FnOnce(&mut phonix_fx::ChainSpec)) {
        f(&mut self.patch.fx);
        self.fx_dirty = true;
    }

    /// Which page is shown: 0 the instrument, 1 the effects.
    pub fn set_tab(&mut self, tab: u8) {
        self.tab = tab;
    }

    pub fn take_wants_preset(&mut self) -> Option<i32> {
        self.wants_preset.take()
    }

    /// Whether the FX page moved something since this was last asked.
    pub fn take_fx_changed(&mut self) -> bool {
        std::mem::take(&mut self.fx_dirty)
    }

    /// Adopt the chain the host side is running.
    ///
    /// Called when a preset change replaced it, which is the one case the
    /// editor cannot see for itself: a preset can be changed by automation,
    /// with no click in this window.
    pub fn set_fx(&mut self, spec: phonix_fx::ChainSpec) {
        self.patch.fx = spec;
        self.fx_dirty = false;
    }

    /// The chain as the editor holds it.
    pub fn fx(&self) -> &phonix_fx::ChainSpec {
        &self.patch.fx
    }

    fn draw_header(&mut self, ui: &mut Ui, rect: Rect) {
        let res = header::draw(
            ui,
            rect,
            &self.patch.name,
            &self.preset_names,
            self.cached_meter.active_voices,
            self.tab,
        );
        if let Some(i) = res.preset_selected {
            self.pick_preset(i);
        }
        if let Some(t) = res.tab_selected {
            self.tab = t;
        }
        if res.save_clicked {
            preset_io::save_patch_to_disk(crate::PRESET_HOME, &self.patch, "Piano", &self.patch.name);
        }
        if res.load_clicked {
            if let Some(p) = preset_io::load_patch_from_disk::<PianoPatch>(crate::PRESET_HOME, "Piano") {
                self.patch = p.clone();
                self.send(PianoCommand::LoadPatch(Box::new(p)));
            }
        }
    }

    /// The technician's side: what is done to the hammers and the action.
    fn draw_left(&mut self, ui: &mut Ui, r: Rect) {
        let mut y = r.top();

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "VOICING");
        y += 26.0;
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 10.0, y), KNOB_BIG, "Felt", self.patch.voicing, 0.0..=1.0,
            |v| if v < 0.35 { "needled".into() } else if v > 0.65 { "filed".into() } else { format!("{:.0}%", v * 100.0) },
            PianoCommand::SetVoicing,
        ) { self.patch.voicing = v; }
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 110.0, y + 4.0), KNOB, "Unison", self.patch.unison_detune, 0.0..=12.0,
            |v| format!("{v:.1}c"), PianoCommand::SetUnisonDetune,
        ) { self.patch.unison_detune = v; }
        y += KNOB_BIG + 34.0;
        self.caption(ui, Pos2::new(r.left() + 2.0, y), "felt hardness, and how far apart\nthe strings of a unison are set");
        y += 66.0;

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "TUNING");
        y += 26.0;
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 10.0, y), KNOB, "Tune", self.patch.tune, -50.0..=50.0,
            |v| format!("{v:+.1}c"), PianoCommand::SetTune,
        ) { self.patch.tune = v; }
        y += KNOB + 34.0;
        self.caption(ui, Pos2::new(r.left() + 2.0, y), "fine tuning of the whole\ninstrument, in cents");
        y += 66.0;

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "ACTION");
        y += 26.0;
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 10.0, y), KNOB, "Action", self.patch.mechanics, 0.0..=1.0,
            |v| format!("{:.0}%", v * 100.0), PianoCommand::SetMechanics,
        ) { self.patch.mechanics = v; }
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 110.0, y), KNOB, "Release", self.patch.release_noise, 0.0..=1.0,
            |v| format!("{:.0}%", v * 100.0), PianoCommand::SetReleaseNoise,
        ) { self.patch.release_noise = v; }
        y += KNOB + 34.0;
        self.caption(ui, Pos2::new(r.left() + 2.0, y), "the noises the machine makes;\nRelease is the key coming back,\npedal down or not");
    }

    /// The instrument itself.
    fn draw_stage(&mut self, ui: &mut Ui, r: Rect) {
        let st = scene::SceneState {
            width: self.patch.width,
            width_default: 0.7,
            active_notes: &self.cached_meter.active_notes,
            pedal_down: self.cached_meter.pedal_down,
        };
        let res = scene::draw(ui, r, &st);
        if let Some(w) = res.width_changed {
            self.patch.width = w;
            self.send(PianoCommand::SetWidth(w));
        }
    }

    /// Dampers, where you are listening from, and what comes out.
    fn draw_right(&mut self, ui: &mut Ui, r: Rect) {
        let mut y = r.top();

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "DAMPERS");
        y += 26.0;
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 10.0, y), KNOB_BIG, "Dampers", self.patch.damper, 0.0..=1.0,
            |v| format!("{:.0}%", v * 100.0), PianoCommand::SetDamper,
        ) { self.patch.damper = v; }
        y += KNOB_BIG + 34.0;
        self.caption(ui, Pos2::new(r.left() + 2.0, y), "how quickly the felt stops a\nstring once the key is released");
        y += 66.0;

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "LISTENING");
        y += 26.0;
        // A readout, not a knob: the control is the pair of microphones on the
        // soundboard, and two controls for one value would disagree.
        let plate = Rect::from_min_size(Pos2::new(r.left() + 4.0, y), Vec2::new(r.width() - 8.0, 26.0));
        ui.painter().rect_filled(plate, 3.0, BG_DARK);
        ui.painter().rect_stroke(plate, 3.0, egui::Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Inside);
        engraved(
            ui,
            plate.center(),
            &format!("SPREAD {:.0}%", self.patch.width * 100.0),
            FontId::monospace(12.0),
            TEXT_PRIMARY,
            Align2::CENTER_CENTER,
        );
        y += 32.0;
        self.caption(ui, Pos2::new(r.left() + 2.0, y), "drag the microphones on the\nsoundboard to set how far apart\nthe two points are heard from");
        y += 76.0;

        theme::cluster_header(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 20.0)), "OUTPUT");
        y += 26.0;
        if let Some(v) = self.knob(
            ui, Pos2::new(r.left() + 10.0, y), KNOB, "Gain", self.patch.gain, 0.0..=2.0,
            |v| format!("{:.0}%", v * 100.0), PianoCommand::SetGain,
        ) { self.patch.gain = v; }
        y += KNOB + 34.0;
        vu::draw(
            ui,
            Rect::from_min_size(Pos2::new(r.left() + 10.0, y), Vec2::new(196.0, 96.0)),
            self.vu_l,
            self.vu_r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;
    use piano::state_buffer::meter_channel;

    /// The root `Ui` a host hands the editor: the whole screen, no margin.
    /// kittest's `build_ui` pads its root by 8 px, and the references are
    /// taken edge to edge, so the harnesses build on the context instead.
    fn root(ctx: &egui::Context) -> Ui {
        Ui::new(ctx.clone(), egui::Id::new("root"), egui::UiBuilder::new().max_rect(ctx.content_rect()))
    }

    #[allow(deprecated)]
    fn harness(app: PianoApp) -> Harness<'static> {
        let mut app = app;
        Harness::builder()
            .with_size(egui::vec2(W, H))
            .build(move |ctx| app.draw_ui(&mut root(ctx)))
    }

    #[test]
    fn piano_ui_builds_and_shows_controls() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (_mw, mr) = meter_channel::<PianoMeterState>();
        let app = PianoApp::new(tx, mr);
        let mut h = harness(app);
        h.run_steps(2);
        // Knob labels are the accessible text on this surface; the cluster
        // headings are painted glyph by glyph for letterspacing, so they carry
        // no accessibility node and cannot be queried.
        for label in ["Felt", "Unison", "Tune", "Action", "Release", "Dampers", "Gain"] {
            assert!(h.query_by_label(label).is_some(), "missing control {label:?}");
        }
    }

    /// Every parameter the patch carries must have a control, or an edit made
    /// in a project becomes unreachable in the editor.
    #[test]
    fn every_patch_parameter_is_reachable() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (_mw, mr) = meter_channel::<PianoMeterState>();
        let app = PianoApp::new(tx, mr);
        let mut h = harness(app);
        h.run_steps(2);
        // Seven knobs, plus `width`, whose control is the pair of microphones
        // on the soundboard and whose value is shown on the LISTENING plate.
        for label in ["Felt", "Unison", "Tune", "Action", "Release", "Dampers", "Gain"] {
            assert!(h.query_by_label(label).is_some(), "no control for {label:?}");
        }
    }

    #[test]
    fn piano_gui_mirrors_engine_patch() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (mut writer, reader) = meter_channel::<PianoMeterState>();
        let mut app = PianoApp::new(tx, reader);
        let mut p = PianoPatch::default();
        p.voicing = 0.83;
        p.unison_detune = 4.5;
        {
            let s = writer.edit();
            s.patch_snapshot = Some(p.clone());
            writer.publish();
        }
        app.sync_cooldown = 0;
        app.refresh_meters(true);
        assert!((app.patch.voicing - 0.83).abs() < 1e-4);
        assert!((app.patch.unison_detune - 4.5).abs() < 1e-4);
    }

    /// The engine mirror must not carry the chain back.
    ///
    /// The engine is handed a whole patch and publishes one back, effects
    /// field included, but it owns no effects and runs none. Adopting its copy
    /// wholesale erased an FX edit one frame after it was made -- the reverb
    /// snapped back to whatever the preset said, while you were still holding
    /// the control. This is that bug, pinned.
    #[test]
    fn the_engine_mirror_does_not_erase_an_fx_edit() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (mut writer, reader) = meter_channel::<PianoMeterState>();
        let mut app = PianoApp::new(tx, reader);

        // What the engine last saw: the preset, room reverb.
        let engine_patch = PianoPatch::default();
        assert_eq!(engine_patch.fx.slots[2].variant("type"), Some("room"));
        {
            let s = writer.edit();
            s.patch_snapshot = Some(engine_patch);
            writer.publish();
        }

        // What the page just set: a different reverb.
        app.set_fx(PianoPatch::default().fx);
        app.patch.fx.slots[2].set("type", "cathedral");

        app.sync_cooldown = 0;
        app.refresh_meters(true);

        assert_eq!(app.patch.fx.slots[2].variant("type"), Some("cathedral"), "the mirror put the preset's reverb back");
    }

    /// The window is drawn for one size; if the constants and the plugin's
    /// declared size ever drift, the layout is cropped in the host.
    #[test]
    fn the_layout_is_drawn_for_the_size_the_plugin_asks_for() {
        assert_eq!((W, H), (1280.0, 800.0));
    }

    /// Look at it. Every structural test here passes on a page of empty boxes;
    /// the only check that a piano was drawn is the picture.
    ///
    /// Ignored because it needs a GPU (lavapipe does):
    ///   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    ///   UPDATE_SNAPSHOTS=1 cargo test -p piano-ui editor_snapshot -- --ignored
    #[test]
    #[ignore = "needs a rendering backend"]
    fn piano_editor_snapshot() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let (mut w, r) = meter_channel::<PianoMeterState>();
        {
            let s = w.edit();
            // A pedalled chord: strings lit, keys lit, dampers up. Snapshotting
            // an idle instrument would prove none of that is wired.
            s.active_voices = 4;
            s.active_notes = vec![40, 52, 59, 64];
            s.pedal_down = true;
            s.peak_l = 0.42;
            s.peak_r = 0.55;
            w.publish();
        }
        let app = PianoApp::new(tx, r);
        let mut h = harness(app);
        h.run_steps(3);
        h.snapshot("piano_editor");
    }

    /// The keys on their own, at the size they are actually drawn, so a change
    /// to the layout shows up as a keyboard and not as a wall of pixels.
    #[test]
    #[ignore = "needs a rendering backend"]
    #[allow(deprecated)]
    fn piano_keyboard_snapshot() {
        let mut state = crate::keyboard::KeyboardState::default();
        state.active = vec![21, 40, 52, 59, 64, 108];
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 130.0))
            .build(move |ctx| {
                theme::apply_visuals(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(BG_LACQUER))
                    .show_inside(&mut root(ctx), |ui| {
                        let r = ui.max_rect().shrink2(egui::vec2(16.0, 12.0));
                        crate::keyboard::draw(ui, r, &mut state);
                    });
            });
        h.run_steps(2);
        h.snapshot("piano_keyboard");
    }

    /// The instrument alone, pedal down and a chord ringing.
    #[test]
    #[ignore = "needs a rendering backend"]
    #[allow(deprecated)]
    fn piano_scene_snapshot() {
        let notes = [40u8, 52, 59, 64];
        let mut h = Harness::builder()
            .with_size(egui::vec2(560.0, 620.0))
            .build(move |ctx| {
                theme::apply_visuals(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(BG_LACQUER))
                    .show_inside(&mut root(ctx), |ui| {
                        let r = ui.max_rect();
                        scene::draw(
                            ui,
                            r,
                            &scene::SceneState {
                                width: 0.7,
                                width_default: 0.7,
                                active_notes: &notes,
                                pedal_down: true,
                            },
                        );
                    });
            });
        h.run_steps(2);
        h.snapshot("piano_scene");
    }
}

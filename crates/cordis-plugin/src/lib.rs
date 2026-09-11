//! Cordis — a physically modelled grand piano. VST3/CLAP plugin.
//!
//! Wraps CordisEngine in nice-plug. Stereo synth out, MIDI in. Host-
//! automatable knobs push targeted Set commands only on change; the mallet
//! selector and the full patch persist with the project.

use nice_plug::prelude::*;
use nice_plug_egui::{create_egui_editor, EguiState};
use std::sync::{mpsc, Arc, RwLock};

use cordis_ui::CordisApp;
use cordis::patch::factory_presets_tagged;
use cordis::{CordisCommand, CordisEngine, CordisMeterState, CordisPatch};
use cordis::state_buffer::{meter_channel, SharedReader, Writer};
mod fx;
use phonix_fx::{Chain, ChainSpec, Musical, Transport};
use std::sync::atomic::{AtomicU64, Ordering};
use fx::FxLink;
use phonix_plugin::vstpreset::{self, ParamValue};

const KNOBS: usize = 8;

pub struct CordisPlugin {
    params: Arc<CordisParams>,
    engine: Option<CordisEngine>,
    tx: mpsc::Sender<CordisCommand>,
    meter: Option<SharedReader<CordisMeterState>>,
    pending: Option<(mpsc::Receiver<CordisCommand>, Writer<CordisMeterState>)>,
    buf: Vec<f32>,
    /// The engine's stereo pair, planar, for the chain.
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    presets: Vec<CordisPatch>,
    last_preset: i32,
    last_knobs: Option<[f32; KNOBS]>,
    /// The curated master chain. `None` until `initialize` knows the sample
    /// rate, and EMPTY until a factory preset is explicitly loaded: a project
    /// saved before this existed drives it through no code path that fills it,
    /// so it stays a documented no-op and the audio is unchanged.
    fx_chain: Option<Chain>,
    /// The chain between the editor and the audio thread. NOT persisted: the
    /// patch carries the chain across a save. It exists because neither side
    /// can see the other's copy.
    fx_link: Arc<FxLink>,
    fx_seen: u64,
}

impl Default for CordisPlugin {
    fn default() -> Self {
        let presets = factory_presets_tagged();
        let mut names = Vec::with_capacity(presets.len() + 1);
        names.push("Init".to_string());
        for p in &presets { names.push(p.name.clone()); }
        let (tx, rx) = mpsc::channel();
        let (mw, mr) = meter_channel::<CordisMeterState>();
        Self {
            params: Arc::new(CordisParams::new(presets.len(), Arc::new(names))),
            engine: None, tx, meter: Some(mr), pending: Some((rx, mw)),
            buf: Vec::new(), buf_l: Vec::new(), buf_r: Vec::new(), presets, last_preset: 0, last_knobs: None,
            fx_chain: None,
            fx_link: Arc::new(FxLink::new(ChainSpec::default())), fx_seen: 0,
        }
    }
}

#[derive(Params)]
struct CordisParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    #[persist = "patch"]
    patch_state: Arc<RwLock<CordisPatch>>,
    #[id = "preset"] preset: IntParam,
    #[id = "voicing"] voicing: FloatParam,
    #[id = "unison"] unison: FloatParam,
    #[id = "width"] width: FloatParam,
    #[id = "damper"] damper: FloatParam,
    #[id = "action"] action: FloatParam,
    #[id = "release"] release: FloatParam,
    #[id = "tune"] tune: FloatParam,
    #[id = "gain"] gain: FloatParam,
}

impl CordisParams {
    fn new(count: usize, names: Arc<Vec<String>>) -> Self {
        let n1 = names.clone();
        let d = CordisPatch::default();
        Self {
            // The editor draws a fixed composition; the window takes its size
            // from the constants that composition is laid out against, so the
            // two cannot drift and leave the piano cropped in a host.
            editor_state: EguiState::from_size(
                cordis_ui::app::W as u32,
                cordis_ui::app::H as u32,
            ),
            patch_state: Arc::new(RwLock::new(CordisPatch::default())),
            preset: IntParam::new("Preset", 0, IntRange::Linear { min: 0, max: count as i32 })
                .with_value_to_string(Arc::new(move |v| n1.get(v as usize).cloned().unwrap_or_else(|| format!("P{v}"))))
                .with_string_to_value(Arc::new(move |s| names.iter().position(|n| n.eq_ignore_ascii_case(s)).map(|i| i as i32))),
            // Defaults come from the default patch, never from a second set of
            // literals: `process` resends every knob on the first block, so a
            // parameter default that disagrees with the patch silently wins
            // over it.
            voicing: FloatParam::new("Voicing", d.voicing, FloatRange::Linear { min: 0.0, max: 1.0 }),
            unison: FloatParam::new("Unison", d.unison_detune, FloatRange::Linear { min: 0.0, max: 12.0 }).with_unit(" cents"),
            width: FloatParam::new("Spread", d.width, FloatRange::Linear { min: 0.0, max: 1.0 }),
            damper: FloatParam::new("Dampers", d.damper, FloatRange::Linear { min: 0.0, max: 1.0 }),
            action: FloatParam::new("Action", d.mechanics, FloatRange::Linear { min: 0.0, max: 1.0 }),
            release: FloatParam::new("Release", d.release_noise, FloatRange::Linear { min: 0.0, max: 1.0 }),
            tune: FloatParam::new("Tune", d.tune, FloatRange::Linear { min: -50.0, max: 50.0 }).with_unit(" cents"),
            gain: FloatParam::new("Gain", d.gain, FloatRange::Linear { min: 0.0, max: 2.0 }),
        }
    }
    fn knob_sig(&self) -> [f32; KNOBS] {
        [self.voicing.value(), self.unison.value(), self.width.value(),
         self.damper.value(), self.action.value(), self.release.value(),
         self.tune.value(), self.gain.value()]
    }
}

impl Default for CordisParams {
    fn default() -> Self { Self::new(0, Arc::new(vec!["Init".to_string()])) }
}


fn gen_vstpresets(presets: &[CordisPatch]) {
    // Not a second literal: the `.vstpreset` header MUST carry the same class id
    // the wrapper registers, or a host resolves the bank to nothing.
    let class_id = &<CordisPlugin as Vst3Plugin>::VST3_CLASS_ID;
    let mapped: Vec<(String, Vec<(&str, ParamValue)>)> = presets.iter().enumerate().map(|(i, p)| {
        let cat = "Cordis";
        (format!("{cat}/{}", p.name), vec![
            ("preset", ParamValue::I32(i as i32 + 1)),
            ("voicing", ParamValue::F32(p.voicing)),
            ("unison", ParamValue::F32(p.unison_detune)),
        ])
    }).collect();
    let refs: Vec<(&str, Vec<(&str, ParamValue)>)> = mapped.iter().map(|(n, p)| (n.as_str(), p.clone())).collect();
    let _ = vstpreset::generate_factory_presets("Phonix Audio", "Cordis", class_id, env!("CARGO_PKG_VERSION"), &refs);
}

impl Plugin for CordisPlugin {
    const NAME: &'static str = "Cordis";
    const VENDOR: &'static str = "Phonix Audio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: Some(unsafe { std::num::NonZeroU32::new_unchecked(2) }),
        aux_input_ports: &[], aux_output_ports: &[], names: PortNames::const_default(),
    }];
    // MidiCCs, not Basic: nice-plug only publishes the IMidiMapping a VST3 host
    // needs when this is MidiCCs. With Basic the NoteEvent::MidiCC arm below is
    // dead code in every VST3 host, so the sustain pedal never arrives at all.
    // The 2080 controller parameters this adds are flagged hidden; a host that
    // shows them is not reading the flags.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn editor(&mut self, _ax: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let patch_state = self.params.patch_state.clone();
        let tx = self.tx.clone();
        let meter = self.meter.take()?;
        let mut app = CordisApp::new(tx, meter);
        // Seed the editor from the restored state before its first frame, or the
        // closure below publishes the default patch over it.
        if let Ok(p) = patch_state.read() { app.set_patch(p.clone()); }
        let host_params = self.params.clone();
        let bank = self.presets.clone();
        let fx_link = self.fx_link.clone();
        // The closure must be Sync, so the counter it remembers is an atomic.
        let seen = AtomicU64::new(fx_link.rev());
        // Seed the page from the restored patch, as the patch itself is above.
        if let Ok(p) = self.params.patch_state.read() {
            self.fx_link.seed(p.fx.clone());
        }
        create_egui_editor(self.params.editor_state.clone(), app, Default::default(),
            |_c, _q, _a| {},
            move |ui, setter, _q, app| {
                // A preset change replaced the chain: adopt it before
                // drawing, or the page shows the previous preset's effects.
                let mut seen_now = seen.load(Ordering::Relaxed);
                if let Some(spec) = fx_link.adopt(&mut seen_now) {
                    app.set_fx(spec);
                    seen.store(seen_now, Ordering::Relaxed);
                }
                app.draw_ui(ui);
                // The page moved something: publish it, and the audio thread
                // takes it on its next block.
                if app.take_fx_changed() {
                    seen.store(fx_link.publish(app.fx().clone()), Ordering::Relaxed);
                }
                if let Some(i) = app.take_wants_preset() {
                    // The preset's chain goes through the link as well as
                    // through the parameter: a preset picked again is the
                    // same parameter value, and the audio thread would never
                    // hear of it otherwise, while the page already shows it.
                    let chain = bank.get((i - 1).max(0) as usize).filter(|_| i > 0).map(|p| p.fx.clone()).unwrap_or_default();
                    seen.store(fx_link.publish(chain), Ordering::Relaxed);
                    setter.begin_set_parameter(&host_params.preset);
                    setter.set_parameter(&host_params.preset, i);
                    setter.end_set_parameter(&host_params.preset);
                    // The knobs follow, or the host's lanes keep the old
                    // values while the engine plays the new ones, and the
                    // first touched knob snaps the instrument back.
                    if let Some(p) = bank.get((i - 1).max(0) as usize).filter(|_| i > 0) {
                        let knobs: [(&FloatParam, f32); 8] = [
                            (&host_params.voicing, p.voicing), (&host_params.unison, p.unison_detune),
                            (&host_params.width, p.width), (&host_params.damper, p.damper),
                            (&host_params.action, p.mechanics), (&host_params.release, p.release_noise),
                            (&host_params.tune, p.tune), (&host_params.gain, p.gain),
                        ];
                        for (param, v) in knobs {
                            setter.begin_set_parameter(param);
                            setter.set_parameter(param, v);
                            setter.end_set_parameter(param);
                        }
                    }
                }
                if let Ok(mut p) = patch_state.write() { *p = app.current_patch(); }
            })
    }

    /// Must be re-entrant. nice-plug calls this again from `set_state` whenever a
    /// buffer config already exists, and a host that calls `setup_processing`
    /// before restoring state — some VST3 hosts do — therefore always
    /// takes the second path. The one-shot version returned `false` there, so
    /// `setState` reported `kResultFalse` and the restored patch never reached
    /// the engine. The channel ends only exist once, so build the engine on the
    /// first call and re-rate it in place afterwards.
    fn initialize(&mut self, _l: &AudioIOLayout, cfg: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        match self.pending.take() {
            Some((rx, mw)) => {
                let mut eng = CordisEngine::new(cfg.sample_rate, rx, mw);
                // Only when the host says it is playing. A bounce runs flat
                // out, slower than real time by design, so an instrument that
                // sheds voices under load would shed them for the whole render
                // — heard as notes cut off in a file that had to be exact.
                eng.set_governor(cfg.process_mode == ProcessMode::Realtime);
                // Same criterion for the board: live play runs the decoupled
                // model (strings push the plate, never read it back; its drain
                // is baked into the banks), a bounce runs the exact one.
                // initialize() is off the audio thread, where rebuilding the
                // 88 prototypes is free.
                eng.set_decoupled(cfg.process_mode == ProcessMode::Realtime);
                self.engine = Some(eng);
                // Writing the factory bank is a first-run side effect, not
                // something a state restore should redo.
                gen_vstpresets(&self.presets);
            }
            None => match self.engine.as_mut() {
                Some(e) => {
                    e.set_governor(cfg.process_mode == ProcessMode::Realtime);
                    e.set_decoupled(cfg.process_mode == ProcessMode::Realtime);
                    e.set_sample_rate(cfg.sample_rate)
                }
                None => return false,
            },
        }
        // Built here rather than in `Default` so it never carries a placeholder
        // rate, and re-rated in place on the second call for the same reason
        // the engine is.
        let max_block = cfg.max_buffer_size as usize;
        match self.fx_chain.as_mut() {
            Some(c) => c.prepare(cfg.sample_rate, max_block),
            None => self.fx_chain = Some(Chain::new(cfg.sample_rate, max_block)),
        }
        // Zero while the chain is empty, and again after a rate change until a
        // preset fills it. The limiter's lookahead is the only slot that ever
        // makes it non-zero.
        let lat = self.fx_chain.as_ref().map_or(0, |c| c.latency_samples());
        ctx.set_latency_samples(lat as u32);

        self.buf = vec![0.0; cfg.max_buffer_size as usize * 2];
        self.buf_l = vec![0.0; cfg.max_buffer_size as usize];
        self.buf_r = vec![0.0; cfg.max_buffer_size as usize];
        let patch = self.params.patch_state.read().map(|p| p.clone()).unwrap_or_default();
        // A restored project brings its chain with it, inside the patch; one
        // saved before the field existed brings an empty spec, and an empty
        // spec leaves an empty chain.
        if let Some(c) = self.fx_chain.as_mut() {
            fx::apply(c, &patch.fx);
        }
        self.fx_link.seed(patch.fx.clone());
        self.fx_seen = self.fx_link.rev();
        let _ = self.tx.send(CordisCommand::LoadPatch(Box::new(patch)));
        self.last_preset = self.params.preset.value();
        // Force the knob diff in `process` to resend all eight Set commands, so
        // the restored parameter values reach the engine even if the patch blob
        // was absent.
        self.last_knobs = None;
        true
    }

    fn reset(&mut self) {}

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let engine = match self.engine.as_mut() { Some(e) => e, None => return ProcessStatus::Normal };
        let tx = &self.tx;

        let cp = self.params.preset.value();
        if cp != self.last_preset {
            self.last_preset = cp;
            if cp > 0 {
                if let Some(p) = self.presets.get((cp - 1) as usize) {
                    let _ = tx.send(CordisCommand::LoadPatch(Box::new(p.clone())));
                    // The one event that fills the chain. `initialize` syncs
                    // `last_preset` to the restored value before the first
                    // block, so reopening a project never reaches this.
                    // The preset's own chain, taken from the bank rather
                    // than from the editor: this branch also fires on host
                    // automation, where no click happened in the window.
                    if let Some(c) = self.fx_chain.as_mut() {
                        fx::apply(c, &p.fx);
                        ctx.set_latency_samples(c.latency_samples() as u32);
                    }
                    self.fx_seen = self.fx_link.publish(p.fx.clone());
                }
            } else {
                // Back to Init: the curated chain goes with the preset it came
                // with.
                if let Some(c) = self.fx_chain.as_mut() {
                    fx::disengage(c);
                    ctx.set_latency_samples(c.latency_samples() as u32);
                }
                self.fx_seen = self.fx_link.publish(ChainSpec::default());
            }
        }
        // An edit made on the FX page, or a preset picked there again.
        if let Some(c) = self.fx_chain.as_mut() {
            let mut latency = None;
            self.fx_link.apply_if_new(&mut self.fx_seen, |spec| {
                fx::apply(c, spec);
                latency = Some(c.latency_samples() as u32);
            });
            if let Some(l) = latency {
                ctx.set_latency_samples(l);
            }
        }
        let sig = self.params.knob_sig();
        let changed = self.last_knobs.map_or(true, |p| p.iter().zip(&sig).any(|(a, b)| (a - b).abs() > 1e-6));
        if changed {
            let prev = self.last_knobs.unwrap_or([f32::NAN; KNOBS]);
            let s = |i: usize, c: CordisCommand| if prev[i].is_nan() || (prev[i] - sig[i]).abs() > 1e-6 { let _ = tx.send(c); };
            s(0, CordisCommand::SetVoicing(sig[0]));
            s(1, CordisCommand::SetUnisonDetune(sig[1]));
            s(2, CordisCommand::SetWidth(sig[2]));
            s(3, CordisCommand::SetDamper(sig[3]));
            s(4, CordisCommand::SetMechanics(sig[4]));
            s(5, CordisCommand::SetReleaseNoise(sig[5]));
            s(6, CordisCommand::SetTune(sig[6]));
            s(7, CordisCommand::SetGain(sig[7]));
            self.last_knobs = Some(sig);
        }

        while let Some(ev) = ctx.next_event() {
            match ev {
                NoteEvent::NoteOn { note, velocity, .. } => { let _ = tx.send(CordisCommand::NoteOn(note, (velocity * 127.0) as u8)); }
                NoteEvent::NoteOff { note, .. } => { let _ = tx.send(CordisCommand::NoteOff(note)); }
                // A piano without its sustain pedal is not a piano.
                NoteEvent::MidiCC { cc: 64, value, .. } => {
                    let _ = tx.send(CordisCommand::SustainPedal(value >= 0.5));
                }
                _ => {}
            }
        }

        let n = buffer.samples();
        let il = n * 2;
        if self.buf.len() < il { self.buf.resize(il, 0.0); }
        for s in &mut self.buf[..il] { *s = 0.0; }
        engine.process_audio(&mut self.buf[..il], 2);
        // The chain runs in stereo before any summing: the room's width and
        // the compressor's stereo linkage need both channels.
        if self.buf_l.len() < n { self.buf_l.resize(n, 0.0); self.buf_r.resize(n, 0.0); }
        for i in 0..n { self.buf_l[i] = self.buf[i * 2]; self.buf_r[i] = self.buf[i * 2 + 1]; }
        if let Some(c) = self.fx_chain.as_mut() {
            c.process(&mut self.buf_l[..n], &mut self.buf_r[..n], &[], Transport::default(), Musical::default());
        }
        let ch = buffer.as_slice();
        if ch.len() >= 2 {
            let (l, r) = ch.split_at_mut(1);
            l[0][..n].copy_from_slice(&self.buf_l[..n]);
            r[0][..n].copy_from_slice(&self.buf_r[..n]);
        } else if !ch.is_empty() {
            for i in 0..n { ch[0][i] = (self.buf_l[i] + self.buf_r[i]) * 0.5; }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for CordisPlugin {
    const CLAP_ID: &'static str = "com.phonix-audio.cordis";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Physically modelled grand piano");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    // No `piano` here, and not an oversight: CLAP's plugin-features.h defines no
    // piano feature, and nice-plug's `ClapFeature` has no such variant. The only
    // way to spell one is `ClapFeature::Custom`, which nice-plug debug-asserts
    // must be namespaced (`phonix:piano`) — a string no host matches on. The
    // word a user searches for is carried by CLAP_DESCRIPTION instead. VST3 does
    // have the category, and takes it below.
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Instrument, ClapFeature::Synthesizer, ClapFeature::Stereo];
}

impl Vst3Plugin for CordisPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"PxCordisPiano001";
    // Piano first: a host that browses by category files us under pianos, where
    // someone looking for one actually looks, rather than only among synths.
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Piano, Vst3SubCategory::Synth];
}

nice_export_clap!(CordisPlugin);
nice_export_vst3!(CordisPlugin);

// ── Frozen identifiers ────────────────────────────────────────────
//
// These four strings are a compatibility surface, not a naming choice.
//
// The class id resolves an exported DAWproject `Vst3Plugin` device and sits in
// the header of every `.vstpreset`; the CLAP id identifies the plugin to a CLAP
// host; the vendor and product names compose the directory Cubase's MediaBay
// indexes. Change any of them and existing projects and preset banks point at a
// plugin no host can find.
//
// They were SET, once, at first publication under the phonix-audio organisation.
// Nothing shipped before that, so there was exactly one moment in which choosing
// them was free. That moment is over: from here they are read-only.
//
// and asserts against the literal rather than against this crate now that the
// dependency runs the other way. Its copy has to be moved to the value below in
// the same change, or its DAWproject export names a plugin that no longer
// exists.
#[cfg(test)]
mod frozen_identifiers {
    use super::*;

    #[test]
    fn the_ids_a_host_resolves_us_by_have_not_moved() {
        assert_eq!(
            <CordisPlugin as Vst3Plugin>::VST3_CLASS_ID,
            *b"PxCordisPiano001",
        );
        // A VST3 class id is exactly 16 bytes: the wrapper hex-encodes it into a
        // 32-character FUID, so a literal of any other length is a silently
        // different plugin rather than a compile error.
        assert_eq!(<CordisPlugin as Vst3Plugin>::VST3_CLASS_ID.len(), 16);
        assert!(<CordisPlugin as Vst3Plugin>::VST3_CLASS_ID.is_ascii());
        assert_eq!(
            <CordisPlugin as ClapPlugin>::CLAP_ID,
            "com.phonix-audio.cordis",
        );
        assert_eq!(<CordisPlugin as Plugin>::NAME, "Cordis");
        assert_eq!(<CordisPlugin as Plugin>::VENDOR, "Phonix Audio");
    }
}

// ── The curated chain's compatibility surface ─────────────────────
#[cfg(test)]
mod fx_chain_compat {
    use super::*;

    /// A plugin that has not been initialised, and a project that never loads
    /// a factory preset, both hold no chain at all. `fx::tests` proves the
    /// other half: that an empty chain returns its input untouched.
    #[test]
    fn a_fresh_plugin_carries_no_chain() {
        let p = CordisPlugin::default();
        assert!(p.fx_chain.is_none());
    }

    /// A fresh instance shows the first preset's name, so its state must
    /// carry that preset's chain: what the window says and what plays are
    /// the same thing. A project saved before the field existed is the other
    /// case, and `cordis` pins it: that one restores to an empty chain.
    #[test]
    fn a_fresh_instance_carries_the_first_preset_chain() {
        let p = CordisParams::default();
        let held = p.patch_state.read().unwrap().fx.clone();
        assert_eq!(held, cordis::patch::factory_presets()[0].fx);
        assert_eq!(held.slots.len(), cordis::fx::FX_SLOTS);
    }
}

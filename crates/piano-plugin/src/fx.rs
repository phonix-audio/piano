//! Running the chain a patch describes.
//!
//! The description lives in `piano::fx`, with the presets that carry it. What
//! is here is the half that needs a live `Chain`, which is the host's and
//! never the engine's.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use phonix_fx::{ApplyReport, Chain, ChainSpec, Registry};

/// The chain the audio thread runs, as the editor and the audio thread hand
/// it to each other: a value and a counter. The value is always written
/// before the counter moves, so a reader that sees a new number reads the
/// value that goes with it. Each side remembers the number it last saw.
pub struct FxLink {
    live: RwLock<ChainSpec>,
    rev: AtomicU64,
}

impl FxLink {
    pub fn new(spec: ChainSpec) -> Self {
        FxLink { live: RwLock::new(spec), rev: AtomicU64::new(0) }
    }

    pub fn rev(&self) -> u64 {
        self.rev.load(Ordering::Acquire)
    }

    /// Replaces the value without moving the counter: what a side that has
    /// already applied `spec` itself writes, so the other side finds it.
    pub fn seed(&self, spec: ChainSpec) {
        if let Ok(mut w) = self.live.write() {
            *w = spec;
        }
    }

    /// Writes `spec`, then moves the counter; returns the number the writer
    /// has now seen.
    pub fn publish(&self, spec: ChainSpec) -> u64 {
        if let Ok(mut w) = self.live.write() {
            *w = spec;
        }
        self.rev.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// The editor's side: the chain, when the counter moved since `seen`.
    /// May wait for the lock.
    pub fn adopt(&self, seen: &mut u64) -> Option<ChainSpec> {
        let rev = self.rev();
        if rev == *seen {
            return None;
        }
        let spec = self.live.read().ok()?.clone();
        *seen = rev;
        Some(spec)
    }

    /// The audio thread's side: runs `f` on the chain when the counter moved
    /// since `seen`, and never waits for the lock; a block missed is picked
    /// up on the next one. Returns whether `f` ran.
    pub fn apply_if_new(&self, seen: &mut u64, f: impl FnOnce(&ChainSpec)) -> bool {
        let rev = self.rev();
        if rev == *seen {
            return false;
        }
        let Ok(spec) = self.live.try_read() else { return false };
        f(&spec);
        *seen = rev;
        true
    }
}

/// The four kinds this build ships, and nothing else.
pub fn registry() -> Registry {
    Registry::builtin()
}

/// Build `spec` into `chain`, replacing whatever it held. Allocates: called
/// on a preset change and on an edit, never per block.
pub fn apply(chain: &mut Chain, spec: &ChainSpec) -> ApplyReport {
    chain.apply(spec, &registry())
}

/// Empty the chain. `Init` is the absence of a factory preset, so it is the
/// absence of the chain one carries.
pub fn disengage(chain: &mut Chain) {
    chain.apply(&ChainSpec::default(), &registry());
}

#[cfg(test)]
mod tests {
    use super::*;
    use piano::fx::{concert_hall, FX_SLOTS};
    use phonix_fx::{Musical, Transport, Value};

    /// What the FX page does to a chain when the compressor is pushed.
    fn pushed(mut spec: ChainSpec) -> ChainSpec {
        spec.slots[1].set("threshold", -60.0_f32);
        spec.slots[1].set("makeup", 24.0_f32);
        spec
    }

    fn makeup(chain: &Chain) -> Value {
        chain.param(chain.param_ref(1, "makeup").unwrap()).unwrap()
    }

    /// The editor pushes the compressor, then picks the preset it is already
    /// on. The host parameter does not move, so nothing but the link can put
    /// the chain back; the audio thread must end on the preset's values.
    #[test]
    fn picking_the_current_preset_again_puts_its_chain_back() {
        let preset = concert_hall();
        let link = FxLink::new(preset.clone());
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &preset);
        let (mut audio_seen, mut editor_seen) = (link.rev(), link.rev());

        // An edit on the page, taken by the next block.
        editor_seen = link.publish(pushed(preset.clone()));
        assert!(link.apply_if_new(&mut audio_seen, |s| { apply(&mut chain, s); }));
        assert_eq!(makeup(&chain), Value::F(24.0));

        // The same preset, picked again: the page publishes its chain.
        editor_seen = link.publish(preset.clone());
        assert!(link.apply_if_new(&mut audio_seen, |s| { apply(&mut chain, s); }));
        assert_eq!(makeup(&chain), Value::F(0.0));
        assert_eq!(chain.spec(), Chain::new(SR, BLOCK).tap(|c| { apply(c, &preset); }).spec());
        assert!(link.adopt(&mut editor_seen).is_none(), "the editor already holds what it published");
    }

    /// A different preset: the host parameter moves and the audio thread
    /// applies the bank's chain itself; the page then adopts the same chain
    /// and neither side re-applies the edit.
    #[test]
    fn a_preset_change_from_the_audio_thread_reaches_the_page_and_cancels_the_edit() {
        let a = concert_hall();
        let b = piano::fx::chain(-5.0, -20.0, piano::fx::Room { size: 0.1, decay: 0.2, mix: 0.05 });
        let link = FxLink::new(a.clone());
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &a);
        let (mut audio_seen, mut editor_seen) = (link.rev(), link.rev());

        let _ = link.publish(pushed(a.clone()));
        editor_seen = link.rev();
        link.apply_if_new(&mut audio_seen, |s| { apply(&mut chain, s); });
        assert_eq!(makeup(&chain), Value::F(24.0));

        // The preset branch on the audio thread.
        apply(&mut chain, &b);
        audio_seen = link.publish(b.clone());
        assert!(!link.apply_if_new(&mut audio_seen, |_| panic!("re-applied its own publication")));
        assert_eq!(makeup(&chain), Value::F(0.0));
        assert_eq!(link.adopt(&mut editor_seen), Some(b.clone()));
        assert_eq!(chain.spec().slots[2].mix, 0.05);
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for Chain {}

    const SR: f32 = 48_000.0;
    const BLOCK: usize = 256;
    const SLOT_EQ: usize = 0;
    const SLOT_COMP: usize = 1;
    const SLOT_ROOM: usize = 2;

    fn run(chain: &mut Chain, l: &mut [f32], r: &mut [f32]) {
        chain.process(l, r, &[], Transport::default(), Musical::default());
    }

    /// A tone loud enough to reach the limiter, as a stereo pair.
    fn tone(n: usize, amp: f32) -> (Vec<f32>, Vec<f32>) {
        let f = 220.0 / SR;
        let l: Vec<f32> = (0..n).map(|i| amp * (i as f32 * f * std::f32::consts::TAU).sin()).collect();
        let r = l.clone();
        (l, r)
    }

    /// The compatibility guarantee, stated as audio rather than as a flag: a
    /// project that never loads a factory preset drives an empty chain, and an
    /// empty chain returns exactly what it was given.
    #[test]
    fn an_empty_chain_is_bit_identical() {
        let mut chain = Chain::new(SR, BLOCK);
        assert!(chain.is_idle());
        assert_eq!(chain.latency_samples(), 0);
        let (mut l, mut r) = tone(2048, 0.5);
        let (l0, r0) = (l.clone(), r.clone());
        run(&mut chain, &mut l, &mut r);
        assert_eq!(l, l0);
        assert_eq!(r, r0);
    }

    /// The recipe's starting values, which a freshly loaded preset gets.
    #[test]
    fn the_recipe_starts_each_effect_where_it_says() {
        let mut chain = Chain::new(SR, BLOCK);
        assert!(apply(&mut chain, &concert_hall()).is_clean());
        let at = |c: &Chain, slot: usize, id: &str| c.param(c.param_ref(slot, id).unwrap()).unwrap();
        assert_eq!(at(&chain, SLOT_EQ, "band.0.gain"), Value::F(-3.0));
        assert_eq!(at(&chain, SLOT_COMP, "threshold"), Value::F(-18.0));
        assert_eq!(at(&chain, SLOT_COMP, "mode"), Value::E("bus"));
        assert_eq!(chain.mix(SLOT_ROOM), 0.22);
        assert_eq!(chain.latency_samples(), 239, "only the limiter's lookahead");
    }

    #[test]
    fn applying_the_recipe_fills_the_slots() {
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &concert_hall());
        assert_eq!(chain.len(), FX_SLOTS);
        assert!(!chain.is_idle());
        assert!(!chain.is_placeholder(SLOT_COMP));
    }

    /// Stepping back to `Init` takes the chain with it.
    #[test]
    fn disengaging_empties_the_chain() {
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &concert_hall());
        assert!(!chain.is_idle());
        disengage(&mut chain);
        assert!(chain.is_idle());
        let (mut l, mut r) = tone(1024, 0.5);
        let (l0, r0) = (l.clone(), r.clone());
        run(&mut chain, &mut l, &mut r);
        assert_eq!(l, l0);
        assert_eq!(r, r0);
    }

    #[test]
    fn the_engaged_chain_renders() {
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &concert_hall());
        let (mut l, mut r) = tone(4096, 0.3);
        let l0 = l.clone();
        run(&mut chain, &mut l, &mut r);
        let diff = l.iter().zip(&l0).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(diff > 1e-4, "engaged chain left the signal alone (max diff {diff})");
    }

    /// The recipe at the rates a host runs it at: the limiter's lookahead
    /// follows the rate, and the ceiling holds at each.
    #[test]
    fn the_chain_keeps_its_ceiling_and_lookahead_at_every_rate() {
        for sr in [44_100.0_f32, 48_000.0, 96_000.0] {
            let mut chain = Chain::new(sr, BLOCK);
            assert!(apply(&mut chain, &concert_hall()).is_clean());
            assert_eq!(chain.latency_samples(), ((sr * 0.005) as usize).max(16) - 1, "{sr} Hz");
            let ceiling = 10.0_f32.powf(-0.3 / 20.0) * 1.02;
            let n = sr as usize;
            let f = 220.0 / sr;
            let mut l: Vec<f32> = (0..n).map(|i| 0.99 * (i as f32 * f * std::f32::consts::TAU).sin()).collect();
            let mut r = l.clone();
            chain.process(&mut l, &mut r, &[], Transport::default(), Musical::default());
            let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak <= ceiling, "{sr} Hz: peak {peak} above the ceiling {ceiling}");
        }
    }

    /// What the ceiling is there for: the three slots above it can add gain,
    /// and the worst case the dial allows must still not leave full scale.
    #[test]
    fn the_curated_chain_stays_under_the_ceiling() {
        let mut chain = Chain::new(SR, BLOCK);
        apply(&mut chain, &concert_hall());
        let ceiling = 10.0_f32.powf(-0.3 / 20.0) * 1.02;
        let (mut l, mut r) = tone(48_000, 0.99);
        run(&mut chain, &mut l, &mut r);
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= ceiling, "peak {peak} above the ceiling {ceiling}");
    }
}

/// The editor and the audio thread, driven in the plugin's own order through
/// the real page, the link and a real chain.
#[cfg(test)]
mod through_the_page {
    use super::*;
    use piano::patch::factory_presets_tagged;
    use piano::state_buffer::meter_channel;
    use piano::{PianoMeterState, PianoPatch};
    use piano_ui::PianoApp;
    use egui_kittest::Harness;
    use phonix_fx::Value;

    const SR: f32 = 48_000.0;

    struct Audio {
        chain: Chain,
        seen: u64,
        last_preset: i32,
        preset: i32,
    }

    /// One editor frame, as `create_editor`'s closure runs it.
    fn frame(h: &mut Harness<'_, PianoApp>, link: &FxLink, seen: &mut u64, bank: &[PianoPatch], param: &mut i32) {
        if let Some(spec) = link.adopt(seen) {
            h.state_mut().set_fx(spec);
        }
        h.run_steps(1);
        let app = h.state_mut();
        if app.take_fx_changed() {
            *seen = link.publish(app.fx().clone());
        }
        if let Some(i) = app.take_wants_preset() {
            let chain = bank.get((i - 1).max(0) as usize).filter(|_| i > 0).map(|p| p.fx.clone()).unwrap_or_default();
            *seen = link.publish(chain);
            *param = i;
        }
    }

    /// One audio block, as `process` runs it.
    fn block(a: &mut Audio, link: &FxLink, bank: &[PianoPatch]) {
        if a.preset != a.last_preset {
            a.last_preset = a.preset;
            match bank.get((a.preset - 1).max(0) as usize).filter(|_| a.preset > 0) {
                Some(p) => {
                    apply(&mut a.chain, &p.fx);
                    a.seen = link.publish(p.fx.clone());
                }
                None => {
                    disengage(&mut a.chain);
                    a.seen = link.publish(ChainSpec::default());
                }
            }
        }
        let chain = &mut a.chain;
        link.apply_if_new(&mut a.seen, |spec| { apply(chain, spec); });
    }

    fn makeup(chain: &Chain) -> Value {
        chain.param(chain.param_ref(1, "makeup").unwrap()).unwrap()
    }

    #[test]
    fn a_pushed_compressor_is_put_back_by_a_preset_picked_on_the_page() {
        let bank = factory_presets_tagged();
        let (tx, _rx) = std::sync::mpsc::channel();
        let (_writer, reader) = meter_channel::<PianoMeterState>();
        let mut app = PianoApp::new(tx, reader);
        app.set_patch(bank[0].clone());
        app.set_tab(1);
        let link = FxLink::new(bank[0].fx.clone());
        let mut audio = Audio { chain: Chain::new(SR, 512), seen: link.rev(), last_preset: 1, preset: 1 };
        apply(&mut audio.chain, &bank[0].fx);
        let mut seen = link.rev();
        let mut param = 1;
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(|ui, app: &mut PianoApp| app.draw_ui(ui), app);

        // Settle, then push the compressor on the page.
        for _ in 0..3 { frame(&mut h, &link, &mut seen, &bank, &mut param); block(&mut audio, &link, &bank); }
        h.state_mut().edit_fx(|fx| { fx.slots[1].set("threshold", -60.0_f32); fx.slots[1].set("makeup", 24.0_f32); });
        frame(&mut h, &link, &mut seen, &bank, &mut param);
        block(&mut audio, &link, &bank);
        assert_eq!(makeup(&audio.chain), Value::F(24.0), "the edit did not reach the audio thread");

        // Another preset, picked on the page.
        h.state_mut().pick_preset(1);
        for _ in 0..3 {
            frame(&mut h, &link, &mut seen, &bank, &mut param);
            audio.preset = param;
            block(&mut audio, &link, &bank);
        }
        assert_eq!(makeup(&audio.chain), Value::F(0.0), "the audio thread kept the edit");
        assert_eq!(audio.chain.spec(), Chain::new(SR, 512).tap(|c| { apply(c, &bank[1].fx); }).spec());
        assert_eq!(h.state_mut().fx(), &bank[1].fx, "the page does not show the preset");

        // The same preset, picked again after another edit.
        h.state_mut().edit_fx(|fx| { fx.slots[1].set("makeup", 24.0_f32); });
        frame(&mut h, &link, &mut seen, &bank, &mut param);
        block(&mut audio, &link, &bank);
        assert_eq!(makeup(&audio.chain), Value::F(24.0));
        h.state_mut().pick_preset(1);
        for _ in 0..3 {
            frame(&mut h, &link, &mut seen, &bank, &mut param);
            audio.preset = param;
            block(&mut audio, &link, &bank);
        }
        assert_eq!(makeup(&audio.chain), Value::F(0.0), "a preset picked again left the edit in place");
        assert_eq!(h.state_mut().fx(), &bank[1].fx);
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for Chain {}
}

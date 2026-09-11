//! The engine at several polyphonies, and nothing else: the realtime factor
//! of the process path, for comparing one build against another.
//! `piano_bench` measures where the time goes; this only measures how much.
//!
//! Two rows of the same thing. The exact board is what a bounce renders; the
//! decoupled board is what live playing runs, and it is the one a plugin's
//! budget is judged on.
use piano::engine::{PianoCommand, PianoEngine};
use piano::patch::factory_presets;
use std::time::Instant;

/// Audio timed per polyphony, after a one-second warm-up that lets the
/// attacks pass.
const SECS: f64 = 5.0;

fn main() {
    // The engine enables this on every process_audio; without it the modes
    // ring down into denormals and the loop measures an artefact.
    piano::denormal::enable_flush_to_zero();
    let sr = 48_000.0f32;
    let preset = factory_presets().into_iter().find(|p| p.name.contains("Close Mics")).unwrap();
    for (label, decoupled) in [("exact board (bounce)", false), ("decoupled board (live)", true)] {
        println!("-- {label} --");
        for &k in &[1usize, 8, 16, 31] {
            let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
            eng.set_decoupled(decoupled);
            tx.send(PianoCommand::LoadPatch(Box::new(preset.clone()))).ok();
            for i in 0..k {
                tx.send(PianoCommand::NoteOn(48 + (i as u8) * 2, 90)).ok();
            }
            let block = 128usize;
            let mut buf = vec![0.0f32; block * 2];
            for _ in 0..(sr as usize / block) {
                eng.process_audio(&mut buf, 2);
            }
            let blocks = (sr as f64 * SECS) as usize / block;
            let t = Instant::now();
            for _ in 0..blocks {
                eng.process_audio(&mut buf, 2);
            }
            let el = t.elapsed().as_secs_f64();
            let audio = (blocks * block) as f64 / sr as f64;
            println!("engine {k:2} voices: {:.2}x realtime ({:.0} ms dsp)", audio / el, el * 1000.0);
        }
    }
}

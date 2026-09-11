//! When is a note over? Renders for the ear to decide.
//!
//! A voice costs the same whether it is loud or inaudible: the coupling reads
//! all 3618 modes of the plate every sample whatever the string is doing. The
//! engine retires a voice when its energy falls under an ABSOLUTE floor, which
//! is a different question from when it stops being audible — measured, a
//! damped C4 falls 60 dB in 0.44 s and is still being computed at 1.97 s.
//!
//! `set_retire_rel` judges it against the note's own loudest moment instead.
//! This renders the same music at several settings, at IDENTICAL gain and with
//! no normalisation, so that anything heard between them is the difference and
//! not the level. It prints what each one cost.
//!
//!   cargo run --release -p piano-research --bin retire_ab
//!
//! Two pieces. A single note released early, which is the tail question in its
//! purest form; and a glissando, which is the case that brought this up — a
//! mouse dragged across the keyboard leaves fifty strings ringing.

use piano::engine::{PianoCommand, PianoEngine};
use piano::patch::factory_presets;
use std::time::Instant;

const SR: f32 = 48_000.0;
const BLOCK: usize = 128;

/// The settings under test. `None` is the instrument as it ships.
const TRIALS: [(&str, Option<f64>); 4] = [
    ("actuel", None),
    ("rel120", Some(1e-12)),
    ("rel100", Some(1e-10)),
    ("rel080", Some(1e-8)),
];

struct Take {
    audio: Vec<f32>,
    dsp: f64,
}

/// Run one score through one setting. The score is a list of
/// `(second, command)`, played in order.
fn render(rel: Option<f64>, secs: f64, score: &[(f64, PianoCommand)]) -> Take {
    let preset = factory_presets()
        .into_iter()
        .find(|p| p.name.contains("Close Mics"))
        .expect("the Close Mics preset");
    let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
    let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
    if let Some(r) = rel {
        eng.set_retire_rel(r);
    }
    // One thread, so the timing below is the DSP and not the pool.
    eng.set_workers(0);

    let blocks = (SR as f64 * secs) as usize / BLOCK;
    let mut audio = Vec::with_capacity(blocks * BLOCK * 2);
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut next = 0usize;
    let mut dsp = 0.0;
    for b in 0..blocks {
        let t = (b * BLOCK) as f64 / SR as f64;
        while next < score.len() && score[next].0 <= t {
            let _ = tx.send(score[next].1.clone());
            next += 1;
        }
        buf.iter_mut().for_each(|x| *x = 0.0);
        let start = Instant::now();
        eng.process_audio(&mut buf, 2);
        dsp += start.elapsed().as_secs_f64();
        audio.extend_from_slice(&buf);
    }
    Take { audio, dsp }
}

fn write(path: &str, audio: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR as u32,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("the wav");
    for &s in audio {
        let v = (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
        w.write_sample(v).expect("a sample");
    }
    w.finalize().expect("closing the wav");
}

/// How far apart two takes are, as dB under the reference's peak, and when the
/// difference first shows.
fn compare(reference: &[f32], other: &[f32]) -> (f64, f64) {
    let peak = reference.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let mut worst = 0.0f32;
    let mut first = f64::NAN;
    for (i, (a, b)) in reference.iter().zip(other.iter()).enumerate() {
        let d = (a - b).abs();
        if d > peak * 1e-4 && first.is_nan() {
            first = (i / 2) as f64 / SR as f64;
        }
        worst = worst.max(d);
    }
    (
        20.0 * (worst / peak).max(1e-30).log10() as f64,
        first,
    )
}

fn main() {
    piano::denormal::enable_flush_to_zero();
    let dir = std::path::Path::new("renders/retire");
    std::fs::create_dir_all(dir).expect("the output directory");

    // ── One note, key up after half a second ───────────────────────────────
    let single: Vec<(f64, PianoCommand)> = vec![
        (0.0, PianoCommand::NoteOn(60, 100)),
        (0.5, PianoCommand::NoteOff(60)),
    ];
    // ── A glissando: thirty keys in a second and a half, each released as the
    //    hand moves on, exactly as a mouse dragged across the keyboard does ──
    let mut gliss: Vec<(f64, PianoCommand)> = Vec::new();
    for i in 0..30u8 {
        let t = i as f64 * 0.05;
        let note = 48 + i;
        gliss.push((t, PianoCommand::NoteOn(note, 90)));
        gliss.push((t + 0.08, PianoCommand::NoteOff(note)));
    }

    for (name, secs, score) in [
        ("note", 5.0, &single),
        ("glissando", 6.0, &gliss),
    ] {
        println!("\n== {name} ==");
        let mut reference: Option<Vec<f32>> = None;
        for (label, rel) in TRIALS {
            let take = render(rel, secs, score);
            let path = format!("{}/{name}_{label}.wav", dir.display());
            write(&path, &take.audio);
            let rt = secs / take.dsp;
            match &reference {
                None => {
                    println!(
                        "  {label:8} {:5.2}s dsp  ({rt:.2}x realtime)   <- reference   {path}",
                        take.dsp
                    );
                    reference = Some(take.audio);
                }
                Some(r) => {
                    let (db, first) = compare(r, &take.audio);
                    let saved = 100.0 * (1.0 - take.dsp / (secs / (secs / take.dsp)).max(1e-9));
                    let _ = saved;
                    println!(
                        "  {label:8} {:5.2}s dsp  ({rt:.2}x realtime)   difference {db:6.1} dB \
                         under peak, first audible at {first:.2}s   {path}",
                        take.dsp
                    );
                }
            }
        }
    }
    println!("\nSame gain, no normalisation: what differs between the files IS the difference.");
}

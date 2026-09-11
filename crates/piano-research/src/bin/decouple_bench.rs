//! The decoupled board, measured against the exact one — chantier #74 phase 1.
//!
//! Three questions, in the order they gate the campaign:
//!   1. COST: what does dropping the per-sample board read buy? The ressac
//!      criterion is one pedalled instance at >= 1x realtime SERIAL, because
//!      the host fan-out gives each instance its own core and the wall is
//!      max(instance).
//!   2. DECAY: does the static drain land where the dynamic one did? Per note
//!      across the compass, the 20 dB slope of the rendered note, exact vs
//!      decoupled — the same ruler as `audit_the_decay_against_the_coupling`.
//!   3. THE CHORD: the crest chord's tail slope (the organ report) both ways.
use piano::engine::{PianoCommand, PianoEngine};
use std::time::Instant;

const SR: f32 = 48_000.0;

/// One engine per mode: everything identical but the coupling.
fn engine(decoupled: bool) -> (PianoEngine, std::sync::mpsc::Sender<PianoCommand>) {
    let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
    std::mem::forget(_mr); // keep the meter alive without holding it
    eng.set_decoupled(decoupled);
    (eng, tx)
}

fn render_note(decoupled: bool, note: u8, vel: u8, secs: f32) -> Vec<f32> {
    let (mut eng, tx) = engine(decoupled);
    let _ = tx.send(PianoCommand::NoteOn(note, vel));
    let total = (SR * secs) as usize;
    let mut out = Vec::with_capacity(total);
    let mut buf = vec![0.0f32; 256 * 2];
    while out.len() < total {
        buf.iter_mut().for_each(|x| *x = 0.0);
        eng.process_audio(&mut buf, 2);
        for fr in buf.chunks(2) {
            out.push((fr[0] + fr[1]) * 0.5);
        }
    }
    out.truncate(total);
    out
}

/// 10 ms RMS envelope.
fn env(x: &[f32]) -> Vec<f64> {
    let blk = (0.01 * SR) as usize;
    x.chunks(blk)
        .map(|c| (c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / c.len() as f64).sqrt())
        .collect()
}

/// Amplitude decay rate over the first 20 dB past the attack, in dB/s — the
/// slope a player hears as "how long the note lasts".
fn slope_db_s(e: &[f64]) -> f64 {
    let i0 = 5.min(e.len() - 1);
    let e0 = e[i0].max(1e-30);
    let i1 = e[i0..].iter().position(|&x| x < e0 * 0.1).map(|k| i0 + k).unwrap_or(e.len() - 1);
    let dt = (i1 - i0) as f64 * 0.01;
    if dt > 1e-3 { 20.0 / dt } else { f64::NAN }
}

fn db(x: f64, r: f64) -> f64 {
    20.0 * ((x + 1e-30) / (r + 1e-30)).log10()
}

fn main() {
    piano::denormal::enable_flush_to_zero();

    // ── 1. COST: 31 pedalled voices, serial and pooled ─────────────────────
    println!("== cout : 31 voix sous pedale, 2 s d'audio ==");
    for &dec in &[false, true] {
        for &workers in &[0usize, 6] {
            let (mut eng, tx) = engine(dec);
            eng.set_workers(workers);
            let _ = tx.send(PianoCommand::SustainPedal(true));
            for i in 0..31 {
                let _ = tx.send(PianoCommand::NoteOn(30 + (i as u8) * 2, 96));
            }
            let block = 128usize;
            let mut buf = vec![0.0f32; block * 2];
            // Warm-up: past the hammers, protos hot, pool spun up.
            for _ in 0..(SR as usize / block) {
                eng.process_audio(&mut buf, 2);
            }
            let blocks = (SR as f64 * 2.0) as usize / block;
            let t = Instant::now();
            for _ in 0..blocks {
                eng.process_audio(&mut buf, 2);
            }
            let el = t.elapsed().as_secs_f64();
            let audio = (blocks * block) as f64 / SR as f64;
            println!(
                "  {} {:>2} workers : {:5.2}x temps reel",
                if dec { "decouple" } else { "exact   " },
                workers,
                audio / el
            );
        }
    }

    // ── 1b. WHAT the cost is made of: voices, or the plate? ────────────────
    //
    // The governor sheds VOICES when an instance runs hot. If most of an
    // instance's cost is the plate — 3618 modes advanced every sample no
    // matter how many strings are sounding — then shedding cannot bring the
    // load down, and the lever is the plate, not the polyphony. This is the
    // measurement that decides which.
    println!("\n== de quoi le cout est fait (decouple, 1 worker + l'audio) ==");
    for &nv in &[0usize, 4, 8, 16, 31] {
        let (mut eng, tx) = engine(true);
        eng.set_workers(1);
        let _ = tx.send(PianoCommand::SustainPedal(true));
        for i in 0..nv {
            let _ = tx.send(PianoCommand::NoteOn(30 + (i as u8) * 2, 96));
        }
        let block = 128usize;
        let mut buf = vec![0.0f32; block * 2];
        for _ in 0..(SR as usize / block) {
            eng.process_audio(&mut buf, 2);
        }
        let blocks = (SR as f64 * 2.0) as usize / block;
        let t = Instant::now();
        for _ in 0..blocks {
            eng.process_audio(&mut buf, 2);
        }
        let el = t.elapsed().as_secs_f64();
        let audio = (blocks * block) as f64 / SR as f64;
        println!(
            "  {nv:>2} voix : {:5.2}x temps reel  ({:5.1}% du budget)",
            audio / el,
            100.0 * el / audio
        );
    }

    // ── 2. DECAY: the compass, exact vs decoupled ──────────────────────────
    println!("\n== declin par note (pente des 20 premiers dB, en dB/s) ==");
    println!(" note   exact  decouple  ecart     niv 0.5s  niv 1.0s (dB, dec-exa)");
    for note in (21u8..=105).step_by(12) {
        let a = render_note(false, note, 100, 4.0);
        let b = render_note(true, note, 100, 4.0);
        let (ea, eb) = (env(&a), env(&b));
        let (sa, sb) = (slope_db_s(&ea), slope_db_s(&eb));
        let at = |e: &[f64], s: f64| e[((s / 0.01) as usize).min(e.len() - 1)];
        println!(
            "  {note:>3}  {sa:>6.1}  {sb:>8.1}  {:>+5.1}     {:>+8.1}  {:>+8.1}",
            sb - sa,
            db(at(&eb, 0.5), at(&ea, 0.5)),
            db(at(&eb, 1.0), at(&ea, 1.0)),
        );
    }

    // ── 3. THE CREST CHORD: the organ's own test, both ways ────────────────
    println!("\n== accord de la crete (26 38 74 81 86 93 98, v112, pedale) ==");
    println!("        t ->   0.3s   0.6s   1.0s   1.5s   2.0s  (dB rel. crete)");
    for &dec in &[false, true] {
        let (mut eng, tx) = engine(dec);
        let _ = tx.send(PianoCommand::SustainPedal(true));
        for &n in &[26u8, 38, 74, 81, 86, 93, 98] {
            let _ = tx.send(PianoCommand::NoteOn(n, 112));
        }
        let total = (SR * 2.2) as usize;
        let mut out = Vec::with_capacity(total);
        let mut buf = vec![0.0f32; 256 * 2];
        while out.len() < total {
            buf.iter_mut().for_each(|x| *x = 0.0);
            eng.process_audio(&mut buf, 2);
            for fr in buf.chunks(2) {
                out.push((fr[0] + fr[1]) * 0.5);
            }
        }
        let e = env(&out);
        let pk = e.iter().cloned().fold(0.0f64, f64::max);
        let at = |s: f64| db(e[((s / 0.01) as usize).min(e.len() - 1)], pk);
        println!(
            "  {} : {:>6.1} {:>6.1} {:>6.1} {:>6.1} {:>6.1}",
            if dec { "decouple" } else { "exact   " },
            at(0.3),
            at(0.6),
            at(1.0),
            at(1.5),
            at(2.0),
        );
    }
}

//! Six pianos at once, and WHERE the time goes — the ressac question.
//!
//! The solo bench says one decoupled instance under a 31-voice pedalled
//! storm costs 64 to 70 percent of its block budget. The user's live log
//! says one track alone reaches 38 ms against a 21.3 ms budget while five
//! others play. A factor of two and a half sits between those two numbers,
//! and guessing which of the three candidates it is — the voice count, the
//! contention between instances, or a governor that is not doing its job —
//! is exactly what this project has been wrong about twice.
//!
//! So this measures the same instance in the same figure, ALONE and then
//! SIX AT ONCE, and reports for each: worst block, overruns, the voices
//! actually sounding, the ceiling the governor set, and how many strings it
//! shed. Six threads, one per instance, released together at a line — the
//! host's layout.
//!
//!   cargo run --release -p piano-research --bin sixpack
use piano::engine::{PianoCommand, PianoEngine};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

const SR: f32 = 48_000.0;
/// The host's own sub-block: what a Piano instance is actually handed.
const BLOCK: usize = 128;
const SECS: f64 = 8.0;

struct Report {
    worst_ms: f64,
    over: usize,
    blocks: usize,
    voices_peak: usize,
    budget_end: usize,
    sheds: u64,
    /// Median and p99 of the per-block time, and the two means that matter:
    /// the blocks carrying a NOTE-ON against all the others. A worst block is
    /// a story about a spike, and a spike has a cause; this says whether the
    /// cause arrives with the hammer.
    p50_ms: f64,
    p99_ms: f64,
    mean_noteon_ms: f64,
    mean_plain_ms: f64,
    modes_min: usize,
    modes_mean: f64,
}

/// One instance's run: a pedalled figure that keeps ADDING notes, the way a
/// piece with the sustain pedal down does, so the voice count climbs into the
/// range the storm actually reaches instead of sitting at a polite six.
/// Pin this thread to one logical CPU. The machine under test is HYBRID —
/// two 4.9 GHz P-cores (with SMT siblings), eight 3.8 GHz E-cores and two
/// 2.1 GHz LP-E cores — so where a thread lands changes its speed by a
/// factor of two or three, and the scheduler is free to put a piano on an
/// LP-E core. That is the hypothesis this switch tests.
fn pin(cpu: usize) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

/// One logical CPU per PHYSICAL core, fastest first, the two slow LP-E cores
/// left out: 0 and 2 are the P-cores (one thread each, not their SMT
/// siblings), 4 to 11 the E-cores.
const FAST_CPUS: [usize; 10] = [0, 2, 4, 5, 6, 7, 8, 9, 10, 11];

fn run_instance(
    id: usize,
    live: bool,
    sync: Arc<AtomicUsize>,
    parties: usize,
    pinned: bool,
) -> Report {
    if pinned {
        pin(FAST_CPUS[id % FAST_CPUS.len()]);
    }
    if let Ok(v) = std::env::var("PIN_CPU") {
        if let Ok(c) = v.parse::<usize>() {
            pin(c);
        }
    }
    let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
    eng.set_decoupled(live);
    eng.set_governor(live);
    if std::env::var("NO_PEDAL").is_err() {
        let _ = tx.send(PianoCommand::SustainPedal(true));
    }

    let base = 26 + (id as u8) * 11; // spread the six across the compass
    let blocks = (SR as f64 * SECS) as usize / BLOCK;
    let budget = BLOCK as f64 / SR as f64;
    let mut buf = vec![0.0f32; BLOCK * 2];

    // Warm the protos and the pool before the line, so the measurement is of
    // the instrument and not of its first block.
    for _ in 0..(SR as usize / BLOCK) {
        eng.process_audio(&mut buf, 2);
    }

    sync.fetch_add(1, Ordering::SeqCst);
    while !sync.load(Ordering::SeqCst).is_multiple_of(parties + 1) {
        std::hint::spin_loop();
    }

    let (mut worst, mut over, mut voices_peak) = (0.0f64, 0usize, 0usize);
    let (mut modes_min, mut modes_sum) = (usize::MAX, 0.0f64);
    let mut times: Vec<f64> = Vec::with_capacity(blocks);
    let mut noteon: Vec<bool> = Vec::with_capacity(blocks);
    // A note every 120 ms under the pedal: by the end of a bar the instrument
    // is carrying a real storm, which is when the log complains.
    let every = ((0.12 * SR as f64) as usize / BLOCK).max(1);
    for b in 0..blocks {
        let hit = b % every == 0;
        if hit {
            let step = (b / every) as u8;
            let n = base + [0u8, 7, 12, 16, 19, 24, 28, 31][step as usize % 8];
            let _ = tx.send(PianoCommand::NoteOn(n.min(105), 96));
        }
        buf.iter_mut().for_each(|x| *x = 0.0);
        let s = Instant::now();
        eng.process_audio(&mut buf, 2);
        let dt = s.elapsed().as_secs_f64();
        worst = worst.max(dt);
        if dt > budget {
            over += 1;
        }
        voices_peak = voices_peak.max(eng.live_voices());
        modes_min = modes_min.min(eng.active_plate_modes().0);
        modes_sum += eng.active_plate_modes().0 as f64;
        times.push(dt * 1000.0);
        noteon.push(hit);
    }
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = |sel: bool| -> f64 {
        let v: Vec<f64> = times
            .iter()
            .zip(&noteon)
            .filter(|(_, h)| **h == sel)
            .map(|(t, _)| *t)
            .collect();
        if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
    };
    Report {
        worst_ms: worst * 1000.0,
        over,
        blocks,
        voices_peak,
        budget_end: eng.voice_budget(),
        sheds: eng.sheds_total(),
        p50_ms: sorted[sorted.len() / 2],
        p99_ms: sorted[sorted.len() * 99 / 100],
        mean_noteon_ms: mean(true),
        mean_plain_ms: mean(false),
        modes_min,
        modes_mean: modes_sum / blocks as f64,
    }
}

fn trial(instances: usize, live: bool, pinned: bool) -> Vec<Report> {
    let sync = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..instances)
        .map(|id| {
            let sync = sync.clone();
            std::thread::spawn(move || run_instance(id, live, sync, instances, pinned))
        })
        .collect();
    while sync.load(Ordering::SeqCst) < instances {
        std::hint::spin_loop();
    }
    sync.fetch_add(1, Ordering::SeqCst);
    handles.into_iter().map(|h| h.join().expect("instance")).collect()
}

fn show(title: &str, reps: &[Report], budget_ms: f64) {
    let worst = reps.iter().fold(0.0f64, |m, r| m.max(r.worst_ms));
    let over: usize = reps.iter().map(|r| r.over).sum();
    let blocks: usize = reps.iter().map(|r| r.blocks).sum();
    println!(
        "\n{title}\n  pire bloc {worst:6.2} ms ({:.0}% du budget)   depassements {over} / {blocks}",
        100.0 * worst / budget_ms
    );
    for (i, r) in reps.iter().enumerate() {
        println!(
            "   #{i}: pire {:6.2}  p99 {:5.2}  median {:5.2}  |  bloc AVEC frappe {:5.2}  sans {:5.2}  (x{:.1})  |  voix {:3}  plafond {:3}  abandons {}",
            r.worst_ms,
            r.p99_ms,
            r.p50_ms,
            r.mean_noteon_ms,
            r.mean_plain_ms,
            r.mean_noteon_ms / r.mean_plain_ms.max(1e-9),
            r.voices_peak,
            r.budget_end,
            r.sheds
        );
        println!(
            "        modes de table vivants : moyenne {:.0}, minimum {}",
            r.modes_mean, r.modes_min
        );
    }
}

fn main() {
    piano::denormal::enable_flush_to_zero();
    let budget_ms = 1000.0 * BLOCK as f64 / SR as f64;
    println!(
        "storm pedalee, {SECS} s, blocs de {BLOCK} (budget {budget_ms:.2} ms), moteur decouple + governor"
    );
    let _ = trial(1, true, false); // warm-up, discarded

    // Quick mode: one instance only, for the core-type comparison (PIN_CPU).
    if std::env::var("SOLO_ONLY").is_ok() {
        show("UNE instance", &trial(1, true, false), budget_ms);
        return;
    }

    // ── The scaling curve: what does ONE more piano cost the others? ──────
    //
    // Identical work per instance, only the count changes. If the cost per
    // instance stays flat until the threads outnumber the cores, the limit
    // is cores. If it climbs from the very first neighbour, the limit is a
    // resource they SHARE — cache or memory — and no scheduling fixes that.
    // Swept up and back down, because a machine that warms during the sweep
    // would otherwise fake exactly the curve being looked for.
    println!("\n== courbe d'echelle (mediane par instance, ms) ==");
    println!(" N   montee   descente   (budget {budget_ms:.2} ms)");
    let mut up = Vec::new();
    for n in [1usize, 2, 3, 4, 6] {
        let reps = trial(n, true, false);
        let med: f64 = reps.iter().map(|r| r.p50_ms).sum::<f64>() / n as f64;
        up.push(med);
    }
    let mut down = Vec::new();
    for n in [6usize, 4, 3, 2, 1] {
        let reps = trial(n, true, false);
        let med: f64 = reps.iter().map(|r| r.p50_ms).sum::<f64>() / n as f64;
        down.push(med);
    }
    down.reverse();
    for (i, n) in [1usize, 2, 3, 4, 6].iter().enumerate() {
        println!(
            "  {n}   {:6.2}   {:6.2}     ({:.0}% et {:.0}% du budget)",
            up[i],
            down[i],
            100.0 * up[i] / budget_ms,
            100.0 * down[i] / budget_ms
        );
    }

    show("UNE instance seule", &trial(1, true, false), budget_ms);
    show("SIX ensemble", &trial(6, true, false), budget_ms);
}

//! Six pianos in one process — the Ressac configuration, measured.
//!
//! Ressac runs six Piano instances at once and was unplayable live. Each
//! instance used to spawn and ENROL a full worker pool, so six pools of
//! per-sample spin barriers fought over the machine. `worker_cap_for` now
//! divides the workers between the instances actually sounding (three or more
//! sounding: everyone goes serial, the host's track-level parallelism is the
//! right axis at that width).
//!
//! This measures exactly that trade: six engines on six threads, each playing
//! a pedalled figure, capped against uncapped, round-robin with a discarded
//! warm-up. Worst block and overruns per instance, 1024-frame budget.
//!
//!   cargo run --release -p piano-research --bin ensemble_bench

use piano::engine::{PianoCommand, PianoEngine};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

const SR: f32 = 48_000.0;
const BLOCK: usize = 1024;
const SECS: f64 = 6.0;
const INSTANCES: usize = 6;

/// One instance's run: a pedalled broken chord in its own register, so the six
/// together look like the piece — every instance sounding at once, moderate
/// polyphony each.
fn run_instance(id: usize, capped: bool, sync: Arc<AtomicUsize>) -> (f64, usize, usize) {
    let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
    eng.set_instance_cap(capped);
    let _ = tx.send(PianoCommand::SustainPedal(true));

    let base = 28 + (id as u8) * 12; // spread the six across the compass
    let blocks = (SR as f64 * SECS) as usize / BLOCK;
    let budget = BLOCK as f64 / SR as f64;
    let mut buf = vec![0.0f32; BLOCK * 2];

    // Everyone waits at the line so the six render TOGETHER — the whole point.
    sync.fetch_add(1, Ordering::SeqCst);
    while !sync.load(Ordering::SeqCst).is_multiple_of(INSTANCES + 1) {
        std::hint::spin_loop();
    }

    let (mut worst, mut over) = (0.0f64, 0usize);
    for b in 0..blocks {
        let t = (b * BLOCK) as f64 / SR as f64;
        // A note every 300 ms, held under the pedal: 5-8 sounding voices each.
        let step = (t / 0.3) as u8;
        if (b * BLOCK) % ((0.3 * SR as f64) as usize / BLOCK * BLOCK) < BLOCK {
            let n = base + [0u8, 7, 12, 16, 19, 24][step as usize % 6];
            let _ = tx.send(PianoCommand::NoteOn(n.min(105), 92));
        }
        buf.iter_mut().for_each(|x| *x = 0.0);
        let s = Instant::now();
        eng.process_audio(&mut buf, 2);
        let dt = s.elapsed().as_secs_f64();
        worst = worst.max(dt);
        if dt > budget {
            over += 1;
        }
    }
    (worst * 1000.0, over, blocks)
}

fn trial(capped: bool) -> (f64, usize, usize) {
    let sync = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..INSTANCES)
        .map(|id| {
            let sync = sync.clone();
            std::thread::spawn(move || run_instance(id, capped, sync))
        })
        .collect();
    // Release the line once everyone has parked an increment.
    while sync.load(Ordering::SeqCst) < INSTANCES {
        std::hint::spin_loop();
    }
    sync.fetch_add(1, Ordering::SeqCst);
    let mut worst = 0.0f64;
    let (mut over, mut blocks) = (0usize, 0usize);
    for h in handles {
        let (w, o, b) = h.join().expect("instance thread");
        worst = worst.max(w);
        over += o;
        blocks += b;
    }
    (worst, over, blocks)
}

fn main() {
    piano::denormal::enable_flush_to_zero();
    let budget_ms = 1000.0 * BLOCK as f64 / SR as f64;
    println!("six instances, {SECS} s, blocs de {BLOCK} (budget {budget_ms:.2} ms)");
    println!("balaye dans les deux sens, prechauffe jetee\n");
    let _ = trial(true); // warm-up, discarded

    for pass in 0..2 {
        let order = if pass == 0 { [false, true] } else { [true, false] };
        for capped in order {
            let (worst, over, blocks) = trial(capped);
            println!(
                "  {}  pire bloc {worst:7.2} ms   depassements {over:5} / {blocks}",
                if capped { "partage (nouveau)" } else { "chacun-pour-soi  " }
            );
        }
    }
}

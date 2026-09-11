//! The block kernel's gather, f64 against f32, alternated in one process.
//!
//! The question: the gather is the engine's arithmetic wall (2·M·V·T flops a
//! block, ~10.8 Gflop/s per instance at 31 voices, against 0.9 for the
//! recursion), and single precision issues twice the FMA lanes for it — but
//! the recursion must then widen 463k values a block, which gives some back.
//!
//! The METHOD is the point. Two passes taken minutes apart on this machine
//! measure the machine: the load average here runs 3.5 to 6.5 with a browser
//! and other sessions on it, and the UNCHANGED exact path has been seen to
//! drift 0.36x to 0.25x between runs. So the two kernels are alternated
//! inside one process, on the same engine state, round after round, and the
//! verdict is the MEDIAN of the per-round ratio — a figure that only moves if
//! one kernel is genuinely faster than the other in the same conditions.
//! Both orders are run (A,B then B,A) so a warming or cooling machine cannot
//! favour whichever went first, and the first round of each is discarded.
//!
//! Three variants share the harness, because the same doubt applies to each:
//!   - f64  : the shipped kernel — f32 weights, f64 accumulate;
//!   - f32  : the accumulate in single precision too (twice the FMA lanes);
//! Half precision on the WEIGHTS rode in this harness too, and rode out on
//! its own number: 1.024, 1.018, 1.017 — two percent slower, three runs, and
//! 72 dB of agreement where single precision gives 129. See `modal_bank`.
//!
//! It also checks that the variants agree to the sample: single or half
//! precision on a sum of at most eighty-eight products must not be audible,
//! and if it is, the speed question is moot.
use piano::engine::{PianoCommand, PianoEngine};
use piano::modal_bank::GATHER_F32;
use std::sync::atomic::Ordering;
use std::time::Instant;

const SR: f32 = 48_000.0;
const BLOCK: usize = 128;
/// Voices in the measured chord: the storm the ressac session actually plays.
const VOICES: usize = 31;
/// Rounds per order. The median of these decides.
const ROUNDS: usize = 9;

/// A pedalled engine on the serial block path — where the A/B lives.
fn armed() -> (PianoEngine, std::sync::mpsc::Sender<PianoCommand>) {
    let (mut eng, tx, mr) = PianoEngine::new_for_plugin(SR);
    std::mem::forget(mr);
    eng.set_decoupled(true);
    eng.set_workers(0);
    let _ = tx.send(PianoCommand::SustainPedal(true));
    for i in 0..VOICES {
        let _ = tx.send(PianoCommand::NoteOn(30 + (i as u8) * 2, 96));
    }
    let mut buf = vec![0.0f32; BLOCK * 2];
    // Past the hammers, into the ring: contact is a thousandth of a note, and
    // it is the ring that costs.
    for _ in 0..(SR as usize / BLOCK) {
        eng.process_audio(&mut buf, 2);
    }
    (eng, tx)
}

/// The kernels under test. Half precision was one of them until its number
/// came in — see `modal_bank`, which keeps the finding.
#[derive(Clone, Copy, PartialEq)]
enum Kernel {
    F64,
    F32,
}

impl Kernel {
    fn arm(self) {
        GATHER_F32.store(self == Kernel::F32, Ordering::Relaxed);
    }
    fn name(self) -> &'static str {
        match self {
            Kernel::F64 => "f64",
            Kernel::F32 => "f32",
        }
    }
}

/// Seconds of DSP for one second of audio, on the engine handed in.
fn time_one(eng: &mut PianoEngine, k: Kernel, buf: &mut [f32]) -> f64 {
    k.arm();
    let blocks = SR as usize / BLOCK;
    let t = Instant::now();
    for _ in 0..blocks {
        eng.process_audio(buf, 2);
    }
    t.elapsed().as_secs_f64()
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    piano::denormal::enable_flush_to_zero();

    // ── Do the variants agree? ────────────────────────────────────────────
    //
    // Same engine, same notes, one second each, compared sample by sample.
    // They are not expected to be bit-identical — that is the point of the
    // change — but the difference must sit where nothing is heard.
    {
        let render = |k: Kernel| -> Vec<f32> {
            k.arm();
            let (mut eng, _tx) = armed();
            let mut out = Vec::new();
            let mut buf = vec![0.0f32; BLOCK * 2];
            for _ in 0..(SR as usize / BLOCK) {
                buf.iter_mut().for_each(|x| *x = 0.0);
                eng.process_audio(&mut buf, 2);
                out.extend_from_slice(&buf);
            }
            out
        };
        let a = render(Kernel::F64);
        let peak = a.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        {
            let k = Kernel::F32;
            let b = render(k);
            let worst = a
                .iter()
                .zip(&b)
                .fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
            let db = 20.0 * (worst / peak.max(1e-30)).max(1e-30).log10();
            println!(
                "accord {} contre f64 : crete {peak:.4}, pire ecart {worst:e} ({db:.1} dB sous la crete)",
                k.name()
            );
        }
    }

    // ── The alternated timing ─────────────────────────────────────────────
    println!("\n{VOICES} voix sous pedale, 1 s d'audio par manche, {ROUNDS} manches par ordre");
    let mut buf = vec![0.0f32; BLOCK * 2];
    for challenger in [Kernel::F32] {
        let mut ratios = Vec::new();
        let (mut base_all, mut chal_all) = (Vec::new(), Vec::new());
        for (pass, chal_first) in [(0usize, false), (1, true)] {
            // One engine per order, warmed once: the state a kernel inherits
            // is then the same for both, round after round.
            let (mut eng, _tx) = armed();
            for round in 0..ROUNDS {
                let (t_base, t_chal) = if chal_first {
                    let c = time_one(&mut eng, challenger, &mut buf);
                    let b = time_one(&mut eng, Kernel::F64, &mut buf);
                    (b, c)
                } else {
                    let b = time_one(&mut eng, Kernel::F64, &mut buf);
                    let c = time_one(&mut eng, challenger, &mut buf);
                    (b, c)
                };
                // The first round of each order runs on a cold cache and a
                // machine that has just been handed work; it is thrown away.
                if round == 0 {
                    continue;
                }
                ratios.push(t_chal / t_base);
                base_all.push(t_base);
                chal_all.push(t_chal);
                let _ = pass;
            }
        }
        let mb = median(base_all);
        let mc = median(chal_all);
        let mr = median(ratios);
        println!(
            "\n{} contre f64 : f64 {:.3}s ({:.0}% du budget)  {} {:.3}s ({:.0}%)  rapport median {mr:.3}",
            challenger.name(),
            mb,
            100.0 * mb,
            challenger.name(),
            mc,
            100.0 * mc
        );
        println!(
            "  verdict : {:.1}% {} que f64",
            (1.0 - mr).abs() * 100.0,
            if mr < 1.0 { "PLUS RAPIDE" } else { "plus lent" }
        );
    }
    Kernel::F64.arm();
}

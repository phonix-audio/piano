//! In-process cost bench for the piano engine. Times the DSP directly (no
//! cargo-run / wall-clock noise) to size the soundboard-vs-strings split and the
//! realtime factor at several polyphonies, as the baseline for RT optimisation.
use piano::soundboard::Soundboard;
use piano::voice::Voice;
use piano::engine::{PianoEngine, PianoCommand};
use piano::patch::factory_presets;
use std::time::Instant;

/// Polyphony the term-by-term split is measured at.
const SPLIT_VOICES: usize = 16;

fn main() {
    // Without this the board's modes ring down into denormals and the loop runs
    // 100x slow -- the engine enables it every process_audio, so the bench must
    // too or it measures a denormal artefact, not the DSP.
    piano::denormal::enable_flush_to_zero();
    let sr = 48_000.0f32;

    // ── Soundboard size + isolated cost ────────────────────────────────────
    let mut board = Soundboard::new(sr, 0.25);
    println!("soundboard modes: {}", board.len());

    // Where the coupling weight lives, per note, below vs above the transition:
    // this decides whether the board can be split into full-modal LF + cheap HF.
    let freqs = board.frequencies().to_vec();
    for note in [40u8, 60, 80] {
        let a = board.attachment(note);
        let (mut lo, mut hi, mut nhi) = (0.0f64, 0.0f64, 0usize);
        for (w, &fr) in a.iter().zip(freqs.iter()) {
            if fr < 1100.0 { lo += w * w; } else { hi += w * w; nhi += 1; }
        }
        println!("note {note}: coupling energy  <1.1kHz {:.1}%  >1.1kHz {:.1}% (in {} modes)",
            100.0 * lo / (lo + hi), 100.0 * hi / (lo + hi), nhi);
    }
    let secs = 5.0f64;
    let n = (sr as f64 * secs) as usize;
    board.drive_bridge(1.0);
    let t = Instant::now();
    let mut acc = 0.0f64;
    for _ in 0..n {
        let (l, r) = board.process();      // tick + read left/right + radiate
        acc += l + r;
    }
    let el = t.elapsed().as_secs_f64();
    println!("board alone: {:.3}s dsp for {:.1}s audio  ->  {:.1}x realtime  (sink {:.3})",
        el, secs, secs / el, acc);

    // ── Where the time goes, term by term ──────────────────────────────────
    //
    // The engine figures below are a total. To optimise, one needs to know
    // which of the three things it does per sample is expensive: advancing the
    // board once, coupling each voice to the board (two full 3618-mode walks
    // per voice), or the string banks themselves. Each is timed here in
    // isolation, which is possible because `Voice::tick` takes the board read
    // as an ARGUMENT: hand it a constant and it runs the strings with no board
    // traffic at all.
    println!("\n-- where the time goes, at {} voices --", SPLIT_VOICES);
    {
        let mut board = Soundboard::new(sr, 0.25);
        let notes: Vec<u8> = (0..SPLIT_VOICES).map(|i| 48 + (i as u8) * 2).collect();
        let attach: Vec<std::sync::Arc<Vec<f64>>> =
            notes.iter().map(|&n| board.attachment_shared(n)).collect();

        // 1. Board advance alone, exactly as the engine calls it.
        board.drive_bridge(1.0);
        let t = Instant::now();
        let mut sink = 0.0f64;
        for _ in 0..n {
            let (l, r) = board.advance();
            sink += l + r;
        }
        let t_board = t.elapsed().as_secs_f64();

        // 2. Per-voice coupling: the read and the drive, no strings.
        let t = Instant::now();
        for _ in 0..n {
            for a in &attach {
                let (y, c) = board.read_and_compliance_at(a);
                sink += y + c;
                board.drive_at(a, 1e-12);
            }
        }
        let t_couple = t.elapsed().as_secs_f64() - t_board;

        // 3. The strings alone, fed a constant board.
        let mut voices: Vec<Voice> = notes
            .iter()
            .map(|&nn| {
                let mut v = Voice::default();
                v.start(nn, 3.0, 2.5, 0.5, 0.5, sr);
                v
            })
            .collect();
        // Past the hammer, into the ring: contact is a thousandth of a note.
        for _ in 0..(sr as usize / 20) {
            for v in voices.iter_mut() {
                sink += v.tick(0.0, 0.0);
            }
        }
        let t = Instant::now();
        for _ in 0..n {
            for v in voices.iter_mut() {
                sink += v.tick(0.0, 0.0);
            }
        }
        let t_strings = t.elapsed().as_secs_f64();

        let total = t_board + t_couple + t_strings;
        let pc = |x: f64| 100.0 * x / total;
        println!("  board advance : {:6.2}s  {:4.1}%   (fixed, whatever the polyphony)", t_board, pc(t_board));
        println!("  voice coupling: {:6.2}s  {:4.1}%   ({:.3}s per voice)", t_couple, pc(t_couple), t_couple / SPLIT_VOICES as f64);
        println!("  string banks  : {:6.2}s  {:4.1}%   ({:.3}s per voice)", t_strings, pc(t_strings), t_strings / SPLIT_VOICES as f64);
        println!("  (for {:.1}s of audio; sink {:.3})", secs, sink);
    }

    // ── How many of the plate's modes does one note actually couple to? ────
    //
    // Every voice reads all 3618 of them every sample, whatever its pitch. If
    // a note's weight is concentrated in a fraction of them, the rest could be
    // dropped from ITS array and the coupling would shrink in proportion. This
    // says whether that prize exists before anyone goes after it.
    {
        println!("\n-- concentration of a note's coupling weight --");
        let board = Soundboard::new(sr, 0.25);
        let total_modes = board.len();
        for note in [28u8, 48, 69, 96] {
            let a = board.attachment(note);
            let mut e: Vec<f64> = a.iter().map(|w| w * w).collect();
            let all: f64 = e.iter().sum();
            e.sort_by(|x, y| y.partial_cmp(x).unwrap());
            let mut acc = 0.0;
            let (mut n90, mut n99, mut n999) = (0usize, 0usize, 0usize);
            for (i, v) in e.iter().enumerate() {
                acc += v;
                if n90 == 0 && acc >= 0.90 * all { n90 = i + 1; }
                if n99 == 0 && acc >= 0.99 * all { n99 = i + 1; }
                if n999 == 0 && acc >= 0.999 * all { n999 = i + 1; }
            }
            println!(
                "  note {:3}: 90% of the weight in {:4} modes, 99% in {:4}, 99.9% in {:4}  (of {})",
                note, n90, n99, n999, total_modes
            );
        }
    }

    // ── How many cores are worth using? ────────────────────────────────────
    //
    // Not obvious on a machine whose cores are not alike: this one has two at
    // 4.9 GHz, eight at 3.8 and two at 2.1, and a barrier makes everyone wait
    // for the slowest. More threads is not automatically better.
    println!("\n-- workers, at 31 voices --");
    {
        for &w in &[0usize, 2, 4, 6, 8, 10, 12] {
            let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
            eng.set_workers(w);
            for i in 0..31 {
                tx.send(PianoCommand::NoteOn(48 + (i as u8) * 2, 90)).ok();
            }
            let block = 128usize;
            let mut buf = vec![0.0f32; block * 2];
            for _ in 0..(sr as usize / block) {
                eng.process_audio(&mut buf, 2);
            }
            let blocks = (sr as f64 * 2.0) as usize / block;
            let t = Instant::now();
            for _ in 0..blocks {
                eng.process_audio(&mut buf, 2);
            }
            let el = t.elapsed().as_secs_f64();
            let audio = (blocks * block) as f64 / sr as f64;
            println!("  {:2} workers: {:.2}x realtime", w, audio / el);
        }
    }

    // ── How long does a damped string go on being computed? ────────────────
    //
    // A glissando is fifty note-ons and fifty note-offs in two seconds, and
    // every one of those strings keeps costing its full coupling until the
    // engine decides it is over. If that decision comes long after the note is
    // inaudible, the cost of a glissando is set by the retirement rule and not
    // by the music.
    println!("\n-- a damped note, after the key comes up --");
    {
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
        let block = 128usize;
        let mut buf = vec![0.0f32; block * 2];
        let _ = tx.send(PianoCommand::NoteOn(60, 100));
        let mut peak_struck = 0.0f32;
        for _ in 0..(sr as usize / block / 4) {
            eng.process_audio(&mut buf, 2);
            for v in buf.iter() { peak_struck = peak_struck.max(v.abs()); }
        }
        let _ = tx.send(PianoCommand::NoteOff(60));
        let mut audible_for = None;
        let mut alive_for = 0.0;
        for i in 0..(sr as usize / block * 6) {
            let mut peak = 0.0f32;
            eng.process_audio(&mut buf, 2);
            for v in buf.iter() { peak = peak.max(v.abs()); }
            let t = (i * block) as f64 / sr as f64;
            let db = 20.0 * (peak / peak_struck).max(1e-30).log10();
            if audible_for.is_none() && db < -60.0 {
                audible_for = Some(t);
            }
            if eng.mode_load().0 > 0 {
                alive_for = t;
            }
        }
        println!(
            "  falls 60 dB in {:.2}s, still computed at {:.2}s  ->  {:.0}% of its life inaudible",
            audible_for.unwrap_or(f64::NAN),
            alive_for,
            100.0 * (1.0 - audible_for.unwrap_or(alive_for) / alive_for.max(1e-9))
        );
    }

    // ── The three scenarios that decide whether this holds ─────────────────
    //
    // Not averages: a dropout is a single block that went over its own time,
    // and a note lost is a single string the governor took away. So both are
    // counted, at the block size a DAW asks for AND at the small one that
    // leaves no slack, on the three things that actually cost:
    //
    //   * a mouse dragged across the keyboard, no pedal — thirty strikes in a
    //     second and a half, each released as the pointer moves on;
    //   * the same with the pedal down, which wakes the whole compass;
    //   * a pedalled chord held, which is the steady state underneath both.
    //
    // The target is zero and zero. The governor is a safety net, and a net
    // that catches something is a net that was needed.
    // How many voices should one participant be given? Six was a first guess
    // that only had to avoid waking nine threads for eight voices; at thirty it
    // caps the pool at five participants when the machine has nine.
    //
    // ── Read this before trusting a number below it ───────────────────────
    //
    // Swept one way this printed 148, 183, 187, 129, 117, 104 overruns; swept
    // back it printed 108, 103, 50, 10, 21, 49. The trend does not follow the
    // setting, it follows the RANK OF THE TRIAL — every value improves as the
    // sweep goes on, from both ends. A first reading of the one-way sweep said
    // "two voices per participant is four times better"; that was the warm-up
    // talking (frequency ramp, page faults, the pool's first wake), not the
    // setting.
    //
    // So: throw a warm-up away first, then measure every setting three times
    // ROUND-ROBIN, and report the median. Interleaving spreads any residual
    // drift across all the settings instead of handing it to whichever ran
    // first. If the medians still sit within each other's spread, the sweep's
    // answer is "this knob does nothing here", and that is a real answer.
    println!("\n-- voix par participant, sur le glisse a 1024 --");
    let glissando = |vpp: usize| -> (f64, usize, usize) {
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
        eng.set_governor(true);
        eng.set_voices_per_participant(vpp);
        let block = 1024usize;
        let mut buf = vec![0.0f32; block * 2];
        let budget_ms = 1000.0 * block as f64 / sr as f64;
        let blocks = (sr as f64 * 4.0) as usize / block;
        let (mut worst, mut over) = (0.0f64, 0usize);
        let mut next = 0usize;
        for b in 0..blocks {
            let t = (b * block) as f64 / sr as f64;
            while next < 30 && (next as f64) * 0.05 <= t {
                let _ = tx.send(PianoCommand::NoteOn(48 + next as u8, 90));
                if next > 0 {
                    let _ = tx.send(PianoCommand::NoteOff(48 + next as u8 - 1));
                }
                next += 1;
            }
            let s = Instant::now();
            eng.process_audio(&mut buf, 2);
            let ms = s.elapsed().as_secs_f64() * 1000.0;
            worst = worst.max(ms);
            if ms > budget_ms {
                over += 1;
            }
        }
        (worst, over, blocks)
    };

    const VPPS: [usize; 6] = [1, 2, 3, 4, 6, 10];
    const REPS: usize = 3;
    for _ in 0..2 {
        let _ = glissando(6); // thrown away: this is the warm-up
    }
    let mut runs: Vec<Vec<(f64, usize)>> = vec![Vec::new(); VPPS.len()];
    let mut blocks = 0usize;
    for _ in 0..REPS {
        for (i, &vpp) in VPPS.iter().enumerate() {
            let (worst, over, n) = glissando(vpp);
            runs[i].push((worst, over));
            blocks = n;
        }
    }
    for (i, &vpp) in VPPS.iter().enumerate() {
        let mut w: Vec<f64> = runs[i].iter().map(|r| r.0).collect();
        let mut o: Vec<usize> = runs[i].iter().map(|r| r.1).collect();
        w.sort_by(|a, b| a.partial_cmp(b).unwrap());
        o.sort_unstable();
        println!(
            "   {vpp:3} voix/participant : pire {:7.2} ms (min {:7.2}, max {:7.2}), \
             depassements median {:4} sur {blocks}  [{:?}]",
            w[REPS / 2],
            w[0],
            w[REPS - 1],
            o[REPS / 2],
            o
        );
    }

    println!("\n== les trois scenarios ==");
    println!("   scenario              bloc   pire bloc / budget   depassements   voix perdues");
    for (name, pedal, chord) in [
        ("glisse, sans pedale", false, false),
        ("glisse, pedale      ", true, false),
        ("accord pedale tenu  ", true, true),
    ] {
        for &block in &[128usize, 1024] {
            let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
            eng.set_governor(true);
            let mut buf = vec![0.0f32; block * 2];
            let budget_ms = 1000.0 * block as f64 / sr as f64;
            if pedal {
                let _ = tx.send(PianoCommand::SustainPedal(true));
            }
            let secs = 6.0;
            let blocks = (sr as f64 * secs) as usize / block;
            let (mut worst, mut over) = (0.0f64, 0usize);
            let mut next = 0usize;
            for b in 0..blocks {
                let t = (b * block) as f64 / sr as f64;
                if chord {
                    // Struck once, then held: the cost that stays.
                    if b == 0 {
                        for n in [40u8, 47, 52, 56, 59, 64] {
                            let _ = tx.send(PianoCommand::NoteOn(n, 100));
                        }
                    }
                } else {
                    while next < 30 && (next as f64) * 0.05 <= t {
                        let _ = tx.send(PianoCommand::NoteOn(48 + next as u8, 90));
                        if next > 0 {
                            let _ = tx.send(PianoCommand::NoteOff(48 + next as u8 - 1));
                        }
                        next += 1;
                    }
                }
                let s = Instant::now();
                eng.process_audio(&mut buf, 2);
                let ms = s.elapsed().as_secs_f64() * 1000.0;
                worst = worst.max(ms);
                if ms > budget_ms {
                    over += 1;
                }
            }
            println!(
                "   {name}  {block:5}   {:6.2} / {:5.2} ms      {:4} / {:<4}     {:5}",
                worst, budget_ms, over, blocks, eng.sheds_total()
            );
        }
    }

    // ── The pedal storm: waking eighty-seven strings at once ───────────────
    //
    // Pedal down, then a note: every other string in the compass is woken
    // sympathetically. Two different costs hide in that one callback — BUILDING
    // the voices, and then PROCESSING them — and only the first is a spike. The
    // block right after the wake pays both; a later block with the same voices
    // still ringing pays only the second, so the difference is what waking
    // actually costs.
    {
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
        let block = 128usize;
        let mut buf = vec![0.0f32; block * 2];
        let budget_ms = 1000.0 * block as f64 / sr as f64;
        tx.send(PianoCommand::SustainPedal(true)).ok();
        for _ in 0..8 {
            eng.process_audio(&mut buf, 2);
        }

        tx.send(PianoCommand::NoteOn(48, 90)).ok();
        let t = Instant::now();
        eng.process_audio(&mut buf, 2);
        let wake = t.elapsed().as_secs_f64() * 1000.0;

        // Same voices, no new ones: pure steady-state cost at that polyphony.
        let mut steady: f64 = 0.0;
        for _ in 0..32 {
            let t = Instant::now();
            eng.process_audio(&mut buf, 2);
            steady = steady.max(t.elapsed().as_secs_f64() * 1000.0);
        }
        println!("\n-- pedal storm (128-frame budget is {:.2} ms) --", budget_ms);
        println!("  block that wakes them : {:7.2} ms  ({:5.1}x the budget)", wake, wake / budget_ms);
        println!("  worst block after     : {:7.2} ms  ({:5.1}x the budget)", steady, steady / budget_ms);
        println!("  so the waking itself  : {:7.2} ms", (wake - steady).max(0.0));
    }

    // ── Full engine at several polyphonies ─────────────────────────────────
    let preset = factory_presets().into_iter().find(|p| p.name.contains("Close Mics")).unwrap();
    for &k in &[1usize, 8, 16, 31] {
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(sr);
        tx.send(PianoCommand::LoadPatch(Box::new(preset.clone()))).ok();
        // hold k notes across the tenor/treble
        for i in 0..k { tx.send(PianoCommand::NoteOn(48 + (i as u8) * 2, 90)).ok(); }
        let block = 128usize;
        let mut buf = vec![0.0f32; block * 2];
        // warm up (let the attack pass) then time the sustain
        for _ in 0..(sr as usize / block) { eng.process_audio(&mut buf, 2); }
        let blocks = (sr as f64 * secs) as usize / block;
        let t = Instant::now();
        for _ in 0..blocks { eng.process_audio(&mut buf, 2); }
        let el = t.elapsed().as_secs_f64();
        let audio = (blocks * block) as f64 / sr as f64;
        let (live, all) = eng.mode_load();
        println!("engine {:2} voices: {:.2}x realtime ({:.0} ms dsp) (string modes live {}/{}, board on top)",
            k, audio / el, el*1000.0, live, all);
    }

}

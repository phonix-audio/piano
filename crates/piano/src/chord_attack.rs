//! Pourquoi un gros accord ne claque pas.
//!
//! The user's report, and it is specific: single notes are fine, but a big
//! forte chord "sonne comme un orgue" — it blooms instead of striking. The
//! decay is not the complaint; the ONSET is.
//!
//! One note and a five-note chord, rendered dry — no room, no master, nothing
//! downstream that could smooth an attack — at the same velocity, so the two
//! envelopes can be laid side by side. If the single note strikes and the chord
//! swells, the fault is something that only appears when several strings drive
//! the same plate at once, and that is a short list.
//!
//!   cargo test -p piano --lib render_the_chord_attack -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::engine::{PianoCommand, PianoEngine};
    use crate::patch::factory_presets;

    const SR: f32 = 48_000.0;

    fn render(notes: &[u8], secs: f64) -> Vec<f32> {
        let preset = factory_presets()
            .into_iter()
            .find(|p| p.name.contains("Close Mics"))
            .expect("the Close Mics preset");
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
        let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
        eng.set_workers(0);

        // Struck on the first block, so the onset sits at a known sample.
        for &n in notes {
            let _ = tx.send(PianoCommand::NoteOn(n, 112));
        }
        let block = 64usize;
        let blocks = (SR as f64 * secs) as usize / block;
        let mut out = Vec::with_capacity(blocks * block * 2);
        let mut buf = vec![0.0f32; block * 2];
        for _ in 0..blocks {
            buf.iter_mut().for_each(|x| *x = 0.0);
            eng.process_audio(&mut buf, 2);
            out.extend_from_slice(&buf);
        }
        out
    }

    fn write(name: &str, audio: &[f32]) {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../renders/attaque");
        std::fs::create_dir_all(dir).expect("the output directory");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let path = format!("{dir}/{name}.wav");
        let mut w = hound::WavWriter::create(&path, spec).expect("wav");
        for &s in audio {
            w.write_sample(s).expect("sample");
        }
        w.finalize().expect("finalize");
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        eprintln!("  -> {path}  (crête {peak:.4})");
    }

    #[test]
    #[ignore = "renders for analysis"]
    fn render_the_chord_attack() {
        // The chord the piece actually plays under the complaint, and each of
        // its notes alone at the same velocity. Rendered float, undithered:
        // the attack is the subject and quantisation is not going to blur it.
        const CHORD: [u8; 5] = [40, 47, 52, 56, 59];
        write("accord", &render(&CHORD, 2.5));
        for n in CHORD {
            write(&format!("seule_{n}"), &render(&[n], 2.5));
        }
        // The sum of the notes played SEPARATELY. If the chord and this sum
        // have different onsets, the difference is what happens when five
        // strings share one plate — the plate is the only thing they share.
        let mut sum = render(&[CHORD[0]], 2.5);
        for &n in &CHORD[1..] {
            for (a, b) in sum.iter_mut().zip(render(&[n], 2.5)) {
                *a += b;
            }
        }
        write("somme_des_notes_seules", &sum);
    }

    /// What the plate's low-frequency damping costs, and what it buys.
    ///
    /// `BOARD_RATE_LOW` is zero, and the comment above it spends twenty lines
    /// explaining why it should not be — including the exact symptom the user
    /// reports: "the instrument's output then peaks a TENTH OF A SECOND after
    /// the hammer has struck". It was zeroed deliberately, on the argument that
    /// Ege's ~80 s⁻¹ is a MEAN over a band whose modes crowd towards its top, so
    /// no floor is needed, and that imposing one "emptied the instrument".
    ///
    /// Both halves of that are testable, and this tests them together, because
    /// they are a trade and neither number means anything alone:
    ///
    /// - **what it fixes**: how long after the strike the output peaks;
    /// - **what it costs**: the level, over the first second and in the tail.
    ///
    /// Printed per note across the compass, since the whole dispute is about
    /// the bass — the mean over the band was never the thing in question.
    #[test]
    #[ignore = "sweep for calibration"]
    fn print_what_the_plate_damping_trades() {
        use crate::soundboard::{Soundboard, RATE_LOW_OVERRIDE};
        use crate::voice::Voice;
        use std::sync::atomic::Ordering;

        let run = |note: u8, floor: u32| -> (f64, f64, f64) {
            RATE_LOW_OVERRIDE.store(floor, Ordering::Relaxed);
            let mut board = Soundboard::new(SR, 1.0);
            let mut v = Voice::default();
            v.start(note, 4.0, 1.0, 0.5, 0.5, SR);
            let n = (SR as f64 * 1.5) as usize;
            let mut x = Vec::with_capacity(n);
            let mut bridge_y = 0.0;
            for _ in 0..n {
                let f = v.tick(bridge_y, board.compliance_at(&v.attach));
                board.drive_bridge(f);
                let (l, _r) = board.process();
                bridge_y = board.bridge_displacement();
                x.push(l);
            }
            RATE_LOW_OVERRIDE.store(u32::MAX, Ordering::Relaxed);
            // The FUNDAMENTAL's own envelope, by complex demodulation: multiply
            // by e^(-iω₀t) and low-pass, and what is left is that partial's
            // amplitude against time. A broadband RMS envelope will not do here
            // — this note's envelope sits within a few dB of its peak for two
            // hundred milliseconds, so `argmax` over it jumps between local
            // maxima and reads as noise.
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let k = std::f64::consts::TAU * f0 / SR as f64;
            let a = (-std::f64::consts::TAU * 25.0 / SR as f64).exp();
            let (mut ri, mut ii) = (0.0f64, 0.0f64);
            let mut amp: Vec<f64> = Vec::with_capacity(x.len());
            for (i, &s) in x.iter().enumerate() {
                let th = k * i as f64;
                ri = (1.0 - a) * s * th.cos() + a * ri;
                ii = (1.0 - a) * -s * th.sin() + a * ii;
                amp.push((ri * ri + ii * ii).sqrt());
            }
            let ipk = amp
                .iter()
                .enumerate()
                .max_by(|p, q| p.1.partial_cmp(q.1).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);
            let db = |v: f64| 20.0 * v.max(1e-30).log10();
            // How far below its own peak the partial still is five milliseconds
            // after the strike. A note that strikes is already there; a note
            // that swells is a long way down.
            let at5 = db(amp[(SR as usize) / 200]) - db(amp[ipk]);
            let first = x[..x.len() / 3].iter().map(|v| v * v).sum::<f64>()
                / (x.len() / 3) as f64;
            (ipk as f64 * 1000.0 / SR as f64, db(first.sqrt()), at5)
        };

        eprintln!("\n  plancher d'amortissement grave de la table (s⁻¹)");
        eprintln!("  fondamentale : quand elle culmine / ou elle en est a 5 ms / niveau");
        for note in [28u8, 40, 47, 52, 59, 67] {
            eprintln!("  -- note {note} --");
            for floor in [u32::MAX, 15, 30, 45, 60, 80, 120] {
                let (tpk, lvl, at5) = run(note, floor);
                let tag = if floor == u32::MAX {
                    "  actuel".to_string()
                } else {
                    format!("{floor:4} s-1")
                };
                eprintln!("     {tag} : pic a {tpk:6.1} ms, a 5 ms {at5:+6.1} dB, niveau {lvl:6.1} dBFS");
            }
        }
        eprintln!("\n  un vrai piano culmine en quelques millisecondes : on cherche");
        eprintln!("  un pic tot ET un '5 ms' proche de zero, au moindre cout en niveau.");
    }

    /// Did the damping floor make the instrument worse, or did it move the peak
    /// the old tests measure from?
    ///
    /// Two tests went red with the floor: `low_notes_ring_far_longer_than_high_
    /// ones` (peak minus final level) and `a_unison_beats_and_a_perfect_one_does_
    /// not` (RMS deviation of the envelope from a straight line). Both are
    /// anchored on the PEAK or on a straight-line fit, and the floor's whole
    /// effect is to move the peak earlier and to bend the curve — so both could
    /// go red without the instrument being any worse.
    ///
    /// That is a comfortable story, so it gets measured instead of told. Beside
    /// each test's own figure this prints one that cannot be fooled the same way:
    /// the decay taken between 0.3 s and 2 s, well after any attack, and a ripple
    /// taken against a QUADRATIC fit, which absorbs the two-stage bend a straight
    /// line reads as wobble.
    #[test]
    #[ignore = "diagnosis"]
    fn print_whether_the_floor_hurt_or_moved_the_ruler() {
        use crate::soundboard::{Soundboard, RATE_LOW_OVERRIDE};
        use crate::voice::Voice;
        use std::sync::atomic::Ordering;

        let env = |note: u8, detune: f64, secs: f64| -> Vec<f64> {
            let mut board = Soundboard::new(SR, 1.0);
            let mut v = Voice::default();
            v.start(note, 2.0, detune, 0.5, 0.5, SR);
            let n = (SR as f64 * secs) as usize;
            let mut x = Vec::with_capacity(n);
            let mut bridge_y = 0.0;
            for _ in 0..n {
                let f = v.tick(bridge_y, board.compliance_at(&v.attach));
                board.drive_bridge(f);
                let (l, _r) = board.process();
                bridge_y = board.bridge_displacement();
                x.push(l);
            }
            let w = (SR as usize) / 100;
            x.chunks(w)
                .map(|c| {
                    let r = (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt();
                    20.0 * r.max(1e-30).log10()
                })
                .collect()
        };

        for (label, floor) in [("sans plancher", 0u32), ("avec plancher", 80)] {
            RATE_LOW_OVERRIDE.store(floor, Ordering::Relaxed);
            eprintln!("\n  == {label} ==");
            for note in [33u8, 93] {
                let e = env(note, 2.0, 2.0);
                let peak = e.iter().cloned().fold(f64::MIN, f64::max);
                let from_peak = peak - e[e.len() - 1];
                // 0.3 s to 2 s, in 10 ms frames: index 30 to the end.
                let late = (e[30] - e[e.len() - 1]) / 1.7;
                eprintln!(
                    "     note {note:3} : pic-fin {from_peak:5.1} dB (metrique du test), \
                     pente 0,3-2 s {late:5.1} dB/s"
                );
            }
            for detune in [5.0f64, 0.0] {
                let e = env(60, detune, 3.0);
                let seg = &e[e.len() / 4..];
                let n = seg.len() as f64;
                // Centred and scaled: see `quad_fit`.
                let t: Vec<f64> = (0..seg.len())
                    .map(|i| 2.0 * i as f64 / (n - 1.0) - 1.0)
                    .collect();
                // Straight line, as the test fits it.
                let raw: Vec<f64> = (0..seg.len()).map(|i| i as f64).collect();
                let (sx, sy): (f64, f64) = (raw.iter().sum(), seg.iter().sum());
                let sxx: f64 = raw.iter().map(|v| v * v).sum();
                let sxy: f64 = raw.iter().zip(seg).map(|(a, b)| a * b).sum();
                let m = (n * sxy - sx * sy) / (n * sxx - sx * sx);
                let c = (sy - m * sx) / n;
                let lin = (seg
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (v - (m * i as f64 + c)).powi(2))
                    .sum::<f64>()
                    / n)
                    .sqrt();
                // And against a quadratic, which absorbs the bend.
                let q = quad_fit(&t, seg);
                let curved = (seg
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = t[i];
                        (v - (q.0 * x * x + q.1 * x + q.2)).powi(2)
                    })
                    .sum::<f64>()
                    / n)
                    .sqrt();
                eprintln!(
                    "     desaccord {detune:3.0} cents : ondulation/droite {lin:.2} dB, \
                     /parabole {curved:.2} dB"
                );
            }
        }
        RATE_LOW_OVERRIDE.store(u32::MAX, Ordering::Relaxed);
    }

    /// Least squares against `a·x² + b·x + c`, by normal equations.
    ///
    /// `t` MUST be centred and scaled to about [-1, 1] before it gets here. Fed
    /// raw frame indices the normal equations carry `x⁴` terms in the billions
    /// and Cramer's rule returns nonsense — it printed ripples of 120 to 198 dB
    /// on an envelope that spans thirty, which is how this was caught.
    #[cfg(test)]
    fn quad_fit(t: &[f64], y: &[f64]) -> (f64, f64, f64) {
        let n = t.len() as f64;
        let (mut s1, mut s2, mut s3, mut s4) = (0.0, 0.0, 0.0, 0.0);
        let (mut y0, mut y1, mut y2) = (0.0, 0.0, 0.0);
        for (x, v) in t.iter().zip(y) {
            let (x2, x3, x4) = (x * x, x * x * x, x * x * x * x);
            s1 += x;
            s2 += x2;
            s3 += x3;
            s4 += x4;
            y0 += v;
            y1 += x * v;
            y2 += x2 * v;
        }
        // Solve the 3x3 normal system by Cramer's rule.
        let d = s4 * (s2 * n - s1 * s1) - s3 * (s3 * n - s1 * s2) + s2 * (s3 * s1 - s2 * s2);
        let da = y2 * (s2 * n - s1 * s1) - s3 * (y1 * n - y0 * s1) + s2 * (y1 * s1 - y0 * s2);
        let db = s4 * (y1 * n - y0 * s1) - y2 * (s3 * n - s1 * s2) + s2 * (s3 * y0 - y1 * s2);
        let dc = s4 * (s2 * y0 - s1 * y1) - s3 * (s3 * y0 - s1 * y2) + y2 * (s3 * s1 - s2 * s2);
        (da / d, db / d, dc / d)
    }

    /// A struck note reaches its peak in milliseconds, not in a tenth of a
    /// second.
    ///
    /// The regression guard that was missing. The plate's damping was licensed
    /// by a test that averaged over every mode below 1.2 kHz, which passed while
    /// the plate rose in 50 to 100 ms under every note and the instrument was
    /// heard as an organ. A mean over modes cannot see that; the observable can,
    /// and this is the observable.
    ///
    /// E2 peaked at 55 ms and E3 at 109 before `BOARD_RATE_LOW` was restored,
    /// and at 9 and 16 after. Forty-five milliseconds sits well clear of both.
    #[test]
    fn a_struck_note_reaches_its_peak_in_milliseconds() {
        use crate::soundboard::Soundboard;
        use crate::voice::Voice;
        for note in [40u8, 52] {
            let mut board = Soundboard::new(SR, 1.0);
            let mut v = Voice::default();
            v.start(note, 4.0, 1.0, 0.5, 0.5, SR);
            let n = (SR as f64 * 0.5) as usize;
            let mut x = Vec::with_capacity(n);
            let mut bridge_y = 0.0;
            for _ in 0..n {
                let f = v.tick(bridge_y, board.compliance_at(&v.attach));
                board.drive_bridge(f);
                let (l, _r) = board.process();
                bridge_y = board.bridge_displacement();
                x.push(l);
            }
            // The fundamental's own envelope, by complex demodulation. A
            // broadband RMS envelope will not answer this: it stays within a few
            // dB of its peak for two hundred milliseconds, so its argmax is
            // noise. See `print_what_the_plate_damping_trades`.
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let k = std::f64::consts::TAU * f0 / SR as f64;
            let a = (-std::f64::consts::TAU * 25.0 / SR as f64).exp();
            let (mut ri, mut ii, mut best, mut ipk) = (0.0f64, 0.0f64, 0.0f64, 0usize);
            for (i, &s) in x.iter().enumerate() {
                let th = k * i as f64;
                ri = (1.0 - a) * s * th.cos() + a * ri;
                ii = (1.0 - a) * -s * th.sin() + a * ii;
                let amp = ri * ri + ii * ii;
                if amp > best {
                    best = amp;
                    ipk = i;
                }
            }
            let ms = ipk as f64 * 1000.0 / SR as f64;
            assert!(
                ms < 45.0,
                "note {note} takes {ms:.0} ms to reach its peak; a struck string does not \
                 swell — check the plate's low-frequency damping"
            );
        }
    }

    /// How much level the damping floor costs, over material and not one note.
    ///
    /// `BOARD_RATE_LOW` was adopted on 2026-08-23 and the plate now takes energy
    /// faster, so the instrument is quieter. The make-up has to be MEASURED, and
    /// measured over more than one note: the cost runs 0.4 dB at B3 to 2.5 at E0,
    /// and picking either end would be choosing a number. This averages the
    /// energy ratio over the compass and over the two pieces of material the A/B
    /// used, and prints what `PLATE_DAMPING_MAKEUP` should hold.
    #[test]
    #[ignore = "measures a constant"]
    fn print_the_makeup_the_damping_floor_needs() {
        use crate::soundboard::RATE_LOW_OVERRIDE;
        use std::sync::atomic::Ordering;

        let rms = |x: &[f32]| -> f64 {
            (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt()
        };
        // Measured against an UNDAMPED plate, so the floor being live in the
        // constant does not fold into its own compensation.
        let mut ratios = Vec::new();
        for notes in [
            vec![28u8], vec![40], vec![47], vec![52], vec![59], vec![67], vec![76], vec![88],
            vec![40, 47, 52, 56, 59],
        ] {
            RATE_LOW_OVERRIDE.store(0, Ordering::Relaxed);
            let dry = render(&notes, 2.0);
            RATE_LOW_OVERRIDE.store(80, Ordering::Relaxed);
            let damped = render(&notes, 2.0);
            RATE_LOW_OVERRIDE.store(u32::MAX, Ordering::Relaxed);
            let r = rms(&dry) / rms(&damped).max(1e-30);
            eprintln!("  {notes:?} : {:+.2} dB", 20.0 * r.log10());
            ratios.push(r);
        }
        // Averaged in energy, which is what a level is.
        let mean = (ratios.iter().map(|r| r * r).sum::<f64>() / ratios.len() as f64).sqrt();
        eprintln!(
            "\n  make-up = {mean:.4}  ({:+.2} dB)",
            20.0 * mean.log10()
        );
    }

    /// The plate's low-frequency damping, for the ear.
    ///
    /// Measured, a floor of 80 s⁻¹ — Ege's published mean rate below 1.2 kHz —
    /// brings the fundamental's peak from 109 ms to 16 ms at E3 and from 55 to 9
    /// at E2, and costs between 0.4 and 2.5 dB of level. The level is made up
    /// here so that what differs between the files is the ATTACK and not the
    /// loudness; the figure used is stated in the output, and it is the measured
    /// difference in RMS over the whole take, nothing chosen.
    ///
    /// Chord and phrase, dry — no room, no master.
    #[test]
    #[ignore = "renders for listening"]
    fn render_the_plate_damping_ab() {
        use crate::soundboard::RATE_LOW_OVERRIDE;
        use std::sync::atomic::Ordering;

        const CHORD: [u8; 5] = [40, 47, 52, 56, 59];
        let phrase: Vec<(f64, PianoCommand)> = {
            let mut v = Vec::new();
            for (i, n) in [52u8, 59, 64, 67, 71, 76].into_iter().enumerate() {
                let t = 0.05 + i as f64 * 0.45;
                v.push((t, PianoCommand::NoteOn(n, 108)));
                v.push((t + 0.38, PianoCommand::NoteOff(n)));
            }
            v
        };

        let scored = |secs: f64, score: &[(f64, PianoCommand)]| -> Vec<f32> {
            let preset = factory_presets()
                .into_iter()
                .find(|p| p.name.contains("Close Mics"))
                .expect("the Close Mics preset");
            let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
            let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
            eng.set_workers(0);
            let block = 128usize;
            let blocks = (SR as f64 * secs) as usize / block;
            let mut out = Vec::with_capacity(blocks * block * 2);
            let mut buf = vec![0.0f32; block * 2];
            let mut next = 0usize;
            for b in 0..blocks {
                let t = (b * block) as f64 / SR as f64;
                while next < score.len() && score[next].0 <= t {
                    let _ = tx.send(score[next].1.clone());
                    next += 1;
                }
                buf.iter_mut().for_each(|x| *x = 0.0);
                eng.process_audio(&mut buf, 2);
                out.extend_from_slice(&buf);
            }
            out
        };

        let chord_score: Vec<(f64, PianoCommand)> = CHORD
            .iter()
            .map(|&n| (0.02, PianoCommand::NoteOn(n, 112)))
            .collect();

        for (name, secs, score) in [
            ("accord", 4.0, &chord_score),
            ("phrase", 4.0, &phrase),
        ] {
            RATE_LOW_OVERRIDE.store(u32::MAX, Ordering::Relaxed);
            let now = scored(secs, score);
            RATE_LOW_OVERRIDE.store(80, Ordering::Relaxed);
            let damped = scored(secs, score);
            RATE_LOW_OVERRIDE.store(u32::MAX, Ordering::Relaxed);

            let rms = |x: &[f32]| -> f64 {
                (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt()
            };
            let g = (rms(&now) / rms(&damped).max(1e-30)) as f32;
            eprintln!("  {name}: le plancher coute {:.2} dB, rendus a niveau egal", 20.0 * g.log10());
            let lifted: Vec<f32> = damped.iter().map(|s| s * g).collect();
            write(&format!("{name}_actuel"), &now);
            write(&format!("{name}_table_amortie"), &lifted);
        }
    }

    /// The forte chord UNDER THE PEDAL — the square the questionnaire missed.
    ///
    /// The user hears the organ in maree_piano_nu.wav, the DRY render: the
    /// room is not the whole story. Yet the blind questionnaire judged the
    /// same chord "piano" — WITHOUT the pedal — and judged pedalled SINGLE
    /// notes near-clean. The organ lives at the intersection nobody isolated:
    /// a big chord held under the pedal, which is how Marée plays every one
    /// of its chords (the pedal holds through whole bars).
    ///
    /// Two things happen only there, and these renders pull them apart:
    ///
    ///   accord_sec        no pedal — the questionnaire's stimulus, the control
    ///   accord_pedale     the piece's actual condition
    ///   accord_pedale_sans_halo   pedal, shared sympathetic bank muted
    ///   accord_pedale_cordes      pedal, the per-string physical model instead
    ///
    /// If the organ appears with the pedal and vanishes without the halo, the
    /// shared bank is the culprit — eighty-eight resonators driven by the
    /// output SUM ring like pipes when the sum is a forte chord. If it stays
    /// without the halo, it is the undamped strings' own held ring (and the
    /// plate's mid bloom). The per-string render is the physical reference.
    /// The same square on the chord the user actually pointed at.
    ///
    /// "les accords proposés ne correspondent pas à ceux qui commencent vers
    /// 34 s, il manque les notes aiguës, et c'est justement là le problème."
    /// Bar 17 of Marée (~33.9 s at 84 BPM) strikes D5 A5 D6 A6 D7 at 112 over
    /// a D1+D2 bass — an OCTAVE STACK planted in the very zone the blind
    /// questionnaire called "synthétique" (notes >= 73), whose tails are
    /// quasi-sines. Stacked in octaves of D they are literally organ pipes.
    /// (The questionnaire's one "orgue" verdict was the stacked-octaves
    /// stimulus; it all agrees.)
    #[test]
    #[ignore = "renders for listening"]
    fn render_the_crest_chord_square() {
        render_square("crete", &[26, 38, 74, 81, 86, 93, 98], 112);
    }

    #[test]
    #[ignore = "renders for listening"]
    fn render_the_pedalled_chord_square() {
        render_square("carre", &[40, 47, 52, 56, 59], 112);
    }

    fn render_square(tag: &str, chord: &[u8], vel: u8) {
        let render = |pedal: bool, halo_gain: Option<f32>, per_string: bool| -> Vec<f32> {
            let _model = per_string.then(crate::voice::PerStringModel::enter);
            let preset = factory_presets()
                .into_iter()
                .find(|p| p.name.contains("Close Mics"))
                .expect("the Close Mics preset");
            let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
            let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
            eng.set_workers(0);
            if let Some(g) = halo_gain {
                eng.set_sympath_gain(g);
            }
            if pedal {
                let _ = tx.send(PianoCommand::SustainPedal(true));
            }
            for &n in chord {
                let _ = tx.send(PianoCommand::NoteOn(n, vel));
            }
            // Held three seconds then released — under the pedal the release
            // changes nothing, which is the point.
            let block = 128usize;
            let secs = 8.0;
            let blocks = (SR as f64 * secs) as usize / block;
            let off_at = (SR as f64 * 3.0) as usize / block;
            let mut out = Vec::with_capacity(blocks * block * 2);
            let mut buf = vec![0.0f32; block * 2];
            for b in 0..blocks {
                if b == off_at {
                    for &n in chord {
                        let _ = tx.send(PianoCommand::NoteOff(n));
                    }
                }
                buf.iter_mut().for_each(|x| *x = 0.0);
                eng.process_audio(&mut buf, 2);
                out.extend_from_slice(&buf);
            }
            out
        };

        write(&format!("{tag}_accord_sec"), &render(false, None, false));
        write(&format!("{tag}_accord_pedale"), &render(true, None, false));
        write(&format!("{tag}_accord_pedale_sans_halo"), &render(true, Some(0.0), false));
        write(&format!("{tag}_accord_pedale_cordes"), &render(true, None, true));
        eprintln!("  meme gain, aucune normalisation.");
    }

    /// The mechanics at PIANISSIMO, for the ear — the audit's open pair.
    ///
    /// The blind questionnaire cleared the action noise at forte (6/6 pairs,
    /// "aucun clic") but its only chord-level "clic" was the Marée chord at pp,
    /// where the strike sits +6.3 dB over the body (vs +4.0 at ff) with its HF
    /// collapsed — the thump's nvel^0.25 law barely drops while the strings
    /// drop by twenty. These are the same A/B pairs at LOW velocity: each file
    /// plays the take WITH the action noise, 0.7 s of silence, then WITHOUT.
    /// If the pp thud is the mechanics, the second half of each file is clean.
    #[test]
    #[ignore = "renders for listening"]
    fn render_the_pp_mechanics_ab() {
        let pair = |notes: &[u8], vel: u8, secs: f64| -> Vec<f32> {
            let take = |mech: bool| -> Vec<f32> {
                let preset = factory_presets()
                    .into_iter()
                    .find(|p| p.name.contains("Close Mics"))
                    .expect("the Close Mics preset");
                let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
                let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
                if !mech {
                    let _ = tx.send(PianoCommand::SetMechanics(0.0));
                }
                eng.set_workers(0);
                for &n in notes {
                    let _ = tx.send(PianoCommand::NoteOn(n, vel));
                }
                let block = 128usize;
                let blocks = (SR as f64 * secs) as usize / block;
                let mut out = Vec::with_capacity(blocks * block * 2);
                let mut buf = vec![0.0f32; block * 2];
                for _ in 0..blocks {
                    buf.iter_mut().for_each(|x| *x = 0.0);
                    eng.process_audio(&mut buf, 2);
                    out.extend_from_slice(&buf);
                }
                out
            };
            let mut x = take(true);
            x.extend(std::iter::repeat(0.0).take((SR * 0.7) as usize * 2));
            x.extend(take(false));
            x
        };

        write("pp_meca_accord_maree", &pair(&[40, 47, 52, 56, 59], 45, 3.0));
        for n in [40u8, 52, 64, 76, 88] {
            write(&format!("pp_meca_note_{n:03}"), &pair(&[n], 30, 2.5));
        }
        eprintln!("  chaque fichier : AVEC mecanique, silence, SANS. Si le toc pp");
        eprintln!("  disparait dans la seconde moitie, c'est la mecanique.");
    }

    /// Is the swell the string's, or the plate's?
    ///
    /// The string's force on the bridge is set by the hammer in the two to five
    /// milliseconds of contact; after that the string only decays. The plate,
    /// on the other hand, is a bank of lightly-damped resonators, and a
    /// resonator driven near its own frequency takes about `1/(π·Δf)` to reach
    /// full amplitude — at 165 Hz with a 2 % loss factor that is ninety
    /// milliseconds, which is suspiciously exactly what the render shows.
    ///
    /// So: record the force going IN and the sound coming OUT, in the same run,
    /// and compare when each reaches its peak. If the force strikes and the
    /// output blooms, the plate is doing it and the string is innocent.
    #[test]
    #[ignore = "renders for analysis"]
    fn render_the_force_and_the_sound() {
        use crate::soundboard::Soundboard;
        use crate::voice::Voice;
        for note in [40u8, 52, 59] {
            let mut board = Soundboard::new(SR, 1.0);
            let mut v = Voice::default();
            v.start(note, 4.0, 1.0, 0.5, 0.5, SR);
            let n = (SR as f64 * 1.0) as usize;
            let (mut force, mut sound) = (Vec::with_capacity(n), Vec::with_capacity(n));
            let mut bridge_y = 0.0;
            for _ in 0..n {
                let f = v.tick(bridge_y, board.compliance_at(&v.attach));
                board.drive_bridge(f);
                let (l, _r) = board.process();
                bridge_y = board.bridge_displacement();
                force.push(f as f32);
                sound.push(l as f32);
            }
            // Each normalised to its own peak: only the TIMING is the subject,
            // and force is in newtons while the output is not.
            for (tag, mut x) in [("force", force), ("sortie", sound)] {
                let pk = x.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-30);
                x.iter_mut().for_each(|v| *v /= pk);
                let stereo: Vec<f32> = x.iter().flat_map(|&s| [s, s]).collect();
                write(&format!("{tag}_{note}"), &stereo);
            }
        }
    }
}

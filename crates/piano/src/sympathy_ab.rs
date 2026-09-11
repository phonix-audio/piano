//! Cordes physiques contre banc partagé : les rendus pour l'oreille.
//!
//! Under the pedal a piano has eighty-seven other sets of strings free to
//! answer, and simulating each of them as a voice is what put this instrument
//! out of reach of real time — a hundred and seventy voices where the music has
//! six. One shared bank of resonators stands in for them, sized against what
//! they measurably put into the air.
//!
//! What the measurement can say: the bank delivers the same AVERAGE sympathetic
//! energy (mean signed error +0.3 dB at a witness note across the compass).
//! What it cannot: whether the evenness matters. The physical strings answer
//! each note differently, because the plate's geometry decides which; the bank
//! answers them all alike. That is a question for an ear, and these are its
//! renders — same gain, no normalisation.
//!
//!   cargo test -p piano --lib render_the_sympathy_ab -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::engine::{PianoCommand, PianoEngine};
    use crate::patch::factory_presets;

    const SR: f32 = 48_000.0;

    fn render(per_string: bool, secs: f64, score: &[(f64, PianoCommand)]) -> Vec<f32> {
        let _model = per_string.then(crate::voice::PerStringModel::enter);
        let preset = factory_presets()
            .into_iter()
            .find(|p| p.name.contains("Close Mics"))
            .expect("the Close Mics preset");
        let (mut eng, tx, _mr) = PianoEngine::new_for_plugin(SR);
        let _ = tx.send(PianoCommand::LoadPatch(Box::new(preset)));
        eng.set_workers(0);

        let block = 256usize;
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
    }

    fn write(name: &str, audio: &[f32]) {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../renders/sympathy");
        std::fs::create_dir_all(dir).expect("the output directory");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR as u32,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let path = format!("{dir}/{name}.wav");
        let mut w = hound::WavWriter::create(&path, spec).expect("wav");
        for &s in audio {
            w.write_sample((s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
                .expect("sample");
        }
        w.finalize().expect("finalize");
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        eprintln!("  -> {path}  (crête {peak:.4})");
    }

    #[test]
    #[ignore = "renders for listening"]
    fn render_the_sympathy_ab() {
        // A chord under the pedal, held: sympathy is a TAIL, and a tail needs
        // room. Then the same chord dry, as the reference for what the pedal is
        // supposed to be adding at all.
        let mut chord: Vec<(f64, PianoCommand)> = vec![(0.0, PianoCommand::SustainPedal(true))];
        for n in [40u8, 47, 52, 56, 59] {
            chord.push((0.02, PianoCommand::NoteOn(n, 96)));
        }
        // And a phrase, because one chord says how loud and a line says whether
        // it belongs: five notes released as the hand moves on, pedal held, so
        // everything rings together as it would under a foot.
        let mut phrase: Vec<(f64, PianoCommand)> =
            vec![(0.0, PianoCommand::SustainPedal(true))];
        for (i, n) in [52u8, 59, 64, 67, 71, 76].into_iter().enumerate() {
            let t = 0.05 + i as f64 * 0.55;
            phrase.push((t, PianoCommand::NoteOn(n, 92)));
            phrase.push((t + 0.45, PianoCommand::NoteOff(n)));
        }

        for (name, secs, score) in [("accord", 7.0, &chord), ("phrase", 8.0, &phrase)] {
            for per_string in [true, false] {
                let x = render(per_string, secs, score);
                write(
                    &format!("{name}_{}", if per_string { "cordes" } else { "banc" }),
                    &x,
                );
            }
        }
        eprintln!("  même gain, aucune normalisation : ce qui diffère EST la différence.");
    }
}

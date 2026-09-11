//! The sympathetic halo under the pedal.
//!
//! A pedalled piano wakes every other set of strings. Simulating each of
//! them is a hundred and seventy voices where the music has six, so one
//! shared bank of resonators, one per note, stands in for them, calibrated
//! against what the physical strings measurably add.

use super::modal_bank::{Mode, ModalBank};

/// The sympathetic halo, shared.
///
/// With the sustain pedal down every string is free, and a struck note's
/// partials that land on another string's modes set it ringing. In the physical
/// engine that is one live Voice per undamped string (thirty to a hundred of
/// them), which is the cost this bank removes. Here it is ONE shared modal bank:
/// a resonator at every note's fundamental, driven by the struck output, so a
/// note excites the resonators its partials fall on and they ring on together.
///
/// It is a halo, not the tone: the engine measures the pedalled sympathetic
/// addition at only 0 to 1.2 dB, so this is summed in low and gated on the pedal.
/// When the pedal lifts, every damper lands, so the output fades over a damper's
/// fall and the bank is then cleared.
pub struct SympatheticBank {
    bank: ModalBank,
    shape: Vec<f64>,
    coupling: f64,
    gain: f32,
    env: f32,
    env_fall: f32,
    live: bool,
}

impl SympatheticBank {
    /// The lowest and highest MIDI notes with strings.
    const LO: u8 = 21;
    const HI: u8 = 108;

    pub fn new(sr: f32) -> Self {
        let mut modes = Vec::with_capacity((Self::HI - Self::LO + 1) as usize);
        for note in Self::LO..=Self::HI {
            let f = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            // Undamped strings ring long: ~15 s in the bass down to ~3 s at the
            // top, the order of the free decays the model itself produces.
            let t = (note - Self::LO) as f64 / (Self::HI - Self::LO) as f64;
            let t60 = 15.0 * (1.0 - t) + 3.0 * t;
            modes.push(Mode::from_t60(f, t60));
        }
        let n = modes.len();
        let mut bank = ModalBank::new();
        bank.set_modes(&modes, sr);
        // A full damper shape so `damp(0.0)` can hard-clear the bank when the
        // pedal lifts and the halo has faded.
        bank.set_damper_shape(&vec![1.0; n]);
        SympatheticBank {
            bank,
            shape: vec![1.0; n],
            coupling: 1.0,
            // ModalBank works in tiny physical units (the modal coordinates are
            // displacements, the same the soundboard's output is scaled up from),
            // so the readout needs a large factor to reach audio level. Calibrated
            // against the live model: with this the pedalled tail regains the ~2.8
            // dB of sympathetic energy the physical engine puts there (a held
            // C-major chord, Close Mics), matching its measured 0 to 1.2 dB add.
            // MEASURED 2026-08-23, against the per-string model it replaces.
            //
            // The obvious target is the wrong one and the measurement says so:
            // matched on TOTAL energy the bank would be silent, because eighty-
            // seven free strings LOAD the bridge and the physical model comes
            // out 2 dB QUIETER with the pedal down than without.
            //
            // What a listener calls the pedal is the energy that appears where
            // the struck note has none. Measured at a witness note a tritone
            // above the one struck, the physical strings put +8.7, +8.0 and
            // +36.3 dB there for three pairs across the compass — an average
            // the bank can match and a SPREAD it cannot, since one shared bank
            // answers every note alike where the plate's geometry answers each
            // differently. At this gain the mean signed error over the three is
            // +0.3 dB: the same sympathetic energy, spread evenly instead of
            // unevenly. See `print_the_pedalled_add_to_calibrate_the_halo`.
            gain: 200.0,
            env: 0.0,
            env_fall: 1.0 / (sr * 0.15),
            live: false,
        }
    }

    /// One sample of halo to add to the output. `drive` is the struck signal
    /// (mono), `pedal` whether the sustain pedal is down.
    pub fn process(&mut self, drive: f32, pedal: bool) -> f32 {
        if pedal {
            self.env = 1.0;
            self.live = true;
            self.bank.add_force(&self.shape, drive as f64 * self.coupling);
            self.bank.tick();
            return self.bank.read(&self.shape) as f32 * self.gain;
        }
        if !self.live {
            return 0.0;
        }
        // Pedal up: the dampers land. Fade the halo over a damper's fall, then
        // clear the bank so nothing stale survives to the next pedal.
        self.env -= self.env_fall;
        if self.env <= 0.0 {
            self.env = 0.0;
            self.bank.damp(0.0);
            self.live = false;
            return 0.0;
        }
        self.bank.tick();
        self.bank.read(&self.shape) as f32 * self.gain * self.env
    }

    /// Whether the halo still has anything to contribute (so the engine keeps
    /// running the block).
    pub fn is_active(&self) -> bool {
        self.live
    }

    /// Give each resonator the coupling its string actually has to the plate.
    ///
    /// The bank shipped with a flat shape — every note driven and read equally
    /// — and measured against the physical model that is 32 dB out between one
    /// note and another: the strings a struck note really shakes are the ones
    /// the bridge carries to, and the plate's geometry decides which. Handing
    /// the same weights the board uses puts the halo where the instrument puts
    /// it. `w[i]` is the coupling strength of note `LO + i`, in any consistent
    /// unit; the set is normalised here, so only the SHAPE matters and the
    /// overall level stays with `gain`.
    pub fn set_shape(&mut self, w: &[f64]) {
        let n = self.shape.len().min(w.len());
        let mean: f64 = w[..n].iter().sum::<f64>() / n.max(1) as f64;
        if mean <= 0.0 {
            return;
        }
        for i in 0..n {
            self.shape[i] = w[i] / mean;
        }
    }

    /// Stop dead, without the damper's fall.
    ///
    /// For an all-notes-off, which is a panic and not a pedal lift: the
    /// hundred and fifty milliseconds this otherwise takes to fade are exactly
    /// what someone hitting panic does not want to hear.
    pub fn reset(&mut self) {
        self.bank.damp(0.0);
        self.env = 0.0;
        self.live = false;
    }

    /// The halo depends on nothing but the note grid, but the gain is the one
    /// tunable, exposed for calibration against the model.
    pub fn set_gain(&mut self, g: f32) {
        self.gain = g;
    }
}

#[cfg(test)]
mod tests {
    const SR: f32 = 48_000.0;
    use super::*;

    #[test]
    fn sympathetic_halo_rings_under_pedal() {
        let mut sb = SympatheticBank::new(SR);
        let mut ring = 0.0f32;
        for i in 0..(SR as usize) {
            let t = i as f32 / SR;
            // A short 440 Hz burst, then silence.
            let drive = if t < 0.05 {
                (core::f32::consts::TAU * 440.0 * t).sin()
            } else {
                0.0
            };
            let h = sb.process(drive, true);
            if t > 0.1 {
                ring += h * h; // energy AFTER the burst is the resonators ringing on
            }
        }
        assert!(ring > 1e-9, "no sympathetic ring under pedal: {ring}");
    }

    #[test]
    fn sympathetic_halo_silent_with_pedal_up() {
        let mut sb = SympatheticBank::new(SR);
        let mut e = 0.0f32;
        for i in 0..(SR as usize / 2) {
            let t = i as f32 / SR;
            let drive = (core::f32::consts::TAU * 440.0 * t).sin();
            e += sb.process(drive, false).abs(); // pedal up: never driven
        }
        assert_eq!(e, 0.0, "halo should be silent with the pedal up, got {e}");
    }

    #[test]
    fn halo_fades_and_clears_on_pedal_up() {
        let mut sb = SympatheticBank::new(SR);
        for i in 0..(SR as usize / 10) {
            let t = i as f32 / SR;
            sb.process((core::f32::consts::TAU * 440.0 * t).sin(), true);
        }
        assert!(sb.is_active());
        let mut n = 0;
        while sb.is_active() && n < SR as usize {
            sb.process(0.0, false);
            n += 1;
        }
        assert!(!sb.is_active(), "halo never cleared after pedal up");
        assert!(n < (SR * 0.3) as usize, "halo cleared too slowly: {n} samples");
    }
}

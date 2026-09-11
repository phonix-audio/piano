//! The noises a piano makes that are not the strings.
//!
//! A piano is a machine with several hundred moving wooden parts, and a
//! listener hears them. The key reaching its bed, the hammer shank and the
//! whippen, the damper felt coming down on a string and lifting off it, the
//! pedal lyre and its rod — none of that is the string vibrating, and all of it
//! is there on any close recording of a real instrument. Leave it out and what
//! remains is a set of tuned resonators: correct, and obviously not a piano.
//!
//! These are short filtered noise bursts, each keyed to a mechanical event:
//!
//! * **the blow** — the action delivering the hammer, heard at every note-on
//!   and louder the harder the key is struck;
//! * **the damper landing** — felt arriving on a ringing string when the key is
//!   released, a softer and lower sound than the blow;
//! * **the damper lifting** — quieter still, and the reason a pedal-down piano
//!   has a sound of its own before anything is played;
//! * **the pedal** — the lyre and its rod, a low wooden knock when the whole
//!   damper rail moves.
//!
//! Their levels are chosen, not measured: this is the one part of the model with
//! no published figures behind it, and it is written here rather than hidden. If
//! measurements of a real action's noise turn up, they belong here.

/// One noise event: a burst through a band, with its own envelope.
///
/// It filters two INDEPENDENT noise streams, one per channel. Feeding one
/// stream to both and mixing them across gives two versions of the same signal —
/// correlation 0.99, which is mono with extra steps. A mechanism spread over two
/// metres of instrument does not do that.
///
/// Four poles of low-pass, not one. An action noise is a wooden knock, and wood
/// hitting felt has almost nothing above a couple of kilohertz. At one pole it
/// came out as hiss: measured against a recording of a real piano playing the
/// same piece, the mechanism alone put 29 dB too much into 4-8 kHz and 41 dB too
/// much above that.
#[derive(Clone, Copy, Debug, Default)]
struct Burst {
    amp: f32,
    decay: f32,
    lp: [[f32; 4]; 2],
    hp: [f32; 2],
    lp_k: f32,
    hp_k: f32,
    /// The onset ramp, 0 to 1. Nothing in a piano starts instantaneously: a key
    /// reaching the keybed and a hammer shank flexing both take a fraction of a
    /// millisecond to load up. Started at full amplitude the noise begins with a
    /// step, and a step in a signal is a CLICK — which is what a dense passage
    /// sounded like, because that is when these fire most often.
    gate: f32,
    rise: f32,
    /// This burst's OWN noise, one stream per channel.
    ///
    /// Every burst used to filter the same two streams the whole mechanism
    /// shared, so several sounding at once were filtered copies of one signal
    /// and added coherently: not eight knocks, one knock eight times as loud,
    /// swelling and ducking as their envelopes crossed. A run of notes made that
    /// obvious, because a run is when they overlap most.
    rng: [u32; 2],
}

impl Burst {
    fn arm(&mut self, amp: f32, ms: f32, low_hz: f32, high_hz: f32, sr: f32, seed: u32) {
        // Two streams that share no history with each other or with any other
        // burst. An odd multiplier keeps the two channels from ever locking.
        self.rng = [seed | 1, seed.wrapping_mul(2_654_435_761) | 1];
        self.amp = amp;
        self.decay = (-1.0 / (ms * 1e-3 * sr).max(1.0)).exp();
        self.lp_k = 1.0 - (-std::f32::consts::TAU * high_hz / sr).exp();
        self.hp_k = 1.0 - (-std::f32::consts::TAU * low_hz / sr).exp();
        // A fifth of a millisecond to come up. Short enough to still read as a
        // knock rather than a swell, long enough that no sample boundary carries
        // a step.
        self.gate = 0.0;
        self.rise = 1.0 / (0.0002 * sr).max(1.0);
    }

    /// How loud this burst still is, so a new one can take the quietest slot
    /// rather than whichever came next in a circle.
    #[inline]
    fn loudness(&self) -> f32 {
        self.amp * self.gate.max(0.05)
    }

    #[inline]
    fn noise(rng: &mut u32) -> f32 {
        *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*rng >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0
    }

    #[inline]
    fn tick(&mut self) -> (f32, f32) {
        if self.amp < 1.0e-6 {
            return (0.0, 0.0);
        }
        let n = [Self::noise(&mut self.rng[0]), Self::noise(&mut self.rng[1])];
        let mut out = [0.0f32; 2];
        for i in 0..2 {
            let mut v = n[i];
            for p in self.lp[i].iter_mut() {
                *p += (v - *p) * self.lp_k;
                v = *p;
            }
            self.hp[i] += (v - self.hp[i]) * self.hp_k;
            out[i] = (v - self.hp[i]) * self.amp * self.gate;
        }
        self.gate = (self.gate + self.rise).min(1.0);
        self.amp *= self.decay;
        (out[0], out[1])
    }
}

/// The attack thump: the instrument's own body shocked by the blow.
///
/// This is not a noise burst and must not be one. Chaigne & Askenfelt measured it
/// directly — strings removed from the piano, the hammer striking a dummy mass,
/// so that nothing but the mechanism could be heard — and what the bridge does is
/// RING, at named frequencies (JASA 95(3) 1631, §III and Fig. 17, note C4 played
/// forte staccato):
///
/// > The bridge motion starts with a percussive component about 20 ms before
/// > hammer-dummy contact, followed by a "wave package" at about 1000 Hz (due to
/// > a resonance in the key). Later, the motion is dominated by a mixture of
/// > resonances in the key bed at 100 and 250 Hz, approximately.
///
/// So: a fast packet around a kilohertz, then two low resonances that outlast it.
/// Three modes, and they are the measured three.
///
/// Why it matters more than its level suggests. Those same authors, having built
/// the string model this instrument is built on, wrote of their own results that
/// "the simulated tones don't mimic real piano tones convincingly when listening"
/// and named the reason: "the essential missing feature in the synthesis is in the
/// attack component, which for a real piano tone includes a strong thump". Adding
/// a recording of it to their synthesis gave "significant improvements in the
/// realism". A piano's attack is substantially NOT the string — and a model that
/// has only the string has to make the string carry all of it, which is heard as
/// an attack that bites instead of one that lands.
///
/// Level, and why it rises so little with dynamic. They put the thump "of the same
/// order as the bridge variations due to string motion AT PP LEVEL" — that is, at
/// a pianissimo it is as loud as the note, and at a fortissimo the note has grown
/// past it while the mechanism has barely moved. The action's geometry does not
/// change with how hard it is played.
#[derive(Clone, Copy, Debug, Default)]
struct Thump {
    /// Per mode, per channel: `y1`, `y2`, and the pair of recursion coefficients.
    y1: [[f32; 2]; 3],
    y2: [[f32; 2]; 3],
    a1: [f32; 3],
    a2: [f32; 3],
    w: [[f32; 2]; 3],
    /// Samples still to wait before the blow lands.
    ///
    /// No two keys of a piano let their hammers go at the same instant. Each is a
    /// separate lever with its own regulation — let-off, drop, key dip, the lost
    /// motion in its own whippen — and a technician's tolerance on those is
    /// measured in fractions of a millimetre, which at the hammer's speed is
    /// milliseconds. A chord is therefore eight events spread over a few
    /// milliseconds, not one event eight times over.
    ///
    /// Without that spread eight thumps land on the same sample with the same
    /// phase and add coherently: measured, eight keys came out **7.2 times** one
    /// key where independent events give 2.8, and the file's own
    /// `simultaneous_knocks_are_independent` rejected it. The spread is a physical
    /// fact of the action, not a trick to decorrelate them — but it does both.
    delay: u32,
    /// The blow, waiting to be delivered on the first tick and then gone.
    ///
    /// A struck body starts from REST and with its velocity at a maximum, so its
    /// displacement leaves zero smoothly — which is what driving the recursion
    /// with an impulse gives and what loading its state directly does not. Loaded
    /// directly the output begins at full amplitude, and a signal that begins at
    /// full amplitude is a step, which is a click. That is a mistake this repo has
    /// made before in other voices; it is not worth making again for the sake of
    /// one field.
    fire: [f32; 3],
    amp: f32,
}

impl Thump {
    /// The three measured frequencies. The kilohertz packet is a resonance of the
    /// KEY and dies quickly; the other two are the key bed, a far heavier thing,
    /// and they are what is still moving when the packet has gone.
    const HZ: [f32; 3] = [100.0, 250.0, 1000.0];
    const MS: [f32; 3] = [55.0, 40.0, 12.0];

    fn arm(&mut self, amp: f32, note: u8, sr: f32, spread: f32) {
        // The three modes are not detuned together — that would just transpose
        // the whole thump and leave its modes as locked to each other as before.
        // Coprime-ish multipliers of opposite sign make each one wander its own
        // way, which is what a localised structure does.
        const SKEW: [f32; 3] = [1.0, -0.7, 0.4];
        self.amp = amp;
        // Up to five milliseconds, which is the order of the regulation scatter
        // across a set of keys.
        self.delay = (spread.clamp(0.0, 1.0) * 0.005 * sr) as u32;
        for m in 0..3 {
            // A key bed two metres wide is not one point. Which of its modes a
            // given key shakes hardest depends on where along it that key sits,
            // and the two ears of a listener are not at the same place either —
            // so the weights differ per mode AND per channel, which is where the
            // width of a real action noise comes from. Anti-symmetric about the
            // middle of the compass, so bass keys lean one way and treble keys the
            // other, exactly as the keyboard is laid out.
            let k = std::f32::consts::PI * (m as f32 + 1.0);
            let x = (note as f32 - 21.0) / 87.0;
            let lobe = (k * x).sin();
            // And the two listening points get their OWN modal weights, which is
            // the whole difference between stereo and a pan.
            //
            // What stood here scaled one signal by (1−tilt) and (1+tilt): the
            // left/right ratio came out the SAME for all three modes, so the two
            // channels were one signal at two gains. Correlation 0.990, and the
            // file's own `the_noise_is_not_mono` caught it. Two points on a body
            // hear different amounts of each mode — a point can sit on a node of
            // one and an antinode of the next — so the weight has to be indexed by
            // mode AND channel. Deliberately not symmetric about the middle:
            // symmetric points make every mode either equal or exactly opposite
            // between the channels, which is its own kind of degenerate.
            const EAR: [f32; 2] = [0.33, 0.71];
            // ── The key BED does not belong on a treble note ───────────────
            //
            // Reported, and unmistakable once described: every treble note opened
            // with "a sound like knocking on something". That is what modes 0 and
            // 1 are — 100 and 250 Hz with forty to fifty-five milliseconds of
            // decay is a knock on a wooden box — and on a note two or three
            // octaves above them they bear no relation to the pitch at all: a
            // thud, and then a thin note over it. Measured at velocity 20 the
            // whole mechanism sat 18.3 dB ABOVE the string at C6.
            //
            // Chaigne & Askenfelt's Fig. 17 is ONE measurement, at C4, with the
            // strings removed. Carrying it unchanged across eighty-eight notes is
            // the same mistake this model already made with `b3` and with the
            // felt above G6: a figure measured at one place is not evidence about
            // another. So the bed's two modes are held through the register where
            // they were measured and taper away above it, while mode 2 — the
            // KEY's own resonance near a kilohertz, and every key has one — stays
            // the whole way up.
            let bed = if m < 2 {
                let above = ((note as f32 - 60.0) / 24.0).clamp(0.0, 1.0);
                1.0 - above
            } else {
                1.0
            };
            self.w[m] = [lobe * bed * (k * EAR[0]).sin(), lobe * bed * (k * EAR[1]).sin()];
            // "100 and 250 Hz, APPROXIMATELY" — the word is the authors'. A key
            // bed is a long beam on supports and every key loads it at a
            // different place, so no two keys see quite the same resonance. This
            // is the same effect Chaigne, Cotté and Viggiano set out to study in
            // soundboards (JASA 133(4) 2456): slight irregularity in a stiffened
            // plate localises its modes, so that the response "differs from one
            // point to another" rather than being one global figure.
            //
            // Six percent, and it is not cosmetic. Without it every key rings the
            // bed at exactly the same frequency with the same phase, so a chord's
            // thumps add coherently: eight keys measured 7.2 times one where
            // independent events give 2.8. Detuning by a few percent lets them
            // drift apart over the tens of milliseconds they last.
            let detune = 1.0 + 0.06 * (2.0 * spread.clamp(0.0, 1.0) - 1.0) * SKEW[m];
            let wd = std::f32::consts::TAU * Self::HZ[m] * detune / sr;
            // T60 in the table, so sigma = ln(1000)/T60.
            let sigma = 6.9078 / (Self::MS[m] * 1e-3);
            let r = (-sigma / sr).exp();
            self.a1[m] = 2.0 * r * wd.cos();
            self.a2[m] = r * r;
            self.y1[m] = [0.0, 0.0];
            self.y2[m] = [0.0, 0.0];
            // The impulse is scaled by sin(ωΔt) so every mode peaks near one
            // whatever its frequency — otherwise the recursion's own gain, which
            // goes as 1/sin(ωΔt), would make the 100 Hz mode ten times the 1 kHz
            // one for no reason but the sample rate.
            self.fire[m] = wd.sin();
        }
    }

    #[inline]
    fn loudness(&self) -> f32 {
        self.amp * (self.y1[0][0].abs() + self.y1[2][0].abs() + self.fire[0])
    }

    #[inline]
    fn tick(&mut self) -> (f32, f32) {
        if self.amp < 1.0e-9 {
            return (0.0, 0.0);
        }
        if self.delay > 0 {
            self.delay -= 1;
            return (0.0, 0.0);
        }
        let mut out = [0.0f32; 2];
        let mut alive = 0.0f32;
        for m in 0..3 {
            let f = self.fire[m];
            self.fire[m] = 0.0;
            for c in 0..2 {
                let y = self.a1[m] * self.y1[m][c] - self.a2[m] * self.y2[m][c] + f;
                self.y2[m][c] = self.y1[m][c];
                self.y1[m][c] = y;
                out[c] += y * self.w[m][c];
                alive += y.abs();
            }
        }
        if alive < 1.0e-7 {
            self.amp = 0.0;
        }
        (out[0] * self.amp, out[1] * self.amp)
    }
}

/// Which slot a new noise should take: the one with least left in it.
///
/// Handing them out in a circle meant that once there were more notes in flight
/// than slots — which a pedalled passage reaches immediately — a burst still
/// sounding at full level was overwritten partway through, and the output
/// stepped. Taking the quietest instead means what gets cut short is whatever
/// was closest to being over.
fn quietest(slots: &[Burst]) -> usize {
    least(slots, Burst::loudness)
}

/// The same rule for the thumps, which are a different type but have exactly the
/// same problem: stealing the loudest slot is what makes a stolen voice audible.
fn quietest_thump(slots: &[Thump]) -> usize {
    least(slots, Thump::loudness)
}

fn least<T>(slots: &[T], loudness: impl Fn(&T) -> f32) -> usize {
    let mut best = 0usize;
    let mut lowest = f32::INFINITY;
    for (i, b) in slots.iter().enumerate() {
        let l = loudness(b);
        if l < lowest {
            lowest = l;
            best = i;
        }
    }
    best
}

/// The piano's action, as a source of sound in its own right.
/// Decibels per semitone the action's knock changes by above A4.
///
/// **−0.35 until 2026-08-24, and negative was the wrong sign.** That value was
/// fitted while the treble strings were louder than they now are, and its own
/// comment said to remeasure with them. Remeasured with `fit_the_treble_knock`
/// against the real C7's attack body (630/1000/1260/1587 Hz under the
/// fundamental: −29.7/−25.0/−21.3/−23.1 dB): at −0.35 the model sat 7 to 18 dB
/// BELOW the real body — an attack with no knock in it, which is why a forte
/// treble chord opened as a pad ("orgue dès le début") and why the blind A/B
/// pairs heard no click at all where a real treble is half click. At +0.50 the
/// 630 Hz band lands on the real figure (−29.1 against −29.7) and the mean
/// error halves; the residual shortfall at 1587 Hz is the knock's own band cap
/// (≤1.6 kHz), a separate question from its level.
const TREBLE_KNOCK_DB: f32 = 0.50;

#[cfg(test)]
pub(crate) static KNOCK_OVERRIDE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(f32::to_bits(TREBLE_KNOCK_DB));

#[inline]
fn treble_knock_db() -> f32 {
    #[cfg(test)]
    {
        f32::from_bits(KNOCK_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        TREBLE_KNOCK_DB
    }
}

#[derive(Clone, Debug)]
pub struct Mechanics {
    sr: f32,
    /// Drives the per-event variation: which seed each burst gets, and the small
    /// spread on its amplitude and filter corners.
    seed: u32,
    /// Several can overlap: a chord is several keys, and a pedal change lands
    /// while notes are still being played.
    /// Sixteen, not eight. A pianist's hands put ten notes down at once and a
    /// pedalled passage leaves their noises overlapping well past the next
    /// chord; eight ran out constantly.
    keys: [Burst; 16],
    /// The body's answer to the blow, one per note-on. Same count as the knocks
    /// they accompany, for the same reason: ten fingers and a pedal.
    thumps: [Thump; 16],
    dampers: [Burst; 16],
    pedal: Burst,
    /// How loud the whole mechanism is, 0 to 1.
    pub level: f32,
}

impl Default for Mechanics {
    fn default() -> Self {
        Mechanics {
            sr: 48_000.0,
            seed: 0x1234_5678,
            keys: [Burst::default(); 16],
            thumps: [Thump::default(); 16],
            dampers: [Burst::default(); 16],
            pedal: Burst::default(),
            level: 1.0,
        }
    }
}

impl Mechanics {
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
    }

    /// A number that differs for every event, so no two knocks are identical and
    /// a run of them is not one sound being swept.
    #[inline]
    fn next_seed(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.seed
    }

    /// A small random multiplier around one, for the same reason.
    #[inline]
    fn jitter(&mut self, spread: f32) -> f32 {
        let u = (self.next_seed() >> 9) as f32 / (1u32 << 23) as f32;
        1.0 + (u - 0.5) * 2.0 * spread
    }

    /// A key is struck. The action noise rises with how hard it is played, and
    /// faster than the tone does — which is why a pianissimo note on a real
    /// piano is proportionally *noisier* than a loud one, not quieter.
    ///
    /// The register tilt is deliberately GENTLE and jittered. Every piano key
    /// works the same lever against the same keybed; the treble action is a
    /// little lighter and its knock a little brighter, but nothing like the
    /// clean monotonic sweep this used to apply. Mapped straight from pitch, a
    /// scale ran the filter smoothly up or down across the noise and the ear
    /// heard a zip following the notes — a synthesiser gesture, not a mechanism.
    pub fn key_struck(&mut self, note: u8, nvel: f32) {
        // ── The mechanism follows the REGISTER, and it did not ─────────────
        //
        // Chaigne & Askenfelt put the attack thump "of the same order as the
        // bridge variations due to string motion at pp level". That relation was
        // checked here at C4 and found right — the mechanism sits 0.6 dB under
        // the string at velocity 20 — and then the same ABSOLUTE level was
        // applied to all eighty-eight notes. The strings are not all the same
        // loudness: measured, this instrument's sustained output falls 16 to 27 dB
        // from A4 to the top of the compass.
        //
        // So the published relation held at one note and nowhere else. Measured
        // at velocity 20: C6 came out with the mechanism **18.3 dB ABOVE** the
        // string and C7 8.4 dB above — the note WAS the knock. And since the
        // thump's resonances are at fixed frequencies, every treble note opened
        // with the same sound, which is heard as a clack that does not change
        // with pitch.
        //
        // The roll-off is taken from this instrument's own measured output per
        // note, because that is what the relation is against: about a third of a
        // decibel per semitone above A4. It is not a taste curve — change how
        // loud the treble strings are and this has to be remeasured with them.
        // ── Remeasured, as the paragraph above said it would have to be ────
        //
        // The roll-off was fitted when the treble strings were louder than they
        // are now. Measured against the Iowa Steinway's own C7, a real piano
        // carries 22 to 30 dB MORE energy between 600 and 1600 Hz than this model
        // does — a percussive body under the fundamental that the ear misses as
        // "little bells", because a pure ringing tone with no body is what a bell
        // is. The action's own spectrum is flat from 315 to 1260 Hz and rolls off
        // above, which is exactly the shape of what is missing; it was simply
        // buried.
        let register = 10f32.powf(treble_knock_db() * (note as f32 - 69.0).max(0.0) / 20.0);
        let n = note as f32;
        // The treble action is lighter and its noise is higher and shorter.
        let t = ((n - 21.0) / 87.0).clamp(0.0, 1.0);
        let amp = 0.010 + 0.045 * nvel.clamp(0.0, 1.0).powf(0.7);
        let i = quietest(&self.keys);
        let (j1, j2, j3, j4) =
            (self.jitter(0.18), self.jitter(0.14), self.jitter(0.12), self.jitter(0.10));
        let seed = self.next_seed();
        let sr = self.sr;
        self.keys[i].arm(
            amp * j4 * register,
            (20.0 - 5.0 * t) * j1,
            (190.0 + 90.0 * t) * j2,
            (1100.0 + 500.0 * t) * j3,
            sr,
            seed,
        );
        // ── And the body's answer to it ───────────────────────────────────
        //
        // The knock above is wood meeting wood, heard in the air. This is the
        // whole instrument shaken by the same event and heard through the
        // bridge, which is a different sound and, by the measurement, a louder
        // one at low dynamics. See `Thump`.
        //
        // The exponent is where the paper's finding lives. At 0.25 a fortissimo
        // is only 1.7 times the thump of a pianissimo where the tone itself has
        // grown more than tenfold — so the thump rules the attack at pp and has
        // receded to a foundation under it at ff, which is what "of the same
        // order as ... at pp level" says. Raise it towards 1 and the thump
        // simply tracks the note, which is the same as not having it.
        let jt = self.jitter(0.12);
        let spread = (self.next_seed() >> 9) as f32 / (1u32 << 23) as f32;
        let k = quietest_thump(&self.thumps);
        self.thumps[k].arm(0.055 * nvel.clamp(0.0, 1.0).powf(0.25) * jt * register, note, sr, spread);
    }

    /// A damper lands on a string. Softer and lower than the blow — felt on
    /// wire, not wood on wood.
    pub fn damper_landed(&mut self, note: u8) {
        let t = ((note as f32 - 21.0) / 87.0).clamp(0.0, 1.0);
        let i = quietest(&self.dampers);
        let (j1, j2, j3) = (self.jitter(0.16), self.jitter(0.12), self.jitter(0.12));
        let seed = self.next_seed();
        let sr = self.sr;
        self.dampers[i].arm(0.006 * j3, (28.0 - 8.0 * t) * j1, 90.0,
            (820.0 + 380.0 * t) * j2, sr, seed);
    }

    /// And lifts off one.
    pub fn damper_lifted(&mut self, note: u8) {
        let t = ((note as f32 - 21.0) / 87.0).clamp(0.0, 1.0);
        let i = quietest(&self.dampers);
        let (j, j2) = (self.jitter(0.15), self.jitter(0.12));
        let seed = self.next_seed();
        let sr = self.sr;
        self.dampers[i].arm(0.0025 * j2, (18.0 - 4.0 * t) * j, 120.0, 1200.0, sr, seed);
    }

    /// The key comes back up.
    ///
    /// This is a DIFFERENT event from the damper landing, and it was missing.
    /// Báron and Holló (1935) named the three noises a piano action makes and
    /// kept them apart: the *Fingergeräusch* of a finger meeting the key, the
    /// *Bodengeräusch* of the key reaching the keybed, and the *obere
    /// Geräusche* — the "upper noises" that appear when the key is RELEASED
    /// again (Goebl, Bresin & Galembo, JASA 118(2), 2005, §I.B). The damper
    /// falling on the string is only part of that last group: the key itself
    /// returns to rest against the front-rail felt, the jack drops back under
    /// the roller and the repetition lever resets, and all of that happens
    /// whether or not a damper is anywhere near a string.
    ///
    /// Which is why this fires unconditionally. With the sustain pedal down the
    /// dampers stay off the strings entirely, so keying release noise to the
    /// damper alone made a pedalled passage — most of a Debussy piece — release
    /// its keys in total silence.
    ///
    /// Softer, duller and slower than the blow: the key comes back under its own
    /// lead weight and a spring, not under a finger. `amount` is the player's
    /// setting for how much of it is heard.
    pub fn key_released(&mut self, note: u8, amount: f32) {
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return;
        }
        let t = ((note as f32 - 21.0) / 87.0).clamp(0.0, 1.0);
        let i = quietest(&self.keys);
        // A third of the blow's floor. Goebl et al. measured finger-key forces
        // in a pressed (legato) touch at about one third of a struck one, and a
        // key returning under gravity is gentler still than either — but the
        // ABSOLUTE level here is chosen, not measured. The published work names
        // this noise and times it; it does not give it a sound pressure.
        let amp = 0.0035 * amount;
        // Lower and longer than the strike. The returning key is heavier and
        // slower than the finger that drove it, and lands on felt.
        let (j1, j2, j3) = (self.jitter(0.18), self.jitter(0.14), self.jitter(0.12));
        let seed = self.next_seed();
        let sr = self.sr;
        self.keys[i].arm(amp * j3, (24.0 - 6.0 * t) * j1, 130.0,
            (640.0 + 260.0 * t) * j2, sr, seed);
    }

    /// The pedal. The whole damper rail moves at once, through a wooden lyre and
    /// a metal rod, and it is the loudest single noise a piano makes without a
    /// note being played.
    pub fn pedal(&mut self, down: bool) {
        if down {
            let seed = self.next_seed();
            let sr = self.sr;
            self.pedal.arm(0.020, 45.0, 55.0, 450.0, sr, seed);
        } else {
            let seed = self.next_seed();
            let sr = self.sr;
            self.pedal.arm(0.013, 60.0, 45.0, 380.0, sr, seed);
        }
    }

    /// One sample, stereo. The two channels get independent noise: a mechanism
    /// is spread across the width of the instrument, not sitting at a point.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for b in self.keys.iter_mut().chain(self.dampers.iter_mut()) {
            let (a, b2) = b.tick();
            l += a;
            r += b2;
        }
        for t in self.thumps.iter_mut() {
            let (a, b2) = t.tick();
            l += a;
            r += b2;
        }
        let (pl, pr) = self.pedal.tick();
        let g = self.level.clamp(0.0, 1.0);
        ((l + pl) * g, (r + pr) * g)
    }

    /// Is anything still moving? The engine needs to know, so it does not go
    /// quiet with a pedal knock still ringing.
    pub fn is_sounding(&self) -> bool {
        self.pedal.amp > 1.0e-6
            || self.keys.iter().any(|b| b.amp > 1.0e-6)
            || self.dampers.iter().any(|b| b.amp > 1.0e-6)
            || self.thumps.iter().any(|t| t.amp > 1.0e-9)
    }

    pub fn silence(&mut self) {
        self.keys = [Burst::default(); 16];
        self.dampers = [Burst::default(); 16];
        self.thumps = [Thump::default(); 16];
        self.pedal = Burst::default();
    }
}

#[cfg(test)]
mod click_tests {
    use super::*;

    /// A dense passage must not click.
    ///
    /// The reported symptom was an unpleasant click "when there are a lot of
    /// notes", which is the signature of audio state being stepped while it is
    /// audible. Two things did it: a burst began at full amplitude, which is a
    /// step by definition, and once more notes were in flight than there were
    /// slots, a burst still sounding at full level was overwritten partway
    /// through by the next one in the circle.
    ///
    /// Measured as the largest jump between consecutive samples against the
    /// largest sample. A signal whose biggest step is a good fraction of its own
    /// peak is a signal with an edge in it.
    #[test]
    fn a_flurry_of_notes_does_not_click() {
        const SR: f32 = 48_000.0;
        let mut m = Mechanics::default();
        m.set_sample_rate(SR);
        m.level = 1.0;
        let mut worst_step = 0.0f32;
        let mut peak = 0.0f32;
        let mut prev = 0.0f32;
        // Forty notes inside a second, which is an ordinary run, against sixteen
        // slots — so slots are certainly reused mid-decay.
        let mut next_note = 0usize;
        for k in 0..(SR as usize) {
            if k >= next_note {
                let n = 40 + (k / 97) % 40;
                m.key_struck(n as u8, 0.8);
                if k % 3 == 0 {
                    m.damper_landed(n as u8);
                }
                next_note = k + (SR as usize) / 40;
            }
            let (l, _r): (f32, f32) = m.process();
            worst_step = worst_step.max((l - prev).abs());
            peak = peak.max(l.abs());
            prev = l;
        }
        let ratio = worst_step / peak.max(1e-12);
        eprintln!("bruit de mecanique : plus grand saut {ratio:.3} de la crete");
        // Filtered noise moves between samples, so this can never be tiny; but a
        // step comparable to the peak is an edge, and that is what was audible.
        assert!(
            ratio < 0.35,
            "the action noise steps by {ratio:.2} of its own peak — that is a click"
        );
    }

    /// Several knocks at once must not add up like one knock made louder.
    ///
    /// Every burst used to filter the SAME two noise streams, so simultaneous
    /// ones were correlated copies: their sum grew with the count instead of
    /// with its square root, and swelled and ducked as their envelopes crossed.
    /// That is what a fast run made audible. Independent sources add in power,
    /// so eight of them should land near sqrt(8) = 2.8 times one, not 8 times.
    /// **Measured above 400 Hz, and that is not a convenience.** Two different
    /// mechanisms sound at a note-on and they are not meant to behave alike. The
    /// knocks are eight separate pieces of wood, each with its own noise, and
    /// their independence is the whole claim this test was written to defend. The
    /// thump is the key BED — one beam a metre and a half long — and eight
    /// adjacent semitones load it within about eleven centimetres of each other,
    /// which at its 100 Hz mode, whose shape spans the entire beam, is the same
    /// point. Those eight thumps genuinely do add close to coherently. That is
    /// why a plated chord lands with a thud on a real piano where a single note
    /// does not, and suppressing it would be modelling the instrument wrongly to
    /// satisfy a threshold.
    ///
    /// So the two claims hold in different bands, and each is asked for where it
    /// applies. The split is at 400 Hz: the knocks are passed between 190 and
    /// 1600 Hz, the coherent part of the thump is its 100 Hz mode.
    #[test]
    fn simultaneous_knocks_are_independent() {
        const SR: f32 = 48_000.0;
        // (rms above 400 Hz, rms below it)
        let rms = |count: usize| -> (f32, f32) {
            let mut m = Mechanics::default();
            m.set_sample_rate(SR);
            m.level = 1.0;
            for k in 0..count {
                m.key_struck(60 + k as u8, 0.8);
            }
            let n = (0.03 * SR) as usize;
            let k = 1.0 - (-std::f32::consts::TAU * 400.0 / SR).exp();
            let (mut lp, mut hi, mut lo) = (0.0f32, 0.0f64, 0.0f64);
            for _ in 0..n {
                let (l, _r): (f32, f32) = m.process();
                lp += (l - lp) * k;
                hi += ((l - lp) as f64) * ((l - lp) as f64);
                lo += (lp as f64) * (lp as f64);
            }
            ((hi / n as f64).sqrt() as f32, (lo / n as f64).sqrt() as f32)
        };
        let (one_hi, one_lo) = rms(1);
        let (eight_hi, eight_lo) = rms(8);
        let growth = eight_hi / one_hi.max(1e-12);
        let low = eight_lo / one_lo.max(1e-12);
        eprintln!(
            "huit coups contre un : x{growth:.2} au-dessus de 400 Hz \
             (coherent = 8, independant = 2.8), x{low:.2} en dessous"
        );
        assert!(
            growth < 4.5,
            "eight knocks came out {growth:.1} times one: they are still the same noise"
        );
        assert!(growth > 1.8, "eight knocks barely louder than one ({growth:.1}): they cancel");
        // The body below may add coherently, but not MORE than coherently: past
        // eight, energy would be coming from somewhere.
        assert!(
            low <= 8.5,
            "the body grew {low:.1} times for eight keys, past coherent addition"
        );
    }

    /// A run must not read as a filter sweep.
    ///
    /// The corner frequencies used to be mapped straight from pitch, so a scale
    /// swept them smoothly and the ear heard a zip chasing the notes — a
    /// synthesiser gesture, not a mechanism. Every key of a piano works the same
    /// lever against the same keybed. Measured as the spread of the noise's
    /// spectral centroid across the compass, which should be modest.
    #[test]
    fn a_run_up_the_keyboard_is_not_a_sweep() {
        const SR: f32 = 48_000.0;
        let centroid = |note: u8| -> f32 {
            let mut m = Mechanics::default();
            m.set_sample_rate(SR);
            m.level = 1.0;
            m.key_struck(note, 0.8);
            let n = 2048usize;
            let x: Vec<f32> = (0..n).map(|_| m.process().0).collect();
            // Centroid by zero-crossing density, which needs no FFT and is
            // monotone in it for a noise band.
            let z = x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
            z as f32 * SR / (2.0 * n as f32)
        };
        let low = centroid(24);
        let high = centroid(100);
        let spread = (high / low.max(1.0)).max(low / high.max(1.0));
        eprintln!("centroide du bruit : grave {low:.0} Hz, aigu {high:.0} Hz, rapport x{spread:.2}");
        assert!(
            spread < 2.2,
            "the action noise sweeps {spread:.1}x across the compass: a run will read as a filter sweep"
        );
    }

    /// And a burst must not begin at full level.
    #[test]
    fn a_burst_comes_up_rather_than_starting_on() {
        const SR: f32 = 48_000.0;
        let mut m = Mechanics::default();
        m.set_sample_rate(SR);
        m.level = 1.0;
        m.key_struck(60, 1.0);
        let first = m.process().0.abs();
        let mut later = 0.0f32;
        for _ in 0..64 {
            later = later.max(m.process().0.abs());
        }
        assert!(
            first < later * 0.25,
            "the noise starts at {first:e} against a peak of {later:e}: that is a step"
        );
    }

    /// The thump has to be the three frequencies that were measured, and no
    /// others.
    ///
    /// Chaigne & Askenfelt, Fig. 17: a wave packet at about 1000 Hz from the key,
    /// then the key bed at roughly 100 and 250 Hz. This is the whole content of
    /// the attack component they identify, so the test asks for exactly those
    /// three and asks that the kilohertz one has GONE by the time the low pair is
    /// still going — "later, the motion is dominated by a mixture of resonances in
    /// the key bed".
    #[test]
    fn the_thump_is_the_measured_resonances() {
        let mut t = Thump::default();
        t.arm(1.0, 60, 48_000.0, 0.0);
        let n = 8192;
        let sig: Vec<f32> = (0..n).map(|_| t.tick().0).collect();
        // Coarse DFT at the three frequencies and at three that must be quiet.
        let power = |hz: f32, from: usize, len: usize| -> f64 {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..len {
                let p = std::f64::consts::TAU * hz as f64 * (i as f64) / 48_000.0;
                re += sig[from + i] as f64 * p.cos();
                im += sig[from + i] as f64 * p.sin();
            }
            (re * re + im * im).sqrt() / len as f64
        };
        for hz in Thump::HZ {
            let here = power(hz, 0, 2048);
            let between = power(hz * 1.7, 0, 2048);
            // Twice, not three times. These are HEAVILY damped: 250 Hz with a
            // T60 of 40 ms is a Q of about four and a half, so the peak is broad
            // by construction and asking for a threefold contrast at 1.7 times
            // the frequency asks for a resonance sharper than the measurement
            // describes. The claim being tested is that these are resonances at
            // all rather than a noise band, and a factor of two settles that.
            assert!(
                here > between * 2.0,
                "{hz} Hz is not a resonance: {here:.3e} against {:.3e} beside it",
                between
            );
        }
        // The key's packet dies first. Measured over the last eighth of a second,
        // the key bed must still be the louder of the two.
        let late_key = power(1000.0, 4096, 4096);
        let late_bed = power(100.0, 4096, 4096);
        assert!(
            late_bed > late_key * 4.0,
            "the kilohertz packet outlasts the key bed: {late_key:.3e} against {late_bed:.3e}"
        );
    }

    /// And it must not start with a step either.
    #[test]
    fn the_thump_starts_from_rest() {
        let mut t = Thump::default();
        t.arm(1.0, 60, 48_000.0, 0.0);
        let first = t.tick().0.abs();
        let mut peak = 0.0f32;
        for _ in 0..4800 {
            peak = peak.max(t.tick().0.abs());
        }
        assert!(
            first < peak * 0.25,
            "the thump opens at {first:e} against a peak of {peak:e}: that is a step"
        );
    }

    /// The finding that makes the thump worth having: it is nearly indifferent to
    /// how hard the note is played, so it RULES a pianissimo attack and merely
    /// underpins a fortissimo one.
    ///
    /// Chaigne & Askenfelt put its level "of the same order as the bridge
    /// variations due to string motion at pp level". A tone grows by more than
    /// twenty decibels from pp to ff; if the thump grew with it, it would be a
    /// fixed colour on every note and would say nothing about dynamic at all.
    #[test]
    fn the_thump_barely_notices_the_dynamic() {
        let level = |nvel: f32| {
            let mut m = Mechanics::default();
            m.key_struck(60, nvel);
            let mut peak = 0.0f32;
            for _ in 0..4800 {
                peak = peak.max(m.process().0.abs());
            }
            peak
        };
        let (pp, ff) = (level(0.1), level(1.0));
        let ratio = ff / pp;
        // The bound is the WHOLE mechanism — the wooden knock as well as the
        // thump — against the tone, which over the same span of dynamic grows by
        // more than tenfold. Anything under about four keeps the mechanism on the
        // right side of that comparison; the number to watch is that it is not
        // ten.
        assert!(
            ratio > 1.0 && ratio < 4.0,
            "the mechanism went from {pp:e} to {ff:e}, a factor of {ratio:.2}: it is \
             tracking the note instead of staying put"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn run(m: &mut Mechanics, secs: f32) -> (f32, f64) {
        let n = (SR * secs) as usize;
        let mut peak = 0.0f32;
        let mut energy = 0.0f64;
        for _ in 0..n {
            let (l, r) = m.process();
            peak = peak.max(l.abs()).max(r.abs());
            energy += (l * l + r * r) as f64;
        }
        (peak, energy)
    }

    /// Silent until something moves, and quiet again after.
    #[test]
    fn it_makes_no_sound_on_its_own() {
        let mut m = Mechanics::default();
        let (peak, _) = run(&mut m, 0.5);
        assert_eq!(peak, 0.0, "a piano at rest is silent");
        m.key_struck(60, 0.8);
        let (struck, _) = run(&mut m, 0.05);
        assert!(struck > 1e-4, "the action made no sound");
        // Skip past the burst's own tail before asking whether it has stopped.
        let _ = run(&mut m, 0.2);
        let (after, _) = run(&mut m, 0.5);
        assert!(after < struck * 0.01, "the noise did not stop");
    }

    /// A firmer touch is a louder mechanism, but not proportionally so: at
    /// pianissimo the action is a larger share of what you hear, which is part
    /// of why a quiet piano still sounds like a piano.
    #[test]
    fn a_firmer_touch_is_noisier_but_not_in_proportion() {
        let energy = |nvel: f32| {
            let mut m = Mechanics::default();
            m.key_struck(60, nvel);
            run(&mut m, 0.2).1
        };
        let soft = energy(0.1);
        let hard = energy(1.0);
        assert!(hard > soft * 2.0, "a hard blow should be clearly louder");
        assert!(hard < soft * 40.0, "and not a hundred times louder");
    }

    /// The pedal is the loudest thing the machine does with no note played.
    #[test]
    fn the_pedal_is_audible_on_its_own() {
        let mut m = Mechanics::default();
        m.pedal(true);
        let (down, _) = run(&mut m, 0.2);
        let mut m2 = Mechanics::default();
        m2.damper_lifted(60);
        let (lift, _) = run(&mut m2, 0.2);
        assert!(down > lift * 2.0, "pressing the pedal moves every damper at once");
    }

    /// The mechanism is spread across the instrument, so its two sides are not
    /// the same signal.
    #[test]
    fn the_noise_is_not_mono() {
        let mut m = Mechanics::default();
        m.key_struck(60, 0.9);
        m.damper_landed(48);
        let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
        for _ in 0..(SR as usize / 10) {
            let (l, r) = m.process();
            ll += (l * l) as f64;
            rr += (r * r) as f64;
            lr += (l * r) as f64;
        }
        let c = lr / (ll.sqrt() * rr.sqrt()).max(1e-30);
        assert!(c.abs() < 0.9, "the two sides correlate at {c:.3}");
    }

    /// The mechanism is a wooden knock, not a hiss. Its energy has to sit low:
    /// measured against a real piano, a single pole of low-pass put 29 dB too
    /// much into 4-8 kHz and 41 dB too much above it.
    #[test]
    fn the_noise_is_low_and_not_hissy() {
        let mut m = Mechanics::default();
        m.key_struck(60, 1.0);
        m.pedal(true);
        let n = (SR as usize) / 2;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            let (l, r) = m.process();
            buf.push(((l + r) * 0.5) as f64);
        }
        let mag = |lo: f64, hi: f64| -> f64 {
            let mut acc = 0.0;
            let mut f = lo;
            while f < hi {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, &v) in buf.iter().enumerate() {
                    let w = std::f64::consts::TAU * f * i as f64 / SR as f64;
                    re += v * w.cos();
                    im += v * w.sin();
                }
                acc += re * re + im * im;
                f *= 1.3;
            }
            acc
        };
        let low = mag(100.0, 1000.0);
        let high = mag(4000.0, 16000.0);
        let db = 10.0 * (high / low.max(1e-30)).log10();
        assert!(db < -30.0, "the top is only {db:.0} dB under the body of the knock");
    }

    /// Several events can be in the air at once — a chord is several keys.
    #[test]
    fn overlapping_events_all_sound() {
        let mut m = Mechanics::default();
        for n in [48u8, 52, 55, 60, 64] {
            m.key_struck(n, 0.8);
        }
        let (five, _) = run(&mut m, 0.1);
        let mut m2 = Mechanics::default();
        m2.key_struck(60, 0.8);
        let (one, _) = run(&mut m2, 0.1);
        assert!(five > one * 1.5, "a chord should be noisier than one key");
    }
}

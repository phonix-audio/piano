//! The stringing design: what every string of the instrument physically is.
//!
//! Everything downstream is derived from here — pitch, tension, inharmonicity,
//! how many strings share a note — so nothing about the tone is tabulated by
//! ear. The measurements are Chabassier's table I.2, taken on the IRCAM
//! Steinway D: six control strings across the compass, with their speaking
//! lengths and the diameters of their steel cores and copper windings.
//!
//! | note | D♯1 | C2 | F3 | C♯5 | G6 | C7 |
//! |---|---|---|---|---|---|---|
//! | length (cm) | 194.5 | 160.0 | 97.3 | 33.2 | 12.6 | 8.8 |
//! | core ⌀ (mm) | 1.380 | 0.935 | 1.057 | 0.937 | 0.890 | 0.900 |
//! | winding ⌀ (mm) | 3.390 | 1.725 | — | — | — | — |
//!
//! Between them the length is interpolated geometrically, which is what a
//! scaling design is: the treble cannot halve its length every octave (the
//! instrument would need to be metres longer at the bottom and millimetres at
//! the top), so makers let the bass strings run short and thicken them with
//! copper instead.
//!
//! From the geometry, the rest follows:
//!
//! * linear mass `μ = ρ_steel·A_core + ρ_copper·A_winding`
//! * tension `T = μ (2 L f₀)²`, from the transverse wave speed `c = √(T/μ)`
//! * inharmonicity `B = π³ E d⁴ / (64 T L²)`, the stiff-string coefficient
//!
//! That last one matters: `B` is the single number that decides how stretched a
//! note's partials are, and here it is *computed*. Doing it this way reproduces
//! the Railsback curve on its own — a few times 10⁻⁵ in the bass, where the
//! copper adds mass without adding stiffness, a few times 10⁻⁴ at middle C, and
//! over 10⁻² at the top where the wire is short and thick.

/// Steel core: density and Young's modulus (Chabassier, table I.1).
const RHO_STEEL: f64 = 7850.0;
pub const E_STEEL: f64 = 2.02e11;
/// Copper winding.
const RHO_COPPER: f64 = 8900.0;

/// The six measured control strings: (MIDI note, length m, core ⌀ m, winding ⌀ m).
/// A winding diameter of zero means a plain steel string.
/// The five strings of a Steinway D published with their measured tensions
/// (Chabassier, Joly & Chaigne, JASA 134(1) 2013, Table III), plus C7 carried
/// over from the earlier thesis table to keep the top of the compass anchored.
///
/// The lengths here were already right to within a percent. The DIAMETERS were
/// not, and diameter is what sets the tension: mass per unit length goes as d²,
/// and tension with it. The bass core was 1.380 mm against a measured 1.480 and
/// came out 17% slack; the treble wire was 0.890 against a measured 0.794 and
/// came out 30% tight. A treble string a third over tension is a treble string
/// with the wrong impedance, the wrong decay and the wrong inharmonicity.
const CONTROL: [(f64, f64, f64, f64); 6] = [
    (27.0, 1.945, 1.480e-3, 3.390e-3), // D♯1, wound — T measured 1781 N
    (36.0, 1.600, 0.9502e-3, 1.725e-3), // C2, wound — 865 N
    (53.0, 0.961, 1.0525e-3, 0.0),     // F3 — 774 N
    (73.0, 0.326, 0.921e-3, 0.0),      // C♯5 — 684 N
    (91.0, 0.124, 0.794e-3, 0.0),      // G6 — 587 N
    // C7 is NOT measured. It only carries the length trend past the top control
    // string so the last octave does not freeze; its gauge is solved like every
    // other plain string's.
    //
    // Its LENGTH is what sets the treble's inharmonicity, and it CANNOT be
    // lengthened, which is worth recording because the ear keeps asking for it.
    //
    // B goes as `T/(L^6 f0^4)` — a sixth power — so C7 at 8.8 cm computes to
    // B = 0.0106, stretching its octave partial 36 cents, which is the
    // harpsichord/electric treble reported by ear. Lengthening it to 9.8 cm
    // halves that (B = 0.0056, 19 cents) and still satisfies both the Railsback
    // floor asserted below and Chaigne & Askenfelt's Table I hammer-to-string
    // mass ratio at C7. What it does NOT survive is the third measurement:
    // whatever is done above C7, the top octave comes out longer, a longer string
    // at the same tension is lighter, the hammer stays on it longer relative to
    // its own period, and `the_contact_lasts_as_many_string_periods_as_was_measured`
    // leaves Chaigne 2016's envelope at note 105 (2.42 to 2.64 periods against a
    // 2.40 ceiling). Anchoring the top octave short enough to keep the contact
    // needs a 3.7 cm top string, which is not a string. Measured every way round
    // on 2026-08-21: C7 at 9.0 cm already trips it.
    //
    // So the treble's excess inharmonicity is not reachable from the scale. It
    // has to come from the felt and the hammer mass at the top of the compass,
    // taken together — the same place the model already contradicts itself at
    // treble pianissimo.
    (96.0, 0.098, 0.800e-3, 0.0),
];

/// How many times the density of plain steel a wound string behaves as.
///
/// A wound bass string is a steel core with copper laid helically round it, and
/// working its mass out from the winding's geometry is both fiddly and wrong at
/// the margins: modelling the copper as a solid annulus overstates it by about a
/// quarter, and modelling it as a helix understated it here by nearly a fifth.
/// The measurements sidestep the question — Chabassier, Joly and Chaigne quote
/// the effective density of each string directly, as a multiple of steel:
/// **5.73 at D♯1 and 3.55 at C2**, and 1 for every plain string from F3 up.
///
/// Checked: at D♯1 that gives µ = 0.0774 kg/m, c = 2Lf₀ = 151.3 m/s and
/// T = µc² = 1772 N against the 1781 N they measured.
fn wound_density_factor(note: f64) -> f64 {
    // The two wound control strings, and the plain wire above them.
    const D_SHARP_1: f64 = 27.0;
    const C2: f64 = 36.0;
    const F_LOW: f64 = 5.73;
    const F_C2: f64 = 3.55;
    // Where the winding actually runs out. It is not at C2 — C2 is simply the
    // highest wound string that was measured. On a grand the wound strings cover
    // the bass bridge, which ends around E2, and the maker tapers the copper away
    // so the tension runs smoothly across the break. Dropping the winding in one
    // step at C2 instead divided the string's mass by three and a half from one
    // semitone to the next, and its tension fell to 261 N — no piano has a break
    // like that.
    const PLAIN: f64 = 41.0;
    if note >= PLAIN {
        return 1.0;
    }
    if note >= C2 {
        let t = (note - C2) / (PLAIN - C2);
        return F_C2 * (1.0 / F_C2).powf(t);
    }
    // Geometric between the two measured strings, and continued below the lowest
    // of them on the same taper: the bottom strings carry the most copper.
    let t = (note - D_SHARP_1) / (C2 - D_SHARP_1);
    F_LOW * (F_C2 / F_LOW).powf(t)
}

/// The tension a plain string is built to carry, through the measured control
/// strings of the Steinway D: 866 N at C2, 774 at F3, 684 at C♯5, 587 at G6.
/// Log-linear between them, which is how a scale runs.
fn plain_tension_curve(note: f64) -> f64 {
    // Every tension published for the Steinway D, wound strings included. One
    // curve for the whole instrument: the wound region solves it with copper and
    // the plain region with wire gauge, which is what a scale designer does and
    // why a piano's tension runs evenly across a break that its geometry does
    // not.
    const ANCHORS: [(f64, f64); 5] = [
        (27.0, 1781.0),
        (36.0, 865.0),
        (53.0, 774.0),
        (73.0, 684.0),
        (91.0, 587.0),
    ];
    if note <= ANCHORS[0].0 {
        return ANCHORS[0].1;
    }
    if note >= ANCHORS[4].0 {
        return ANCHORS[4].1;
    }
    for w in ANCHORS.windows(2) {
        let ((n0, t0), (n1, t1)) = (w[0], w[1]);
        if note <= n1 {
            let u = (note - n0) / (n1 - n0);
            return t0 * (t1 / t0).powf(u);
        }
    }
    ANCHORS[4].1
}

/// One note's string, fully specified.
#[derive(Clone, Copy, Debug)]
pub struct StringDesign {
    /// Speaking length, m.
    pub length: f64,
    /// Steel core diameter, m.
    pub core_d: f64,
    /// Outer diameter of the copper winding, m. Zero for a plain string.
    pub wrap_d: f64,
    /// Linear mass, kg/m.
    pub mu: f64,
    /// Resting tension, N.
    pub tension: f64,
    /// Stiff-string inharmonicity coefficient, dimensionless.
    pub b: f64,
    /// Fundamental of the transverse wave, Hz.
    pub f0: f64,
    /// How many strings this note has. A piano's lowest notes have one thick
    /// wound string, the bass-to-tenor break has two, and most of the compass
    /// has three (Chabassier §I.4.4).
    pub strings: u8,
    /// Where the hammer meets the string, as a fraction of its length.
    pub strike: f64,
}

const TOP_TAPER: f64 = 0.65;

#[cfg(test)]
pub(crate) static TAPER_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(f64::to_bits(TOP_TAPER));

#[inline]
fn top_taper() -> f64 {
    #[cfg(test)]
    {
        f64::from_bits(TAPER_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        TOP_TAPER
    }
}

fn interp_geometric(note: f64) -> (f64, f64, f64) {
    if note <= CONTROL[0].0 {
        // Below the lowest measured string the case cannot grow, so the length
        // stays put and the maker holds the tension by winding on more copper.
        // Freezing the geometry instead would swing the tension by a factor of
        // two over the bottom half-octave, which no instrument does.
        let (_, l, d, w) = CONTROL[0];
        if (note - CONTROL[0].0).abs() < 1e-9 {
            return (l, d, w);
        }
        let f_ref = 440.0 * 2f64.powf((CONTROL[0].0 - 69.0) / 12.0);
        let mu_ref = RHO_STEEL * std::f64::consts::PI * 0.25 * d * d
            + RHO_COPPER * winding_volume_per_length(d, w);
        let t_ref = mu_ref * (2.0 * l * f_ref).powi(2);
        let f0 = 440.0 * 2f64.powf((note - 69.0) / 12.0);
        let mu_needed = t_ref / (2.0 * l * f0).powi(2);
        let v = ((mu_needed - RHO_STEEL * std::f64::consts::PI * 0.25 * d * d) / RHO_COPPER).max(0.0);
        // v = (pi^2/4) w (d + w)  ->  w^2 + d w - 4v/pi^2 = 0
        let k = 4.0 * v / (std::f64::consts::PI * std::f64::consts::PI);
        let wire = 0.5 * (-d + (d * d + 4.0 * k).sqrt());
        return (l, d, d + 2.0 * wire.max(0.0));
    }
    let last = CONTROL[CONTROL.len() - 1];
    if note > last.0 {
        // Above the top control string the length keeps shortening on the
        // trend the last pair sets, rather than freezing.
        let (n0, l0, c0, _) = CONTROL[CONTROL.len() - 2];
        // How fast the scale goes on shortening past the last measured string.
        // At 1.0 it continues the G6 trend, which drives B into bell territory:
        // the octave partial reaches +59 cents at note 99 and +119 at 103, where
        // a real Steinway's C7 sits near +10. A maker flattens the top of the
        // scale precisely to hold B down, and `B ~ d^2/(L^4 f0^2)` puts length in
        // at the fourth power.
        let rate = (last.1 / l0).ln() / (last.0 - n0) * top_taper();
        let l = last.1 * (rate * (note - last.0)).exp();
        return (l.max(0.045), (c0 + last.2) * 0.5, 0.0);
    }
    for w in CONTROL.windows(2) {
        let (n0, l0, c0, p0) = w[0];
        let (n1, l1, c1, p1) = w[1];
        if note >= n0 && note <= n1 {
            let t = (note - n0) / (n1 - n0);
            let length = (l0.ln() + t * (l1.ln() - l0.ln())).exp();
            let core = c0 + t * (c1 - c0);
            // A wound string stops being wound at the break; do not interpolate
            // a phantom half-winding across it.
            let wrap = if p0 > 0.0 && p1 > 0.0 {
                (p0.ln() + t * (p1.ln() - p0.ln())).exp()
            } else if p0 > 0.0 && t < 0.5 {
                p0 * (1.0 - 2.0 * t) + core * 2.0 * t
            } else {
                0.0
            };
            return (length, core, wrap.max(0.0));
        }
    }
    (CONTROL[0].1, CONTROL[0].2, CONTROL[0].3)
}

/// Copper carried per metre of a helically wound string, as a volume per unit
/// length; multiply by the copper's density for the mass.
fn winding_volume_per_length(core_d: f64, wrap_d: f64) -> f64 {
    if wrap_d <= core_d {
        return 0.0;
    }
    let w = 0.5 * (wrap_d - core_d);
    0.25 * std::f64::consts::PI * std::f64::consts::PI * w * (core_d + w)
}

/// Strings per note. The two breaks are where a real instrument changes from
/// one wound string, to two, to three plain ones.
fn strings_for(note: u8) -> u8 {
    match note {
        0..=30 => 1,
        31..=40 => 2,
        _ => 3,
    }
}

/// Where the hammer strikes, as a fraction of the speaking length. Makers put
/// it near a seventh or an eighth so the seventh partial — the one that is
/// badly out of tune with the scale — is barely excited, and move it closer to
/// the end in the treble.
fn strike_for(note: f64) -> f64 {
    // Measured on a Steinway D, note by note (Chabassier, Joly & Chaigne, JASA
    // 134(1) 2013, Table III): striking distance over speaking length comes to
    //
    //     D#1 0.129   C2 0.125   F3 0.120   C#5 0.120   G6 0.121
    //
    // — flat, within a few percent, across five octaves. This ran from 0.125 at
    // the bottom down to 0.070 at the top, on the belief that makers move the
    // strike towards the end in the treble. They move it in ABSOLUTE terms
    // because the strings get short; the RATIO is what a maker holds, and the
    // ratio is what sets which partials the blow misses. Halving it up top moved
    // the whole strike comb.
    //
    // Chaigne & Askenfelt II (Table I) give α = 0.0625 for their C7 — half of
    // this, and the only published figure that disagrees. It is NOT followed, and
    // the reason is which instrument each source measured. Chabassier's five
    // numbers are a note-by-note survey of a Steinway D-274, which is the
    // instrument this model is of; Chaigne's is one note on a piano he does not
    // name, in a table whose stated purpose is to be representative rather than
    // specific. He himself writes that the striking point sits "roughly equal to
    // L/10 from the agraffe" and calls 0.12 at C4 "representative for the
    // striking ratios for this note in most pianos". Five measurements on the
    // target instrument outweigh one on another.
    //
    // The choice is not harmless either way: his own Fig. 14(c) has peak hammer
    // force RISING as the striking ratio falls, so adopting 0.0625 would make the
    // top octave strike harder still — the opposite of what it needs.
    let t = ((note - 21.0) / 87.0).clamp(0.0, 1.0);
    0.129 - 0.009 * t
}

/// Against the five strings of a Steinway D measured by Chabassier, Joly and
/// Chaigne (JASA 134(1) 2013, Table III). Their table is the only note-by-note
/// set of numbers published for this instrument, so it is the yardstick.
#[cfg(test)]
pub fn compare_to_the_steinway() {
    // (MIDI, name, length m, core diameter mm, tension N)
    let measured = [
        (27u8, "D#1", 1.945, 1.48, 1781.0),
        (36, "C2", 1.600, 0.9502, 865.0),
        (53, "F3", 0.961, 1.0525, 774.0),
        (73, "C#5", 0.326, 0.921, 684.0),
        (91, "G6", 0.124, 0.794, 587.0),
    ];
    for n in [33u8, 45, 57, 69, 81, 93] {
        let d = design(n);
        let m = crate::string::StringModes::build(&d, 1.0, 48_000.0);
        let z0 = (d.tension * d.mu).sqrt();
        eprintln!(
            "  note {n}: Z0 {z0:.2} kg/s, w1 {:.0} N/m, rigidite du chevalet {:.3e}, \
             rapport annulation {:.2}",
            m.bridge[0].abs(),
            m.bridge_stiffness,
            m.bridge_stiffness * m.bridge[0].abs() / 1.0e6,
        );
    }
    for n in [] as [u8; 0] {
        let m = design(n);
        let f0 = 440.0 * 2f64.powf((n as f64 - 69.0) / 12.0);
        let c = (m.tension / m.mu).sqrt();
        eprintln!(
            "  note {n}: L {:.4} m, d {:.3} mm, T {:.0} N, f0 attendu {f0:.1} Hz, f0 corde {:.1} Hz, B {:.2e}",
            m.length,
            m.core_d * 1000.0,
            m.tension,
            c / (2.0 * m.length),
            m.b
        );
    }
    eprintln!("        longueur (m)      diametre (mm)      tension (N)");
    eprintln!("note    modele  mesure    modele  mesure     modele  mesure   ecart");
    for (n, name, l, d, t) in measured {
        let m = design(n);
        eprintln!(
            "{name:<5} {:>7.3} {l:>7.3}   {:>7.3} {d:>7.3}   {:>8.0} {t:>7.0}  {:>+5.0}%",
            m.length,
            m.core_d * 1000.0,
            m.tension,
            100.0 * (m.tension / t - 1.0),
        );
    }
}

/// Equal temperament, before the tuner touches it.
fn equal_f0(n: f64) -> f64 {
    440.0 * 2f64.powf((n - 69.0) / 12.0)
}

/// How far above equal temperament each note is tuned, in cents.
///
/// ── Why a piano is not tuned in equal temperament ──────────────────────────
///
/// A tuner sets an octave by ear, and the beat he listens to is between the
/// upper note's FUNDAMENTAL and the lower note's SECOND PARTIAL. That partial is
/// not at twice the fundamental — string stiffness puts it at
/// `2 f0 sqrt(1 + 4B)` — so a beatless octave is WIDER than 2:1. That widening,
/// accumulated up and down from the middle, is the Railsback curve, and every
/// piano on earth carries it.
///
/// This model did not. Measured on its own strings, tuned to exact equal
/// temperament, the octave beat runs 0.33 Hz at C4-C5, 1.86 at C5-C6, 10.06 at
/// C6-C7 and **15.33 Hz at Eb6-Eb7**. Fifteen hertz is not a beat any more, it is
/// a rattle: in the nocturne's octave-doubled passage the top note's level swings
/// 10 to 17 dB every twenty milliseconds, which is heard as the note breaking up.
/// It also mistunes partial against partial across the whole treble, which is a
/// large part of what makes that register sound like a bell rather than a piano.
///
/// Built here from the instrument's OWN inharmonicity rather than from a fitted
/// curve: walk out from A4 an octave at a time, each step adding the lower note's
/// own `1200 log2 sqrt(1 + 4B)`, and interpolate between the anchors in
/// semitones. B is taken from the equal-tempered design, which is a few cents away
/// and moves B by far less than a percent.
fn stretch_cents(note: f64) -> f64 {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[f64; 128]> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        // The anchor is the octave A4..G#5, left at equal temperament; every other
        // note is propagated from it an octave at a time, so EVERY octave pair on
        // the keyboard is beatless and not just those that land on an anchor.
        let excess = |n: f64| -> f64 {
            let b = design_with_f0(n as u8, equal_f0(n)).b;
            1200.0 * (1.0 + 4.0 * b).sqrt().log2()
        };
        // Capped at what a tuner actually does. Railsback's curve reaches about
        // thirty cents at the top of the keyboard and stops; this model's top
        // octave carries an inharmonicity its own notes call harpsichord-like
        // (B = 0.045 at note 104, see `print_inharmonicity_and_stretch`), and
        // stretching to follow a B that is itself wrong follows the error twice.
        // Left uncapped it also broke the contact at note 108, which then
        // delivered 2.3 times the hammer's momentum.
        const CAP: f64 = 30.0;
        let mut out = [0.0f64; 128];
        for m in 81..128usize {
            out[m] = (out[m - 12] + excess((m - 12) as f64)).min(CAP);
        }
        for m in (0..69usize).rev() {
            out[m] = out[m + 12] - excess(m as f64);
        }
        // Released over the last octave. Two reasons, both measured. The model's
        // own inharmonicity up there is not trustworthy — B reaches 0.045 at note
        // 104, which its own notes call harpsichord territory — so stretching to
        // follow it follows the error twice. And note 108's contact is a handful
        // of samples and sits on the edge: fifteen cents of stretch was enough to
        // make it deliver 2.3 times the hammer's momentum, which is energy out of
        // nothing. Every octave the music actually plays is below C7 and keeps its
        // full stretch.
        // Held to note 99 and released over the nine semitones above it, so every
        // octave a piano part reaches keeps its full stretch and only the last few
        // keys give it up.
        for m in 100..128usize {
            let t = ((m - 99) as f64 / 9.0).min(1.0);
            out[m] = out[99] * (1.0 - t);
        }

        out
    });
    t[(note.round().clamp(0.0, 127.0)) as usize]
}

pub fn design(note: u8) -> StringDesign {
    let n = note as f64;
    design_with_f0(note, equal_f0(n) * 2f64.powf(stretch_cents(n) / 1200.0))
}

fn design_with_f0(note: u8, f0: f64) -> StringDesign {
    let n = note as f64;
    let (length, core_d, wrap_d) = interp_geometric(n);
    let a_core = std::f64::consts::PI * 0.25 * core_d * core_d;
    // A wound string is a round copper wire laid helically around the core, not
    // a solid copper tube: taking the annulus as solid overstates the mass by
    // about a quarter, and the tension with it. One turn of wire of diameter w
    // has length ~pi(d+w), and there are 1/w turns per metre.
    // For a PLAIN string the wire gauge is not interpolated — it is SOLVED for.
    //
    // A scale designer does not pick a diameter and see what tension comes out;
    // they hold the tension along a smooth curve and choose the wire that gives
    // it. Interpolating the diameter instead let the tension collapse to 343 N
    // around the tenor break, where the winding runs out and the strings are
    // still long: no piano is built like that, and the break is exactly where a
    // maker works hardest to keep the tension even.
    //
    // The measured control strings are the curve's anchors, so this reproduces
    // them exactly and only decides what happens BETWEEN them.
    let wound = wound_density_factor(n);
    let speed = 2.0 * length * f0;
    let (a_core, mu) = if wound > 1.0 {
        // Wound: the core is the measured wire and the COPPER is what holds the
        // tension. That is how the bottom of a piano is built — the case cannot
        // grow, so the maker winds on more metal.
        let mu_needed = plain_tension_curve(n) / (speed * speed);
        (a_core, mu_needed.max(RHO_STEEL * a_core))
    } else {
        let mu_needed = plain_tension_curve(n) / (speed * speed);
        (mu_needed / RHO_STEEL, mu_needed)
    };
    let core_d = (4.0 * a_core / std::f64::consts::PI).sqrt();
    let tension = mu * speed * speed;
    // B = π³ E d⁴ / (64 T L²). Only the STEEL CORE carries bending stiffness —
    // a winding adds mass and no stiffness, which is exactly why a piano's bass
    // is so much closer to harmonic than its thickness alone would suggest.
    let b = std::f64::consts::PI.powi(3) * E_STEEL * core_d.powi(4)
        / (64.0 * tension * length * length);
    StringDesign {
        length,
        core_d,
        wrap_d,
        mu,
        tension,
        b,
        f0,
        strings: strings_for(note),
        strike: strike_for(n),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn print_against_the_steinway() {
        super::compare_to_the_steinway();
    }

    use super::*;

    #[test]
    #[ignore]
    fn print_the_scale() {
        println!("note    f0      L(cm)  core(mm) wrap(mm)  mu(g/m)   T(N)      B        cordes strike");
        for note in (21u8..=108).step_by(3) {
            let d = design(note);
            println!(
                "{note:4}  {:7.1}  {:6.1}  {:7.3}  {:7.3}  {:7.2}  {:7.0}  {:9.2e}  {:3}   {:.3}",
                d.f0, d.length * 100.0, d.core_d * 1e3, d.wrap_d * 1e3,
                d.mu * 1000.0, d.tension, d.b, d.strings, d.strike
            );
        }
        let total: f64 = (21u8..=108).map(|n| { let d = design(n); d.tension * d.strings as f64 }).sum();
        println!("\ntension totale sur le cadre : {:.0} N  ({:.1} tonnes-force)", total, total / 9810.0);
    }

    /// The five strings of the Steinway D, held to account rather than printed.
    ///
    /// Chabassier, Joly & Chaigne, JASA 134(1) 2013, Table III, is the only
    /// note-by-note survey published for this instrument, and every length,
    /// gauge and tension in `scale.rs` descends from it. It was reproduced
    /// exactly — tensions to the newton, lengths and gauges inside half a
    /// percent — and until now that fact lived in an `#[ignore]`d print. A table
    /// that is only printed is a table that can drift without anyone noticing,
    /// which is the whole reason the rest of this module is pinned.
    ///
    /// Tension is the one to watch. It is not a free choice: it follows from the
    /// length, the gauge and the pitch, so it is where an error in any of the
    /// three shows up first, and it is the number a scale is actually built to.
    #[test]
    fn it_matches_the_steinway_table() {
        // (MIDI, name, length m, core diameter mm, tension N)
        const TABLE: [(u8, &str, f64, f64, f64); 5] = [
            (27, "D#1", 1.945, 1.480, 1781.0),
            (36, "C2", 1.600, 0.950, 865.0),
            (53, "F3", 0.961, 1.052, 774.0),
            (73, "C#5", 0.326, 0.921, 684.0),
            (91, "G6", 0.124, 0.794, 587.0),
        ];
        for (n, name, l, c, t) in TABLE {
            let d = design(n);
            assert!(
                (d.length - l).abs() < l * 0.005,
                "{name}: length {:.4} m against the measured {l:.4}",
                d.length
            );
            assert!(
                (d.core_d * 1000.0 - c).abs() < c * 0.01,
                "{name}: core {:.3} mm against the measured {c:.3}",
                d.core_d * 1000.0
            );
            assert!(
                (d.tension / t - 1.0).abs() < 0.02,
                "{name}: tension {:.0} N against the measured {t:.0} — a scale is built \
                 to its tensions, so this is where a wrong length or gauge surfaces",
                d.tension
            );
        }
    }

    /// The design must reproduce the strings it was built from.
    #[test]
    fn it_returns_the_measured_control_strings() {
        // Lengths exactly; gauges to within a percent.
        //
        // A plain string's wire is no longer read off the table — it is SOLVED
        // from the tension the maker holds, because interpolating gauges between
        // the control strings let the tension collapse to 343 N around the tenor
        // break. Solving it reproduces the measured wire to a few tenths of a
        // percent at the anchors and, far more importantly, reproduces the
        // measured TENSION exactly, which is the number a scale is built to.
        for (n, l, c, w) in CONTROL {
            let d = design(n as u8);
            assert!((d.length - l).abs() < 1e-6, "note {n}: length {} vs {l}", d.length);
            if n >= 96.0 {
                // C7 is an extrapolation of the length trend, not a measurement:
                // there is no published gauge for it to match.
                continue;
            }
            assert!(
                (d.core_d - c).abs() < c * 0.01,
                "note {n}: core {:.4} mm vs the measured {:.4}",
                d.core_d * 1000.0,
                c * 1000.0
            );
            if w > 0.0 {
                assert!(d.wrap_d > c, "note {n}: a wound string needs copper on it");
            }
        }
    }

    /// Tension is the number a maker actually cares about, and it is what tells
    /// us the scaling is not nonsense: a piano string pulls somewhere around
    /// half a tonne-force spread over the compass, never a few newtons and
    /// never several thousand.
    #[test]
    fn every_string_pulls_a_plausible_tension() {
        for note in 21u8..=108 {
            let d = design(note);
            assert!(
                // The upper bound was 1600 N, which is tighter than the
                // instrument: the D♯1 of the Steinway D that Chabassier, Joly
                // and Chaigne measured pulls 1781 N.
                d.tension > 400.0 && d.tension < 2000.0,
                "note {note}: {:.0} N is not a piano string",
                d.tension
            );
        }
    }

    /// The Railsback curve, derived rather than tabulated. Published values for
    /// a grand run from a few times 10⁻⁵ in the bass to over 10⁻² at the top,
    /// and the curve must rise all the way — it is the single number that sets
    /// how stretched each note's partials are.
    #[test]
    #[ignore]
    fn print_inharmonicity_and_stretch() {
        // Real Steinway D published B (Railsback / Young): C7 ~ 0.002-0.005.
        // Partial n frequency = n*f0*sqrt(1+B*n^2); the cents stretch of the
        // n-th partial from pure = 1200*log2(sqrt(1+B*n^2)).
        for note in [60u8, 72, 84, 88, 92, 96, 100, 104, 108] {
            let d = design(note);
            let cents = |n: f64| 1200.0 * (1.0 + d.b * n * n).sqrt().log2();
            println!(
                "note {note:3} f0 {:6.0}  B {:.4e}   H2 +{:5.1}c  H4 +{:6.1}c  H8 +{:6.1}c",
                d.f0, d.b, cents(2.0), cents(4.0), cents(8.0)
            );
        }
    }

    #[test]
    fn inharmonicity_follows_the_railsback_curve() {
        let b21 = design(21).b;
        let b36 = design(36).b;
        let b60 = design(60).b;
        let b96 = design(96).b;
        assert!(b36 > 1e-5 && b36 < 1.5e-4, "C2 B = {b36:e}");
        assert!(b60 > 1.5e-4 && b60 < 6e-4, "C4 B = {b60:e}");
        assert!(b96 > 4e-3 && b96 < 6e-2, "C7 B = {b96:e}");
        assert!(b21 < b60 && b60 < b96, "B must climb into the treble");
        for note in 22u8..=108 {
            assert!(
                design(note).b >= design(note - 1).b * 0.75,
                "note {note}: B dips sharply below its neighbour"
            );
        }
    }

    /// A wound string carries its stiffness in the core alone. Take the winding
    /// off C2 and, at the same pitch, it would be markedly more inharmonic —
    /// the reason the bass is wound in the first place.
    #[test]
    fn winding_adds_mass_without_stiffness() {
        let d = design(36);
        assert!(d.wrap_d > d.core_d, "C2 should be wound");
        let plain_mu = RHO_STEEL * std::f64::consts::PI * 0.25 * d.core_d * d.core_d;
        let _ = winding_volume_per_length(d.core_d, d.wrap_d);
        assert!(d.mu > plain_mu * 2.5, "the winding should dominate the mass");
        let plain_tension = plain_mu * (2.0 * d.length * d.f0).powi(2);
        let plain_b = std::f64::consts::PI.powi(3) * E_STEEL * d.core_d.powi(4)
            / (64.0 * plain_tension * d.length * d.length);
        assert!(plain_b > d.b * 2.0, "unwound, the same wire would be far stiffer");
    }

    /// Lengths shorten monotonically and stay inside a real instrument.
    #[test]
    fn the_scale_shortens_from_bass_to_treble() {
        for note in 22u8..=108 {
            assert!(design(note).length <= design(note - 1).length + 1e-9, "note {note}");
        }
        assert!(design(21).length < 2.1, "a concert grand is under three metres");
        assert!(design(108).length > 0.04, "the top string is still a string");
    }

    /// Strings per note and strike point, the two things the geometry does not
    /// give on its own.
    /// The striking ratio must be the one measured on the Steinway D, note by
    /// note.
    ///
    /// Chabassier, Joly & Chaigne (JASA 134(1) 2013, Table III), striking distance
    /// over speaking length:
    ///
    /// ```text
    ///     D#1 0.129   C2 0.125   F3 0.120   C#5 0.120   G6 0.121
    /// ```
    ///
    /// Flat within a few percent across five octaves, which is the point: a maker
    /// holds the RATIO, not the distance, and the ratio is what decides which
    /// partials the blow misses. A law that ran to 0.070 at the top moved the
    /// whole strike comb and was heard as a change of instrument between
    /// registers.
    ///
    /// Chaigne & Askenfelt II, Table I give 0.0625 for their C7 and it is
    /// deliberately not followed — see the comment on `strike_for`. This test
    /// exists so that choice stays a decision with a reason rather than drifting
    /// back in unnoticed.
    #[test]
    fn the_strike_point_is_where_it_was_measured() {
        const MEASURED: [(u8, f64); 5] =
            [(27, 0.129), (36, 0.125), (53, 0.120), (73, 0.120), (91, 0.121)];
        for (note, want) in MEASURED {
            let got = design(note).strike;
            assert!(
                (got - want).abs() < 0.006,
                "note {note}: strike ratio {got:.4}, measured {want:.3}"
            );
        }
        // And flat: no register may sit a tenth away from another.
        let all: Vec<f64> = (21u8..=108).map(|n| design(n).strike).collect();
        let lo = all.iter().cloned().fold(f64::MAX, f64::min);
        let hi = all.iter().cloned().fold(0.0f64, f64::max);
        assert!(
            hi / lo < 1.15,
            "the strike ratio runs from {lo:.3} to {hi:.3}; the measured set spans 0.120 to 0.129"
        );
    }

    #[test]
    fn choirs_and_strike_points_are_sane() {
        assert_eq!(design(21).strings, 1);
        assert_eq!(design(36).strings, 2);
        assert_eq!(design(60).strings, 3);
        for note in 21u8..=108 {
            let s = design(note).strike;
            assert!(s > 0.05 && s < 0.14, "note {note}: strike at {s}");
        }
        assert!(design(21).strike > design(108).strike);
    }
}

#[cfg(test)]
mod octave_beat_probe {
    /// The very top, where the ear reports little bells. A bell is partials that
    /// do not line up; a piano's are stretched, but only a little. Real Steinway
    /// C7 measures B around 0.003 to 0.005, which puts its octave partial some ten
    /// cents sharp.
    #[test]
    #[ignore]
    fn how_bell_like_is_the_top() {
        eprintln!("  note    B         partiel 2   partiel 3   partiel 4   (cents au-dessus du juste)");
        for n in [84u8, 90, 96, 99, 103, 108] {
            let d = super::design(n);
            let c = |k: f64| 1200.0 * (1.0 + d.b * k * k).sqrt().log2();
            eprintln!(
                "   {n:3}  {:.4}   {:+8.0}    {:+8.0}    {:+8.0}",
                d.b, c(2.0), c(3.0), c(4.0)
            );
        }
        eprintln!("  un vrai Steinway au do7 : B ~ 0.004, partiel 2 a +10 cents environ");
        eprintln!("  effet d'aplatir le haut du diapason :");
        for tap in [1.0f64, 0.8, 0.65, 0.5] {
            super::TAPER_OVERRIDE.store(tap.to_bits(), std::sync::atomic::Ordering::Relaxed);
            let mut line = format!("   taper {tap:.2} ");
            for n in [96u8, 99, 103, 108] {
                let d = super::design(n);
                line += &format!(
                    "| {n}: L {:4.0} mm B {:.4} P2 {:+4.0}c ",
                    d.length * 1000.0,
                    d.b,
                    1200.0 * (1.0 + 4.0 * d.b).sqrt().log2()
                );
            }
            eprintln!("{line}");
        }
        super::TAPER_OVERRIDE.store(1.0f64.to_bits(), std::sync::atomic::Ordering::Relaxed);
        eprintln!("  geometrie du haut : longueur, fil, tension");
        for n in [90u8, 96, 99, 103, 108] {
            let d = super::design(n);
            eprintln!(
                "   {n:3}  L {:5.1} mm   fil {:.3} mm   T {:5.0} N   f0 {:6.0} Hz",
                d.length * 1000.0,
                d.core_d * 1000.0,
                d.tension,
                d.f0
            );
        }
        eprintln!("  Steinway D en haut : fil 0.85 a 0.95 mm, tension 700 a 800 N");
    }

    /// A tuner stretches the octaves so that the upper note lands ON the lower
    /// note's second partial; if it does not, the two beat, and at the top of the
    /// keyboard that beat is fast enough to be heard as the note breaking up
    /// rather than as a wave. The nocturne doubles this passage at the octave, so
    /// it is exactly what is under the ear.
    #[test]
    #[ignore]
    fn do_the_octaves_beat() {
        eprintln!("  octave par octave : le 2e partiel du grave contre le fondamental de l'aigu");
        eprintln!("  et l'etirement lui-meme, en cents contre le tempere egal :");
        for n in [21u8, 33, 45, 57, 69, 81, 93, 105] {
            eprintln!("    note {n:3} : {:+6.1} cents", super::stretch_cents(n as f64));
        }
        for lo in [60u8, 72, 75, 84, 87] {
            let hi = lo + 12;
            let dl = super::design(lo);
            let dh = super::design(hi);
            let p2 = 2.0 * dl.f0 * (1.0 + 4.0 * dl.b).sqrt();
            let beat = p2 - dh.f0;
            eprintln!(
                "    {lo:3} -> {hi:3} : partiel 2 du grave {p2:8.2} Hz, fondamental de l'aigu {:8.2} Hz, battement {beat:6.2} Hz ({:+5.1} cents)",
                dh.f0,
                1200.0 * (p2 / dh.f0).log2()
            );
        }
    }
}

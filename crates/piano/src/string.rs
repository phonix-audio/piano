//! A piano string, as a set of modes and the two places anything touches it.
//!
//! ## Frequencies
//!
//! A stiff string's partials are stretched: `f_k = k·f₀·√(1 + B k²)`
//! (Chabassier §I.1). `B` is not a taste parameter here — `scale.rs` computes it
//! from the wire's own geometry, so the stretch of every note follows from how
//! the instrument is strung.
//!
//! ## Mode shapes
//!
//! Pinned at both ends, so `φₖ(x) ∝ sin(kπx/L)`. The constant matters: the modal
//! equations in `modal_bank` assume mass-normalised modes (`φᵀMφ = I`), which
//! for a string of total mass `Mₛ` means
//!
//! ```text
//!     φₖ(x) = √(2/Mₛ) · sin(kπx/L)
//! ```
//!
//! Get that wrong and forces are in the wrong units, so the hammer either
//! bounces off a wall or sinks through the string.
//!
//! ## The bridge
//!
//! The string is pinned at the bridge, so it does not *move* there — it *pulls*.
//! The transverse force it applies is `T·∂y/∂x` at the end, which in modal terms
//! is a second weight vector
//!
//! ```text
//!     wₖ = T · √(2/Mₛ) · (kπ/L) · (−1)^{k+1}
//! ```
//!
//! The same vector works in reverse: when the bridge moves, it drives the string
//! through exactly these weights. Using one vector for both directions is what
//! keeps the string/soundboard exchange energetically honest.
//!
//! ## Damping, and what actually stops a piano note
//!
//! The losses written here are the string's OWN — the steel's internal friction
//! and the air it drags — in the form Chabassier gives (§I.1.135) and Chaigne &
//! Askenfelt fit:
//!
//! ```text
//!     σₖ = b₁ + b₃ ωₖ²
//! ```
//!
//! Both coefficients are MEASURED, not chosen: see `B1` and `b3_for` below. `b₃`
//! in particular is not constant over the compass, and treating it as constant
//! was the single largest error this file carried.
//!
//! They are deliberately small. A steel string left to itself would ring for
//! tens of seconds; what actually stops a piano note is that the bridge carries
//! its energy away into the soundboard. So the decay of a note is NOT set here.
//! It comes out of the coupling, which is also where the double decay, the
//! beating of a unison and the sympathetic ring of the undamped strings come
//! from. An earlier version of this instrument tabulated a T60 per note and got
//! all of those wrong, because none of them are properties of a string alone.

use super::modal_bank::Mode;
use super::scale::{StringDesign, E_STEEL};

/// Modes per string. A bass string needs a great many: capped at 128 a bottom A
/// stops at 3.5 kHz and the note has nothing at all above it, which is most of
/// what made this instrument read as a plucked one rather than a struck one. The
/// cap is what the top of hearing costs on the longest string.
pub const MAX_MODES: usize = 420;

/// Chaigne & Askenfelt II, Table I: `b₁ = 0.5 s⁻¹` at C2, C4 and C7 alike, the
/// same value across seven octaves. Both coefficients there were "estimated from
/// experimental data by means of standard curve-fitting procedures" against
/// measured string velocities, and the model built on them reproduced the
/// measured contact durations within 6%. They are not free parameters here.
const B1: f64 = 0.5;
/// HF-damping correction for the control-rate coupling deficit; see its use in
/// `build` and `engine::COUPLE_K`. Extra sigma = COUPLE_COMP * w^2.
const COUPLE_COMP: f64 = 1e-9;

/// How much tighter the bridge's transformer is than the bare ratio below. See
/// `bridge_ratio` for what sets it.
const BRIDGE_TRANSFORMER: f64 = 0.75;

/// `b₃`, the viscoelastic loss, from the same table — and the one number that
/// was badly wrong.
///
/// It is fitted at **6.25e-9 s at both C2 (65.4 Hz) and C4 (262 Hz)** and at
/// **2.6e-10 s at C7 (2093 Hz)**: flat through the bass and mid, then a
/// twenty-fourfold collapse over the top three octaves. This model carried one
/// constant, 5e-9, over the whole compass.
///
/// What that cost is not subtle. Loss goes as `b₃ω²`, so at C7 the fifth partial
/// (10 kHz) decayed with `σ = 20.0` where the measurement gives `1.53` — a T60 of
/// 0.35 s against 4.5 s, **thirteen times too much damping**. The treble's upper
/// partials were being extinguished while the attack was still sounding, which is
/// why it read as three partials and a thud. I had put that down to the physics of
/// the excitation, citing Askenfelt on long contacts exciting high partials
/// weakly; the excitation was fine and the string was eating them.
///
/// Held flat outside the measured span rather than extrapolated: below C4 the two
/// anchors agree, and above C7 there is nothing measured to extrapolate from.
/// Extra HF damping in the WOUND bass, an unmeasured region.
///
/// Chaigne & Askenfelt's lowest anchor is C2 (65 Hz). Below it the strings are
/// copper-wound and markedly lossier than plain steel, and `b3_for` used to hold
/// flat at the C2 value all the way to A0 — so a 6 kHz partial of the bottom A
/// rang with T60 = 0.73 s. Eight bass notes over a second then piled a dense
/// forest of long-ringing high partials that beat into the "frisure" the user
/// heard. Confirmed by ear: capping those partials removed it.
///
/// This fills the unmeasured octave and a half with the correct physical trend —
/// more loss the deeper into the wound bass — as a multiplier on b3 that is 1 at
/// C2 and BASS_BOOST_A0 at the bottom note, interpolated on a log-frequency axis.
const BASS_BOOST_A0: f64 = 6.0;
const F_C2: f64 = 65.4;

#[cfg(test)]
pub static BASS_BOOST_OVERRIDE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

fn bass_hf_boost(f0: f64) -> f64 {
    let a0 = {
        #[cfg(test)]
        {
            let o = BASS_BOOST_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
            if o > 0 { o as f64 / 10.0 } else { BASS_BOOST_A0 }
        }
        #[cfg(not(test))]
        {
            BASS_BOOST_A0
        }
    };
    if f0 >= F_C2 {
        1.0
    } else {
        // 1 at C2, a0 at A0 (27.5 Hz), on a log axis.
        let t = (f0 / F_C2).ln() / (27.5f64 / F_C2).ln();
        a0.powf(t.clamp(0.0, 1.0))
    }
}

/// HF corner (Hz) of the bridge coupling for the WOUND bass, at A0.
///
/// A copper-wound string is a coherent string only up to a frequency set by its
/// winding pitch; above it the winding no longer moves as one, so the string's
/// high partials are weaker AND transmit less force to the bridge. This is why a
/// wound bass string sounds duller than its length and tension imply — and the
/// model, treating it as bare steel, delivered those partials to the board at full
/// strength (the bridge weight grows as k), which is the "frisure" of eight bass
/// notes piling long, loud high partials.
///
/// So the bridge weight is rolled off above a corner that is low in the wound bass
/// (A0) and effectively absent by C2, mirroring where the strings stop being
/// wound. Second order.
const BRIDGE_HF_CORNER_A0: f64 = 2200.0;

#[cfg(test)]
pub static BRIDGE_CORNER_OVERRIDE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

fn bridge_hf_corner(f0: f64) -> f64 {
    let a0 = {
        #[cfg(test)]
        {
            let o = BRIDGE_CORNER_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
            if o > 0 { o as f64 } else { BRIDGE_HF_CORNER_A0 }
        }
        #[cfg(not(test))]
        {
            BRIDGE_HF_CORNER_A0
        }
    };
    // FLAT across the wound bass, then released over a short span up to the plain
    // tenor. The winding is the reason, and it is either there or it is not.
    if f0 <= F_C2 {
        a0
    } else if f0 >= 2.0 * F_C2 {
        1.0e9
    } else {
        let t = (f0 / F_C2).ln() / 2.0f64.ln();
        a0 * (1.0e9f64 / a0).powf(t.clamp(0.0, 1.0))
    }
}

fn b3_for(f0: f64) -> f64 {
    const MID: f64 = 6.25e-9;
    const TOP: f64 = 2.6e-10;
    const F_MID: f64 = 262.0;
    const F_TOP: f64 = 2093.0;
    let base = if f0 <= F_MID {
        MID
    } else if f0 >= F_TOP {
        TOP
    } else {
        let t = (f0 / F_MID).ln() / (F_TOP / F_MID).ln();
        MID * (TOP / MID).powf(t)
    };
    base * bass_hf_boost(f0)
}

/// One string's modal description.
#[derive(Clone, Debug, Default)]
pub struct StringModes {
    pub modes: Vec<Mode>,
    /// `φₖ` at the hammer's strike point.
    pub strike: Vec<f64>,
    /// Modal coordinate to bridge force, in newtons.
    pub bridge: Vec<f64>,
    /// Total mass of the string, kg.
    pub mass: f64,
    /// Turns the transverse modal state into the string's stretch, and so into
    /// the change in its tension: `ΔT = Σ eₖ qₖ²`.
    ///
    /// A string that moves is a string that is longer, and a longer string is a
    /// tighter one. That tension wobbles at twice the frequency of whatever is
    /// moving, and it is the only route by which a struck string produces
    /// anything a transverse partial cannot account for. Bank quotes Conklin:
    /// the resulting phantom partials are "only about 10 dB lower in amplitude
    /// than the nearest real partials" — and measurement here says the bass of a
    /// linear model is 40 dB short above 2 kHz, which is precisely what is
    /// missing.
    pub elong: Vec<f64>,
    /// The compliance the TRUNCATION threw away, at the strike point.
    ///
    /// A string has infinitely many modes and this bank keeps a few hundred. The
    /// ones dropped carry almost no sound — but between them they carry a real
    /// share of how far the string GIVES when something presses on it, and they
    /// give it instantly, because they are all far above the timescale of the
    /// blow. Leave them out and the hammer meets a string that is too stiff: this
    /// repo already measured 17 modes making a C6 string fifteen times too stiff
    /// at one millisecond. A hammer rebounding off something too hard leaves too
    /// cleanly, its force pulse comes out too smooth, and a smooth pulse has no
    /// high harmonics in it — which is exactly the fault heard as a plucked or
    /// electric tone.
    ///
    /// It needs no fitting. A string pressed at `x` with a steady force `F`
    /// deflects by `F·x(L−x)/(T·L)` — that is the WHOLE compliance, over every
    /// mode there is. Subtract what the retained modes account for and the
    /// remainder is exactly what was lost, per newton.
    pub residual_compliance: f64,
    /// The share of the hammer's force the TRUNCATION stops reaching the bridge.
    ///
    /// The companion to `residual_compliance`, at the other end of the string, and
    /// the more damaging of the two. A point force `F` at `x_H` is held up by both
    /// terminations: the bridge takes `F·a` and the agraffe `F·(1−a)`, exactly, by
    /// statics. Through the modes that transmission is
    ///
    /// ```text
    ///     Σ bridgeₖ·strikeₖ/ωₖ²  =  Σ (−1)^{k+1} (2/πk) sin(πka)  =  a
    /// ```
    ///
    /// using `Σ (−1)^{k+1} sin(kθ)/k = θ/2`. The sum is right — and it converges
    /// like `1/k`, which is to say hardly at all. In the bass, 420 partials reach
    /// it. **At the top of the compass FOUR partials remain below Nyquist**, and
    /// the partial sum is 0.0495 against 0.12: the treble delivers two fifths of
    /// the force the hammer actually puts through it, and the missing three fifths
    /// are not physics but arithmetic that was stopped early.
    ///
    /// Measured symptom this accounts for: the bridge force is flat at about 7 N
    /// from F1 to D#5 and then falls to 5.5, 2.3, 1.1, 1.0, 0.5 — collapsing
    /// exactly as the partial count passes below twenty, while first principles
    /// (`Fₖ = 4·f₀·J·sin(πka)`) say it should RISE with pitch.
    ///
    /// So the remainder is put back. It costs nothing where it is not needed,
    /// because in the bass the sum has already converged and this is zero; it is
    /// self-targeting. And it takes the force of THIS sample, not the last, so it
    /// carries none of the lag that keeps `residual_compliance` switched off.
    pub residual_bridge: f64,
    /// The string's longitudinal modes, which the same tension change drives.
    /// Their fundamental sits at `√(E·A/T)` times the transverse one — a ratio
    /// that depends only on how hard the string is stretched, which is why Bank
    /// reports it as 42 to 52 semitones "constant irrespectively of the tuning".
    pub long_modes: Vec<Mode>,
    /// Longitudinal modal coordinate to bridge force, in newtons: `EA·∂ξ/∂x`
    /// evaluated at the termination, so `±EA·norm·mπ/L` — the same shape as the
    /// transverse `bridge` with `EA` in place of `T`.
    pub long_bridge: Vec<f64>,
    /// `φ_m` at the strike point, for the longitudinal modes.
    ///
    /// This is the weight the hammer drives them with, and it is the whole
    /// reason they exist. Chaigne & Askenfelt close their paper by naming what
    /// their synthesis lacked: "the essential missing feature is in the attack
    /// component, which for a real piano tone includes a strong thump... a large
    /// part of the thump originates from the soundboard, which is set in motion
    /// almost immediately at the hammer impact by a **longitudinally transmitted
    /// pulse train reflecting between bridge and hammer**. This longitudinal
    /// motion precedes the first transversal string pulse by 1-2 ms."
    ///
    /// The source is NOT the tension integral, and that distinction is why the
    /// previous attempt at this diverged. A uniform tension change on a string
    /// clamped at both ends is taken up by the terminations and excites no
    /// longitudinal mode at all — correct, and the reason forcing them from the
    /// scalar `Σ eₖ qₖ²` sent the instrument to infinity. The real source is the
    /// GRADIENT of the squared slope, `∂/∂x[(EA/2)(∂y/∂x)²]`, and integrating it
    /// against `φ_m` by parts kills the boundary terms and leaves
    ///
    /// ```text
    ///     f_m = (EA/2) · φ_m(x_H) · [ y'(x_H⁺)² − y'(x_H⁻)² ]
    /// ```
    ///
    /// a POINT source at the hammer. A point force `F` bends the string into a
    /// corner whose slopes are `F(1−a)/T` and `−F·a/T`, so the bracket is
    /// `(F/T)²(2a−1)` and the drive is quadratic in the hammer force and nothing
    /// else. It takes no feedback from the string, which is what makes it safe
    /// where the earlier version was not.
    pub long_strike: Vec<f64>,
    /// The whole of the drive above, gathered: `f_m = long_drive · F² · φ_m(x_H)`
    /// with `long_drive = (EA/2)·(2a−1)/T²`.
    pub long_drive: f64,
    /// The rear duplex (aliquot) resonances: undamped high modes tuned to
    /// harmonics of the speaking pitch, shaken through the shared bridge. Empty
    /// below the treble, where there is no tuned duplex.
    pub duplex_modes: Vec<Mode>,
    /// How each duplex mode couples to the bridge, used for both the drive it
    /// takes from the speaking string's bridge force and the force it returns.
    pub duplex_couple: Vec<f64>,
    /// The bridge as an impedance transformer.
    ///
    /// Bank: "The bridge functions as an impedance transformer, presenting
    /// higher impedance to the string than that if the strings were directly
    /// connected to the soundboard. In the latter case decay times would be too
    /// short." Tying the strings straight onto the board — which is what this
    /// model did at first — reproduces that fault exactly: an A4 lost 26 dB in a
    /// second where a real grand loses 12, and the tone came out plucked.
    ///
    /// One number, applied to BOTH directions of the exchange, so it is a real
    /// transformer and not a leak: the board sees `ratio` of the string's pull,
    /// and the string sees `ratio` of the board's motion. Energy transfer goes
    /// as its square, so the note's decay does too.
    pub bridge_ratio: f64,
    /// Sine of the angle the string makes over the bridge. A tension change acts
    /// along the string; only this fraction of it pushes the soundboard.
    pub bridge_angle: f64,
    /// The static stiffness this string adds to the bridge it is tied to,
    /// `Σ wₖ²/ωₖ²`, in N/m.
    ///
    /// This is not a refinement. Attaching a string to a support that can move
    /// contributes a whole quadratic form to the energy: the cross term (which
    /// is the coupling) *and* a term in the support's own displacement, which is
    /// the string's downbearing stiffening the board. Write only the cross term
    /// and the coupled system stops being positive definite — it stays stable
    /// for one or two strings and then diverges once enough of them pull on the
    /// same bridge, which is exactly what happened here at eight voices.
    pub bridge_stiffness: f64,
}


#[cfg(test)]
mod fitted_damping_tests {
    use super::*;

    /// The damping coefficients must BE the fitted ones.
    ///
    /// Chaigne & Askenfelt II, Table I, estimated from measured string
    /// velocities by curve-fitting and then checked against measured contact
    /// durations to within 6%:
    ///
    /// ```text
    ///     b₁ = 0.5 s⁻¹ at C2, C4 and C7 alike
    ///     b₃ = 6.25e-9 s at C2 (65.4 Hz) and C4 (262 Hz), 2.6e-10 s at C7 (2093 Hz)
    /// ```
    ///
    /// This is pinned because it was WRONG for months and nothing caught it: a
    /// single constant 5e-9 across the compass, which over-damped C7's fifth
    /// partial by thirteen times and was heard as an aigu with three partials.
    /// A law that varies by a factor of twenty-four over three octaves cannot be
    /// left to a comment.
    #[test]
    fn the_damping_is_the_fitted_damping() {
        assert!((B1 - 0.5).abs() < 1e-12, "b1 is {B1}, Table I fits 0.5 s^-1");
        for (f0, want) in [(65.4f64, 6.25e-9f64), (262.0, 6.25e-9), (2093.0, 2.6e-10)] {
            let got = b3_for(f0);
            assert!(
                (got / want - 1.0).abs() < 1e-6,
                "b3 at {f0} Hz is {got:e}, Table I fits {want:e}"
            );
        }
        // And it is monotone between the anchors, not a step.
        let mut last = b3_for(262.0);
        for f in [400.0, 600.0, 900.0, 1400.0, 2093.0] {
            let v = b3_for(f);
            assert!(v <= last, "b3 rose between anchors, at {f} Hz");
            last = v;
        }
        // The published anchors are untouched: the wound-bass boost is exactly 1
        // at C2 and above, so C2/C4/C7 still read Table I to the part per million.
        assert!((b3_for(65.4) / 6.25e-9 - 1.0).abs() < 1e-6, "C2 must still be Table I");
        // Above the measured span, held flat.
        assert_eq!(b3_for(8000.0), b3_for(2093.0));
        // BELOW C2 the wound copper bass is lossier — an unmeasured region filled
        // with the correct physical trend, not the flat hold it used to carry.
        // See `bass_hf_boost`: this is what removed the "frisure" of eight bass
        // notes piling long-ringing high partials, confirmed by ear.
        assert!(b3_for(27.5) > b3_for(65.4), "the wound bass must be lossier than C2");
        assert!(b3_for(41.2) > b3_for(65.4) && b3_for(41.2) < b3_for(27.5),
            "the boost must grow monotonically into the bass");
    }

    /// What that damping does to a partial, against the figure it was fitted to.
    ///
    /// `σ = b₁ + b₃ω²`, so at C7 the fifth partial (10 kHz) must decay with
    /// σ ≈ 1.53 s⁻¹ — a T60 of about 4.5 s. The constant this model used to carry
    /// put it at 20.0, a T60 of 0.35 s, and a treble partial that dies inside half
    /// a second is gone before the note is.
    #[test]
    fn a_treble_partial_lives_as_long_as_it_was_measured_to() {
        let w = std::f64::consts::TAU * 10_000.0;
        let sigma = B1 + b3_for(2093.0) * w * w;
        let t60 = 6.9078 / sigma;
        assert!(
            (3.5..6.0).contains(&t60),
            "C7's 10 kHz partial has T60 {t60:.2} s; b1 + b3·ω² with the fitted \
             coefficients gives about 4.5"
        );
    }
}

/// Test-only ceiling (Hz) on which string partials reach the bridge. 0 = no cap.
/// Used only to answer whether a rasp lives in the strings' high partials or in
/// the board, by silencing the strings' HF drive and remeasuring.
#[cfg(test)]
pub static BRIDGE_HF_CAP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl StringModes {
    /// Build the modes of one string. `detune` is a multiplier on the pitch, so
    /// the strings of a unison can sit a few cents apart.
    pub fn build(d: &StringDesign, detune: f64, sr: f32) -> StringModes {
        let nyq = 0.45 * sr as f64;
        let mass = d.mu * d.length;
        let norm = (2.0 / mass).sqrt();
        let f0 = d.f0 * detune;
        let b3 = b3_for(d.f0);
        // Extra HF damping that compensates the control-rate coupling deficit
        // (engine::COUPLE_K > 1): decimating the board read starves the high
        // partials of the energy path into the plate, so they hang; adding
        // `COUPLE_COMP * w^2` restores their decay (grows with frequency like the
        // modes that hang). 0 when COUPLE_K == 1. Calibrated on held A2/A4/C7 to
        // match the exact tail (validated by ear).
        let comp: f64 = if super::engine::COUPLE_K > 1 { COUPLE_COMP } else { 0.0 };
        let mut modes = Vec::with_capacity(MAX_MODES);
        let mut strike = Vec::with_capacity(MAX_MODES);
        let mut bridge = Vec::with_capacity(MAX_MODES);
        for k in 1..=MAX_MODES {
            let kf = k as f64;
            let f = kf * f0 * (1.0 + d.b * kf * kf).sqrt();
            if f >= nyq {
                break;
            }
            let w = std::f64::consts::TAU * f;
            let sigma = B1 + (b3 + comp) * w * w;
            modes.push(Mode { w, sigma });
            strike.push(norm * (std::f64::consts::PI * kf * d.strike).sin());
            let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
            bridge.push(sign * d.tension * norm * std::f64::consts::PI * kf / d.length);
        }
        // ── Tension modulation ────────────────────────────────────────────
        // Stretch: ΔL = ½∫(∂y/∂x)² dx, which over the sine modes is
        // (L/4)·norm²·Σ qₖ²(kπ/L)², and ΔT = (E·A/L)·ΔL.
        let a_core = std::f64::consts::PI * 0.25 * d.core_d * d.core_d;
        let ea = E_STEEL * a_core;
        // ── And it is SQUARED, so half the spectrum is out of bounds ───────
        //
        // `ΔT = Σ eₖ qₖ²` is a product of the modal state with itself, so mode `k`
        // contributes at **twice** its own frequency. The bank keeps modes up to
        // 0.45·sr — 21.6 kHz at 48 k — and twice that is 43.2 kHz. Everything
        // above Nyquist folds straight back down: modes from 12 kHz upwards
        // return as inharmonic content spread from 4.8 kHz to the top of the
        // audible band, which does not belong to any partial of any note and is
        // heard as a metallic buzz on the note — the "frisement" reported through
        // the bass and middle in every sweep so far.
        //
        // It is the high modes that do it, not a few stragglers: `eₖ ∝ k²`, so
        // the weight of this term grows with the square of the mode number and
        // the very partials that alias are the ones that dominate it.
        //
        // The cure is exact rather than approximate, because the stretch integral
        // is DIAGONAL — `∫cos(jπx/L)cos(kπx/L)dx` vanishes unless `j = k`, so the
        // only frequencies present are `2fₖ` and DC, with no cross terms to worry
        // about. Dropping every mode above `sr/4` therefore removes exactly the
        // components that would alias and keeps every one that would not: the real
        // phantom partials up to 24 kHz all survive, and nothing else is touched.
        let fold = 0.25 * sr as f64;
        let elong: Vec<f64> = (1..=modes.len())
            .map(|k| {
                if modes[k - 1].w > std::f64::consts::TAU * fold {
                    return 0.0;
                }
                let kpl = std::f64::consts::PI * k as f64 / d.length;
                0.25 * ea * norm * norm * kpl * kpl
            })
            .collect();
        // Longitudinal modes. Only the core carries the stretch, so the ratio to
        // the transverse fundamental is √(E·A_core/T) — the reciprocal root of
        // the string's working strain.
        let ratio = (ea / d.tension).sqrt();
        let mut long_modes = Vec::new();
        let mut long_bridge = Vec::new();
        let mut long_strike = Vec::new();
        for m in 1..=8 {
            let f = m as f64 * ratio * f0;
            if f >= nyq {
                break;
            }
            let w = std::f64::consts::TAU * f;
            long_modes.push(Mode { w, sigma: B1 + b3 * w * w });
            // `EA·∂ξ/∂x` at the termination. The `mπ/L` and the alternating sign
            // were both missing: without them every longitudinal mode pulled the
            // bridge with the same weight and the same sign, which is a bank of
            // modes wired in parallel rather than a string.
            let sign = if m % 2 == 1 { 1.0 } else { -1.0 };
            long_bridge.push(sign * norm * ea * std::f64::consts::PI * m as f64 / d.length);
            long_strike.push(norm * (std::f64::consts::PI * m as f64 * d.strike).sin());
        }

        // ── Rear duplex (the aliquot) ──────────────────────────────────────
        //
        // The string segment behind the bridge, between it and the hitch pin, is
        // not damped. On a grand it is deliberately tuned — Steinway's aliquot
        // bars set its length so it rings at a harmonic of the speaking pitch —
        // and it sounds whenever the note is played, shaken through the shared
        // bridge. It is where a good treble gets its shimmer and part of its
        // body: an undamped high resonator ringing for seconds after the
        // speaking string has gone quiet, and, tuned to the octave and double
        // octave, reinforcing exactly the upper partials a point strike leaves
        // weakest. Absent in the bass, where there is no speaking-length duplex
        // to tune, so it ramps in across the treble.
        let mut duplex_modes = Vec::new();
        let mut duplex_couple = Vec::new();
        if f0 > 500.0 {
            let strength = ((f0 - 500.0) / 1400.0).clamp(0.0, 1.0);
            for mult in [2.0f64, 3.0, 4.0] {
                // ── Tuned to the partial that is actually there ───────────
                //
                // A maker tunes the duplex segment BY EAR against the note, and
                // what he hears is the string's real partial — which is sharp
                // of the ideal harmonic by its inharmonicity. Set at the exact
                // multiple instead, as this was, the resonator sits beside the
                // only thing that could drive it: at C7, B puts the second
                // partial 2 percent high, which is 85 Hz, against a resonator
                // whose bandwidth is under one. So it was never excited.
                //
                // MEASURED before this change, by rendering each note with the
                // duplex summed in and with it zeroed: the whole aliquot bank
                // contributed between 62 and 132 dB UNDER the note across the
                // treble. Three modal banks built and advanced per voice, for
                // nothing audible. See `print_whether_the_duplex_is_heard_at_all`.
                let f = mult * f0 * (1.0 + d.b * mult * mult).sqrt();
                if f >= nyq {
                    break;
                }
                let w = std::f64::consts::TAU * f;
                // Undamped by any felt, it loses energy only to radiation, so it
                // rings far longer than the struck string: a light fixed loss,
                // not the string's own frequency-scaled damping.
                duplex_modes.push(Mode { w, sigma: 2.2 });
                // Weaker for the higher aliquots, so the shimmer thins with
                // height rather than piling up.
                duplex_couple.push(strength * norm / mult);
            }
        }

        // ── This is NOT the string's physical stiffness, and that matters ──
        //
        // Derived properly, a string whose termination moves puts
        // `Σ wₖ qₖ − (T/L)·y_b` on the bridge: split off the quasi-static shape
        // `y_b·x/L`, and T/L is the whole of the second term. Working out one
        // element of the sum below gives `wₖ/ωₖ = T√(2/M)/c`, which has no k in
        // it — so every term is identical and the SUM IS N TIMES THE PHYSICAL
        // VALUE: 54 times over at A4, 242 in the bass.
        //
        // Substituting the derived T/L was tried on 2026-08-07, twice: once with
        // the old explicit bridge and again after the bridge was made implicit.
        // Both DIVERGED — the tuning went 197 cents out at A1, the pedal test
        // returned `inf`, and an undamped string picked up nothing. This term is
        // holding the explicit scheme together, not describing a string.
        //
        // Making the bridge implicit was NOT enough, and knowing why is the
        // useful part. What that solved was the STATIC half of the exchange: the
        // plate's displacement and the strings' downbearing settled at the same
        // instant instead of a step apart. The loop that actually runs away is
        // the other one — the plate's motion drives the string's MODES, and those
        // only raise its force on the next step, a delayed loop whose gain is
        // `C·Σ(w²b)`. No amount of solving the static part touches it.
        //
        // Closing that means changing what is being coupled: enforcing
        // CONTINUITY, `u_string(L) = y_board`, with the bridge force as the
        // unknown — which is precisely Chabassier's formulation, "the forces at
        // the bridge are introduced as additional unknowns", eliminated with a
        // Schur complement. It is a different scheme, not a different constant.
        //
        // ── And it is NOT what stands between this instrument and a real one ──
        //
        // That claim stood here and is withdrawn: it was an argument, not a
        // measurement, and the measurement contradicts it.
        // `audit_the_cross_coupling_at_the_bridge` (2026-08-13) computes the term
        // the Schur solve would add — one Neumann step of
        // `F_i = pull_i − S_i(ŷ_i + Σⱼ c_ij F_j)`, read as `Σₖ φₖ(i)·bₖ·gₖ` off
        // the board's accumulated modal drive, so it needs no N×N matrix at all —
        // and it comes to **0.3% of the bridge force with one voice, 0.8% with
        // three, 1.4% with ten**. (Individual samples show far larger ratios, but
        // only where the force itself is crossing zero and the denominator
        // vanishes; the mean is the figure that means anything.)
        //
        // A one-percent correction to the bridge force is not audible, and it is
        // certainly not worth the divergence the per-voice implicit solve produced
        // at sixteen voices. The scheme stays explicit, on evidence.
        //
        // ── And the same for the inflation of THIS constant ────────────────
        //
        // The factor of 54 to 242 above has never been given a size either, and it
        // does not need a new measurement — it follows from figures already taken.
        // This stiffness only ever acts through the product `S·C` against the
        // board's compliance, and that product measures **0.004 to 0.023** across
        // the compass. So however wrong `S` is, the term it feeds is at most a
        // couple of percent of the bridge force — the same order as the Schur
        // correction just dismissed above, and by the same argument inaudible.
        //
        // That is worth stating plainly because the comment above reads like an
        // alarm. It is a real deviation from Chabassier's formulation and it is a
        // small one, and the two facts belong together: an unquantified error
        // invites somebody to spend a week on a two-percent term.
        let bridge_stiffness = modes
            .iter()
            .zip(bridge.iter())
            .map(|(m, w)| w * w / (m.w * m.w))
            .sum();
        // What the kept modes account for, against what the string really does.
        let total = d.strike * d.length * (1.0 - d.strike) * d.length
            / (d.tension * d.length);
        let kept: f64 = strike
            .iter()
            .zip(modes.iter())
            .map(|(phi, m)| phi * phi / (m.w * m.w))
            .sum();
        let residual_compliance = (total - kept).max(0.0);
        // The same accounting at the bridge. `strike` and `bridge` already carry
        // the normalisation and the alternating sign, so the sum is taken as it
        // stands and compared with the exact `a` that statics gives.
        let kept_bridge: f64 = strike
            .iter()
            .zip(bridge.iter())
            .zip(modes.iter())
            .map(|((phi, w), m)| phi * w / (m.w * m.w))
            .sum();
        let residual_bridge = d.strike - kept_bridge;

        // The wound-bass bridge rolloff: a coherent-string coupling only up to the
        // winding's limit. Second order above the corner.
        {
            let corner = std::f64::consts::TAU * bridge_hf_corner(d.f0);
            for (k, m) in modes.iter().enumerate() {
                let r = m.w / corner;
                bridge[k] /= (1.0 + r * r * r * r).sqrt();
            }
        }
        #[cfg(test)]
        {
            let cap = BRIDGE_HF_CAP.load(std::sync::atomic::Ordering::Relaxed);
            if cap > 0 {
                let wcap = std::f64::consts::TAU * cap as f64;
                for (k, m) in modes.iter().enumerate() {
                    if m.w > wcap {
                        bridge[k] = 0.0;
                    }
                }
            }
        }
        StringModes {
            modes,
            strike,
            bridge,
            mass,
            elong,
            residual_compliance,
            residual_bridge,
            long_modes,
            long_bridge,
            long_strike,
            long_drive: 0.5 * ea * (2.0 * d.strike - 1.0) / (d.tension * d.tension),
            duplex_modes,
            duplex_couple,
            // Fitted against the decay of a real grand, register by register:
            // about 0.8 through the bass and middle, tightening towards the top
            // where a short light string would otherwise be emptied into the
            // board in a fraction of a second.
            // ── No per-register coupling coefficient ───────────────────
            //
            // This was `0.70 - 0.30 t²` — more coupling in the bass, less in the
            // treble — fitted "against the decay of a real grand, register by
            // register" at a time when the bridge itself was five to fifty times
            // too stiff. With the bridge given the mobility a real one has, the
            // fudge is not merely unnecessary: it is BACKWARDS. Measured with it
            // in place, the decay ran 8.4 s at A1, 16.0 at A4 and 4.3 at A6 —
            // longest in the middle — where a real piano falls monotonically from
            // about twenty-five seconds at the bottom to a second or two at the
            // top. A note that rings three times too long on a handful of
            // partials is heard as a plucked string, which is exactly what the
            // middle and treble were reported to sound like.
            //
            // The register dependence needs no coefficient at all. A string's
            // characteristic impedance is √(Tµ), and it runs 5.9 kg/s at A1 down
            // to 1.5 at A6: the bass is four times harder for the same bridge to
            // move, so it gives up its energy four times more slowly. That IS the
            // decay curve of a piano, and it falls out of the scaling.
            // Kept, but under suspicion and NOT because it is understood.
            //
            // It was fitted "against the decay of a real grand, register by
            // register" when the bridge was five to fifty times too stiff, so it
            // is compensating for a fault that has since been fixed. Removing it
            // was tried on 2026-08-07: it changed the decay curve almost not at
            // all — so it is NOT what makes the middle and treble ring too long —
            // and it pushed the top note to a peak of 4.1, out of range. Put back
            // until the real cause is found, so that the instrument is at least
            // stable while the search continues.
            // ── The transformer ratio, set from the decay it produces ──────
            //
            // Bank names this element and says why it exists: "The bridge
            // functions as an impedance transformer, presenting higher impedance
            // to the string than that if the strings were directly connected to
            // the soundboard. In the latter case decay times would be too short."
            // So the string does NOT see the board's admittance; it sees it
            // through a transformer, and the board's own mobility can sit in the
            // middle of the measured band while the string still rings.
            //
            // Its value is fixed on the observable, because there is no published
            // figure for it: the shape stays, the scale comes from what a piano's
            // notes actually do. Reference for the target is the sampled piano in
            // this repo — its closing chord loses about 5 dB in the first second
            // and is still at −30 dB eight seconds later. A single dry note must
            // decay faster than a pedalled chord in a room, but not by the margin
            // this model had: A4 lost 31.5 dB in its first second before the
            // board's coupling was brought into the measured band, 23.3 after,
            // and 13 with the transformer scaled here.
            //
            // Scaled, not reshaped. The `1 − t²` fall across the compass is
            // untouched; only the overall tightness changes, and it changes for
            // one stated reason.
            bridge_ratio: {
                let t = ((d.f0 / 27.5).log2() / 7.0).clamp(0.0, 1.0);
                BRIDGE_TRANSFORMER * (0.70 - 0.30 * t * t)
            },

            // Piano strings cross the bridge with a few degrees of downbearing.
            bridge_angle: (5.0f64.to_radians()).sin(),
            bridge_stiffness,
        }
    }

    pub fn len(&self) -> usize {
        self.modes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modes.is_empty()
    }
}

#[cfg(test)]
mod compliance_tests {
    use super::*;
    use crate::scale::design;

    /// The truncation must not make the string stiff.
    ///
    /// A string pressed at `x` with a steady force gives by `F·x(L−x)/(T·L)`,
    /// over every mode it has. A bank that keeps a few hundred of them accounts
    /// for only part of that, and the hammer has to be told about the rest or it
    /// meets something harder than a piano string — which leaves its force pulse
    /// too smooth and the tone too poor in high harmonics.
    #[test]
    fn the_residual_compliance_completes_the_string() {
        for note in [28u8, 45, 60, 76, 96] {
            let d = design(note);
            let m = StringModes::build(&d, 1.0, 48_000.0);
            let total = d.strike * d.length * (1.0 - d.strike) * d.length
                / (d.tension * d.length);
            let kept: f64 = m
                .strike
                .iter()
                .zip(m.modes.iter())
                .map(|(p, mo)| p * p / (mo.w * mo.w))
                .sum();
            let share = 100.0 * m.residual_compliance / total;
            eprintln!(
                "note {note:>3} : {} modes, complaisance totale {total:.3e} m/N, \
                 residuelle {:.0}%",
                m.modes.len(),
                share
            );
            assert!(m.residual_compliance >= 0.0, "note {note}: negative residual");
            assert!(
                (kept + m.residual_compliance - total).abs() < total * 1e-9,
                "note {note}: the parts do not add up to the whole string"
            );
            // It has to MATTER somewhere and never be the whole thing: a bank
            // that carried none of the compliance would mean the modes are wrong.
            assert!(share < 60.0, "note {note}: the kept modes carry almost nothing ({share:.0}%)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modal_bank::ModalBank;
    use crate::scale::design;

    const SR: f32 = 48_000.0;

    /// What the hammer actually presses on. For times shorter than the round
    /// trip to the near bridge, a string looks INFINITE, and an infinite string
    /// is a pure resistance 2Z with Z = mu*c: the point under a steady force
    /// moves at F/(2Z). Bank's account of the contact rests on this — the hammer
    /// is slowed by that resistance and then thrown off by the reflection.
    #[test]
    #[ignore]
    fn print_the_point_impedance() {
        for note in [33u8, 60, 84] {
            let d = design(note);
            let m = StringModes::build(&d, 1.0, SR);
            let mut bank = ModalBank::new();
            bank.set_modes(&m.modes, SR);
            let c = 2.0 * d.length * d.f0;
            let z2 = 2.0 * d.mu * c;
            let ideal = 1.0 / (z2 * SR as f64);
            let got = bank.compliance(&m.strike);
            println!("note {note}: {} modes, 2Z = {z2:.2} kg/s", m.len());
            println!("   compliance par echantillon : modele {got:.3e}   ideal {ideal:.3e}   rapport {:.2}", got / ideal);
            // And how far the point really travels under a steady force.
            for &ms in &[0.05f64, 0.2, 0.5, 1.0, 2.0] {
                let mut b2 = ModalBank::new();
                b2.set_modes(&m.modes, SR);
                let n = (SR as f64 * ms * 1e-3) as usize;
                for _ in 0..n {
                    b2.add_force(&m.strike, 1.0);
                    b2.tick();
                }
                let y = b2.read(&m.strike);
                let want = ms * 1e-3 / z2;
                println!("   a {ms:5.2} ms : {y:.3e} m   ideal {want:.3e}   rapport {:.2}", y / want);
            }
            let rt = 2.0 * d.strike * d.length / c * 1000.0;
            println!("   aller-retour a l'agrafe : {rt:.2} ms\n");
        }
    }

    /// The partials must land on the stiff-string law, with the B the wire's
    /// geometry gives — not near it, on it.
    #[test]
    fn partials_follow_the_stiff_string_law() {
        for note in [21u8, 36, 60, 84, 108] {
            let d = design(note);
            let m = StringModes::build(&d, 1.0, SR);
            assert!(!m.is_empty(), "note {note}: no modes");
            for (i, mode) in m.modes.iter().enumerate() {
                let k = (i + 1) as f64;
                let want = k * d.f0 * (1.0 + d.b * k * k).sqrt();
                let got = mode.w / std::f64::consts::TAU;
                assert!(
                    (got - want).abs() < want * 1e-9,
                    "note {note} partial {k}: {got} vs {want}"
                );
            }
        }
    }

    /// Middle C's twelfth partial should be stretched by a few tens of cents,
    /// and a bottom note's by only a handful — the shape of the Railsback curve
    /// showing up as audible stretch rather than as a number in a table.
    #[test]
    fn the_stretch_is_audible_in_the_treble_and_slight_in_the_bass() {
        let cents_at = |note: u8, k: usize| -> f64 {
            let d = design(note);
            let m = StringModes::build(&d, 1.0, SR);
            let f = m.modes[k - 1].w / std::f64::consts::TAU;
            1200.0 * (f / (k as f64 * d.f0)).log2()
        };
        let bass = cents_at(36, 12);
        let mid = cents_at(60, 12);
        assert!(bass > 1.0 && bass < 15.0, "C2 twelfth partial stretched {bass:.1} cents");
        assert!(mid > 20.0 && mid < 90.0, "C4 twelfth partial stretched {mid:.1} cents");
        assert!(mid > bass * 3.0, "the treble must stretch far more than the bass");
    }

    /// The hammer never excites a partial with a node where it strikes: a blow
    /// at a seventh of the string barely moves the seventh partial. This is the
    /// oldest fact about piano voicing and it has to come out of the geometry.
    #[test]
    fn the_strike_point_silences_its_own_partial() {
        let mut d = design(60);
        d.strike = 1.0 / 7.0;
        let m = StringModes::build(&d, 1.0, SR);
        let seventh = m.strike[6].abs();
        let neighbours = 0.5 * (m.strike[5].abs() + m.strike[7].abs());
        assert!(
            seventh < neighbours * 0.05,
            "seventh partial got {seventh:e} against neighbours {neighbours:e}"
        );
    }

    /// Mass normalisation. A static force in the middle of a string deflects it
    /// by `FL/(4T)`; the modal sum has to reproduce that, or every force in the
    /// instrument is in the wrong units.
    #[test]
    fn the_modes_are_mass_normalised() {
        let d = design(60);
        let mut m = StringModes::build(&d, 1.0, SR);
        // Push in the middle, and read there.
        let norm = (2.0 / m.mass).sqrt();
        let shape: Vec<f64> = (1..=m.len())
            .map(|k| norm * (std::f64::consts::PI * k as f64 * 0.5).sin())
            .collect();
        m.strike = shape.clone();
        let mut deflection = 0.0;
        for (i, mode) in m.modes.iter().enumerate() {
            deflection += shape[i] * shape[i] / (mode.w * mode.w);
        }
        let exact = d.length / (4.0 * d.tension);
        assert!(
            (deflection - exact).abs() < exact * 0.02,
            "static deflection {deflection:e} against the exact {exact:e}"
        );
    }

    /// Left alone, a steel string rings for a very long time. This is the
    /// premise the whole coupling rests on: if the string decayed properly by
    /// itself there would be nothing for the soundboard to explain.
    #[test]
    fn an_uncoupled_string_rings_far_longer_than_any_piano_note() {
        let d = design(60);
        let m = StringModes::build(&d, 1.0, SR);
        let t60 = 6.907_755_279 / m.modes[0].sigma;
        assert!(
            t60 > 12.0,
            "a free middle C should ring for tens of seconds, got {t60:.1} s"
        );
        // And its top partials must still go first.
        let top = 6.907_755_279 / m.modes[m.len() - 1].sigma;
        assert!(top < t60 * 0.2, "the highest partial should die well before the first");
    }

    /// The bridge weights have to be a real force. Strike the string and the
    /// force it pulls with should be newtons, of the order a hammer delivers —
    /// not an arbitrary number that happens to sound loud.
    #[test]
    fn the_bridge_force_is_in_newtons() {
        let d = design(60);
        let m = StringModes::build(&d, 1.0, SR);
        let mut bank = ModalBank::new();
        bank.set_modes(&m.modes, SR);
        // A brief blow, roughly what a mezzo-forte hammer delivers.
        bank.add_force(&m.strike, 20.0);
        let mut peak = 0.0f64;
        for _ in 0..(SR as usize / 10) {
            bank.tick();
            peak = peak.max(bank.read(&m.bridge).abs());
        }
        assert!(
            peak > 0.01 && peak < 20.0,
            "bridge force peaked at {peak:e} N, which is not a piano string pulling"
        );
    }
}

//! Per-note coefficient tables ported verbatim from faust-stk piano.h
//! (Romain Michon, STK-4.3 licence). Piecewise-linear breakpoints vs MIDI note
//! (except NORM_VELOCITY, indexed by normalized velocity 0..1).
//!
//! The breakpoints are measured values, not constants: a 0.318 here is a
//! pole, not 1/pi.
#![allow(clippy::approx_constant)]

pub const SINGLE_STRING_DECAY_RATE: &[(f32, f32)] = &[(21.0, -1.5), (24.0, -1.5), (28.0, -1.5), (29.0, -6.0), (36.0, -6.0), (42.0, -6.1), (48.0, -7.0), (52.836, -7.0), (60.0, -7.3), (66.0, -7.7), (72.0, -8.0), (78.0, -8.8), (84.0, -10.0), (88.619, -11.215), (92.368, -12.348), (95.684, -13.934), (99.0, -15.0)];
pub const SINGLE_STRING_ZERO: &[(f32, f32)] = &[(21.0, -1.0), (24.0, -1.0), (28.0, -1.0), (29.0, -1.0), (32.534, -1.0), (36.0, -0.7), (42.0, -0.4), (48.0, -0.2), (54.0, -0.12), (60.0, -0.08), (66.0, -0.07), (72.0, -0.07), (79.0, -0.065), (84.0, -0.063), (88.0, -0.06), (96.0, -0.05), (99.0, -0.05)];
pub const SINGLE_STRING_POLE: &[(f32, f32)] = &[(21.0, 0.35), (24.604, 0.318), (26.335, 0.279), (28.0, 0.25), (32.0, 0.15), (36.0, 0.0), (42.0, 0.0), (48.0, 0.0), (54.0, 0.0), (60.0, 0.0), (66.0, 0.0), (72.0, 0.0), (76.0, 0.0), (84.0, 0.0), (88.0, 0.0), (96.0, 0.0), (99.0, 0.0)];
pub const RELEASE_LOOP_GAIN: &[(f32, f32)] = &[(21.0, 0.865), (24.0, 0.88), (29.0, 0.896), (36.0, 0.91), (48.0, 0.92), (60.0, 0.95), (72.0, 0.965), (84.0, 0.988), (88.0, 0.997), (99.0, 0.988)];
pub const DETUNING_HZ: &[(f32, f32)] = &[(21.0, 0.003), (24.0, 0.003), (28.0, 0.003), (29.0, 0.06), (31.0, 0.1), (36.0, 0.11), (42.0, 0.12), (48.0, 0.2), (54.0, 0.2), (60.0, 0.25), (66.0, 0.27), (72.232, 0.3), (78.0, 0.35), (84.0, 0.5), (88.531, 0.582), (92.116, 0.664), (95.844, 0.793), (99.0, 1.0)];
pub const STIFFNESS_COEF: &[(f32, f32)] = &[(21.0, -0.85), (23.595, -0.85), (27.055, -0.83), (29.0, -0.7), (37.725, -0.516), (46.952, -0.352), (60.0, -0.25), (73.625, -0.036), (93.81, -0.006), (99.0, 1.011)];
pub const STRIKE_POSITION: &[(f32, f32)] = &[(21.0, 0.05), (24.0, 0.05), (28.0, 0.05), (35.0, 0.05), (41.0, 0.05), (42.0, 0.125), (48.0, 0.125), (60.0, 0.125), (72.0, 0.125), (84.0, 0.125), (96.0, 0.125), (99.0, 0.125)];
pub const EQ_GAIN: &[(f32, f32)] = &[(21.0, 2.0), (24.0, 2.0), (28.0, 2.0), (30.0, 2.0), (35.562, 1.882), (41.0, 1.2), (42.0, 0.6), (48.0, 0.5), (54.0, 0.5), (59.928, 0.502), (66.704, 0.489), (74.201, 0.477), (91.791, 1.0), (99.0, 1.0)];
pub const EQ_BANDWIDTH: &[(f32, f32)] = &[(21.0, 5.0), (24.112, 5.0), (28.0, 5.0), (35.0, 4.956), (41.0, 6.0), (42.0, 2.0), (48.773, 1.072), (57.558, 1.001), (63.226, 1.048), (69.178, 1.12), (72.862, 1.525), (80.404, 2.788), (97.659, 1.739)];
pub const LOUD_POLE: &[(f32, f32)] = &[(21.0, 0.875), (23.719, 0.871), (27.237, 0.836), (28.996, 0.828), (32.355, 0.82), (36.672, 0.816), (40.671, 0.82), (45.788, 0.812), (47.867, 0.812), (54.0, 0.81), (60.0, 0.8), (66.0, 0.8), (72.0, 0.81), (78.839, 0.824), (84.446, 0.844), (89.894, 0.844), (96.463, 0.848), (103.512, 0.84), (107.678, 0.84)];
pub const SOFT_POLE: &[(f32, f32)] = &[(21.0, 0.99), (24.0, 0.99), (28.0, 0.99), (29.0, 0.99), (36.0, 0.99), (42.0, 0.99), (48.0, 0.985), (54.0, 0.97), (60.0, 0.96), (66.0, 0.96), (72.0, 0.96), (78.0, 0.97), (84.673, 0.975), (91.157, 0.99), (100.982, 0.97), (104.205, 0.95)];
pub const NORM_VELOCITY: &[(f32, f32)] = &[(0.0, 0.0), (0.17, 0.318), (0.316, 0.546), (0.46, 0.709), (0.599, 0.825), (0.717, 0.894), (0.841, 0.945), (1.0, 1.0)];
pub const LOUD_GAIN: &[(f32, f32)] = &[(21.873, 0.891), (25.194, 0.87), (30.538, 0.848), (35.448, 0.853), (41.513, 0.842), (47.434, 0.826), (53.644, 0.82), (60.72, 0.815), (65.63, 0.82), (72.995, 0.853), (79.06, 0.92), (85.27, 1.028), (91.624, 1.247), (95.668, 1.296), (99.0, 1.3), (100.0, 1.1)];
pub const SOFT_GAIN: &[(f32, f32)] = &[(20.865, 0.4), (22.705, 0.4), (25.96, 0.4), (28.224, 0.4), (31.196, 0.4), (36.715, 0.4), (44.499, 0.4), (53.981, 0.4), (60.0, 0.35), (66.0, 0.35), (72.661, 0.35), (81.435, 0.43), (88.311, 0.45), (93.04, 0.5), (96.434, 0.5)];
pub const SUSTAIN_PEDAL_LEVEL: &[(f32, f32)] = &[(21.0, 0.05), (24.0, 0.05), (31.0, 0.03), (36.0, 0.025), (48.0, 0.01), (60.0, 0.005), (66.0, 0.003), (72.0, 0.002), (78.0, 0.002), (84.0, 0.003), (90.0, 0.003), (96.0, 0.003), (108.0, 0.002)];
pub const DRY_TAP_T60: &[(f32, f32)] = &[(21.001, 0.491), (26.587, 0.498), (34.249, 0.47), (40.794, 0.441), (47.977, 0.392), (55.0, 0.37), (60.0, 0.37), (66.0, 0.37), (71.934, 0.37), (78.0, 0.37), (83.936, 0.39), (88.557, 0.387), (92.858, 0.4), (97.319, 0.469), (102.4, 0.5), (107.198, 0.494)];
pub const DCB_A1: &[(f32, f32)] = &[(21.0, -0.999), (24.0, -0.999), (30.0, -0.999), (36.0, -0.999), (42.0, -0.999), (48.027, -0.993), (60.0, -0.995), (72.335, -0.96), (78.412, -0.924), (84.329, -0.85), (87.688, -0.77), (91.0, -0.7), (92.0, -0.91), (96.783, -0.85), (99.0, -0.8), (100.0, -0.85), (104.634, -0.7), (107.518, -0.5)];
pub const SECOND_STAGE_AMP: &[(f32, f32)] = &[(82.277, -18.508), (88.0, -30.0), (90.0, -30.0), (93.451, -30.488), (98.891, -30.633), (107.573, -30.633)];
pub const R1_1DB: &[(f32, f32)] = &[(100.0, -75.0), (103.802, -237.513), (108.0, -400.0)];
pub const R1_2DB: &[(f32, f32)] = &[(98.388, -16.562), (100.743, -75.531), (103.242, -154.156), (108.0, -300.0)];
pub const R2DB: &[(f32, f32)] = &[(100.0, -115.898), (107.858, -250.0)];
pub const R3DB: &[(f32, f32)] = &[(100.0, -150.0), (108.0, -400.0)];
pub const SECOND_PARTIAL: &[(f32, f32)] = &[(88.0, 2.0), (108.0, 2.1)];
pub const THIRD_PARTIAL: &[(f32, f32)] = &[(88.0, 3.1), (108.0, 3.1)];
pub const BQ4_GAIN: &[(f32, f32)] = &[(100.0, 0.04), (102.477, 0.1), (104.518, 0.3), (106.0, 0.5), (107.0, 1.0), (108.0, 1.5)];

/// String-stiffness allpass coefficient by note, CALIBRATED (not reference
/// data): fitted so the model's measured inharmonicity matches a medium grand,
/// B rising from about 0.00014 in the bass to 0.0019 at the top of the
/// waveguide range (Conklin; Fletcher & Rossing).
///
/// It replaces a multiplier on `STIFFNESS_COEF`, whose values fall towards zero
/// above middle C — which left the whole melodic register vibrating as an
/// almost harmonic, guitar-like string. Measured before: B = 0.00007 at C4 and
/// 0.00004 at C6, five to thirty times too little.
///
/// Refit with `cargo test --lib solve_stiffness_table -- --ignored --nocapture`
/// after any change to the string loop; the fit depends on the dispersion
/// filter's order.
pub const STIFFNESS_TUNED: &[(f32, f32)] = &[
    (21.0, -0.9200), (27.0, -0.8992), (33.0, -0.8640), (39.0, -0.8238),
    (45.0, -0.7729), (51.0, -0.7138), (57.0, -0.6342), (60.0, -0.6100),
    (63.0, -0.5740), (69.0, -0.4823), (72.0, -0.4555), (75.0, -0.4228),
    (78.0, -0.3859), (81.0, -0.3366), (84.0, -0.3169), (87.0, -0.2732),
];

/// Held loop gain by note, CALIBRATED against a sampled piano's decay
/// (timidity + FluidR3_GM): about -10.9 dB one second after the strike at C2,
/// -8.5 at C4, -19.1 at C6.
///
/// Several values sit just above 1, which is correct and not a runaway: the
/// coupling filter is inside the feedback path and is itself lossy (roughly
/// -0.3% at DC and -28% at Nyquist for the string pair), so the round-trip gain
/// is `loop_gain * (1 + 2*cf)`. The loop gain compensates the coupling rather
/// than supplying the damping on its own. A flat 0.9996 for every note, which
/// is what this replaced, gave -14 dB per second at middle C and far worse in
/// the treble — a note that died like a plucked string.
///
/// Refit with `cargo test --lib solve_decay_table -- --ignored --nocapture`.
pub const LOOP_GAIN_TUNED: &[(f32, f32)] = &[
    (21.0, 0.97741), (33.0, 0.98669), (45.0, 0.98615), (57.0, 0.99223),
    (60.0, 0.99323), (69.0, 0.99579),
    // Above here the solve stops being trustworthy — the note is gone before
    // the analysis window ends and the fit reads a noise floor — so the trend
    // is continued by hand rather than taken from it.
    (75.0, 0.99680), (81.0, 0.99760), (84.0, 0.99810), (87.0, 0.99860),
];

/// Hammer-string contact time in milliseconds, by note (Askenfelt & Jansson,
/// "From touch to string vibrations"). Shortens with striking velocity, which
/// is applied at the call site. This is the single strongest shaper of a
/// piano's spectral slope.
pub const CONTACT_MS: &[(f32, f32)] = &[
    (21.0, 3.8), (30.0, 3.0), (36.0, 2.6), (48.0, 2.1), (60.0, 1.8),
    (72.0, 1.0), (84.0, 0.42), (96.0, 0.25), (108.0, 0.18),
];

/// Loop damping cutoff, as a MULTIPLE of the note's fundamental. This sets how
/// much faster the upper partials die than the fundamental; expressing it
/// relative to f0 is what lets one number work across the keyboard, since a
/// fixed cutoff cannot separate a bass note's partials at all. Calibrated
/// against a sampled piano; refit with `solve_loop_tables`.
pub const LOOP_DAMP_MULT: &[(f32, f32)] = &[
    (21.0, 15.36), (33.0, 28.44), (45.0, 31.93), (57.0, 63.94),
    (60.0, 77.08), (69.0, 48.00), (75.0, 45.0), (81.0, 42.0),
    (84.0, 40.0), (87.0, 38.0),
];

/// Inharmonicity coefficient B by note, for the modal string's stiff-string
/// law `f_k = k f0 sqrt(1 + B k^2)`.
///
/// MEASURED, note by note, off a sampled grand (thirty notes three semitones
/// apart, partials tracked one at a time and B fitted by least squares), not
/// fitted by ear or copied from a textbook range. The values it replaces were
/// three to four times too high in the bass — 22 cents of stretch at the
/// twelfth partial of a C2 where a real grand gives 5 or 6 — and that is what
/// made the low end read as a struck wire rather than a piano. It is the
/// Railsback curve: flattest around the bottom two octaves, climbing steeply
/// into the treble where the wire is short and thick.
pub const INHARMONICITY_B: &[(f32, f32)] = &[
    (21.0, 0.000053), (27.0, 0.000036), (33.0, 0.000039),
    (39.0, 0.000053), (45.0, 0.000078), (51.0, 0.000134),
    (57.0, 0.000236), (63.0, 0.000381), (69.0, 0.000639),
    (75.0, 0.001172), (81.0, 0.001750), (87.0, 0.002389),
    (93.0, 0.004798), (99.0, 0.010272), (105.0, 0.015324),
    (108.0, 0.018082),
];

/// T60 of the FUNDAMENTAL, in seconds, with the key held.
///
/// MEASURED off a sampled grand, thirty notes three semitones apart, by
/// heterodyning the fundamental and fitting a straight line to its level in dB.
/// The values it replaces were between 1.3 and 4.5 times too long — a bottom D
/// rang for 71 seconds against the reference's 16 — which is what made the low
/// end drone like a sustained pad instead of dying away like a piano.
///
/// The figures are the measured curve divided by 1.39: the aftersound bank
/// keeps ringing after the prompt sound has gone, so the note as a whole decays
/// more slowly than this table alone would say. The reference's own note-to-note
/// scatter is wider than that correction, so it is applied as one number rather
/// than fitted per note.
///
/// The reference's note-to-note scatter is wide (a C2 measured 5.6 s between
/// two neighbours at 10.5 and 10.3, different samples decaying differently), so
/// the curve is a MONOTONE fit rather than a free smooth one: a piano's decay
/// can only shorten as the pitch rises, and a free fit let the treble come out
/// longer than the bass.
///
/// Finally scaled to match the reference's OVERALL decay rather than its
/// fundamental's, because that is what a listener hears: the aftersound bank
/// and the upper partials both feed the tail, and matching the fundamental
/// alone left a C2 eleven dB too quiet at four seconds.
pub const MODAL_T60: &[(f32, f32)] = &[
    (21.0, 28.72), (27.0, 15.16), (33.0, 12.65), (39.0, 10.32),
    (45.0, 9.09), (51.0, 8.10), (57.0, 7.08), (63.0, 6.47),
    (69.0, 6.17), (75.0, 5.99), (81.0, 5.54), (87.0, 3.65),
    (93.0, 2.76), (99.0, 1.95), (105.0, 1.56), (108.0, 1.44),
];

/// How much faster partial k dies than the fundamental: `T60_k = T60_1 / k^a`.
/// A strict 1/f law (a = 1) is the textbook idealisation and measures too dull;
/// real spectra keep more life in partials 5 to 16.
pub const MODAL_DAMP_EXP: &[(f32, f32)] = &[
    (21.0, 0.55), (45.0, 0.62), (60.0, 0.68), (75.0, 0.75), (87.0, 0.85),
    (108.0, 0.95),
];




/// Partial amplitudes of a real grand, in dB relative to the strongest partial
/// of the note, MEASURED off a sampled instrument: thirty notes three semitones
/// apart, each partial found at its stiff-string frequency, smoothed across
/// neighbouring notes and renormalised.
///
/// This replaces four curves that were invented and then hand-tuned for weeks —
/// an analytic half-sine hammer spectrum with a blanket fudge on its contact
/// time, a strike comb with an arbitrary floor, and two radiation tables. Every
/// one of them was wrong, and they were wrong together, so tuning any one of
/// them moved the others' errors around instead of removing them.
///
/// The single largest thing they got backwards is visible in the first rows: on
/// a real grand the bass FUNDAMENTAL is 25 to 40 dB BELOW the second partial.
/// The bottom of a piano is heard through its harmonics, the fundamental barely
/// radiates at all. The invented "shelf" did the exact opposite, lifting the
/// first two partials above everything else, which is what gave the low end a
/// fat sustained tone that no grand piano has.
pub const PARTIAL_NOTES: [f32; 15] = [21.0, 27.0, 33.0, 39.0, 45.0, 51.0, 57.0, 63.0, 69.0, 75.0, 81.0, 87.0, 93.0, 99.0, 105.0];
pub const PARTIAL_DB: [[f32; 24]; 15] = [
    // 21
    [-40.6, 0.0, -5.5, -7.5, -15.1, -12.2, -14.3, -30.2, -22.7, -19.4, -18.3, -25.0, -19.1, -24.8, -23.7, -37.6, -29.0, -35.8, -34.9, -35.7, -28.2, -27.9, -31.3, -37.4],
    // 27
    [-25.5, 0.0, -6.9, -8.9, -15.1, -10.7, -14.9, -27.2, -20.9, -18.9, -22.7, -21.9, -17.9, -20.5, -22.5, -40.0, -30.9, -32.8, -30.1, -30.9, -26.0, -20.4, -26.8, -32.5],
    // 33
    [-3.9, 0.0, -3.1, -9.7, -12.3, -9.4, -12.5, -22.3, -13.8, -12.9, -16.9, -19.2, -13.3, -16.2, -17.9, -30.1, -29.4, -19.3, -22.7, -21.5, -17.3, -14.3, -17.1, -24.9],
    // 39
    [0.0, -8.0, -12.6, -14.1, -16.8, -21.8, -17.9, -28.8, -21.3, -18.2, -19.7, -26.4, -22.8, -21.8, -20.2, -30.8, -34.2, -21.2, -25.8, -29.8, -21.7, -19.4, -21.8, -31.1],
    // 45
    [0.0, -4.7, -12.4, -13.0, -19.8, -20.9, -20.0, -28.3, -22.9, -15.3, -22.2, -23.8, -22.4, -22.7, -18.6, -30.5, -32.3, -24.9, -25.4, -28.9, -26.6, -23.9, -21.8, -30.2],
    // 51
    [0.0, -0.6, -14.1, -15.7, -19.9, -22.1, -18.0, -19.8, -22.0, -13.0, -24.6, -21.8, -20.7, -29.0, -20.6, -25.8, -36.8, -32.7, -25.0, -31.9, -29.1, -30.0, -25.2, -29.8],
    // 57
    [0.0, -3.9, -17.8, -20.2, -17.7, -21.7, -16.4, -18.8, -20.6, -16.2, -21.1, -22.6, -19.7, -28.6, -26.4, -28.1, -40.0, -36.3, -32.5, -37.3, -29.8, -37.9, -34.5, -34.4],
    // 63
    [0.0, -6.8, -16.8, -18.4, -12.6, -15.1, -21.3, -18.5, -12.7, -22.1, -22.1, -19.1, -21.9, -22.9, -24.6, -23.6, -34.9, -38.5, -36.9, -36.1, -34.8, -41.6, -41.7, -43.9],
    // 69
    [0.0, -10.9, -16.8, -17.9, -14.9, -15.5, -25.8, -22.2, -15.2, -28.6, -32.2, -25.7, -28.7, -30.3, -33.0, -29.0, -41.4, -46.7, -43.4, -42.6, -47.8, -48.9, -52.0, -54.8],
    // 75
    [0.0, -7.8, -14.6, -11.0, -10.3, -17.8, -18.9, -25.6, -21.0, -26.4, -38.4, -35.3, -40.2, -41.1, -54.2, -49.0, -57.1, -65.0, -60.6, -59.0, -60.4, -54.8, -63.5, -59.7],
    // 81
    [0.0, -8.4, -13.9, -11.5, -11.1, -18.9, -22.3, -27.2, -28.4, -30.8, -45.0, -39.9, -50.8, -49.5, -61.8, -54.3, -59.7, -59.3, -55.0, -55.5, -54.7, -48.2, -61.5, -56.4],
    // 87
    [0.0, -12.9, -16.9, -16.6, -17.8, -25.1, -32.6, -34.2, -45.4, -46.8, -53.6, -56.7, -57.6, -52.7, -56.2, -50.9, -60.5, -55.7, -52.2, -53.5, -52.8, -46.2, -60.5, -55.3],
    // 93
    [0.0, -15.7, -23.6, -24.6, -28.6, -44.6, -47.9, -40.1, -59.6, -54.9, -48.6, -62.1, -54.9, -49.8, -54.2, -49.2, -60.7, -54.8, -51.6, -80.0, -80.0, -80.0, -80.0, -80.0],
    // 99
    [0.0, -19.0, -28.7, -37.7, -45.0, -45.8, -41.4, -33.0, -59.7, -53.5, -46.7, -62.2, -53.9, -48.9, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0],
    // 105
    [0.0, -24.6, -38.4, -50.3, -42.8, -39.8, -38.7, -30.8, -58.9, -53.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0, -80.0],
];

/// Amplitude of partial `k` (1-based) at `note`, in dB, interpolated between the
/// measured rows. Partials past the table fall away on the slope the last two
/// give, which is what the reference does above them anyway.
pub fn partial_db(note: f32, k: usize) -> f32 {
    if k == 0 {
        return -80.0;
    }
    let row = |i: usize| -> f32 {
        let r = &PARTIAL_DB[i];
        if k <= r.len() {
            r[k - 1]
        } else {
            let n = r.len();
            let slope = (r[n - 1] - r[n - 2]).min(0.0);
            r[n - 1] + slope * (k - n) as f32
        }
    };
    let ns = &PARTIAL_NOTES;
    if note <= ns[0] {
        return row(0);
    }
    if note >= ns[ns.len() - 1] {
        return row(ns.len() - 1);
    }
    for i in 0..ns.len() - 1 {
        if note >= ns[i] && note <= ns[i + 1] {
            let t = (note - ns[i]) / (ns[i + 1] - ns[i]);
            return row(i) + (row(i + 1) - row(i)) * t;
        }
    }
    row(ns.len() - 1)
}

/// Linear interpolation between breakpoints, clamped at the ends (STK LookupTable).
#[inline]
pub fn lookup(table: &[(f32, f32)], x: f32) -> f32 {
    if x <= table[0].0 { return table[0].1; }
    let last = table[table.len() - 1];
    if x >= last.0 { return last.1; }
    let mut i = 0;
    while i + 1 < table.len() && table[i + 1].0 < x { i += 1; }
    let (x0, y0) = table[i];
    let (x1, y1) = table[i + 1];
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

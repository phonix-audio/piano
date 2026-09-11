//! A bank of second-order modal oscillators, driven by point forces.
//!
//! This is the one primitive the whole instrument is built from. Humbert (§2.3)
//! sets it out: any vibrating structure obeys `M ÿ + D ẏ + K y = F`, and in the
//! modal basis that diagonalises into independent equations
//!
//! ```text
//!     q̈ₖ + 2σₖ q̇ₖ + ωₖ² qₖ = (φᵀF)ₖ
//! ```
//!
//! one per mode. A string and a soundboard differ only in their mode table, so
//! they are the same type here.
//!
//! ## Why the force lags by one sample
//!
//! Humbert §3.2 solves those equations exactly with Green's functions and §3.3
//! turns the result into a recursion (Van Den Doel):
//!
//! ```text
//!     q(k) = a₁ q(k−1) − a₂ q(k−2) + b g(k−1)
//! ```
//!
//! The position at step `k` depends on the force at step `k−1` and not at `k`.
//! That is the whole reason this scheme is used here rather than an exact
//! per-step integration: it makes the hammer loop
//!
//! ```text
//!     y(t) → F(t) → y(t+1) → F(t+1) → …
//! ```
//!
//! explicit and unconditionally sequenced. A scheme whose position depends on
//! the force at the same step forces you to solve for the contact force
//! implicitly, and that is exactly where an earlier attempt at this instrument
//! came apart. Velocity does depend on the force at the same step (Humbert
//! §3.3.2), so velocity is only ever read out — never fed back into a contact.
//!
//! ## Coordinates and readouts
//!
//! The bank holds the modal coordinates `qₖ`, not any particular point of the
//! structure. A force applied at a point enters weighted by that point's modal
//! shape `φ(p,k)`; a reading taken at a point sums the coordinates weighted by
//! `φ(e,k)`. So one bank serves every point of the structure at once — the
//! hammer's strike point, the bridge, and the two places the soundboard is
//! listened to — which is what makes the strike-position comb and the stereo
//! image fall out of the geometry instead of being imposed on it.
//!
//! State is `f64`. The lightly damped modes of a piano string sit close enough
//! to the unit circle that `f32` accumulates audible error in the tail; the
//! bowed-string model next door (`src/archet/modal.rs`) found the same thing.

/// Lanes per tile of the block-major plate step: wide enough to fill the
/// vector units, small enough that a tile's force matrix stays in L1.
pub const BLOCK_TILE: usize = 16;

/// Run the block kernel's GATHER in single precision. ON — measured.
///
/// The gather is this engine's arithmetic wall: 2·M·V·T flops a block,
/// ~10.8 Gflop/s per instance at 31 voices, against 0.9 for the recursion.
/// Single precision issues twice the FMA lanes for it, and pays some of that
/// back widening 463k values into the f64 recursion.
///
/// MEASURED 2026-08-25 by `piano-research --bin gather_ab`, which is the
/// only reason this is on: two earlier passes taken minutes apart said the
/// opposite, because on this machine an unchanged path drifts by a third
/// between runs. The bench alternates the kernels INSIDE one process, on one
/// engine, nine rounds per order, both orders, first round of each thrown
/// away, verdict on the median ratio — 0.933, 0.911, 0.926, i.e. 7 to 9
/// percent faster, three runs, load average 0.5 to 2.2.
///
/// And it is inaudible: the whole rendered second differs from the f64
/// kernel by 3e-8 at worst, 129 dB under the peak — below a 24-bit floor.
/// `g` sums at most eighty-eight products, so twenty-four mantissa bits are
/// ample; the RECURSION keeps f64, where the poles sit close enough to the
/// unit circle that error compounds sample after sample.
///
/// Left switchable so the A/B can be re-run whenever the kernel changes.
pub static GATHER_F32: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Half-precision WEIGHTS: measured, and not kept.
///
/// The idea was sound and the analogy with the 16-bit fixed point that lost
/// did NOT settle it: that one paid a sign-extend, an int-to-float and a
/// scale multiply per weight, where half precision pays one `vcvtph2ps`, an
/// instruction the baseline this project builds for always has. So it was
/// built and put through the same alternated A/B as the f32 gather.
///
/// The answer, three runs: ratio 1.024, 1.018, 1.017 — two percent SLOWER,
/// consistently. Halving the bytes buys nothing once the weights are f32,
/// because the gather is then bound by its arithmetic and not by the bus,
/// and the conversion lands on the critical path. It is also the least
/// accurate of the three by a wide margin: 21e-6 against the f64 kernel,
/// 72 dB under the peak, where single precision sits at 129 — ten mantissa
/// bits against twenty-four.
///
/// Kept as a note rather than as code: the switch, the store and the arm
/// were removed once the number was in.
static _F16_MEASURED_AND_REJECTED: () = ();

/// One mode's constants, before discretisation.
#[derive(Clone, Copy, Debug)]
pub struct Mode {
    /// Undamped angular frequency, rad/s.
    pub w: f64,
    /// Damping rate σ, s⁻¹. The mode decays as `e^{−σt}`, so its T60 is
    /// `6.9078/σ` and its loss factor is `η = 2σ/ω`.
    pub sigma: f64,
}

impl Mode {
    /// From a frequency in Hz and a T60 in seconds.
    pub fn from_t60(f_hz: f64, t60: f64) -> Mode {
        Mode {
            w: std::f64::consts::TAU * f_hz,
            sigma: 6.907_755_279 / t60.max(1e-4),
        }
    }

    /// From a frequency in Hz and a loss factor η (the form measured studies of
    /// soundboards report, e.g. η between 1% and 3% for spruce).
    pub fn from_loss_factor(f_hz: f64, eta: f64) -> Mode {
        let w = std::f64::consts::TAU * f_hz;
        Mode { w, sigma: 0.5 * eta * w }
    }
}

/// How fast a damper's absorption climbs with frequency, as a reference angular
/// frequency: the extra loss a mode takes is `ω / DAMPER_HF_REF` nepers a second.
///
/// **Taken from Bank's Fig. 6.1**, which plots the measured decay times of a
/// damped A♯4 (466 Hz) partial by partial: about **0.2 s at the fundamental
/// falling to about 0.05 s by the twenty-fifth** — a factor of four across the
/// spectrum of one note. That is the only published curve of a real damper in
/// hand, and it fixes the shape without a knob.
///
/// Solving `(σ_b + ω₂₅/R) / (σ_b + ω₁/R) = 4` at the default damper rate gives
/// **R = 742 rad/s**, a reference at 118 Hz. What makes it more than a fit is
/// that the ratio was solved for and the ABSOLUTE times then land on their own:
/// 0.219 s at the fundamental and 0.055 s at the twenty-fifth, against Bank's
/// 0.2 and 0.05. Held to account by `a_damped_note_dies_as_bank_measured_it`.
///
/// It stood at 3141.59 — a round 500 Hz, chosen by reasoning rather than measured
/// — which gave a ratio of 1.8 where the measurement says 4, so the top of a
/// damped note rang nearly three times too long.
const DAMPER_HF_REF: f64 = 741.8;

#[derive(Clone, Default)]
pub struct ModalBank {
    n: usize,
    /// How many of those modes are still worth computing.
    ///
    /// A string's internal loss is `σₖ = R + η ωₖ²`, so a mode's decay rate
    /// grows with the SQUARE of its frequency: on a bass string the top of the
    /// bank is gone in a fraction of a second while the fundamental is still
    /// ringing seconds later. The modes are held in ascending frequency, so
    /// what is still audible is always a prefix, and the tail of a long note
    /// costs a fraction of its attack instead of the same 420 modes throughout.
    n_active: usize,
    /// The loudest single-mode contribution this bank has ever produced, which
    /// is what "inaudible" has to be measured against.
    peak_ref: f64,
    sr: f64,
    /// Recursion coefficients, per mode.
    a1: Vec<f64>,
    a2: Vec<f64>,
    b: Vec<f64>,
    /// Per-mode extra attenuation applied while a damper is on the string.
    damp_hf: Vec<f64>,
    /// How much of the damper each mode actually feels, 0..1. See
    /// `set_damper_shape`.
    damp_pos: Vec<f64>,
    /// Modal coordinates at the two previous steps.
    q1: Vec<f64>,
    q2: Vec<f64>,
    /// Generalised force accumulated during the current step, applied on the
    /// next one — the one-sample lag the scheme is built around.
    drive: Vec<f64>,
    /// Kept so callers can build readout weights and inspect the modes.
    modes: Vec<Mode>,
    /// Sum over modes of the step's response to a unit generalised force. This
    /// is the structure's numerical compliance: how far a point moves, per
    /// newton, in one sample. Contact models need it to know what they are
    /// pushing against.
    unit_response: f64,
    /// Sub-step recursion coefficients: the SAME modes at dt/sub_steps. Advancing
    /// the string this finely THROUGH a hammer contact presses the felt against
    /// the string's real, frequency-dependent yield instead of one flat
    /// compliance that low-passes the force — the cause of the treble deficit.
    /// Empty until `set_substep` is called.
    a1s: Vec<f64>,
    a2s: Vec<f64>,
    bs: Vec<f64>,
    /// Compliance over ONE sub-step (Σ bs): what the contact solve pushes against
    /// each sub-step, far smaller than `unit_response` because a mode barely moves
    /// in a microsecond.
    unit_response_sub: f64,
    sub_steps: usize,
}

impl ModalBank {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild the bank for a set of modes. Modes at or above Nyquist are
    /// dropped: they cannot be represented and would alias into the tone.
    pub fn set_modes(&mut self, modes: &[Mode], sr: f32) {
        let sr = sr as f64;
        let dt = 1.0 / sr;
        let nyq = std::f64::consts::PI * sr;
        self.sr = sr;
        self.n = 0;
        self.a1.clear();
        self.a2.clear();
        self.b.clear();
        self.damp_hf.clear();
        self.damp_pos.clear();
        self.modes.clear();
        let mut unit = 0.0f64;
        for m in modes {
            if !(m.w > 0.0) || m.w >= nyq || !m.sigma.is_finite() || m.sigma < 0.0 {
                continue;
            }
            // Damped frequency. A mode damped past its own frequency is
            // overdamped and has no oscillation left to represent.
            let wd2 = m.w * m.w - m.sigma * m.sigma;
            if wd2 <= 0.0 {
                continue;
            }
            let wd = wd2.sqrt();
            let e = (-m.sigma * dt).exp();
            let a1 = 2.0 * e * (wd * dt).cos();
            let a2 = e * e;
            // Impulse invariance: a unit generalised force impulse must leave
            // the mode tracing dt·e^{−σt}·sin(ω_d t)/ω_d.
            //
            //
            // ── Why impulse invariance is the RIGHT choice here ────────────
            //
            // For a force applied over exactly ONE sample and then removed, this
            // moves the mode by `dt²` where the continuous response gives
            // `dt²/2` — twice too far, measured in
            // `one_sample_of_force_moves_the_string_the_same_either_way`. The
            // step-invariant (zero-order hold) form of the same resonator splits
            // the input over two samples and gets that first step right
            // (`the_step_invariant_input_gets_both_ends_right`).
            //
            // It was built, and it is NOT kept, because its two coefficients sum
            // to `b1 + b2 = dt²` — this very number. A hammer never applies a
            // one-sample impulse: it leans on the string for the whole contact,
            // so what the contact solve needs is the response to a SUSTAINED
            // force, which is the sum, which is what `b` already is. Substituting
            // `b1` alone made the solve assume the string yields half as much as
            // it does, so it called for forces that were too large, and
            // `a_hammer_gives_no_more_than_it_carries` caught it at once: note 96
            // delivered 2.3x the momentum the hammer carries.
            //
            // That is also the whole explanation of why the sub-stepped contact
            // diverged in a real piece: `compliance_sub` is the response to one
            // SUB-step, roughly twenty times smaller again, so the solve asked
            // for forces twenty times too large. +16 dB by the second second,
            // then NaN.
            let b = dt * e * (wd * dt).sin() / wd;
            unit += b;
            self.a1.push(a1);
            self.a2.push(a2);
            self.b.push(b);
            // The damper's extra, frequency-shaped loss. See `damp`: felt absorbs
            // the top of a string far faster than the bottom, and a flat
            // multiplier is a fader rather than a damper.
            self.damp_hf.push((-m.w / (DAMPER_HF_REF * sr)).exp());
            self.damp_pos.push(1.0);
            self.modes.push(*m);
            self.n += 1;
        }
        self.q1 = vec![0.0; self.n];
        self.q2 = vec![0.0; self.n];
        self.drive = vec![0.0; self.n];
        self.n_active = self.n;
        self.peak_ref = 0.0;
        self.unit_response = unit;
    }

    /// Build the sub-step coefficient set: the same modes stepped at dt/steps, so
    /// the string can be advanced finely through a hammer contact. Call after
    /// `set_modes`.
    /// How many sub-steps the sub-step coefficient set was built for.
    #[inline]
    pub fn sub_steps(&self) -> usize {
        self.sub_steps
    }

    pub fn set_substep(&mut self, steps: usize) {
        let steps = steps.max(1);
        self.sub_steps = steps;
        let dt = 1.0 / self.sr / steps as f64;
        self.a1s.clear();
        self.a2s.clear();
        self.bs.clear();
        let mut unit = 0.0f64;
        for i in 0..self.modes.len() {
            let m = self.modes[i];
            let wd2 = m.w * m.w - m.sigma * m.sigma;
            let wd = if wd2 > 0.0 { wd2.sqrt() } else { m.w };
            let e = (-m.sigma * dt).exp();
            self.a1s.push(2.0 * e * (wd * dt).cos());
            self.a2s.push(e * e);
            let b = dt * e * (wd * dt).sin() / wd;
            self.bs.push(b);
            unit += b;
        }
        self.unit_response_sub = unit;
    }

    /// The compliance the contact solve pushes against over ONE sub-step.
    #[inline]
    pub fn unit_response_sub(&self) -> f64 {
        self.unit_response_sub
    }

    /// Compliance at a POINT over one sub-step: `Σ φ(e,k)^2 · b_sub`, the give the
    /// felt sees per newton at the strike when the string is advanced sub-step by
    /// sub-step. The sub-step analogue of `compliance`.
    #[inline]
    pub fn compliance_sub(&self, shape: &[f64]) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n.min(shape.len()) {
            acc += shape[i] * shape[i] * self.bs[i];
        }
        acc
    }

    /// The read-point value the string WOULD show after one free sub-step, with
    /// no force applied, WITHOUT advancing the state. The coupled contact solve
    /// presses the felt against this free-evolved position, so that the force it
    /// then applies moves the string by exactly the linear `compliance_sub * f`
    /// the solve assumed (`read` after `add_force`+`tick_sub` equals this plus
    /// `compliance_sub * f`, by superposition). Pressing against the pre-tick
    /// position instead ignores the string's own velocity across the sub-step,
    /// which at treble frequencies is large enough to make the contact diverge.
    #[inline]
    pub fn peek_free_sub(&self, shape: &[f64]) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n.min(shape.len()) {
            let q = self.a1s[i] * self.q1[i] - self.a2s[i] * self.q2[i];
            acc += shape[i] * q;
        }
        acc
    }

    /// One sub-step with a force at `shape` applied to it, returning the
    /// read-point value the string will show after the NEXT free sub-step:
    /// `add_force`, `tick_sub` and `peek_free_sub` in one pass over the modes.
    #[inline]
    pub fn drive_tick_sub_peek(&mut self, shape: &[f64], f: f64) -> f64 {
        let n = self.n.min(shape.len());
        let mut acc = 0.0;
        for i in 0..n {
            let s = shape[i];
            let q = self.a1s[i] * self.q1[i] - self.a2s[i] * self.q2[i]
                + self.bs[i] * (self.drive[i] + s * f);
            self.q2[i] = self.q1[i];
            self.q1[i] = q;
            self.drive[i] = 0.0;
            acc += s * (self.a1s[i] * q - self.a2s[i] * self.q2[i]);
        }
        acc
    }

    /// Advance every mode by ONE sub-step (dt/sub_steps), using the sub-step
    /// coefficients. Shares the same q1/q2/drive state as `tick`, so a run of
    /// sub-steps through a contact and the ordinary once-per-sample ticks outside
    /// it join up seamlessly.
    #[inline]
    pub fn tick_sub(&mut self) {
        for i in 0..self.n {
            let q = self.a1s[i] * self.q1[i] - self.a2s[i] * self.q2[i] + self.bs[i] * self.drive[i];
            self.q2[i] = self.q1[i];
            self.q1[i] = q;
            self.drive[i] = 0.0;
        }
    }

    /// Re-space the state (q2, "one step ago") from `old_dt` to `new_dt`, keeping
    /// q1. A mode's recursion state is tied to its sample spacing, so when a
    /// string leaves a contact — where it was advanced at the sub-step rate — and
    /// returns to the audio rate, the "previous sample" value has to be rebuilt at
    /// the new spacing, or the first ordinary tick reads a phantom velocity.
    /// Reconstructs each mode's free phasor from (q1, q2) at `old_dt` and
    /// evaluates it one `new_dt` into the past.
    pub fn respace(&mut self, old_dt: f64, new_dt: f64) {
        for i in 0..self.n {
            let m = self.modes[i];
            let wd2 = m.w * m.w - m.sigma * m.sigma;
            if wd2 <= 0.0 {
                continue;
            }
            let wd = wd2.sqrt();
            let so = (wd * old_dt).sin();
            if so.abs() < 1.0e-15 {
                continue;
            }
            let x = self.q1[i];
            let y = self.q2[i];
            let beta = ((wd * old_dt).cos() * x - y * (-m.sigma * old_dt).exp()) / so;
            self.q2[i] =
                (m.sigma * new_dt).exp() * (x * (wd * new_dt).cos() - beta * (wd * new_dt).sin());
        }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    pub fn modes(&self) -> &[Mode] {
        &self.modes
    }

    /// Silence every mode without rebuilding the bank.
    pub fn clear(&mut self) {
        for v in self.q1.iter_mut().chain(self.q2.iter_mut()).chain(self.drive.iter_mut()) {
            *v = 0.0;
        }
        self.n_active = self.n;
        self.peak_ref = 0.0;
    }

    /// Apply a force at a point, given that point's modal shape. Accumulates
    /// into this step's drive; it takes effect on the next `tick`.
    #[inline]
    pub fn add_force(&mut self, shape: &[f64], f: f64) {
        if f == 0.0 {
            return;
        }
        for (d, s) in self.drive.iter_mut().zip(shape.iter()) {
            *d += *s * f;
        }
    }

    /// Apply a generalised force directly, already projected onto the modes.
    #[inline]
    pub fn add_modal_force(&mut self, g: &[f64]) {
        for (d, s) in self.drive.iter_mut().zip(g.iter()) {
            *d += *s;
        }
    }

    /// Add `weight` times every mode's velocity into `out`, over the live
    /// prefix. The unison's mean velocity, mode by mode, is built from this.
    #[inline]
    pub fn velocity_accumulate(&self, out: &mut [f64], weight: f64) {
        let n = self.n_active.min(out.len());
        let w = weight * self.sr;
        for i in 0..n {
            out[i] += w * (self.q1[i] - self.q2[i]);
        }
    }

    /// A damper on the motion this bank has that `reference` has not: the
    /// force `-gamma * (v - reference)` on every mode, for the next tick. The
    /// unison's antisymmetric motion pushes no net force on the bridge and so
    /// is never drained by it; this is the loss the termination gives it.
    #[inline]
    pub fn add_cross_damping(&mut self, gamma: &[f64], reference: &[f64]) {
        let n = self.n_active.min(gamma.len()).min(reference.len());
        let sr = self.sr;
        for i in 0..n {
            let v = (self.q1[i] - self.q2[i]) * sr;
            self.drive[i] -= gamma[i] * (v - reference[i]);
        }
    }

    /// Advance every mode by one sample.
    #[inline]
    pub fn tick(&mut self) {
        for i in 0..self.n {
            let q = self.a1[i] * self.q1[i] - self.a2[i] * self.q2[i] + self.b[i] * self.drive[i];
            self.q2[i] = self.q1[i];
            self.q1[i] = q;
            self.drive[i] = 0.0;
        }
    }

    /// Displacement at a point: `Σ φ(e,k) qₖ`.
    #[inline]
    pub fn read(&self, shape: &[f64]) -> f64 {
        // Over the LIVE prefix, as every other readout here does. A retired
        // mode's coordinate is zeroed, so the terms this drops are `w * 0.0`
        // and the sum is the same to the bit — it just stops fetching four
        // hundred zeroes per sample on a bass string whose top has died.
        let n = self.n_active.min(shape.len());
        let mut acc = 0.0;
        for i in 0..n {
            acc += shape[i] * self.q1[i];
        }
        acc
    }

    /// Take every voice's push over a RANGE of modes, step that range, and
    /// read what the two ears see of it — all without touching a mode outside
    /// the range.
    ///
    /// This is the plate's half of the parallel sample: the workers each own a
    /// slice of the modes, and a modal bank is a set of independent two-pole
    /// recursions, so slices do not interact. Only the ear readings are sums,
    /// and those come back as partials for the caller to add up in a fixed
    /// order.
    ///
    /// # Safety
    ///
    /// `lo..hi` must be within `n_active`, and no other thread may touch any
    /// mode in that range for the duration of the call. The pool guarantees
    /// both: the ranges it hands out tile `0..n_active` and never overlap, and
    /// every participant is inside the same barrier pair.
    #[allow(clippy::mut_from_ref)]
    /// `range_drive_tick_read2` over two pairs of shapes.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn range_drive_tick_read4(
        &self,
        lo: usize,
        hi: usize,
        points: &[std::sync::Arc<Vec<f64>>],
        forces: &[f64],
        va: &[f64],
        vb: &[f64],
        vc: &[f64],
        vd: &[f64],
    ) -> [f64; 4] {
        debug_assert!(lo <= hi && hi <= self.n_active);
        let n = hi - lo;
        if n == 0 {
            return [0.0; 4];
        }
        let q1 = unsafe { std::slice::from_raw_parts_mut(self.q1.as_ptr().add(lo) as *mut f64, n) };
        let q2 = unsafe { std::slice::from_raw_parts_mut(self.q2.as_ptr().add(lo) as *mut f64, n) };
        let dr = unsafe { std::slice::from_raw_parts_mut(self.drive.as_ptr().add(lo) as *mut f64, n) };
        let (a1, a2, bc) = (&self.a1[lo..hi], &self.a2[lo..hi], &self.b[lo..hi]);
        let (va, vb, vc, vd) = (&va[lo..hi], &vb[lo..hi], &vc[lo..hi], &vd[lo..hi]);
        for (p, &f) in points.iter().zip(forces.iter()) {
            if f == 0.0 || p.len() < hi {
                continue;
            }
            let sh = &p[lo..hi];
            for i in 0..n {
                dr[i] += sh[i] * f;
            }
        }
        let mut r = [0.0f64; 4];
        for i in 0..n {
            let prev = q1[i];
            let q = a1[i] * prev - a2[i] * q2[i] + bc[i] * dr[i];
            let step = q - prev;
            r[0] += va[i] * step;
            r[1] += vb[i] * step;
            r[2] += vc[i] * step;
            r[3] += vd[i] * step;
            q2[i] = prev;
            q1[i] = q;
            dr[i] = 0.0;
        }
        r.map(|x| x * self.sr)
    }

    /// Drives, steps and reads the modes in `lo..hi` through a shared
    /// reference, so several ranges can advance on several threads at once.
    ///
    /// # Safety
    ///
    /// `lo <= hi <= n_active`, and no other call touches any mode in
    /// `lo..hi`, on any thread, until this one returns: the ranges of the
    /// calls running at the same time must be disjoint. The pool's tiling
    /// guarantees it; a debug build checks the bounds.
    pub unsafe fn range_drive_tick_read2(
        &self,
        lo: usize,
        hi: usize,
        points: &[std::sync::Arc<Vec<f64>>],
        forces: &[f64],
        va: &[f64],
        vb: &[f64],
    ) -> (f64, f64) {
        debug_assert!(lo <= hi && hi <= self.n_active);
        let n = hi - lo;
        if n == 0 {
            return (0.0, 0.0);
        }
        // The disjointness is the caller's contract, checked above in debug and
        // guaranteed by the pool's tiling.
        let q1 = unsafe { std::slice::from_raw_parts_mut(self.q1.as_ptr().add(lo) as *mut f64, n) };
        let q2 = unsafe { std::slice::from_raw_parts_mut(self.q2.as_ptr().add(lo) as *mut f64, n) };
        let dr = unsafe { std::slice::from_raw_parts_mut(self.drive.as_ptr().add(lo) as *mut f64, n) };
        let (a1, a2, bc) = (&self.a1[lo..hi], &self.a2[lo..hi], &self.b[lo..hi]);
        let (va, vb) = (&va[lo..hi], &vb[lo..hi]);

        // Every voice's push, over this range only.
        for (p, &f) in points.iter().zip(forces.iter()) {
            if f == 0.0 || p.len() < hi {
                continue;
            }
            let sh = &p[lo..hi];
            for i in 0..n {
                dr[i] += sh[i] * f;
            }
        }
        // Then the step and the two readings, in one walk as `advance` does.
        let (mut ra, mut rb) = (0.0, 0.0);
        for i in 0..n {
            let prev = q1[i];
            let q = a1[i] * prev - a2[i] * q2[i] + bc[i] * dr[i];
            let step = q - prev;
            ra += va[i] * step;
            rb += vb[i] * step;
            q2[i] = prev;
            q1[i] = q;
            dr[i] = 0.0;
        }
        (ra * self.sr, rb * self.sr)
    }

    /// How many modes are live, for tiling the parallel plate phase.
    pub fn live(&self) -> usize {
        self.n_active
    }

    /// A whole BLOCK of the plate, tile-major — the decoupled board's step.
    ///
    /// The per-sample loop walks every coefficient array and every voice's
    /// 29 kB shape once PER SAMPLE, and this engine is measured memory-bound:
    /// at 31 voices that is ~0.9 MB of traffic per sample, 43 GB/s at 48 kHz,
    /// which is the whole story of the coupling's cost. When the board is
    /// DECOUPLED the strings' forces for the block are known before the plate
    /// moves at all, so the loops can be exchanged — but not all the way:
    /// pure mode-major strands each mode's recursion on its own dependency
    /// chain, and MEASURED that gave back half of what the traffic saved
    /// (0.75x where the tiles below reach further). So the modes go in TILES:
    ///
    ///  1. the tile's generalised forces for the whole block are gathered
    ///     into a t-major matrix (each voice's shape slice read once per
    ///     block, unit-stride, the GEMM the compiler can vectorise), then
    ///  2. the block runs sample by sample with the lane loop INSIDE — the
    ///     tile's recursions are independent, which is exactly the shape the
    ///     per-sample `tick` vectorised across all modes.
    ///
    /// Traffic per block: every array once. Arithmetic: identical, mode for
    /// mode and sample for sample, to `range_drive_tick_read2`.
    ///
    /// `forces` is voice-major (`forces[v * frames + t]`), `out` receives the
    /// two ears' VELOCITIES per sample (the same numbers `advance` returns
    /// before radiation). Any force already sitting in `drive` (a push from
    /// outside the block loop) lands on the first sample, as it would have.
    pub fn block_drive_tick_read2(
        &mut self,
        points: &[std::sync::Arc<Vec<f32>>],
        forces: &[f64],
        frames: usize,
        va: &[f64],
        vb: &[f64],
        out: &mut [(f64, f64)],
        scratch: &mut Vec<f64>,
        scratch32: &mut Vec<f32>,
        forces32: &[f32],
    ) {
        debug_assert_eq!(forces.len(), points.len() * frames);
        debug_assert!(out.len() >= frames);
        for o in out[..frames].iter_mut() {
            *o = (0.0, 0.0);
        }
        scratch.clear();
        scratch.resize(BLOCK_TILE * frames, 0.0);
        scratch32.clear();
        scratch32.resize(BLOCK_TILE * frames, 0.0);
        let n = self.n_active;
        unsafe {
            self.range_block_drive_tick_read2(
                0, n, points, forces, frames, va, vb, out, scratch, scratch32, forces32,
            )
        };
        for o in out[..frames].iter_mut() {
            o.0 *= self.sr;
            o.1 *= self.sr;
        }
    }

    /// One participant's slice of the block-major step: the modes `lo..hi`,
    /// for every sample of the block, accumulated into `out` WITHOUT the
    /// final sample-rate scaling (the caller sums participants, then scales).
    /// `scratch` must hold at least `BLOCK_TILE * frames`.
    ///
    /// # Safety
    /// As `range_drive_tick_read2`: concurrent callers' ranges must tile
    /// `0..n_active` without overlapping, and each needs its own `out` and
    /// `scratch`.
    pub unsafe fn range_block_drive_tick_read2(
        &self,
        range_lo: usize,
        range_hi: usize,
        points: &[std::sync::Arc<Vec<f32>>],
        forces: &[f64],
        frames: usize,
        va: &[f64],
        vb: &[f64],
        out: &mut [(f64, f64)],
        scratch: &mut [f64],
        scratch32: &mut [f32],
        forces32: &[f32],
    ) {
        const TK: usize = BLOCK_TILE;
        // Read once, outside every loop: this is an A/B handle, not a knob.
        let gather32 = GATHER_F32.load(std::sync::atomic::Ordering::Relaxed)
            && forces32.len() == points.len() * frames;
        let n = range_hi;
        let mut lo = range_lo;
        while lo < n {
            let tk = TK.min(n - lo);
            let g = &mut scratch[..tk * frames];
            // Sliced only where it is used: a caller that never asks for the
            // single-precision gather is entitled to hand an empty buffer,
            // and slicing it unconditionally panicked the moment the f32
            // kernel became the default (the worker pool did exactly that).
            let g32 = if gather32 {
                &mut scratch32[..tk * frames]
            } else {
                &mut scratch32[..0]
            };
            if gather32 {
                g32.iter_mut().for_each(|x| *x = 0.0);
                for (v, p) in points.iter().enumerate() {
                    if p.len() < lo + tk {
                        continue;
                    }
                    let fv = &forces32[v * frames..(v + 1) * frames];
                    for k in 0..tk {
                        let w = p[lo + k];
                        if w == 0.0 {
                            continue;
                        }
                        let gk = &mut g32[k * frames..k * frames + frames];
                        for t in 0..frames {
                            gk[t] += w * fv[t];
                        }
                    }
                }
            }
            g.iter_mut().for_each(|x| *x = 0.0);
            // ── 1. Gather the tile's forces, k-major: g[k·frames + t] ─────
            // Unit stride over the block per (mode, voice) — the layout the
            // vector units like; the t-major variant was measured slower.
            // ── Why the GATHER stays in double, though it is the wall ─────
            //
            // This loop is the engine's arithmetic wall: 2·M·V·T flops a
            // block, 10.8 Gflop/s per instance at 31 voices, against 0.9 for
            // the recursion below. Single precision should double what one
            // AVX2 FMA issues, and the accuracy is not in question here (`g`
            // sums at most eighty-eight products; twenty-four mantissa bits
            // leave it correct to a part in ten million, far under the
            // modelling error of the law that produced the forces).
            //
            // It was BUILT, 2026-08-25, and it is not kept: measured at 73,
            // 80, 81 and 83 percent of budget at 31 voices where f64 sat at
            // 64-67 — but every one of those runs was taken on a machine
            // carrying a load average of 3.5 to 6.5, and the unchanged exact
            // path drifted 0.36x to 0.25x across the same runs, so what they
            // measured was the machine and not the kernel. The change also
            // has a real cost the arithmetic hides: the recursion then reads
            // f32 and widens per element, 463k conversions a block, giving
            // back what the wider FMA won. Reinstating it needs an in-process
            // A/B that alternates the two kernels — the discipline every
            // other sweep in this project uses — not another pass on a busy
            // machine.
            for (v, p) in points.iter().enumerate() {
                if gather32 || p.len() < lo + tk {
                    continue;
                }
                let fv = &forces[v * frames..(v + 1) * frames];
                for k in 0..tk {
                    // Single precision on the way in, double from here on:
                    // the widening is paid once per (mode, voice) per BLOCK
                    // and the multiply-add below runs 128 times on it. See
                    // `Soundboard::attachment_f32_shared`.
                    let w = p[lo + k] as f64;
                    if w == 0.0 {
                        continue;
                    }
                    let gk = &mut g[k * frames..k * frames + frames];
                    for t in 0..frames {
                        gk[t] += w * fv[t];
                    }
                }
            }
            // The range's disjointness is the caller's contract, as in
            // `range_drive_tick_read2`.
            let dr = unsafe {
                std::slice::from_raw_parts_mut(self.drive.as_ptr().add(lo) as *mut f64, tk)
            };
            for k in 0..tk {
                if gather32 {
                    g32[k * frames] += dr[k] as f32;
                } else {
                    g[k * frames] += dr[k];
                }
                dr[k] = 0.0;
            }
            // ── 2. The block, lanes inside: independent recursions ────────
            let (a1, a2, bc) = (&self.a1[lo..lo + tk], &self.a2[lo..lo + tk], &self.b[lo..lo + tk]);
            let (sa, sb) = (&va[lo..lo + tk], &vb[lo..lo + tk]);
            let q1 = unsafe {
                std::slice::from_raw_parts_mut(self.q1.as_ptr().add(lo) as *mut f64, tk)
            };
            let q2 = unsafe {
                std::slice::from_raw_parts_mut(self.q2.as_ptr().add(lo) as *mut f64, tk)
            };
            // The tile's forces (TK·frames doubles) are L1-resident, so the
            // stride-`frames` read per lane costs nothing.
            for t in 0..frames {
                let (mut ra, mut rb) = (0.0, 0.0);
                for k in 0..tk {
                    let prev = q1[k];
                    let drive = if gather32 {
                        g32[k * frames + t] as f64
                    } else {
                        g[k * frames + t]
                    };
                    let q = a1[k] * prev - a2[k] * q2[k] + bc[k] * drive;
                    let step = q - prev;
                    ra += sa[k] * step;
                    rb += sb[k] * step;
                    q2[k] = prev;
                    q1[k] = q;
                }
                out[t].0 += ra;
                out[t].1 += rb;
            }
            lo += tk;
        }
    }

    /// MEASURED 2026-08-23, and both alternatives looked better on paper.
    ///
    /// Walking the bank a TILE of modes at a time, so `q1` and `b` are fetched
    /// once for all the voices instead of once per voice, is SLOWER: 0.25x
    /// realtime at 31 voices against 0.34x. The plate's arrays are 29 kB each
    /// and were already staying in cache across the voice loop, so there was
    /// little to win, and hopping between eighty shape arrays every 512 modes
    /// destroys the hardware prefetch that makes reading one shape end to end
    /// almost free.
    ///
    /// MEASURED 2026-08-23: fusing this with `add_force` into a single walk —
    /// which would need the drive to carry the PREVIOUS sample's force, and so
    /// a sample of extra delay in the string-to-plate path — comes out at
    /// 0.93x, i.e. SLOWER, at both 16 and 88 voices. The store into `drive`
    /// costs the read its vector registers. So the two walks stay two walks,
    /// and no fidelity was spent finding that out.
    ///
    /// Position AND compliance at a point in ONE sweep of `shape`.
    ///
    /// The engine needs both before it can solve a voice's bridge force, and both
    /// are weighted sums over the same shape — the board's `shape` is 3618 f64
    /// (~29 KB) and reading it twice, once per call, is pure memory traffic on a
    /// kernel that is bandwidth-bound. Fused here it is streamed once. Identical
    /// arithmetic to `read` + `compliance`, so bit-for-bit the same numbers.
    #[inline]

    pub fn read_and_compliance(&self, shape: &[f64]) -> (f64, f64) {
        // `read` sums over `n`, `compliance` over `n_active`; for the board the
        // two are equal (it never prunes), and `read`'s terms past `n_active`
        // are on already-retired modes whose `q1` is zero, so summing to
        // `n_active` here changes nothing for it. Do the overlap fused, then the
        // read-only tail if the caller's shape is longer than `n_active`.
        let m = self.n_active.min(shape.len());
        // ── Four accumulator chains, not one ──────────────────────────────
        //
        // This is the hottest loop in the instrument: every sounding string
        // walks the board's 3618 modes here, every sample, and at sixteen
        // voices that alone is 80% of the engine's time. Written as one running
        // sum it CANNOT vectorise — Rust does not reassociate floating point,
        // so LLVM emits one FMA per mode and the loop runs at the latency of
        // the accumulator chain instead of the throughput of the machine. Four
        // independent chains give the pipelines something to do and let the
        // vectoriser in.
        //
        // The terms are therefore summed in a different ORDER. That is a
        // reassociation of the same products, not a different computation, and
        // it is why this one is held to a null test rather than to bit equality.
        let (sh, q1, b) = (&shape[..m], &self.q1[..m], &self.b[..m]);
        let mut pa = [0.0f64; 4];
        let mut ca = [0.0f64; 4];
        let chunks = m / 4;
        for c in 0..chunks {
            let i = c * 4;
            for k in 0..4 {
                let s = sh[i + k];
                pa[k] += s * q1[i + k];
                ca[k] += s * s * b[i + k];
            }
        }
        let mut pos = (pa[0] + pa[1]) + (pa[2] + pa[3]);
        let mut comp = (ca[0] + ca[1]) + (ca[2] + ca[3]);
        for i in (chunks * 4)..m {
            let s = sh[i];
            pos += s * q1[i];
            comp += s * s * b[i];
        }
        for i in m..self.n.min(shape.len()) {
            pos += shape[i] * self.q1[i];
        }
        (pos, comp)
    }

    /// `Σ wₖ qₖ²` — a weighted sum of the SQUARES of the coordinates, which is
    /// what a quantity quadratic in the motion needs. The string's stretch is
    /// one: it does not care which way the string moved, only how far.
    #[inline]
    pub fn read_squared(&self, weights: &[f64]) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n {
            acc += weights[i] * self.q1[i] * self.q1[i];
        }
        acc
    }

    /// Velocity at a point, from the step the coordinates just took. This lags
    /// the true velocity by half a sample, which is inaudible, and keeps the
    /// scheme explicit — unlike Humbert's exact velocity recursion (§3.3.2),
    /// whose dependence on the force at the current step would put a contact
    /// model back into an implicit solve.
    #[inline]
    pub fn read_velocity(&self, shape: &[f64]) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n {
            acc += shape[i] * (self.q1[i] - self.q2[i]);
        }
        acc * self.sr
    }

    /// Advance every mode and read the velocity at TWO points, in one walk.
    ///
    /// What the soundboard does every sample: take the forces the strings left,
    /// step, and read how fast the plate moves under each of the two ears. Done
    /// as `tick` + two `read_velocity` calls it walks 3618 modes three times —
    /// and this bank is 29 kB, so the third walk is reading it back out of L2,
    /// not out of cache. Same arithmetic in the same order, one walk.
    #[inline]
    /// `tick_read2_velocity` over two pairs of shapes at once: the ears over
    /// the global modes and over the rest.
    pub fn tick_read4_velocity(&mut self, va: &[f64], vb: &[f64], vc: &[f64], vd: &[f64]) -> [f64; 4] {
        let n = self.n;
        let (a1, a2, bc) = (&self.a1[..n], &self.a2[..n], &self.b[..n]);
        let (va, vb, vc, vd) = (&va[..n], &vb[..n], &vc[..n], &vd[..n]);
        let drive = &mut self.drive[..n];
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        let mut r = [0.0f64; 4];
        for i in 0..n {
            let prev = q1[i];
            let q = a1[i] * prev - a2[i] * q2[i] + bc[i] * drive[i];
            let step = q - prev;
            r[0] += va[i] * step;
            r[1] += vb[i] * step;
            r[2] += vc[i] * step;
            r[3] += vd[i] * step;
            q2[i] = prev;
            q1[i] = q;
            drive[i] = 0.0;
        }
        r.map(|x| x * self.sr)
    }

    pub fn tick_read2_velocity(&mut self, va: &[f64], vb: &[f64]) -> (f64, f64) {
        let n = self.n;
        let (a1, a2, bc) = (&self.a1[..n], &self.a2[..n], &self.b[..n]);
        let (va, vb) = (&va[..n], &vb[..n]);
        let drive = &mut self.drive[..n];
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        let (mut ra, mut rb) = (0.0, 0.0);
        for i in 0..n {
            let prev = q1[i];
            let q = a1[i] * prev - a2[i] * q2[i] + bc[i] * drive[i];
            // After the step the previous coordinate IS q2, so `read_velocity`
            // called after `tick` sums exactly this difference.
            let step = q - prev;
            ra += va[i] * step;
            rb += vb[i] * step;
            q2[i] = prev;
            q1[i] = q;
            drive[i] = 0.0;
        }
        (ra * self.sr, rb * self.sr)
    }

    /// Where a point will be after the NEXT step if nothing new pushes on it.
    ///
    /// The other half of an implicit coupling. The recursion's new coordinate is
    /// `a₁q₁ − a₂q₂ + b·g`, so a point's position after the step splits cleanly
    /// into a part already determined — this — and `compliance(shape)·g`, its
    /// response to whatever force is applied during the step. Knowing both, the
    /// force at the bridge can be SOLVED for at the same instant it acts instead
    /// of being carried over from the step before.
    ///
    /// That lag is why the coupling has to be held back: an explicit exchange
    /// between a string and a plate goes unstable once the loop gain approaches
    /// one, which is what the inflated `bridge_stiffness` term exists to prevent.
    ///
    /// Forces ALREADY accumulated for this step count as determined: they are in
    /// `drive` and will land in the same recursion. Leaving them out makes each
    /// caller solve as though it were the only thing pushing on the structure,
    /// which is fine for one string and wrong for a chord — the plate is being
    /// driven by every note at once. Counting them makes a sequence of scalar
    /// solves exact in the order they are taken, rather than an approximation.
    #[inline]
    pub fn free_response(&self, shape: &[f64]) -> f64 {
        let n = self.n_active;
        let (a1, a2, q1, q2, b, dr, sh) = (
            &self.a1[..n],
            &self.a2[..n],
            &self.q1[..n],
            &self.q2[..n],
            &self.b[..n],
            &self.drive[..n],
            &shape[..n],
        );
        let mut acc = 0.0;
        for i in 0..n {
            acc += sh[i] * (a1[i] * q1[i] - a2[i] * q2[i] + b[i] * dr[i]);
        }
        acc
    }

    /// The free response at a FRACTION of a sample — the exact solution, not an
    /// extrapolation of it.
    ///
    /// Six attempts at giving the hammer the string's motion inside a sample
    /// failed because they all guessed where the string would be: a compliance, a
    /// t² give, a delay line, a tangent, a curvature. Guessing the state of a
    /// resonator diverges as soon as you go far enough. But its UNFORCED motion
    /// has a closed form,
    ///
    /// ```text
    ///     q(t) = e^{−σt} · [ q₀·cos(ω_d t) + ((q̇₀ + σq₀)/ω_d)·sin(ω_d t) ]
    /// ```
    ///
    /// so it can be evaluated at any instant from the state the bank already
    /// holds. That is an integration and it cannot run away, which is precisely
    /// what every failed attempt was not.
    ///
    /// `t` is in seconds from the last step. Only worth calling while a hammer is
    /// down, which is about a thousandth of what this engine does.
    pub fn free_at(&self, shape: &[f64], t: f64) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n_active.min(shape.len()) {
            let m = &self.modes[i];
            let wd2 = m.w * m.w - m.sigma * m.sigma;
            if wd2 <= 0.0 {
                continue;
            }
            let wd = wd2.sqrt();
            let q0 = self.q1[i];
            let v0 = (self.q1[i] - self.q2[i]) * self.sr;
            let e = (-m.sigma * t).exp();
            acc += shape[i] * e * (q0 * (wd * t).cos() + ((v0 + m.sigma * q0) / wd) * (wd * t).sin());
        }
        acc
    }

    /// How far a point driven through `shape` moves per newton in one sample.
    /// A contact needs this to know the compliance of what it is pressing on.
    /// The real part of the driving-point admittance at a point, at one
    /// frequency, in closed form: velocity per newton, from the same modal
    /// data the time loop integrates.
    ///
    /// Each mode is a unit-mass oscillator in its shape-normalised coordinate
    /// (that is what impulse invariance above encodes), so its velocity FRF is
    /// `iω/(ω_k² − ω² + 2iσ_k ω)` and the point sees `Σ φ_k²·Re{·}`:
    ///
    /// ```text
    ///     Re{Y}(p, ω) = Σ_k φ_k(p)² · 2σ_k ω² / ((ω_k²−ω²)² + (2σ_k ω)²)
    /// ```
    ///
    /// This is what the impulse-and-FFT measurement in
    /// `audit_the_decay_against_the_coupling` estimates numerically; here it is
    /// exact and cheap enough to evaluate per string mode at build time. The
    /// DECOUPLED board uses it to write the coupling's drain into the string
    /// banks: `α_m = T·r²·Re{Y}(p, ω_m)/L`, the same published law the audit
    /// holds the exact model to — with the board's real peaks and valleys at
    /// this note's own attachment, not a flat mean.
    ///
    /// Over ALL modes, not `n_active`: retirement is a state of the time loop,
    /// and this is a property of the instrument.
    pub fn re_admittance(&self, shape: &[f64], w: f64) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.modes.len().min(shape.len()) {
            let m = &self.modes[i];
            let d1 = m.w * m.w - w * w;
            let d2 = 2.0 * m.sigma * w;
            acc += shape[i] * shape[i] * (d2 * w) / (d1 * d1 + d2 * d2);
        }
        acc
    }

    pub fn compliance(&self, shape: &[f64]) -> f64 {
        let mut acc = 0.0;
        // Bounded by the SHORTER of the two. A caller may legitimately hand a
        // shape that does not cover every mode — a voice built for a test has no
        // attachment point at all — and a compliance is a sum, so the missing
        // terms are simply zero. Indexing past the end instead turned that into
        // a panic in the middle of the audio path.
        for i in 0..self.n_active.min(shape.len()) {
            acc += shape[i] * shape[i] * self.b[i];
        }
        acc
    }

    /// How far the forces ALREADY applied this step will move a point, once the
    /// step is taken: `Σ φₖ·bₖ·gₖ` over the drive accumulated so far.
    ///
    /// This is the cross-term of Chabassier's coupled system. Each string solves
    /// its own bridge displacement from the board's FREE response, as though it
    /// were the only thing touching the board; what every other string is doing to
    /// the same board in the same sample is exactly this. `add_force` accumulates
    /// `gₖ = Σⱼ φₖ(j)·Fⱼ` and `tick` clears it, so between the two the sum is
    /// available for the price of one read — no N×N matrix has to be formed.
    pub fn pending_response(&self, shape: &[f64]) -> f64 {
        let mut acc = 0.0;
        for i in 0..self.n_active.min(shape.len()) {
            acc += shape[i] * self.b[i] * self.drive[i];
        }
        acc
    }

    /// Total modal energy, as a stand-in for "is anything still ringing".
    pub fn energy(&self) -> f64 {
        // The live prefix: past it the coordinates are zeroed, so they add
        // nothing but the fetch.
        let mut acc = 0.0;
        for i in 0..self.n_active {
            acc += self.q1[i] * self.q1[i];
        }
        acc
    }

    /// Give up on the modes that have decayed out of hearing.
    ///
    /// Judged on what each mode actually CONTRIBUTES — its coordinate times the
    /// weight it is read out through — because the bridge weights climb with
    /// mode number, so a high mode with a small coordinate can still be doing
    /// audible work. Anything more than `rel` below the loudest contribution in
    /// the bank is dropped, and dropped from the TOP only: the modes are in
    /// ascending frequency and it is the high ones that die first, so the live
    /// set stays a contiguous prefix and the loops stay flat.
    ///
    /// Retired modes are zeroed rather than frozen, so that if the bank is
    /// driven hard again they come back from where physics would have left
    /// them — at rest — instead of resuming a stale value.
    pub fn retire_quiet(&mut self, weight: &[f64], rel: f64) {
        let n = self.n_active;
        if n == 0 {
            return;
        }
        let mut peak = 0.0f64;
        for i in 0..n {
            let c = (weight[i] * self.q1[i]).abs();
            if c > peak {
                peak = c;
            }
        }
        if peak == 0.0 {
            return;
        }
        // Measure against the loudest this bank has EVER been, not against what
        // it is now. Judged against the present, a decaying note never retires
        // anything: the bridge keeps feeding every mode a little, so as the note
        // fades the high partials hold their RATIO to it even as all of them
        // sink towards nothing together. Against the attack, they fall away one
        // after another exactly as hearing loses them.
        //
        // A rise past the old maximum means this string is being driven again —
        // struck, or shaken by something else through an undamped bridge — so
        // the whole bank comes back rather than staying pinned to a floor set by
        // a note that is over.
        if peak > self.peak_ref {
            self.peak_ref = peak;
            self.n_active = self.n;
            return;
        }
        let floor = self.peak_ref * rel;
        let mut live = n;
        while live > 1 {
            let i = live - 1;
            let a = (weight[i] * self.q1[i]).abs();
            let b = (weight[i] * self.q2[i]).abs();
            if a > floor || b > floor {
                break;
            }
            self.q1[i] = 0.0;
            self.q2[i] = 0.0;
            self.drive[i] = 0.0;
            live -= 1;
        }
        self.n_active = live;
    }

    /// How many modes are still being computed, against how many exist.
    pub fn active(&self) -> usize {
        self.n_active
    }

    /// Damp every mode by a factor per sample, which is what a damper felt
    /// landing on a string does: it shortens every partial rather than stopping
    /// the string dead.
    #[inline]
    /// A damper is FELT, and felt eats the top of a string first.
    ///
    /// This multiplied every mode by the same number, which is not a damper but a
    /// fader: it makes a note quieter without changing its colour. What a pad of
    /// felt laid on a string actually does is absorb, and absorption in a soft
    /// lossy contact rises with frequency — the same reason the string's own loss
    /// carries a `b₃ω²` term and the reason a damped bass note is heard as a thud
    /// and not as a quiet note.
    ///
    /// It matters most exactly where the fault was reported. At the default
    /// setting the broadband part removes only **6 dB in 25 ms**, which is the gap
    /// the sweep leaves between a release and the next blow, so in a run of
    /// sixteenths every note is played over the residue of the one before it at
    /// −6 dB — with, until now, ALL of its brightness intact. Those bright
    /// residues stack up through a passage, and a wash of un-dulled high partials
    /// from notes that are supposed to be over is a rasp.
    ///
    /// `DAMPER_HF` sets how much faster the top goes than the bottom: the extra
    /// per-sample loss is `exp(−ω/ω_ref/sr)`, so a mode at the reference frequency
    /// loses one neper per second more than one at DC, and the shape is fixed
    /// while the knob still sets the overall rate.
    pub fn damp(&mut self, factor: f64) {
        for i in 0..self.n {
            let f = self.damped(factor, i);
            self.q1[i] *= f;
            self.q2[i] *= f;
        }
    }

    /// A damper stands at ONE PLACE on the string, and a mode with a node there
    /// hardly feels it.
    ///
    /// Bank, *Physics-Based Sound Synthesis of the Piano* §5.2.5, on what the
    /// usual "cascade a real coefficient onto the loop filter" cannot do — which
    /// is exactly what `damp` was before it had a shape at all: "every 7th partial
    /// is damped inefficiently, since the damper cannot act well on a mode which
    /// has a node at the damper position... **The difference can be heard
    /// especially at the lowest two octaves of the piano.**" He is equally plain
    /// about the flat multiplier: "the damping which arises when one uses such a
    /// simple method is too clean. For the high range, where the damping is fast,
    /// it works well, but in the middle and low registers more refined methods are
    /// needed."
    ///
    /// So a mode is damped in proportion to how much of it is under the felt,
    /// `φₖ(x_d)² = sin²(kπ·a_d)`, and a mode with a node there is left alone. The
    /// floor is not a fudge: a damper is a PAD a centimetre or two wide, not a
    /// point, so it always covers something even where the ideal node falls.
    pub fn set_damper_shape(&mut self, weights: &[f64]) {
        for (d, w) in self.damp_pos.iter_mut().zip(weights.iter()) {
            *d = w.clamp(0.0, 1.0);
        }
    }

    /// The per-sample factor mode `i` sees while a damper is down: the full rate
    /// where the felt is pressing hardest, none of it at a node.
    #[inline]
    fn damped(&self, factor: f64, i: usize) -> f64 {
        1.0 - (1.0 - factor * self.damp_hf[i]) * self.damp_pos[i]
    }

    // ── The same work, in two passes instead of seven ────────────────────────
    //
    // A string is read at three points and driven at two, every sample. Done
    // with the calls above that is seven separate walks over the mode arrays,
    // and a bass string's arrays are far too big to stay in cache between them:
    // 420 modes over six `f64` arrays is 20 kB per string, 60 kB per note, and
    // ten notes of that thrash L2 on every single sample. The arithmetic was
    // never the cost — fetching the same modes over and over was.
    //
    // The two below do exactly what the sequence did, in one read pass and one
    // write pass. They are not an approximation: every read takes its value
    // from `q1`, which `add_force` never touches (it only fills `drive`, which
    // lands on the NEXT step), so the reads and the forces commute and the
    // results are bit-for-bit what the separate calls gave.

    /// Every reading a string needs, in one pass: displacement at two points
    /// and one weighted sum of squares. Replaces `read` + `read` +
    /// `read_squared`.
    #[inline]
    pub fn read3(&self, a: &[f64], b: &[f64], sq: &[f64]) -> (f64, f64, f64) {
        let (mut ra, mut rb, mut rs) = (0.0, 0.0, 0.0);
        let n = self.n_active;
        let (q1, a, b, sq) = (&self.q1[..n], &a[..n], &b[..n], &sq[..n]);
        for i in 0..n {
            let q = q1[i];
            ra += a[i] * q;
            rb += b[i] * q;
            rs += sq[i] * q * q;
        }
        (ra, rb, rs)
    }

    /// Drive at one point, advance, and take three readings — two velocities
    /// and a displacement — in a SINGLE walk over the modes.
    ///
    /// This is the soundboard's whole per-sample job. Written with the calls it
    /// replaces it was five separate walks (`add_force`, `tick`, two
    /// `read_velocity`, `read`) over a bank as big as everything the strings
    /// have between them, which made the plate a rival to the entire choir of
    /// strings for cost rather than the afterthought it looks like in the code.
    ///
    /// Returns `(velocity_a, velocity_b, displacement_c)`. The velocities come
    /// from the step the coordinates just took, which is what `read_velocity`
    /// means, and the displacement is read AFTER the advance exactly as calling
    /// `tick` and then `read` would have given.
    #[inline]
    pub fn drive_tick_read3(
        &mut self,
        drive_shape: &[f64],
        f: f64,
        va: &[f64],
        vb: &[f64],
        dc: &[f64],
    ) -> (f64, f64, f64) {
        let n = self.n_active;
        let (a1, a2, bc) = (&self.a1[..n], &self.a2[..n], &self.b[..n]);
        let (ds, va, vb, dc) = (&drive_shape[..n], &va[..n], &vb[..n], &dc[..n]);
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        let (mut ra, mut rb, mut rc) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let prev = q1[i];
            let q = a1[i] * prev - a2[i] * q2[i] + bc[i] * (ds[i] * f);
            // After the advance the previous coordinate IS q2, so the step the
            // mode just took is `q - prev`.
            let step = q - prev;
            ra += va[i] * step;
            rb += vb[i] * step;
            rc += dc[i] * q;
            q2[i] = prev;
            q1[i] = q;
        }
        (ra * self.sr, rb * self.sr, rc)
    }

    /// The same readings, minus the strike point — which is worth its own
    /// method because a hammer is in contact with a string for two to nine
    /// MILLISECONDS and the note rings for seconds afterwards. For all but a
    /// thousandth of a note's life the strike shape is fetched, multiplied and
    /// summed to produce a number nothing reads.
    #[inline]
    pub fn read2(&self, b: &[f64], sq: &[f64]) -> (f64, f64) {
        let (mut rb, mut rs) = (0.0, 0.0);
        let n = self.n_active;
        let (q1, b, sq) = (&self.q1[..n], &b[..n], &sq[..n]);
        for i in 0..n {
            let q = q1[i];
            rb += b[i] * q;
            rs += sq[i] * q * q;
        }
        (rb, rs)
    }

    /// Drive from ONE point and advance. The counterpart of `read2`: once the
    /// hammer has left, the only thing still pushing the string is the bridge,
    /// and multiplying the strike shape by a force of exactly zero is a whole
    /// array fetched per sample to add nothing. Identical to `drive2_tick` with
    /// `fa = 0.0`, since `0.0 + x` is exactly `x`.
    #[inline]
    pub fn drive1_tick(&mut self, sb: &[f64], fb: f64, damp: f64) {
        let n = self.n_active;
        let (a1, a2, bc) = (&self.a1[..n], &self.a2[..n], &self.b[..n]);
        // The damper's frequency shape, the same one `damp` applies. Folded in
        // here as well or the fused path and the plain one stop being the same
        // arithmetic — which is what `the_fused_pass_is_the_same_arithmetic` is
        // for, and it caught exactly that.
        let (dh, dp) = (&self.damp_hf[..n], &self.damp_pos[..n]);
        let sb = &sb[..n];
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        for i in 0..n {
            let q = a1[i] * q1[i] - a2[i] * q2[i] + bc[i] * (sb[i] * fb);
            let d = if damp == 1.0 { 1.0 } else { 1.0 - (1.0 - damp * dh[i]) * dp[i] };
            q2[i] = q1[i] * d;
            q1[i] = q * d;
        }
    }

    /// Advance one sample with nothing driving, damping on the way out.
    ///
    /// `tick` with a damper, and without the array walk `drive1_tick` spends on a
    /// force that is zero. A polarisation the hammer has finished with is in
    /// exactly this state for the whole life of the note.
    #[inline]
    pub fn damp_tick(&mut self, damp: f64) {
        let n = self.n_active;
        let (a1, a2) = (&self.a1[..n], &self.a2[..n]);
        let (dh, dp) = (&self.damp_hf[..n], &self.damp_pos[..n]);
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        for i in 0..n {
            let q = a1[i] * q1[i] - a2[i] * q2[i];
            let d = if damp == 1.0 { 1.0 } else { 1.0 - (1.0 - damp * dh[i]) * dp[i] };
            q2[i] = q1[i] * d;
            q1[i] = q * d;
        }
    }

    /// Drive the bank at two points and advance one sample, optionally damping
    /// on the way out. Replaces `add_force` + `add_force` + `tick` + `damp`,
    /// and needs no `drive` array at all: the force is known here, so it goes
    /// straight into the recursion instead of being accumulated, read back and
    /// zeroed.
    #[inline]
    pub fn drive2_tick(&mut self, sa: &[f64], fa: f64, sb: &[f64], fb: f64, damp: f64) {
        let n = self.n_active;
        let (a1, a2, bc) = (&self.a1[..n], &self.a2[..n], &self.b[..n]);
        let (dh, dp) = (&self.damp_hf[..n], &self.damp_pos[..n]);
        let (sa, sb) = (&sa[..n], &sb[..n]);
        let (q1, q2) = (&mut self.q1[..n], &mut self.q2[..n]);
        for i in 0..n {
            let g = sa[i] * fa + sb[i] * fb;
            let q = a1[i] * q1[i] - a2[i] * q2[i] + bc[i] * g;
            let d = if damp == 1.0 { 1.0 } else { 1.0 - (1.0 - damp * dh[i]) * dp[i] };
            q2[i] = q1[i] * d;
            q1[i] = q * d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// A mode advanced by `tick_sub` must trace a decaying oscillation at its own
    /// frequency: a smoke test that the sub-step coefficients build a stable,
    /// correctly-tuned recursion. (It cannot be checked against the normal-rate
    /// state, which is tied to the sample spacing; the real validation of the
    /// sub-step contact is the treble level and the published contact durations.)
    #[test]
    fn substep_traces_the_mode_frequency() {
        let f0 = 1976.0;
        let modes = vec![Mode::from_t60(f0, 2.0)];
        let mut b = ModalBank::new();
        b.set_modes(&modes, SR);
        b.set_substep(20);
        let shape = vec![1.0; b.len()];
        b.add_force(&shape, 1.0);
        // Advance a full audio sample's worth of sub-steps and count zero
        // crossings over ~2 ms; the rate should be ~f0.
        let mut prev = 0.0;
        let mut crossings = 0usize;
        let secs = 0.02;
        let subs = (SR as f64 * secs) as usize * 20;
        for i in 0..subs {
            b.tick_sub();
            let x = b.read(&shape);
            if i > 20 && prev < 0.0 && x >= 0.0 {
                crossings += 1;
            }
            prev = x;
            assert!(x.is_finite(), "sub-step diverged");
        }
        let hz = crossings as f64 / secs;
        assert!((hz - f0).abs() < f0 * 0.1, "sub-step traced {hz:.0} Hz, expected ~{f0}");
    }

    /// Re-spacing the state to another rate and back must restore it: the phasor
    /// reconstruction has to be exact, or every contact would leave a small jump
    /// in the string when the hammer let go.
    #[test]
    fn respace_round_trips() {
        let modes = vec![
            Mode::from_t60(55.0, 8.0),
            Mode::from_t60(440.0, 3.0),
            Mode::from_t60(2093.0, 1.5),
        ];
        let mut b = ModalBank::new();
        b.set_modes(&modes, SR);
        let shape = vec![1.0; b.len()];
        // Seed a non-trivial state.
        b.add_force(&shape, 1.0);
        for _ in 0..7 {
            b.tick();
        }
        let dt = 1.0 / SR as f64;
        let dts = dt / 20.0;
        let q1_before: Vec<f64> = (0..b.len()).map(|i| b.q1[i]).collect();
        let q2_before: Vec<f64> = (0..b.len()).map(|i| b.q2[i]).collect();
        b.respace(dt, dts);
        b.respace(dts, dt);
        for i in 0..b.len() {
            assert!((b.q1[i] - q1_before[i]).abs() < 1e-15, "q1 moved");
            assert!(
                (b.q2[i] - q2_before[i]).abs() < 1e-9 * q2_before[i].abs().max(1e-9),
                "respace did not round-trip q2: {} vs {}",
                b.q2[i],
                q2_before[i]
            );
        }
    }

    /// The split an implicit coupling rests on: after a step, a point sits at
    /// its free response plus its compliance times whatever force acted during
    /// that step. If that identity does not hold exactly, solving for the force
    /// solves the wrong equation.
    #[test]
    fn a_step_splits_into_free_response_plus_compliance() {
        let modes: Vec<Mode> = (1..=120)
            .map(|k| Mode::from_t60(110.0 * k as f64 * (1.0 + 2e-4 * (k * k) as f64), 5.0))
            .collect();
        let mut a = ModalBank::new();
        a.set_modes(&modes, SR);
        let mut b = ModalBank::new();
        b.set_modes(&modes, SR);
        let n = a.len();
        let shape: Vec<f64> = (0..n).map(|i| ((i + 1) as f64 * 0.37).sin() / (i + 1) as f64).collect();
        let zero = vec![0.0f64; n];

        for k in 0..500 {
            let f = 30.0 * ((k as f64 * 0.01).sin());
            // Predicted from the split, before the step is taken.
            let predicted = a.free_response(&shape) + a.compliance(&shape) * f;
            a.drive2_tick(&shape, f, &zero, 0.0, 1.0);
            let actual = a.read(&shape);
            assert!(
                (predicted - actual).abs() <= actual.abs() * 1e-9 + 1e-18,
                "step {k}: predicted {predicted:e}, got {actual:e}"
            );
            b.drive2_tick(&shape, f, &zero, 0.0, 1.0);
        }
    }

    /// A damped note has to die the way a damped note was measured to die.
    ///
    /// Bank, *Physics-Based Sound Synthesis of the Piano*, Fig. 6.1: the decay
    /// times of a damped A♯4 (466 Hz), partial by partial, run from about 0.2 s at
    /// the fundamental to about 0.05 s by the twenty-fifth. A flat gain on the
    /// loop — what this bank applied until 2026-08-13 — gives the SAME time to
    /// every partial by construction and can never produce that curve, which is
    /// what Bank means by "the damping is too clean".
    #[test]
    fn a_damped_note_dies_as_bank_measured_it() {
        const F0: f64 = 466.16;
        // The default damper rate, mid-way between the two regulation extremes.
        let factor = (0.99985f64 + 0.9990) * 0.5;
        let modes: Vec<Mode> = (1..=25)
            .map(|k| Mode::from_t60(F0 * k as f64, 6.0))
            .collect();
        let mut b = ModalBank::new();
        b.set_modes(&modes, SR);
        let t60 = |k: usize| -> f64 {
            // The per-sample factor this bank would apply to mode k, turned into
            // the time it takes that mode to fall sixty decibels.
            let d = b.damped(factor, k - 1);
            6.907 / (-d.ln() * SR as f64)
        };
        let (first, last) = (t60(1), t60(25));
        assert!(
            (0.15..=0.30).contains(&first),
            "the fundamental of a damped A#4 takes {first:.3} s to fall 60 dB; Bank Fig. 6.1 \
             measures about 0.2"
        );
        assert!(
            (0.03..=0.08).contains(&last),
            "its twenty-fifth partial takes {last:.3} s; Bank Fig. 6.1 measures about 0.05"
        );
        assert!(
            (3.0..=5.0).contains(&(first / last)),
            "the damper spans {:.1}x from the fundamental to the twenty-fifth partial \
             where Bank measures about 4 — a flat gain would give exactly 1",
            first / last
        );
    }

    /// Giving up on a partial has to be inaudible, and "inaudible" is a number,
    /// not an opinion. A bass string is struck and left to ring for eight
    /// seconds — the length a pedalled bass note actually lasts — once with
    /// dead partials retired and once computing all of them, and the two
    /// signals are compared against the note's own peak.
    ///
    /// Unlike the fused pass this is a genuine approximation, so it is measured
    /// rather than asserted equal.
    ///
    /// NOTE this measures a lone string, which is the friendly case: nothing is
    /// re-exciting it, so its high partials really do fall away. Inside the
    /// instrument the bridge keeps feeding them and far fewer retire — about
    /// 10% under the pedal against the 85% here. Read this as a bound on the
    /// ERROR, which is what it is good for, and not as a promise about the
    /// saving.
    #[test]
    fn retiring_dead_partials_is_inaudible() {
        // A bass string: low fundamental, stiff, hundreds of partials whose
        // damping climbs as ω² exactly as a real one's does.
        let f0 = 41.2;
        let modes: Vec<Mode> = (1..=400)
            .map(|k| {
                let f = f0 * k as f64 * (1.0 + 1.2e-4 * (k * k) as f64).sqrt();
                let w = std::f64::consts::TAU * f;
                Mode { w, sigma: 0.30 + 5.0e-9 * w * w }
            })
            .collect();
        let mk = |b: &ModalBank| -> Vec<f64> {
            (0..b.len()).map(|i| ((i + 1) as f64 * 0.121).sin() / (i + 1) as f64).collect()
        };
        let mut kept = ModalBank::new();
        kept.set_modes(&modes, SR);
        let mut all = ModalBank::new();
        all.set_modes(&modes, SR);
        let (strike, bridge) = (mk(&kept), mk(&kept));
        let zero = vec![0.0f64; kept.len()];

        let n = (SR * 8.0) as usize;
        let mut peak = 0.0f64;
        let mut worst = 0.0f64;
        for k in 0..n {
            // A hammer's blow: a couple of milliseconds of force, once.
            let f = if k < 96 {
                80.0 * (std::f64::consts::PI * k as f64 / 96.0).sin()
            } else {
                0.0
            };
            let (a, _, _) = kept.read3(&bridge, &zero, &zero);
            let (b, _, _) = all.read3(&bridge, &zero, &zero);
            peak = peak.max(b.abs());
            worst = worst.max((a - b).abs());
            kept.drive2_tick(&strike, f, &zero, 0.0, 1.0);
            all.drive2_tick(&strike, f, &zero, 0.0, 1.0);
            if k % 256 == 0 {
                kept.retire_quiet(&bridge, super::super::voice::RETIRE_BELOW);
            }
        }
        let db = 20.0 * (worst / peak).log10();
        let saved = 100.0 - 100.0 * kept.active() as f64 / all.active() as f64;
        eprintln!(
            "retraite des partiels : erreur crete {db:.1} dB sous la note, \
             {saved:.0}% des modes abandonnes apres 8 s"
        );
        // Measured, not assumed. The threshold was chosen from a sweep: the
        // error is the retired modes' residue ADDING UP, so it lands about
        // sqrt(count) above the per-mode floor — 1e-6 gave −94 dB, which is
        // inside what a 16-bit recording carries. Two decades tighter costs
        // seven points of savings and buys forty-seven decibels.
        assert!(db < -130.0, "retiring partials changed the sound by {db:.1} dB");
        assert!(saved > 70.0, "retirement saved almost nothing ({saved:.0}%)");
    }

    /// The fused pair has to be the SAME MODEL, not a cheaper approximation of
    /// it. `read3` and `drive2_tick` exist only to stop walking the mode arrays
    /// seven times a sample; if they ever drift from the calls they replace,
    /// the piano has quietly become a different instrument. Bit-for-bit, over a
    /// bank big enough and a run long enough for any discrepancy to show.
    #[test]
    fn the_fused_pass_is_the_same_arithmetic() {
        let modes: Vec<Mode> = (1..=200)
            .map(|k| Mode::from_t60(55.0 * k as f64 * (1.0 + 3e-4 * (k * k) as f64), 6.0))
            .collect();
        let mut slow = ModalBank::new();
        slow.set_modes(&modes, SR);
        let mut fast = ModalBank::new();
        fast.set_modes(&modes, SR);
        let n = slow.len();
        assert!(n > 80, "the bank is too small to prove anything ({n} modes)");

        // Two unrelated excitation points and a set of weights, as a string has.
        let mk = |seed: f64| -> Vec<f64> {
            (0..n).map(|i| ((i as f64 * seed).sin() * 0.7 + 0.1) / (i + 1) as f64).collect()
        };
        let (strike, bridge, elong) = (mk(1.7), mk(0.3), mk(2.9));

        for k in 0..4000 {
            let t = k as f64 / SR as f64;
            // A force that stops partway, so the zero-force path is covered too.
            let fa = if k < 300 { 40.0 * (t * 900.0).sin().max(0.0) } else { 0.0 };
            let fb = 1e-6 * (t * 130.0).sin();
            let damp = if k > 2000 { 0.9993 } else { 1.0 };

            let a = slow.read(&strike);
            let b = slow.read(&bridge);
            let c = slow.read_squared(&elong);
            let (fa2, fb2, fc2) = fast.read3(&strike, &bridge, &elong);
            assert_eq!(a.to_bits(), fa2.to_bits(), "strike readout diverged at {k}");
            assert_eq!(b.to_bits(), fb2.to_bits(), "bridge readout diverged at {k}");
            assert_eq!(c.to_bits(), fc2.to_bits(), "stretch readout diverged at {k}");

            // The one-point pair has to agree with the two-point pair whenever
            // the hammer is gone, which is nearly always.
            if fa == 0.0 {
                let (rb, rs) = fast.read2(&bridge, &elong);
                assert_eq!(b.to_bits(), rb.to_bits(), "read2 bridge diverged at {k}");
                assert_eq!(c.to_bits(), rs.to_bits(), "read2 stretch diverged at {k}");
            }
            if fa != 0.0 {
                slow.add_force(&strike, fa);
            }
            if fb != 0.0 {
                slow.add_force(&bridge, fb);
            }
            slow.tick();
            if damp != 1.0 {
                slow.damp(damp);
            }
            if fa == 0.0 {
                fast.drive1_tick(&bridge, fb, damp);
            } else {
                fast.drive2_tick(&strike, fa, &bridge, fb, damp);
            }
        }
        // And the states themselves, not just what was read out of them.
        for i in 0..n {
            assert_eq!(slow.q1[i].to_bits(), fast.q1[i].to_bits(), "mode {i} state diverged");
        }
    }

    /// The soundboard's fused advance must be the old three walks, to the bit.
    ///
    /// This one is worth pinning hard: it runs on every sample of every note
    /// the instrument ever plays, and a difference here would be a difference
    /// in the sound of the plate itself.
    #[test]
    fn the_fused_board_advance_is_the_same_arithmetic() {
        let modes: Vec<Mode> = (1..=300)
            .map(|k| Mode::from_t60(31.0 * k as f64 * (1.0 + 1e-4 * (k * k) as f64), 4.0))
            .collect();
        let mut slow = ModalBank::new();
        slow.set_modes(&modes, SR);
        let mut fast = ModalBank::new();
        fast.set_modes(&modes, SR);
        let n = slow.len();

        let mk = |seed: f64| -> Vec<f64> {
            (0..n).map(|i| ((i as f64 * seed).cos() * 0.6 + 0.2) / (i + 3) as f64).collect()
        };
        let (left, right, bridge) = (mk(0.9), mk(2.3), mk(1.1));

        for k in 0..4000 {
            let t = k as f64 / SR as f64;
            // Silence part way through, so the no-force path is covered too.
            let f = if k < 2500 { 12.0 * (t * 410.0).sin() } else { 0.0 };
            slow.add_force(&bridge, f);
            fast.add_force(&bridge, f);

            slow.tick();
            let vl = slow.read_velocity(&left);
            let vr = slow.read_velocity(&right);
            let (fl, fr) = fast.tick_read2_velocity(&left, &right);
            assert_eq!(vl.to_bits(), fl.to_bits(), "left ear diverged at {k}");
            assert_eq!(vr.to_bits(), fr.to_bits(), "right ear diverged at {k}");
        }
        for i in 0..n {
            assert_eq!(slow.q1[i].to_bits(), fast.q1[i].to_bits(), "mode {i} state diverged");
            assert_eq!(slow.drive[i].to_bits(), fast.drive[i].to_bits(), "mode {i} drive not cleared");
        }
    }

    fn single(f_hz: f64, t60: f64) -> ModalBank {
        let mut b = ModalBank::new();
        b.set_modes(&[Mode::from_t60(f_hz, t60)], SR);
        b
    }

    /// The whole scheme rests on this: struck once, a mode must ring at exactly
    /// the frequency it was given.
    #[test]
    fn a_mode_rings_at_its_own_frequency() {
        for f in [55.0, 440.0, 3520.0] {
            let mut b = single(f, 4.0);
            let shape = [1.0f64];
            b.add_force(&shape, 1.0);
            let n = (SR as usize) / 2;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                b.tick();
                out.push(b.read(&shape));
            }
            // Count zero crossings over a whole number of periods.
            let (mut first, mut last, mut crossings) = (0usize, 0usize, 0usize);
            for i in 1..out.len() {
                if out[i - 1] <= 0.0 && out[i] > 0.0 {
                    if crossings == 0 {
                        first = i;
                    }
                    last = i;
                    crossings += 1;
                }
            }
            assert!(crossings > 4, "{f} Hz: no oscillation");
            let got = (crossings - 1) as f64 * SR as f64 / (last - first) as f64;
            let cents = 1200.0 * (got / f).log2();
            assert!(cents.abs() < 1.0, "{f} Hz came out at {got:.2} Hz ({cents:+.2} cents)");
        }
    }

    /// And decay in the time it was told to.
    #[test]
    fn a_mode_decays_over_its_t60() {
        let t60 = 0.5f64;
        let mut b = single(220.0, t60);
        let shape = [1.0f64];
        b.add_force(&shape, 1.0);
        let mut peak_early = 0.0f64;
        let mut peak_late = 0.0f64;
        let late_from = (SR as f64 * t60) as usize;
        for i in 0..(SR as usize) {
            b.tick();
            let y = b.read(&shape).abs();
            if i < (SR as usize) / 100 {
                peak_early = peak_early.max(y);
            }
            if i >= late_from && i < late_from + (SR as usize) / 100 {
                peak_late = peak_late.max(y);
            }
        }
        let drop = 20.0 * (peak_late / peak_early).log10();
        assert!((drop + 60.0).abs() < 3.0, "expected about -60 dB after T60, got {drop:.1} dB");
    }

    /// A loss factor is the form modal studies of soundboards publish, so the
    /// conversion has to be right: η = 2σ/ω.
    #[test]
    fn loss_factor_matches_its_decay() {
        // Spruce soundboard, the mean Ege measured over the 55 lowest modes.
        let m = Mode::from_loss_factor(500.0, 0.023);
        let expected_sigma = 0.023 * std::f64::consts::PI * 500.0;
        assert!((m.sigma - expected_sigma).abs() < 1e-9);
        // ~36 s⁻¹ here, and about 80 s⁻¹ up at 1.1 kHz, which is what that
        // study reports as the mean below its transition frequency.
        let at_1100 = Mode::from_loss_factor(1100.0, 0.023).sigma;
        assert!((at_1100 - 80.0).abs() < 5.0, "got {at_1100:.1} s⁻¹");
    }

    /// Modes that cannot be represented are dropped rather than aliased.
    #[test]
    fn modes_above_nyquist_are_dropped() {
        let mut b = ModalBank::new();
        b.set_modes(
            &[
                Mode::from_t60(440.0, 1.0),
                Mode::from_t60(23_000.0, 1.0),
                Mode::from_t60(30_000.0, 1.0),
            ],
            SR,
        );
        assert_eq!(b.len(), 2, "only the modes below Nyquist should survive");
    }

    /// The compliance has to be the real thing, because the hammer will be
    /// pressing against it: one sample of a unit force must move the point by
    /// exactly that much.
    #[test]
    fn compliance_predicts_the_first_step() {
        let mut b = ModalBank::new();
        let modes: Vec<Mode> = (1..=12).map(|k| Mode::from_t60(110.0 * k as f64, 3.0)).collect();
        b.set_modes(&modes, SR);
        let shape: Vec<f64> = (1..=b.len())
            .map(|k| (std::f64::consts::PI * k as f64 * 0.13).sin())
            .collect();
        let predicted = b.compliance(&shape);
        b.add_force(&shape, 1.0);
        b.tick();
        let moved = b.read(&shape);
        assert!(
            (moved - predicted).abs() < 1e-15,
            "predicted {predicted:e}, moved {moved:e}"
        );
    }

    /// Forces from several points add up, which is what lets one bank carry a
    /// hammer at one place and a bridge at another.
    #[test]
    fn forces_from_different_points_superpose() {
        let modes: Vec<Mode> = (1..=8).map(|k| Mode::from_t60(220.0 * k as f64, 2.0)).collect();
        let shape_a: Vec<f64> = (1..=8).map(|k| (std::f64::consts::PI * k as f64 * 0.11).sin()).collect();
        let shape_b: Vec<f64> = (1..=8).map(|k| (std::f64::consts::PI * k as f64 * 0.37).sin()).collect();
        let run = |fa: f64, fb: f64| -> Vec<f64> {
            let mut bank = ModalBank::new();
            bank.set_modes(&modes, SR);
            let mut out = Vec::new();
            for i in 0..2000 {
                if i == 0 {
                    bank.add_force(&shape_a, fa);
                    bank.add_force(&shape_b, fb);
                }
                bank.tick();
                out.push(bank.read(&shape_a));
            }
            out
        };
        let both = run(1.0, 1.0);
        let only_a = run(1.0, 0.0);
        let only_b = run(0.0, 1.0);
        for i in 0..both.len() {
            assert!(
                (both[i] - (only_a[i] + only_b[i])).abs() < 1e-12,
                "superposition broke at sample {i}"
            );
        }
    }
}

#[cfg(test)]
mod drive_probe {
    use super::*;
    /// What a unit force impulse leaves in a mode, as a function of its
    /// frequency.
    ///
    /// A mode of unit modal mass struck with an impulse `J` swings with
    /// amplitude `J / omega`: the stiffer the mode, the less it moves for the
    /// same kick. If this bank gives the same amplitude at 100 Hz and at 10 kHz
    /// then every partial is over-driven by its own frequency, and every note is
    /// 6 dB per octave too bright.
    #[test]
    #[ignore]
    fn what_an_impulse_leaves_in_a_mode() {
        let sr = 48_000.0f32;
        eprintln!("  mode Hz    peak amplitude after a unit impulse    vs 1/omega");
        let mut first = None;
        for &hz in &[100.0f64, 400.0, 1000.0, 2500.0, 5000.0, 10000.0] {
            let mut b = ModalBank::new();
            b.set_modes(&[Mode { w: std::f64::consts::TAU * hz, sigma: 0.5 }], sr);
            b.add_force(&[1.0], 1.0);
            let mut peak = 0.0f64;
            for _ in 0..(sr as usize / 20) {
                b.tick();
                peak = peak.max(b.read(&[1.0]).abs());
            }
            let expect = 1.0 / (std::f64::consts::TAU * hz);
            if first.is_none() {
                first = Some((peak, expect));
            }
            let (p0, e0) = first.unwrap();
            eprintln!(
                "  {hz:7.0}    {peak:.6e}   {:+6.1} dB from 100 Hz     physics says {:+6.1} dB",
                20.0 * (peak / p0).log10(),
                20.0 * (expect / e0).log10()
            );
        }
    }
}

#[cfg(test)]
mod drive_scale_probe {
    use super::*;

    /// The proposed cure for the first-step error, checked before anything is
    /// built on it. The bank's input is impulse-invariant: right for an impulse,
    /// right at DC, twice too large for a force HELD across the sample — which is
    /// the only kind the contact ever applies. The step-invariant (zero-order
    /// hold) equivalent of the same resonator keeps the SAME denominator and
    /// spreads the input over two coefficients:
    ///
    ///     b1 = (1 - E c - (sigma/wd) E s) / w0^2
    ///     b2 = (E^2 - E c + (sigma/wd) E s) / w0^2
    ///
    /// with `E = exp(-sigma*T)`, `c = cos(wd*T)`, `s = sin(wd*T)`. It must satisfy
    /// both ends at once: first step `T^2/2`, and DC exactly `1/w0^2`.
    #[test]
    #[ignore]
    fn the_step_invariant_input_gets_both_ends_right() {
        let sr = 48_000.0f64;
        let t = 1.0 / sr;
        for f0 in [100.0f64, 1000.0, 2500.0] {
            let w0 = std::f64::consts::TAU * f0;
            let sigma = 0.15 * w0;
            let wd = (w0 * w0 - sigma * sigma).sqrt();
            let e = (-sigma * t).exp();
            let (c, sn) = ((wd * t).cos(), (wd * t).sin());
            let b1 = (1.0 - e * c - (sigma / wd) * e * sn) / (w0 * w0);
            let b2 = (e * e - e * c + (sigma / wd) * e * sn) / (w0 * w0);
            let (a1, a2) = (2.0 * e * c, e * e);
            // Run it on a force held from the first sample onwards.
            let (mut q1, mut q2, mut d1, mut d2) = (0.0f64, 0.0, 0.0, 0.0);
            let mut first = 0.0f64;
            for i in 0..(sr as usize / 4) {
                let q = a1 * q1 - a2 * q2 + b1 * d1 + b2 * d2;
                q2 = q1;
                q1 = q;
                d2 = d1;
                d1 = 1.0;
                // `b1` multiplies f[n-1], so the first response to a force that
                // starts at sample 0 lands at sample 1.
                if i == 1 {
                    first = q;
                }
            }
            eprintln!(
                "  {f0:6.0} Hz: first step {first:.4e} against the exact T^2/2 = {:.4e} ({:.3}x);  settles {q1:.4e} against 1/w0^2 = {:.4e} ({:.3}x)",
                t * t / 2.0,
                first / (t * t / 2.0),
                1.0 / (w0 * w0),
                q1 * w0 * w0
            );
        }
    }

    /// A force held for exactly ONE AUDIO SAMPLE, applied two ways: one full tick,
    /// or N sub-ticks. The string must end up in the same place either way, or the
    /// contact solve is pushing against a different string depending on which path
    /// it takes — which is what makes the two contact durations disagree.
    #[test]
    #[ignore]
    fn one_sample_of_force_moves_the_string_the_same_either_way() {
        let sr = 48_000.0f32;
        for f0 in [100.0f64, 1000.0, 2500.0] {
            let w = std::f64::consts::TAU * f0;
            let m = Mode { w, sigma: 0.15 * w };
            let shape = [1.0f64];
            let mut full = ModalBank::new();
            full.set_modes(&[m], sr);
            full.add_force(&shape, 1.0);
            full.tick();
            let yf = full.read(&shape);
            let cf = full.compliance(&shape);
            for steps in [4usize, 20] {
                let mut sub = ModalBank::new();
                sub.set_modes(&[m], sr);
                sub.set_substep(steps);
                for _ in 0..steps {
                    sub.add_force(&shape, 1.0);
                    sub.tick_sub();
                }
                let ys = sub.read(&shape);
                let cs = sub.compliance_sub(&shape);
                eprintln!(
                    "  {f0:6.0} Hz: one tick moves it {yf:.4e}, {steps:2} sub-ticks move it {ys:.4e}  ({:.2}x) | compliance {cf:.3e} vs sub {cs:.3e} x{steps} = {:.3e}",
                    ys / yf,
                    cs * steps as f64
                );
            }
        }
    }

    /// Hold a CONSTANT force on one mode and let it settle. A resonator's static
    /// deflection is `f / w0^2`, with nothing discrete about it — so this says
    /// plainly whether the bank injects the right amount of force, at the audio
    /// rate and sub-stepped, with no convention to argue about.
    #[test]
    #[ignore]
    fn constant_force_gives_the_static_deflection() {
        let sr = 48_000.0f32;
        for f0 in [100.0f64, 1000.0, 2500.0] {
            let w = std::f64::consts::TAU * f0;
            let m = Mode { w, sigma: 0.15 * w };
            let want = 1.0 / (w * w);
            for steps in [1usize, 4, 20] {
                let mut b = ModalBank::new();
                b.set_modes(&[m], sr);
                b.set_substep(steps);
                let shape = [1.0f64];
                let n = (sr as usize / 4) * steps;
                for _ in 0..n {
                    b.add_force(&shape, 1.0);
                    if steps == 1 { b.tick() } else { b.tick_sub() }
                }
                let got = b.read(&shape);
                eprintln!(
                    "  {f0:6.0} Hz, {steps:2} sub-step(s): settles at {got:.6e}, exact is {want:.6e}  ({:+.2}x)",
                    got / want
                );
            }
        }
    }
}

#[cfg(test)]
mod scale_probe {
    use super::*;
    /// Does a force held for one sample do the same thing whether the bank is
    /// advanced once or in sub-steps? If it does not, anything that drives a bank
    /// through a contact is off by that factor.
    #[test]
    #[ignore]
    fn one_step_against_sub_steps() {
        let sr = 48_000.0f32;
        for hz in [200.0f64, 800.0, 1661.0, 3300.0, 6600.0, 12000.0] {
        let md = [Mode { w: std::f64::consts::TAU * hz, sigma: 1.0 }];
        let mut a = ModalBank::new();
        a.set_modes(&md, sr);
        a.add_force(&[1.0], 1.0);
        a.tick();
        let mut b = ModalBank::new();
        b.set_modes(&md, sr);
        b.set_substep(20);
        for _ in 0..20 {
            b.add_force(&[1.0], 1.0);
            b.tick_sub();
        }
        eprintln!(
            "  {hz:7.0} Hz: one step {:.4e}   sub-stepped {:.4e}   ratio {:.3}",
            a.read(&[1.0]),
            b.read(&[1.0]),
            b.read(&[1.0]) / a.read(&[1.0])
        );
        }
    }
}

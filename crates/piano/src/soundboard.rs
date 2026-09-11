//! The soundboard: one plate, shared by every string.
//!
//! Chabassier models it as an orthotropic Reissner-Mindlin plate stiffened by
//! ribs and loaded by the bridges, and says plainly that its modes cannot be
//! computed analytically — "il n'est pas possible de calculer analytiquement les
//! modes d'une plaque de Reissner Mindlin, même isotrope carrée". So they are
//! not computed here either. They come from measurement.
//!
//! ## What was measured
//!
//! Ege's modal study of a piano soundboard (thesis, ENSMP/LMS 2009; Ege,
//! Boutillon & Rébillat, *Vibroacoustics of the piano soundboard*, JSV 2013) is
//! entirely quantitative, and it is the source for everything below.
//!
//! * The average spacing between modes is **≈ 22 Hz** over the 21 lowest, which
//!   agrees with the ≈ 22 Hz another study reports for a baby grand.
//! * The modal density climbs slowly towards **0.06 modes/Hz** — a spacing of
//!   about 17 Hz — below the transition.
//! * **Two regimes.** Below **1.1 kHz** the ribbed board behaves as a
//!   homogeneous plate and its modes extend over the whole surface. Above it the
//!   board becomes "a juxtaposition of waveguides" between the ribs, the
//!   vibration localises, and the modal density *seen from one point* falls and
//!   differs from point to point.
//! * The modal loss factor runs **1% to 3%** up to about 1.2 kHz, mean **2.3%**
//!   over the 55 lowest modes — the values of spruce itself.
//! * The damping factor averages **~80 s⁻¹** below 1.2 kHz and rises to
//!   **~130 s⁻¹** between 1.2 and 1.5 kHz, where radiation becomes efficient
//!   (Suzuki puts that transition at 1–1.6 kHz), then returns to the material's
//!   own values above 1.8 kHz. A board therefore rings for something like 90 ms.
//! * The lowest modes of a stripped 2.74 m grand, from Conklin's Chladni figures
//!   (via Chabassier): **49, 67, 89, 112, 184, 306 Hz**.
//!
//! ## Mode shapes
//!
//! Nobody publishes the shape of the four-hundredth mode of a soundboard at the
//! exact point a bridge pin sits. What is known is their statistics: below the
//! transition every point sees every mode, so a mode's amplitude at an arbitrary
//! point is a draw from a distribution of zero mean; above it the modes
//! localise, so a given point sees only some of them. That is what is built
//! here — deterministically, so an instrument always sounds like itself.
//!
//! Two consequences fall out rather than being arranged. The board is what makes
//! a note decay, because it is where the string's energy goes. And the two
//! points it is listened at see genuinely different combinations of the same
//! modes, so the instrument is stereo without a pan pot anywhere in it.

use super::modal_bank::{Mode, ModalBank};

/// Where the board stops being a plate and becomes a set of waveguides.
const TRANSITION_HZ: f64 = 1100.0;

/// The lowest mode; below this a board does not resonate, it just moves.
/// The plate's lowest mode, from the finite-element model of an actual
/// Steinway D (Chabassier, Joly & Chaigne, JASA 134(1) 2013, Fig. 7): mode 1 at
/// 23 Hz, mode 4 at 67, mode 9 at 139, mode 18 at 252.
///
/// This was 49 Hz — Conklin's lowest Chladni figure on a stripped 2.74 m board —
/// which is not the same thing. A bare board on a bench and a board built into a
/// piano with its rim, ribs, bridges and string downbearing do not start in the
/// same place.
const FIRST_MODE_HZ: f64 = 23.0;
/// Highest mode carried. This is a hard ceiling on the whole instrument: the
/// board is the only thing that radiates, so nothing above it can be heard no
/// matter what the strings do. Set at 7 kHz it left the tone with 70 dB too
/// little above 8 kHz and made the piano sound plucked.
const TOP_HZ: f64 = 16_000.0;

/// Modes per hertz the plate settles at. Chabassier's Steinway D needs 2400
/// modes to 10 kHz; its mode 392 is at 2693 Hz. Both give about a quarter of a
/// mode per hertz.
const BOARD_DENSITY: f64 = 0.24;

/// Effective vibrating mass of the board, kg. A concert grand's soundboard is
/// something under two square metres of spruce a centimetre thick.
const BOARD_MASS: f64 = 6.0;

/// Test-only override of the radiation-tilt depth (in thousandths). 0 = default.
#[cfg(test)]
pub static DEPTH_OVERRIDE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Test-only override of the low-frequency damping floor, in s⁻¹.
/// `u32::MAX` leaves `BOARD_RATE_LOW` alone.
///
/// For the sweep that calibrates it against the one observable that can decide
/// it: how long after the hammer the instrument's output peaks.
#[cfg(test)]
pub static RATE_LOW_OVERRIDE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(u32::MAX);

#[cfg(test)]
fn board_rate_low() -> f64 {
    match RATE_LOW_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        u32::MAX => BOARD_RATE_LOW,
        v => v as f64,
    }
}
#[cfg(not(test))]
fn board_rate_low() -> f64 {
    BOARD_RATE_LOW
}

/// Loss factor of spruce, and the extra radiation damping over the band where
/// the board starts radiating efficiently.
const ETA_SPRUCE: f64 = 0.023;
/// The lowest modes lose energy into the rim the board is glued to, on top of
/// what they lose inside the wood. Ege sets the first four aside for exactly
/// this reason: "at which the energy losses at the rim are probably not
/// negligible compared to those inside wood". Without it the board booms for a
/// second and a half, which no piano does.
const RIM_LOSS: f64 = 26.0;
const RIM_KNEE_HZ: f64 = 90.0;

/// The damping RATE Ege actually measured, which is not the same statement as
/// the loss factor and does not reduce to it.
///
/// A constant loss factor means `σ = ½ηω`, so the damping falls away in
/// proportion to frequency: at 1.1 kHz it lands on the measured 80 s⁻¹, and at
/// 110 Hz it gives 8. Ege reports **80 s⁻¹ as a MEAN below 1.2 kHz** — a rate,
/// roughly flat across the band, not a slope. The two agree only where they
/// cross, and below that the material figure alone leaves the board five times
/// too lively.
///
/// It is not an academic difference. A resonator rises into its steady state in
/// about its own decay time, so a board mode five times too lightly damped takes
/// five times too long to come up — and the instrument's output then peaks a
/// TENTH OF A SECOND after the hammer has struck, measured at 148 ms for A2.
/// A piano peaks within a few milliseconds; a sound that swells instead of
/// striking is the whole difference between a struck string and a plucked one,
/// which is exactly what it was being heard as.
///
/// Physically the low end is not governed by the wood at all: it is the rim the
/// board is glued to, the bridge, the strings' downbearing and the air the board
/// is pushing. A loss factor of 23% at 110 Hz sounds enormous for spruce and is
/// perfectly ordinary for a loaded, radiating plate.
/// How far up its range the bridge sits on each mode, and the overall lift that
/// brings the board's mobility into the measured band. Both are held to account
/// by `bridge_mobility_matches_a_real_grand`.
/// Half-wavelengths a mode at the transition frequency lays along the bridge.
///
/// Set by what it has to produce at the two ends of the scale. Below a couple of
/// hundred hertz the plate must move in ONE PIECE — that is what a long mode is
/// — so a mode there may not manage even half an alternation along the bridge;
/// at six lobes it managed more than one, and the bottom of the keyboard came
/// out in opposition to the top at 60 Hz, which no board does. By the transition
/// frequency there should be enough for the ends to go their own way.
const BRIDGE_LOBES: f64 = 1.5;

const BRIDGE_FLOOR: f64 = 0.55;
/// How hard the strings and the board are tied together, as a multiplier on the
/// bridge point's modal amplitudes.
///
/// **Set from the decay a real piano has, inside the mobility a real bridge was
/// measured to have.** Two independent constraints, and they agree.
///
/// The observable first. An A4 on a grand loses about 12 dB in its first second.
/// This model lost 31.5 with the unison as it stands, and 51.3 with the unison
/// closed — so the fast first stage is not Weinreich's coupled-string split, it
/// is one string emptying itself into the bridge. (That was worth establishing:
/// widening the unison SLOWS the decay here, from 51 dB to 14 at four cents, so
/// the detuning was mitigating the fault rather than causing it.) A shortfall of
/// 31.5 against 12 is 2.6x too much energy per second, which is 1.6x in
/// amplitude.
///
/// Then the published band. Wogram and Giordano measure bridge mobility between
/// 1e-3 and 1e-2 m/s/N; this board sat at 6.7e-3 at 440 Hz, at the TOP of it, and
/// peaked at 1.24e-2 just outside. Dividing the coupling by 1.6 divides `|Y|` by
/// 2.6 and lands it at 2.6e-3 — the middle of the measured range.
///
/// So this is not a knob turned until something sounded right. It is the one
/// value at which the decay matches the instrument and the mobility sits in the
/// middle of what was measured rather than at its edge.
const BRIDGE_COUPLING: f64 = 0.97;

/// Ege's measured mean rate below the transition, imposed as a floor.
///
/// **Zero until 2026-08-23**, on the argument that ~80 s⁻¹ is a mean over a band
/// whose modes crowd towards its top, so the mean comes out right on the
/// material figure alone and no floor is needed. That argument is correct about
/// the mean and wrong about the instrument: a big chord does not excite the mean
/// of a band, it excites its own fundamentals at 80 to 300 Hz, and down there
/// the material figure gives σ = 6 to 22 s⁻¹. A resonator rises in about 1/σ, so
/// the plate took 50 to 100 ms to come up under every note.
///
/// What that sounds like, reported by the user on the Marée demo: "les accords
/// forts sonnent comme un orgue". Measured on a dry chord, no room and no
/// master: the string's force on the bridge peaks at 10 ms and the sound out of
/// the plate peaks at 104 ms; at E2 the force is 2 dB under its peak at five
/// milliseconds where the output is still 25 down. The top band peaks at 0-3 ms,
/// so the strike is there — it just arrives detached from a body blooming behind
/// it. And a chord equals the same notes played separately to within 1 dB, so
/// the shared plate never was the culprit: the fault is per note, and a chord
/// merely stacks five of them.
///
/// The trade, measured note by note (peak of the fundamental, by complex
/// demodulation — a broadband envelope will not do, it sits within a few dB of
/// peak for two hundred milliseconds and its argmax reads as noise):
///
/// ```text
/// note 28: 109 -> 36 ms   note 40:  55 ->  9 ms   note 47: 44 -> 29 ms
/// note 52: 109 -> 16 ms   note 67:  54 -> 23 ms
/// ```
///
/// for 0.4 to 2.5 dB of level per note. "Imposing a floor emptied the
/// instrument" was the objection; the emptying is two decibels, and it is made
/// back by `PLATE_DAMPING_MAKEUP`.
///
/// Eighty rather than sixty because eighty is Ege's published figure and sixty
/// would be a fit, and because eighty is what the user listened to and approved.
const BOARD_RATE_LOW: f64 = 80.0;
/// And what it climbs to where the board begins to radiate properly.
///
/// Still zero: the A/B that settled `BOARD_RATE_LOW` moved only the band below
/// `RADIATION_BAND.0`, so this one has not been heard and is not being changed
/// on the strength of its neighbour.
const BOARD_RATE_RADIATING: f64 = 0.0;
/// What `BOARD_RATE_LOW` costs in level, given back.
///
/// A plate that takes energy faster radiates less of it, so the floor adopted
/// above makes the instrument quieter. Measured (`print_the_makeup_the_damping_
/// floor_needs`) as the energy ratio against an undamped plate, over eight notes
/// spanning the compass and the chord the complaint was about: **+3.27 dB**, and
/// the per-note figures are 4.65 / 3.61 / 2.96 / 4.86 / 2.07 / 3.88 / 1.84 /
/// 0.03 dB from E0 to E6.
///
/// That last one matters and is not an error: the top of the keyboard has no
/// partial inside the floor's band, so it loses nothing and a flat make-up
/// leaves it 3.3 dB louder in relative terms. That happens to push against the
/// treble deficit recorded elsewhere rather than with it — but it is a side
/// effect of a level correction, not a treatment of that fault, and it should
/// not be read as one.
///
/// Applied to the radiated output only. The coupling loop reads the bridge
/// through `compliance_at` and `bridge_displacement`, which must stay physical.
const PLATE_DAMPING_MAKEUP: f64 = 1.4570;

const RADIATION_BAND: (f64, f64) = (1200.0, 1800.0);
const RADIATION_EXTRA: f64 = 50.0;

/// Where the plate starts radiating efficiently. Below it, adjacent parts of the
/// surface move in opposite directions and their sound cancels before it gets
/// anywhere; above it, the bending wavelength exceeds the wavelength in air and
/// the board couples properly. Ege puts the change at 1.2-1.5 kHz and quotes
/// Suzuki: "the transition range from less efficient to efficient sound
/// radiation is 1-1.6 kHz".
///
/// It stood at **850 Hz**, which is not the low end of that window — it is
/// underneath it, below both Ege's 1.2 kHz and Suzuki's 1.0. Put there by fitting
/// against a recording, and the fit is not evidence when it lands outside the
/// measurement it claims to sit in.
///
/// What it costs is the treble, and the arithmetic is short. The impulse a note
/// hands the board is `F·a · bridge_ratio · s_H`, and measured on 2026-08-12 that
/// is 1.5e-2 N·s in the bass against **2.3e-3 N·s at the top** — a factor of six,
/// −16 dB, because a treble contact lasts 0.56 ms where a bass one lasts 3.3.
/// That is precisely the peak deficit the compass shows (notes 87-108 at −15 to
/// −16 dB). What is supposed to give it back is this tilt: a bass impulse puts its
/// energy around 150 Hz and a treble one around 1.8 kHz, so a board that radiates
/// 6 dB per octave better as it climbs hands the treble back what its short
/// contact cost it. Setting the corner too low spends that compensation early and
/// leaves the top flat.
///
/// Moved to the middle of the published window, where both sources overlap.
const CRITICAL_HZ: f64 = 1200.0;

/// Radiate through the plate's own efficiency law rather than through a fitted
/// shelf.
///
/// Cremer and Maidanik: a plate's radiation efficiency rises as `(f/fc)²` in
/// power below its critical frequency and **saturates at 1 above it**. In
/// amplitude that is a first-order high-pass at `fc` — 6 dB per octave below,
/// flat above — and nothing else.
///
/// The shelf it replaces was two one-poles at `fc/4` and `2 fc`. Measured
/// (`radiation_shelf_alone`), that spread its 22 dB over 65 Hz to 10 kHz: only
/// **3.8 dB per octave below fc**, where the law wants 6, and **another 6 dB of
/// rise above fc**, where the law wants none. Both halves of the error push the
/// same way as the instrument's measured fault against a real Steinway: its bass
/// partials come out too weak and its treble partials far too strong.
const RADIATION_LAW: bool = true;

/// Maidanik's edge and corner radiation, added under the law for a finite plate.
/// **Zero**: the law reaches -26 dB on its own at `fc/20` = 60 Hz, which is below
/// the lowest note, so the floor has nothing to add over the compass and adding
/// it coherently only flattened the slope it was meant to support (measured: the
/// bass rise fell from 6 to 3.9 dB per octave).
const RAD_FLOOR: f64 = 0.0;

/// Pins the middle of the compass to the level the shelf left it at, so the law
/// re-tilts the instrument around C4 instead of changing how loud it is. Measured
/// on the two curves at 262 Hz, the law sits 5.3 dB lower there, and this puts it
/// back.
const RAD_PLATEAU: f64 = 0.784;

/// What the notes' direct sound (their bridge forces above `DIRECT_HZ`) is
/// worth against the board's global modes at the listening points, set so the
/// instrument's level across the compass is what it was with the two points
/// alone (the compass mean, measured).
const DIRECT_GAIN: f64 = 2.1e-3;
/// Where the direct sound takes over from the board's global modes at the
/// listening points. Below it the board moves as a whole and every note has
/// partials enough for no listening point's null to matter; above it a note
/// is heard by the force it puts on the bridge.
const DIRECT_HZ: f64 = 250.0;
/// The share of the board's modes above `DIRECT_HZ` mixed in with the direct
/// sound, the same on both sides: the colour of the wood, kept under the
/// direct sound so a listening point's null cannot take a note away.
const BOARD_HIGH_SHARE: f64 = 0.13;
/// The share of the difference between the two listening points, at full
/// width: what the two sides of the board do differently is the width of a
/// single note. It moves the image, not the level: a null in it narrows a
/// note where a null in the sum would silence it.
const BOARD_SIDE_SHARE: f64 = 0.25;

/// The bridge's own mass, as the corner above which the strings' force no
/// longer reaches the board in full: the coupling to every mode is tapered
/// as `1 / (1 + (f/fc)^2)`, and the direct sound with it. A bridge is a
/// beam of tens of grams glued to the plate, and above a few kilohertz its
/// inertia takes the force the string pulls with. Zero means no taper.
const BRIDGE_MASS_HZ: f64 = 0.0;

#[cfg(test)]
pub(crate) static BRIDGE_MASS_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(f64::to_bits(BRIDGE_MASS_HZ));

#[inline]
fn bridge_mass_hz() -> f64 {
    #[cfg(test)]
    {
        f64::from_bits(BRIDGE_MASS_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        BRIDGE_MASS_HZ
    }
}

/// The bridge's transmission at `f`, per direction.
#[inline]
fn bridge_taper(f: f64) -> f64 {
    let fc = bridge_mass_hz();
    if fc <= 0.0 {
        1.0
    } else {
        1.0 / (1.0 + (f / fc) * (f / fc))
    }
}

#[cfg(test)]
pub(crate) static SIDE_SHARE_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(f64::to_bits(BOARD_SIDE_SHARE));

#[inline]
fn board_side_share() -> f64 {
    #[cfg(test)]
    {
        f64::from_bits(SIDE_SHARE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        BOARD_SIDE_SHARE
    }
}

#[cfg(test)]
pub(crate) static DIRECT_GAIN_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(f64::to_bits(DIRECT_GAIN));
#[cfg(test)]
pub(crate) static HIGH_SHARE_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(f64::to_bits(BOARD_HIGH_SHARE));

#[inline]
fn direct_gain() -> f64 {
    #[cfg(test)]
    {
        f64::from_bits(DIRECT_GAIN_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        DIRECT_GAIN
    }
}

#[inline]
fn board_high_share() -> f64 {
    #[cfg(test)]
    {
        f64::from_bits(HIGH_SHARE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(test))]
    {
        BOARD_HIGH_SHARE
    }
}

/// The law's own low end, for the record: at A0 (27.5 Hz) it is -32.8 dB against
/// the plateau, against the shelf's -30.8. The bass fundamental therefore sits 2
/// dB lower and every partial above it rises 6 dB per octave instead of 3.8,
/// which is the correction the bench asks for in the bass.
const _RAD_A0_NOTE: () = ();

/// Deterministic, uniform in [-1, 1).
fn hashed(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    ((*seed >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
}

/// The compass this board has attachment points for: A0 to C8.
pub const LOWEST_NOTE: u8 = 21;
pub const HIGHEST_NOTE: u8 = 108;

/// A point on the board: how strongly each mode shows there.
#[derive(Clone, Debug, Default)]
pub struct BoardPoint {
    pub shape: Vec<f64>,
}

#[derive(Clone, Default)]
pub struct Soundboard {
    bank: ModalBank,
    /// Two staggered one-poles per channel for the radiation tilt.
    rad_l: [f64; 2],
    rad_r: [f64; 2],
    rad_k: [f64; 2],
    /// Where the bridge pulls, and the two places the instrument is heard.
    pub bridge: BoardPoint,
    pub left: BoardPoint,
    pub right: BoardPoint,
    freqs: Vec<f64>,
    /// The two listening points split at `DIRECT_HZ`: the global modes below
    /// it, which move the whole board and are what the board itself sounds
    /// like, and those above it, which colour the direct sound each note is
    /// heard with (`radiate_mix`).
    left_low: Vec<f64>,
    right_low: Vec<f64>,
    left_high: Vec<f64>,
    right_high: Vec<f64>,
    /// The direct sound's high-pass, one state per channel: the notes' bridge
    /// forces are heard above `DIRECT_HZ`, the board's global modes below.
    direct_hp: [[f64; 2]; 2],
    direct_hp_k: f64,
    /// And its low-pass, the bridge's mass (`BRIDGE_MASS_HZ`), two poles per
    /// channel; a coefficient of one means none.
    direct_lp: [[f64; 2]; 2],
    direct_lp_k: f64,
    /// The patch's width, which scales the side share.
    width: f64,
    /// Per-mode amplitude and phase along the bridge, so every note can be given
    /// its OWN place on it.
    bridge_amp: Vec<f64>,
    bridge_phase: Vec<f64>,
    /// The second, independent draw the right ear is mixed from. Kept so the
    /// listening points can be moved apart without rebuilding the plate.
    right_alt: Vec<f64>,
    /// Every note's coupling weights, computed once.
    ///
    /// These used to be built on the audio thread, one 29 kB vector and some
    /// seven thousand sines at a time, every time a string was woken — and the
    /// pedal wakes up to eighty-seven at once. Shared behind an `Arc` because
    /// they are read-only and identical for every voice on the same note.
    attachments: Vec<std::sync::Arc<Vec<f64>>>,
    /// The same weights in SINGLE precision, for the decoupled block kernel
    /// only. See `attachment_f32_shared`.
    attachments_f32: Vec<std::sync::Arc<Vec<f32>>>,
    /// Per-mode force buffer for the block-major decoupled step. State, not
    /// data: kept here so the audio thread never allocates.
    block_scratch: Vec<f64>,
    /// How loudly each mode reaches the ears — the weight the plate's own
    /// retirement is judged on. See `retire_inaudible`.
    heard_weight: Vec<f64>,
    /// Its single-precision twin, for the gather A/B (`GATHER_F32`).
    block_scratch32: Vec<f32>,
}

/// Every board at a given sample rate shares one set of attachment tables.
/// See the note in `Soundboard::new`.
type AttachTables = (
    Vec<std::sync::Arc<Vec<f64>>>,
    Vec<std::sync::Arc<Vec<f32>>>,
);

static ATTACH_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(u32, u64), AttachTables>>> =
    std::sync::OnceLock::new();

/// The tables for this rate, computed once per process. `board` is used only
/// to compute them on a miss; on a hit nothing of it is read.
fn attachment_tables(sr: f32, board: &Soundboard) -> AttachTables {
    let cache = ATTACH_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let key = (sr.to_bits(), bridge_mass_hz().to_bits());
    if let Ok(map) = cache.lock() {
        if let Some((a, b)) = map.get(&key) {
            return (a.clone(), b.clone());
        }
    }
    let att: Vec<std::sync::Arc<Vec<f64>>> = (LOWEST_NOTE..=HIGHEST_NOTE)
        .map(|n| std::sync::Arc::new(board.compute_attachment(n)))
        .collect();
    let att32: Vec<std::sync::Arc<Vec<f32>>> = att
        .iter()
        .map(|a| std::sync::Arc::new(a.iter().map(|w| *w as f32).collect::<Vec<f32>>()))
        .collect();
    if let Ok(mut map) = cache.lock() {
        map.insert(key, (att.clone(), att32.clone()));
    }
    (att, att32)
}

impl Soundboard {
    /// Build the board. `spread` (0..1) moves the two listening points apart on
    /// the plate, which widens the image because they then share fewer modes.
    pub fn new(sr: f32, spread: f64) -> Self {
        let mut freqs = Vec::new();
        let mut modes = Vec::new();
        // ── Mode frequencies ────────────────────────────────────────────────
        // Start on Conklin's measured low modes, then continue on the measured
        // spacing, which tightens from 22 Hz towards the 17 Hz that a density of
        // 0.06 modes/Hz means.
        // The lowest modes of a Steinway D's board, computed from its geometry
        // by Chabassier et al. and shown in their Fig. 7.
        let measured = [23.0, 39.0, 52.0, 67.0, 89.0, 112.0, 139.0, 180.0, 252.0];
        let mut f = FIRST_MODE_HZ;
        let mut seed = 0x5EED_B0A2_D000_1111u64;
        let mut i = 0usize;
        while f < TOP_HZ {
            let this = if i < measured.len() {
                measured[i]
            } else {
                // Spacing, plus the scatter a real plate has: modes are not on
                // a grid, and a grid would ring like a comb filter.
                // 22 Hz apart over the lowest modes, tightening towards the
                // 0.06 modes/Hz measured at the transition.
                // Density, and this is where the board was four times too poor.
                //
                // Ege's 0.06 modes/Hz is what a listener hears AT ONE POINT above
                // the transition, where the vibration localises between the ribs
                // and a point sits inside some waveguides and not others. It is
                // not how many modes the plate HAS. Chabassier's model of a
                // Steinway D needs **2400 modes to reach 10 kHz** — an average of
                // 0.24 modes/Hz — and its mode 392 sits at 2693 Hz, which is
                // 0.146 modes/Hz that low down.
                //
                // A plate's modal density is very nearly constant with frequency
                // (Courant), so it climbs from the 22 Hz spacing measured over the
                // lowest modes and then flattens. Too few modes and the board
                // answers with a handful of discrete resonances instead of a
                // continuum — which is heard as a ringing, electric colour rather
                // than as wood.
                let density = 1.0 / 22.0
                    + (BOARD_DENSITY - 1.0 / 22.0) * (f / (TRANSITION_HZ * 2.0)).min(1.0);
                let spacing = 1.0 / density;
                f + spacing * (1.0 + 0.45 * hashed(&mut seed))
            };
            if this >= TOP_HZ {
                break;
            }
            // What the wood alone would do, plus the rim on the lowest modes.
            let material = 0.5 * ETA_SPRUCE * std::f64::consts::TAU * this
                + RIM_LOSS / (1.0 + (this / RIM_KNEE_HZ).powi(4));
            // And the floor the measurement sets, whichever is greater. Above
            // about 1.8 kHz the material figure has caught up on its own and the
            // floor stops mattering, exactly as Ege describes the damping
            // "returning to the values of the material" up there.
            let floor = if this <= RADIATION_BAND.0 {
                board_rate_low()
            } else if this <= RADIATION_BAND.1 {
                BOARD_RATE_RADIATING
            } else {
                0.0
            };
            let mut sigma = material.max(floor);
            if this >= RADIATION_BAND.0 && this <= RADIATION_BAND.1 {
                // The band where the board starts radiating efficiently costs it
                // extra energy, which is why the measured damping jumps there.
                sigma += RADIATION_EXTRA;
            }
            modes.push(Mode { w: std::f64::consts::TAU * this, sigma });
            freqs.push(this);
            f = this;
            i += 1;
        }

        // ── The tenor modes the low list skips ─────────────────────────────
        // 204 and 229 Hz are Ege's measurements of a real board (JSV 2013 /
        // thèse 2009: (1,2)=204, (2,2)=229; he also has (3,1)=194); 161 fills
        // the 139->180 step at the same ~22 Hz spacing. Without them the list
        // jumps 180 -> 252, a 72 Hz gap with no mode in it, which is a 30 dB
        // antiresonance in the bridge's driving-point mobility: a string whose
        // fundamental lands in it (G3, 196 Hz) couples almost nothing at H1 and
        // comes out thin and bright — the "electric piano / harpsichord" note,
        // the discrete-resonance colour the density comment above warns about.
        //
        // APPENDED here rather than inserted into `measured`, and deliberately:
        // the modal SHAPES below are drawn from `freqs` in order off a running
        // seed, so inserting a mode mid-list re-draws every mode after it and
        // silently repaints the whole instrument. Appending leaves all of that
        // byte-identical and adds only these three; `frequencies()` is no longer
        // sorted, which nothing downstream needs (the bank sums over modes).
        for &tf in &[161.0_f64, 204.0, 229.0] {
            let material = 0.5 * ETA_SPRUCE * std::f64::consts::TAU * tf
                + RIM_LOSS / (1.0 + (tf / RIM_KNEE_HZ).powi(4));
            let floor = if tf <= RADIATION_BAND.0 {
                board_rate_low()
            } else if tf <= RADIATION_BAND.1 {
                BOARD_RATE_RADIATING
            } else {
                0.0
            };
            let mut sigma = material.max(floor);
            if tf >= RADIATION_BAND.0 && tf <= RADIATION_BAND.1 {
                sigma += RADIATION_EXTRA;
            }
            modes.push(Mode { w: std::f64::consts::TAU * tf, sigma });
            freqs.push(tf);
        }

        let mut bank = ModalBank::new();
        bank.set_modes(&modes, sr);
        let n = bank.len();
        freqs.truncate(n);

        // ── Mode shapes at the three points ────────────────────────────────
        let norm = (1.0 / BOARD_MASS).sqrt();
        let base = |seed: &mut u64| -> Vec<f64> {
            freqs
                .iter()
                .map(|&fr| {
                    let a = hashed(seed);
                    if fr <= TRANSITION_HZ {
                        // Below the transition the modes span the whole board,
                        // so every point sees every one of them.
                        norm * a
                    } else {
                        // Above it the vibration localises between the ribs: a
                        // point sits inside some waveguides and not others, so
                        // it sees a fraction of the modes strongly and the rest
                        // hardly at all. This is why the density measured at one
                        // point falls above the transition while the board as a
                        // whole still has just as many modes.
                        if hashed(seed).abs() > 0.45 {
                            norm * a * 1.6
                        } else {
                            norm * a * 0.12
                        }
                    }
                })
                .collect()
        };
        // ── The bridge is not a random point of the board ──────────────────
        //
        // Every listening point above is drawn as an arbitrary spot on the
        // plate, which is right for a microphone. The BRIDGE is not arbitrary: it
        // is glued along the line where the board is meant to be driven, and a
        // maker puts it where the plate answers, not where it happens to have a
        // node. Drawn like a random point it sits near a node as often as not,
        // and the whole instrument stiffens.
        //
        // How much that mattered, measured against published bridge mobility
        // (Wogram, Giordano: roughly 1e-3 to 1e-2 m/s/N through the middle of a
        // grand's range): this board came out at 4e-4 to 1.7e-3, and the coupling
        // ratio then took another factor of two out of it both ways. A bridge
        // that stiff keeps the string's energy instead of passing it on, so what
        // is heard is a STRING rather than an instrument — which is the whole of
        // "it sounds like a harp, a guitar, an electric piano".
        //
        // So the bridge's modal amplitudes are lifted towards the top of their
        // range rather than spread over it. The spread is kept — a bridge is a
        // long line and no single mode is uniform along it — but it no longer
        // includes sitting on a node.
        // ── The admittance has to be FLAT, and it was not ─────────────────
        //
        // What a listener hears out of a piano depends on how much energy the
        // bridge takes from each partial, and that is `|Y|`, the driving-point
        // admittance. For a plate it is asymptotically CONSTANT with frequency —
        // the classical infinite-plate result `Y∞ = 1/(8√(Dρh))` — and Wogram's
        // and Giordano's measurements on real bridges show no rise either.
        //
        // Measured here, it rose by a factor of **8.4 between 200 Hz and 4 kHz**
        // and left the published 1e-3..1e-2 band above a kilohertz. A bridge that
        // is more mobile the higher you go drains every partial in proportion to
        // its pitch: each note loses its top first and the treble loses
        // everything, which measured as 91 dB/s of initial decay at D#5 and a
        // four-hundredfold gap between the two slopes of A4. That is not a piano
        // decaying, that is a string being plucked — the reported "harp", "guitar"
        // and "pincé" for the same thing.
        //
        // Where the rise comes from is the modal density. Averaged over many
        // modes the admittance goes as `(π/2)·φ̄²·n(f)`, and this board's density
        // climbs from 0.089 modes/Hz under 1.1 kHz to 0.239 above it — measured,
        // and correct, since Ege's low-frequency spacing and Chabassier's 2400
        // modes to 10 kHz are both real. What was missing is that `φ̄²` must then
        // fall by the same factor for `Y` to stay put. The density is the
        // measurement; holding the product constant is the physics.
        let density_at = |f: f64| -> f64 {
            1.0 / 22.0 + (BOARD_DENSITY - 1.0 / 22.0) * (f / (TRANSITION_HZ * 2.0)).min(1.0)
        };
        let d_ref = density_at(TRANSITION_HZ);
        let mut s_bridge = 0x1111_2222_3333_4444u64;
        let bridge = BoardPoint {
            shape: base(&mut s_bridge)
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    // Recover the raw 0..1 share this mode gives at a random
                    // point, lift it onto BRIDGE_FLOOR..1, and put the
                    // normalisation back. Applying the floor to the already
                    // normalised value instead multiplies the whole board by
                    // seven, which overshot the measured mobility by a decade.
                    let sign = if v < 0.0 { -1.0 } else { 1.0 };
                    let share = (v.abs() / norm).min(1.0);
                    let lifted = BRIDGE_FLOOR + (1.0 - BRIDGE_FLOOR) * share;
                    // ── The density compensation goes LAST, and that is the
                    // whole of it ─────────────────────────────────────────────
                    //
                    // Written before the lift, it was scaled by `flat` and then
                    // put through `min(1.0)` and the floor remap — a ceiling and
                    // a compression, both of which eat exactly the correction
                    // they are applied to. Measured on 2026-08-12 by driving the
                    // attachment point with one newton-second and reading the
                    // velocity back, the driving-point admittance came out
                    //
                    //     3.6e-4 at 110 Hz  ...  2.3e-3 at 1760 Hz
                    //
                    // a factor of 6.4 UP where the infinite-plate result
                    // `Y∞ = 1/(8√(Dρh))` says flat, and with the whole bass and
                    // low middle sitting BELOW the 1e-3..1e-2 that Wogram and
                    // Giordano measured on real bridges. A board that stiff under
                    // 300 Hz keeps the bass strings' energy instead of turning it
                    // into sound, and the treble then has to be heard against a
                    // bass that rings on: measured, the instrument's T60 spanned
                    // only 3.4x from bottom to top where `α = T·Re{Y}/L` gives 14.
                    //
                    // Applied here, after the clamp, `φ̄²·n(f)` is constant by
                    // construction and the admittance is flat because the algebra
                    // makes it so, not because a number was chosen.
                    let flat = (d_ref / density_at(freqs[i])).sqrt();
                    sign * norm * lifted * flat * BRIDGE_COUPLING
                })
                .collect(),
        };

        // The two listening points are two places over one plate, and how much
        // they have in common depends on frequency. A mode whose wavelength
        // spans the whole board moves it in one piece, so both points see it
        // with the same sign; a short-wavelength mode has several lobes between
        // them and they see it independently.
        //
        // Getting this wrong is not a subtlety. Drawing both points'
        // weights independently at every frequency puts the lowest modes — the
        // loudest ones — into opposite phase, and a single note comes out with
        // its channels correlating at −0.73: hollow, and gone in mono.
        let mut s_left = 0xAAAA_5555_0000_9999u64;
        let mut s_alt = 0x9999_0000_5555_AAAAu64;
        // Two listening points sample the board's field; a sum over modes of
        // random sign has deep nulls, and a treble note's fundamental is one
        // frequency. So the points carry the board's global modes below
        // `DIRECT_HZ` in full and the rest as colour, while a note's level
        // comes from its own bridge force (`radiate_mix`).
        let l = base(&mut s_left);
        let alt = base(&mut s_alt);
        let sep = spread.clamp(0.0, 1.0);
        let right: Vec<f64> = l
            .iter()
            .zip(alt.iter())
            .zip(freqs.iter())
            .map(|((a, b), &fr)| {
                // Fully shared at the plate's first modes, fully independent by
                // the transition, and how fast it gets there is how far apart
                // the two points are.
                let t = (fr / (TRANSITION_HZ * (1.05 - sep))).min(1.0);
                let theta = 0.5 * std::f64::consts::PI * t * sep;
                a * theta.cos() + b * theta.sin()
            })
            .collect();
        let band = |v: &Vec<f64>, high: bool| -> Vec<f64> {
            v.iter()
                .zip(freqs.iter())
                .map(|(w, &fr)| if (fr >= DIRECT_HZ) == high { *w } else { 0.0 })
                .collect()
        };
        let left_low = band(&l, false);
        let right_low = band(&right, false);
        let left_high = band(&l, true);
        let right_high = band(&right, true);
        let left = BoardPoint { shape: l };
        let right = BoardPoint { shape: right };

        // ── Where each note meets the bridge ───────────────────────────────
        //
        // Until now every one of the 88 notes drove the SAME point of the plate
        // and was pushed back by the same single number. There was no geometry
        // between them at all: a bass note and a treble note pumped one point in
        // phase and each felt the sum of all the others coherently, so the more
        // strings were undamped the more they were forced into one another. That
        // is why the dense PEDALLED repertoire came apart while sparse pedalled
        // playing and dense DRY playing did not.
        //
        // A grand has two bridges, physically separate — a short one for the
        // wound bass and the long one for everything else — and each note is
        // pinned at its own place along them, metres apart at the extremes. Two
        // notes are coupled by the board's modes according to how far apart they
        // are pinned, not by sharing a point.
        //
        // The bridge is a LINE across the plate, so the weight a mode gives along
        // it is a standing wave: `A·sin(ψ + π·ν·p)`, with ν half-wavelengths
        // across the span. ν grows with frequency, which is the same reasoning
        // the two listening points already use — a mode long enough to span the
        // board moves it in one piece, so every note sees it alike; a short one
        // has several lobes along the bridge and notes at different ends see it
        // independently, or in opposition.
        let bridge_amp: Vec<f64> = bridge.shape.iter().map(|v| v.abs()).collect();
        let mut s_phase = 0x5EED_1234_ABCD_9876u64;
        let bridge_phase: Vec<f64> =
            (0..bridge.shape.len()).map(|_| hashed(&mut s_phase) * std::f64::consts::PI).collect();

        // The image is not read off these points: each note is placed by
        // where it is pinned on the bridge (`Voice::set_listener`).

        // Radiation efficiency. What a listener hears is not the board's
        // velocity: it is the pressure that velocity radiates, and a plate is a
        // poor radiator below its critical frequency. In power the efficiency
        // climbs as (f/fc)², so in amplitude it is a 6 dB per octave tilt up to
        // fc and flat above — a first-order high shelf, which is what the
        // literature means by "the radiation effects of the soundboard" being a
        // filtering operation on the string's output.
        //
        // Measured against a recording of a real grand, leaving it out cost 15
        // to 21 dB across 500 Hz to 4 kHz — the whole register a piano lives in,
        // and its absence is most of why this sounded plucked.
        // Two corners rather than one. A single pole is a 6 dB per octave tilt,
        // which is the INFINITE plate's answer: below its critical frequency
        // adjacent half-wavelengths cancel completely. A real board is finite,
        // and Maidanik's result is that its edges and corners go on radiating
        // where its interior no longer does — so it falls away far more gently.
        // Measured against a recording of a real grand, the single pole left the
        // bass 13 dB light and the top 17 dB hot: the right shape, too steep.
        let k = |hz: f64| 1.0 - (-std::f64::consts::TAU * hz / sr as f64).exp();
        // `rad_k[0]` is the radiation high-pass coefficient when RADIATION_LAW is
        // on; the two shelf corners below are what it replaces.
        let wc = std::f64::consts::TAU * CRITICAL_HZ / sr as f64;
        let rad_k = if RADIATION_LAW {
            [1.0 / (1.0 + wc), 0.0]
        } else {
            [k(CRITICAL_HZ * 0.25), k(CRITICAL_HZ * 2.0)]
        };
        let mut board = Soundboard {
            bank,
            rad_l: [0.0; 2],
            rad_r: [0.0; 2],
            rad_k,
            bridge,
            left,
            right,
            freqs,
            left_low,
            right_low,
            left_high,
            right_high,
            direct_hp: [[0.0; 2]; 2],
            direct_hp_k: (-std::f64::consts::TAU * DIRECT_HZ / sr as f64).exp(),
            direct_lp: [[0.0; 2]; 2],
            direct_lp_k: if bridge_mass_hz() > 0.0 {
                1.0 - (-std::f64::consts::TAU * bridge_mass_hz() / sr as f64).exp()
            } else {
                1.0
            },
            width: spread.clamp(0.0, 1.0),
            bridge_amp,
            bridge_phase,
            right_alt: alt,
            attachments: Vec::new(),
            attachments_f32: Vec::new(),
            block_scratch: Vec::new(),
            block_scratch32: Vec::new(),
            heard_weight: Vec::new(),
        };
        // ── The attachment tables are SHARED between instances ────────────
        //
        // They are read-only, they are 3.6 MB per board (88 notes × 3618
        // modes, in double and in single), and they depend on nothing but the
        // sample rate: two boards at 48 kHz hold the same numbers, bit for
        // bit, because the mode set and the bridge geometry that produce them
        // are deterministic.
        //
        // MEASURED 2026-08-25, and this is why it matters. Six instances of
        // this instrument each carrying private copies put 8 MB of identical
        // read-only data into a 12 MB L3, and the scaling curve shows exactly
        // what that does: the cost of ONE instance's block climbs from 0.93 ms
        // alone to 0.97 at two, 1.5 at three, 1.9 at four and 2.4 at six —
        // while cores sit idle. Sharing the tables leaves ONE copy for the
        // whole process.
        let (att, att32) = attachment_tables(sr, &board);
        board.attachments = att;
        board.attachments_f32 = att32;
        board.heard_weight = board
            .left
            .shape
            .iter()
            .zip(board.right.shape.iter())
            .map(|(l, r)| l.abs().max(r.abs()))
            .collect();
        board
    }

    /// Move the two listening points apart, without rebuilding the plate.
    ///
    /// Only the right ear depends on the spread, and only through a blend of
    /// two draws this already holds. Rebuilding the whole board for it meant
    /// three thousand six hundred modes re-derived and every note's attachment
    /// recomputed ON THE AUDIO THREAD for a knob turn, and it silenced the
    /// plate mid-note into the bargain.
    pub fn set_spread(&mut self, spread: f64) {
        let sep = spread.clamp(0.0, 1.0);
        self.width = sep;
        for (i, &fr) in self.freqs.iter().enumerate() {
            let t = (fr / (TRANSITION_HZ * (1.05 - sep))).min(1.0);
            let theta = 0.5 * std::f64::consts::PI * t * sep;
            self.right.shape[i] = self.left.shape[i] * theta.cos() + self.right_alt[i] * theta.sin();
            if fr < DIRECT_HZ {
                self.right_low[i] = self.right.shape[i];
            } else {
                self.right_high[i] = self.right.shape[i];
            }
        }
    }

    pub fn len(&self) -> usize {
        self.bank.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bank.is_empty()
    }

    pub fn frequencies(&self) -> &[f64] {
        &self.freqs
    }

    pub fn clear(&mut self) {
        self.bank.clear();
        self.rad_l = [0.0; 2];
        self.rad_r = [0.0; 2];
    }

    /// The bridge's displacement right now. The strings need it: a bridge that
    /// moves is a boundary that gives, and that is how their energy leaves.
    #[inline]
    pub fn bridge_displacement(&self) -> f64 {
        self.bank.read(&self.bridge.shape)
    }

    #[inline]
    pub fn bridge_velocity(&self) -> f64 {
        self.bank.read_velocity(&self.bridge.shape)
    }

    /// How far the bridge moves per newton in one sample — its compliance.
    pub fn bridge_compliance(&self) -> f64 {
        self.bank.compliance(&self.bridge.shape)
    }

    /// Apply the force the strings pull the bridge with.
    #[inline]
    pub fn drive_bridge(&mut self, force: f64) {
        self.bank.add_force(&self.bridge.shape, force);
    }

    /// Read the plate where a note is pinned.
    #[inline]
    pub fn read_at(&self, point: &[f64]) -> f64 {
        self.bank.read(point)
    }

    /// How fast the plate is moving where a note is pinned.
    #[inline]
    pub fn read_velocity_at(&self, point: &[f64]) -> f64 {
        self.bank.read_velocity(point)
    }

    /// Where a pinned point will be after the next step with no new force on it.
    #[inline]
    pub fn free_response(&self, point: &[f64]) -> f64 {
        self.bank.free_response(point)
    }

    /// And how much further it moves per newton applied during that step.
    #[inline]
    /// What every OTHER string already driving the board this sample will move
    /// this point by. See `ModalBank::pending_response`.
    pub fn pending_response_at(&self, point: &[f64]) -> f64 {
        self.bank.pending_response(point)
    }

    pub fn compliance_at(&self, point: &[f64]) -> f64 {
        self.bank.compliance(point)
    }

    /// A note's attachment weights in SINGLE precision — for the DECOUPLED
    /// block kernel, and nowhere else.
    ///
    /// These arrays are the engine's dominant memory stream by a wide margin:
    /// 3618 doubles per sounding voice, 876 kB per block at 31 voices against
    /// 227 kB for everything else the plate touches, on a kernel this file
    /// has measured to be bandwidth-bound. Halving them halves the traffic.
    ///
    /// Single precision was MEASURED and rejected for the per-sample path
    /// (25% SLOWER: each weight was converted on every sample, and the
    /// kernel is latency-bound there, so the widening cost more than the
    /// halved bandwidth saved). The block kernel inverts that arithmetic:
    /// each weight is read ONCE PER BLOCK and used for all 128 samples, so
    /// the conversion is amortised 128-fold while the traffic saving stands.
    ///
    /// What it does NOT touch: the modal state and coefficients stay f64
    /// (the poles sit too close to the unit circle for f32 to carry a tail),
    /// and the exact model — the bounce, the audits — never reads these at
    /// all. This is a live-only path whose drain laws are already static.
    ///
    /// MEASURED and stopped here, 2026-08-25. Halving the stream AGAIN — the
    /// same weights as 16-bit fixed point with one scale per note, which is
    /// 90 dB of headroom on a term summed over thousands of modes — was built
    /// and timed: 71, 77 and 77 percent of budget at 31 voices against 64 for
    /// f32, over three runs. Two bytes cost more than four because the gather
    /// stops being bandwidth-bound once the shapes are f32: the sign-extend,
    /// the int-to-float and the scale multiply then sit on the critical path
    /// of a loop that was reading its way through memory before. Neither is
    /// f16 worth trying for the same reason, and int32 is four bytes — it
    /// could only match f32 while adding a conversion. This is the floor.
    pub fn attachment_f32_shared(&self, note: u8) -> std::sync::Arc<Vec<f32>> {
        let i = note.clamp(LOWEST_NOTE, HIGHEST_NOTE) - LOWEST_NOTE;
        std::sync::Arc::clone(&self.attachments_f32[i as usize])
    }

    /// Re{Y} at a note's point at one frequency, in closed form — what a string
    /// mode ringing at `w` loses to this board here. See
    /// `ModalBank::re_admittance`.
    pub fn re_admittance_at(&self, point: &[f64], w: f64) -> f64 {
        self.bank.re_admittance(point, w)
    }

    /// Free position and compliance at a note's point, in one sweep of the
    /// board's mode arrays. See `ModalBank::read_and_compliance` — the coupling
    /// is bandwidth-bound, so the engine reads the point once, not twice.
    #[inline]
    pub fn read_and_compliance_at(&self, point: &[f64]) -> (f64, f64) {
        self.bank.read_and_compliance(point)
    }

    /// The whole plate for a whole BLOCK, mode-major — the decoupled board's
    /// step. `forces` is voice-major (`forces[v*frames + t]`); `out` receives
    /// the two ears' velocities per sample, to be radiated by the caller
    /// exactly as the pooled path's are. See `ModalBank::block_drive_tick_read2`.
    pub fn advance_block(
        &mut self,
        points: &[std::sync::Arc<Vec<f32>>],
        forces: &[f64],
        forces32: &[f32],
        frames: usize,
        out: &mut [(f64, f64)],
    ) {
        self.bank.block_drive_tick_read2(
            points,
            forces,
            frames,
            &self.left.shape,
            &self.right.shape,
            out,
            &mut self.block_scratch,
            &mut self.block_scratch32,
            forces32,
        );
    }

    /// Stop computing the plate modes that nothing can hear any more —
    /// BUILT, MEASURED, AND NOT WIRED IN. Kept because the number is worth
    /// having and the next person will have the same idea.
    ///
    /// Under a pedalled storm it retires NOTHING: 3618 of 3618 modes stay
    /// live from the first block to the last. The plate is driven broadband
    /// by every hammer and then held there by eight ringing strings, so no
    /// mode ever falls 140 dB under the plate's own peak while the music is
    /// playing — and the passages where it would (a decay into silence) are
    /// the ones that were never short of budget.
    ///
    /// The static form fares no better: `mode_census` shows the radiated
    /// energy spread across 3021 of the 3618 modes at 99.9%, and 2722-2772
    /// per note. Pruning the plate at preset load would buy 17 to 25 percent
    /// and would cost fidelity. Neither is the lever.
    ///
    /// The strings have done this since the beginning (`retire_quiet`, held to
    /// account by `retiring_dead_partials_is_inaudible`); the PLATE never has,
    /// and it is the instrument's fixed floor: 3618 modes advanced every
    /// sample whatever the music, measured at 23% of a block's budget with no
    /// string sounding at all and near 40% at the clocks a loaded machine
    /// actually runs.
    ///
    /// The weight is what the EARS pick up, not the mode's own amplitude: a
    /// mode that moves but does not reach either listening point is not worth
    /// a multiply. Retirement runs from the top of the frequency order down,
    /// stops at the first mode still above the floor, and the whole bank comes
    /// back the instant the plate is driven past its old peak — a hammer
    /// anywhere revives everything, which is why this cannot swallow an
    /// attack.
    pub fn retire_inaudible(&mut self, rel: f64) {
        self.bank.retire_quiet(&self.heard_weight, rel);
    }

    /// How loudly each mode reaches the ears. Diagnostic; see `mode_census`.
    pub fn heard_weights(&self) -> &[f64] {
        &self.heard_weight
    }

    /// How many plate modes are still being computed, of how many exist.
    pub fn active_modes(&self) -> (usize, usize) {
        (self.bank.live(), self.bank.len())
    }

    /// One participant's slice of the block-major decoupled step: modes
    /// `lo..hi` for the whole block, ear velocities accumulated UNSCALED into
    /// `out` (the caller sums participants, then multiplies by the sample
    /// rate). See `ModalBank::range_block_drive_tick_read2`.
    ///
    /// # Safety
    /// Concurrent callers' ranges must tile `0..live()` without overlapping,
    /// each with its own `out` and `scratch`.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn range_advance_block(
        &self,
        lo: usize,
        hi: usize,
        points: &[std::sync::Arc<Vec<f32>>],
        forces: &[f64],
        forces32: &[f32],
        frames: usize,
        out: &mut [(f64, f64)],
        scratch: &mut [f64],
        scratch32: &mut [f32],
    ) {
        unsafe {
            self.bank.range_block_drive_tick_read2(
                lo,
                hi,
                points,
                forces,
                frames,
                &self.left.shape,
                &self.right.shape,
                out,
                scratch,
                scratch32,
                forces32,
            )
        }
    }

    /// One worker's slice of the plate: take the pushes, step, read the ears.
    /// See `ModalBank::range_drive_tick_read2`.
    ///
    /// # Safety
    ///
    /// The ranges given to concurrent callers must tile `0..live()` without
    /// overlapping.
    /// `range_advance` reading the listening points split at `DIRECT_HZ`, as
    /// `advance_split` does.
    pub unsafe fn range_advance_split(
        &self,
        lo: usize,
        hi: usize,
        points: &[std::sync::Arc<Vec<f64>>],
        forces: &[f64],
    ) -> [f64; 4] {
        unsafe {
            self.bank.range_drive_tick_read4(
                lo,
                hi,
                points,
                forces,
                &self.left_low,
                &self.right_low,
                &self.left_high,
                &self.right_high,
            )
        }
    }

    /// Advances the plate's modes in `lo..hi` and returns what the two ears
    /// hear from them.
    ///
    /// # Safety
    ///
    /// The contract of `ModalBank::range_drive_tick_read2`: the ranges of
    /// the calls running at the same time are disjoint and within the live
    /// modes.
    pub unsafe fn range_advance(
        &self,
        lo: usize,
        hi: usize,
        points: &[std::sync::Arc<Vec<f64>>],
        forces: &[f64],
    ) -> (f64, f64) {
        unsafe {
            self.bank
                .range_drive_tick_read2(lo, hi, points, forces, &self.left.shape, &self.right.shape)
        }
    }

    /// How many of the plate's modes are live — the range the parallel phase
    /// tiles.
    pub fn live(&self) -> usize {
        self.bank.live()
    }

    /// Turn the two ears' velocities into what the microphones hear. Split out
    /// so the parallel path can combine its partial velocities first.
    pub fn radiate_pair(&mut self, vl: f64, vr: f64) -> (f64, f64) {
        self.radiate(vl, vr)
    }

    /// Push on the plate where a note is pinned.
    #[inline]
    pub fn drive_at(&mut self, point: &[f64], force: f64) {
        self.bank.add_force(point, force);
    }

    /// Advance the plate without reading it: the output is read at each
    /// note's own point (`Voice::heard`) and radiated with `radiate_pair`.
    #[inline]
    pub fn step(&mut self) {
        self.bank.tick();
    }

    /// Advance the plate and read what it radiates at the two fixed listening
    /// points, the forces having already been applied at each note's own point.
    #[inline]
    pub fn advance(&mut self) -> (f64, f64) {
        let (vl, vr) = self
            .bank
            .tick_read2_velocity(&self.left.shape, &self.right.shape);
        self.radiate(vl, vr)
    }

    /// Advance the plate and read it at the two listening points, the modes
    /// below and above `DIRECT_HZ` apart: [left low, right low, left high,
    /// right high], unradiated. Pairs with `radiate_mix`.
    #[inline]
    pub fn advance_split(&mut self) -> [f64; 4] {
        self.bank
            .tick_read4_velocity(&self.left_low, &self.right_low, &self.left_high, &self.right_high)
    }

    /// What the pair hears: the board's global modes at the listening points,
    /// the notes' direct sound (their bridge forces, placed per note by
    /// `Voice::heard`) above `DIRECT_HZ`, and a share of the board's other
    /// modes for the colour and the ringing of the wood. The direct sound is
    /// what keeps every note at its own level: the bridge's admittance is
    /// flat, so the board radiates in proportion to the force on it, and a
    /// force needs no luck with a listening point's modal signs.
    #[inline]
    pub fn radiate_mix(&mut self, ears: [f64; 4], direct: (f64, f64)) -> (f64, f64) {
        let k = self.direct_hp_k;
        let mut d = [direct.0, direct.1];
        for (ch, x) in d.iter_mut().enumerate() {
            // Two poles at `DIRECT_HZ`, so the direct sound hands over to the
            // board's global modes at 12 dB per octave.
            let st = &mut self.direct_hp[ch];
            let in0 = *x;
            let hp1 = k * (st[0] + in0);
            st[0] = hp1 - in0;
            let hp2 = k * (st[1] + hp1);
            st[1] = hp2 - hp1;
            let lp = &mut self.direct_lp[ch];
            let kl = self.direct_lp_k;
            lp[0] += (hp2 - lp[0]) * kl;
            lp[1] += (lp[0] - lp[1]) * kl;
            *x = lp[1];
        }
        let (share, gain) = (board_high_share(), direct_gain());
        let side = board_side_share() * self.width;
        let mid = 0.5 * (ears[2] + ears[3]);
        let dif = 0.5 * (ears[2] - ears[3]);
        self.radiate(
            ears[0] + share * mid + side * dif + gain * d[0],
            ears[1] + share * mid - side * dif + gain * d[1],
        )
    }

    /// Take the strings' force, advance the plate, read what it radiates on
    /// each side, and hand back where the bridge now sits — all in one walk over
    /// the modes. A board radiates in proportion to how fast its surface moves.
    ///
    /// Fused for the same reason the strings were: the plate's bank is large,
    /// and touching it five times a sample cost far more than the arithmetic in
    /// it. Identical to `drive_bridge` + `process` + `bridge_displacement`.
    #[inline]
    pub fn drive_and_process(&mut self, force: f64) -> (f64, f64, f64) {
        let (vl, vr, disp) = self.bank.drive_tick_read3(
            &self.bridge.shape,
            force,
            &self.left.shape,
            &self.right.shape,
            &self.bridge.shape,
        );
        let (l, r) = self.radiate(vl, vr);
        (l, r, disp)
    }

    /// The radiation tilt, and the make-up, shared by EVERY entry point.
    ///
    /// There are four ways out of this plate — `process`, `advance`,
    /// `radiate_pair` and `drive_and_process` — and they all funnel through
    /// here, which is why anything that scales the radiated sound belongs here
    /// and nowhere else. `PLATE_DAMPING_MAKEUP` first went into `process` alone;
    /// the engine's hot loop uses `radiate_pair`, so the instrument shipped with
    /// the damping and without its compensation and the Marée render came out
    /// 3.4 dB down — against a make-up of 3.27. It also quietly broke
    /// `drive_and_process`'s promise to be identical to `drive_bridge` +
    /// `process`, which is the sort of thing a fused fast path exists to keep.
    #[inline]
    fn radiate(&mut self, vl: f64, vr: f64) -> (f64, f64) {
        if RADIATION_LAW {
            let a = self.rad_k[0];
            let mut out = [0.0f64; 2];
            for (ch, x) in [vl, vr].into_iter().enumerate() {
                let st = if ch == 0 { &mut self.rad_l } else { &mut self.rad_r };
                st[0] = a * (st[0] + x - st[1]);
                st[1] = x;
                out[ch] = (st[0] + RAD_FLOOR * x) * RAD_PLATEAU * PLATE_DAMPING_MAKEUP;
            }
            return (out[0], out[1]);
        }
        // Each stage takes out part of the low end, and the two corners spread
        // the slope over a couple of octaves instead of stacking it into one.
        //
        // ── The depth was compensating for a fault that is now fixed ───────
        //
        // At 0.55 the two stages give `(1 − 0.55)² = −13.9 dB` between the bottom
        // of the compass and the top. A plate's radiation efficiency goes as
        // `(f/fc)²` in power, which is 6 dB per octave in amplitude, and from 55
        // Hz to the 1.2 kHz corner is 4.4 octaves — **26 dB**. The tilt was set to
        // half the law.
        //
        // It was set there by fitting against a recording, and the fit is void:
        // it was made while the bridge's driving-point mobility rose by a factor
        // of 6.4 from bass to treble instead of being flat (see `bridge` above,
        // corrected 2026-08-12). A mobility that climbs with pitch makes the
        // treble artificially bright, and halving the radiation tilt is exactly
        // what would hide that. The mobility is now flat and inside Wogram's and
        // Giordano's band, so the compensation has nothing left to compensate and
        // the published slope stands on its own.
        //
        // 0.776 restores it: `(1 − 0.776)² = −26 dB` across the two corners, the
        // infinite-plate slope, while the two knees keep Maidanik's gentler shape
        // through the transition where a finite plate's edges go on radiating.
        #[cfg(test)]
        let depth = { let o = DEPTH_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
            if o > 0 { o as f64 / 1000.0 } else { 0.776 } };
        #[cfg(not(test))]
        let depth = 0.776;
        let _ = 0.776f64;
        let mut l = vl;
        let mut r = vr;
        for i in 0..2 {
            self.rad_l[i] += (l - self.rad_l[i]) * self.rad_k[i];
            self.rad_r[i] += (r - self.rad_r[i]) * self.rad_k[i];
            l -= depth * self.rad_l[i];
            r -= depth * self.rad_r[i];
        }
        (l * PLATE_DAMPING_MAKEUP, r * PLATE_DAMPING_MAKEUP)
    }

    /// Advance the plate, then read what it radiates on each side.
    #[inline]
    pub fn process(&mut self) -> (f64, f64) {
        self.bank.tick();
        let vl = self.bank.read_velocity(&self.left.shape);
        let vr = self.bank.read_velocity(&self.right.shape);
        self.radiate(vl, vr)
    }

    /// How far the bridge moves per newton held on it, over every mode: the
    /// static compliance. An independent check on the admittance.
    pub fn bridge_compliance_static(&self) -> f64 {
        self.bank
            .modes()
            .iter()
            .zip(self.bridge.shape.iter())
            .map(|(m, phi)| phi * phi / (m.w * m.w))
            .sum()
    }

    /// Where a note is pinned along the bridge, from 0 at the bass end to 1 at
    /// the top.
    ///
    /// A grand carries two bridges. The wound bass strings cross a short one set
    /// closer to the rim, and everything from about E2 up crosses the long one.
    /// They are separate pieces of wood, so there is a real gap between the two
    /// runs — which is why the bass of a piano couples to the rest of the
    /// instrument so much more loosely than its own length would suggest.
    pub fn bridge_position(note: u8) -> f64 {
        let n = note.clamp(21, 108) as f64;
        if n <= 40.0 {
            // Bass bridge: A0 to E2, its own short run.
            (n - 21.0) / 19.0 * 0.35
        } else {
            // The long bridge, starting past the gap.
            0.45 + (n - 41.0) / 67.0 * 0.55
        }
    }

    /// The modal weights where THIS note meets the bridge.
    ///
    /// Two notes pinned close together see almost the same board and are
    /// strongly coupled; two at opposite ends share only the modes long enough
    /// to span the whole plate. That relationship IS the physical constraint
    /// between notes, and until each note had its own point there was none: all
    /// 88 drove one place in phase.
    /// A note's coupling weights along the bridge.
    ///
    /// MEASURED 2026-08-23, because this file used to claim the opposite: these
    /// arrays are NOT worth holding in f32. Single precision halves the traffic
    /// of the largest stream in the engine, and it still came out 25 percent
    /// SLOWER — 0.74x at 16 voices, 0.76x at 88, where the shapes total 2.5 MB
    /// and cannot be sitting in cache. The kernel that reads them is bound by
    /// instruction latency, not by memory, so the widening conversion costs
    /// more than the halved bandwidth saves. Doubles it stays.
    pub fn attachment(&self, note: u8) -> Vec<f64> {
        (*self.attachment_shared(note)).clone()
    }

    /// The same weights, shared rather than copied. What the engine uses: a
    /// voice only ever reads them.
    pub fn attachment_shared(&self, note: u8) -> std::sync::Arc<Vec<f64>> {
        let i = note.clamp(LOWEST_NOTE, HIGHEST_NOTE) - LOWEST_NOTE;
        std::sync::Arc::clone(&self.attachments[i as usize])
    }



    fn compute_attachment(&self, note: u8) -> Vec<f64> {
        let p = Self::bridge_position(note);
        // ── How far this note is pinned from the rim ──────────────────────
        //
        // The plate is glued to the rim all the way round, so every mode is
        // still there, and it comes up to its full swing only a fraction of its
        // own wavelength away. Near the rim a mode with one or two lobes across
        // the whole board is therefore nearly motionless while a short-wave mode
        // is already at full amplitude — which is why the treble end of the
        // bridge, a hand's breadth from the rim, radiates its own high partials
        // and does NOT boom the plate's lowest modes.
        //
        // Without this the position term `nu * p` vanishes for the low modes
        // (they have well under one lobe across the bridge) and all that is left
        // is each mode's random phase, so a note at the top of the compass drove
        // the plate below 800 Hz exactly as hard as a note at the bottom:
        // measured 4.5 against 5.1 units of coupling energy, a 12 percent spread
        // across 88 notes where the geometry should give an order of magnitude.
        // A fortissimo D#7 came out with a rumble three octaves below itself
        // (strays at 320 to 620 Hz, 13.5 dB under the note's own partials
        // against 29 dB at middle C), which is heard as a treble that does not
        // sound like a piano.
        //
        // The TREBLE end only. The long bridge runs out towards the tail and its
        // last notes sit a hand's breadth from the rim; the bass bridge does the
        // opposite, crossing the widest part of the plate where the low modes
        // swing most, which is exactly why a maker puts it there. So the distance
        // that matters is measured from the treble end, and a bass note keeps the
        // low-mode coupling that gives it its foundation.
        // `RIM_STANDOFF` is how far the last note still sits from the rim.
        const RIM_STANDOFF: f64 = 0.07;
        let d = (1.0 - p) + RIM_STANDOFF;
        // Only the last stretch of the long bridge runs alongside the rim. Below
        // that the bridge is out over the plate and the rim is nowhere near, so
        // the factor ramps in and the bass keeps the low-mode coupling its
        // driving-point mobility is measured to have (Wogram and Giordano,
        // 1e-3 to 1e-2 m/s/N — applying this everywhere took the bottom of the
        // compass to 7e-5 and out of the band).
        let near_rim = ((p - 0.60) / 0.40).clamp(0.0, 1.0);
        self.bridge_amp
            .iter()
            .zip(self.bridge_phase.iter())
            .zip(self.freqs.iter())
            .map(|((amp, phase), &fr)| {
                let nu = BRIDGE_LOBES * fr / TRANSITION_HZ;
                // Rises with distance-in-wavelengths from the rim, then saturates
                // once the point is a quarter wave in and the mode is at full
                // swing there.
                let edge_raw = (std::f64::consts::PI * nu * d)
                    .min(std::f64::consts::FRAC_PI_2)
                    .sin();
                let edge = 1.0 - near_rim * (1.0 - edge_raw);
                amp * edge * bridge_taper(fr) * (phase + std::f64::consts::PI * nu * p).sin()
            })
            .collect()
    }

    /// The plate's modes, for auditing against published measurements.
    pub fn modes_for_audit(&self) -> &[crate::modal_bank::Mode] {
        self.bank.modes()
    }

    pub fn energy(&self) -> f64 {
        self.bank.energy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Notes must be coupled to each other ACCORDING TO WHERE THEY ARE PINNED.
    ///
    /// Before this, all 88 notes drove one point of the plate and were pushed
    /// back by one number, so every pair was coupled identically and in phase —
    /// no geometry at all, which is why dense pedalled playing forced the strings
    /// into one another and came apart. Two neighbours on the bridge should share
    /// almost everything; the bottom of the bass bridge and the top of the long
    /// one are over a metre apart and should share only the modes long enough to
    /// span the whole board.
    #[test]
    fn notes_couple_by_how_far_apart_they_are_pinned() {
        let b = Soundboard::new(SR, 0.7);
        let modes = b.modes_for_audit().to_vec();
        let corr = |x: &[f64], y: &[f64], hi: f64| -> f64 {
            let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
            for i in 0..x.len() {
                if modes[i].w / std::f64::consts::TAU > hi {
                    continue;
                }
                xy += x[i] * y[i];
                xx += x[i] * x[i];
                yy += y[i] * y[i];
            }
            xy / (xx.sqrt() * yy.sqrt()).max(1e-30)
        };
        let a2 = b.attachment(45);
        let neighbour = b.attachment(46);
        let far = b.attachment(104);
        let bottom = b.attachment(21);

        let near_all = corr(&a2, &neighbour, 16_000.0);
        let far_all = corr(&a2, &far, 16_000.0);
        let far_low = corr(&bottom, &far, 200.0);
        eprintln!(
            "attaches : voisins {near_all:.2}, extremes {far_all:.2}, extremes sous 200 Hz {far_low:.2}"
        );
        let near_low = corr(&a2, &neighbour, 500.0);
        eprintln!("  voisins sous 500 Hz {near_low:.2}");
        // Two pins ten millimetres apart are the same point of the board for any
        // mode whose wavelength is measured in tens of centimetres.
        assert!(
            near_low > 0.95,
            "neighbouring notes differ ({near_low:.2}) even on the long modes"
        );
        // The longest modes move the whole plate together, ends included. This is
        // what six lobes got wrong: it put the bottom of the keyboard in
        // opposition to the top at 60 Hz.
        assert!(
            far_low > 0.5,
            "even the longest modes do not move the whole board together ({far_low:.2})"
        );
        // And across the whole spectrum, neighbours must still be far more
        // strongly tied to each other than the extremes are. That ORDERING is
        // the physical constraint between notes; before it existed every pair was
        // coupled identically, which is what made dense pedalled playing collapse.
        assert!(
            near_all > far_all + 0.3,
            "neighbours couple {near_all:.2} and the extremes {far_all:.2}: too little geometry"
        );
    }

    /// No two modes of the plate may sit almost on top of each other.
    ///
    /// A pair a couple of hertz apart beats at that rate, and because the board
    /// is shared by every note the SAME slow beat then colours everything: the
    /// fundamental of an A3 and of an A4 both wobbling at 2 Hz, which is heard
    /// as phasing rather than as an instrument. Measured against a real piano,
    /// the low partials were wobbling two and a half times too much.
    #[test]
    fn no_two_modes_sit_on_top_of_each_other() {
        let b = Soundboard::new(SR, 1.0);
        // The tenor fill (161/204/229) is appended, not inserted, so the mode
        // list is not sorted; spacing is a property of the SET, so sort first.
        let mut f = b.frequencies().to_vec();
        f.sort_by(|a, c| a.partial_cmp(c).unwrap());
        let mut worst = (f64::MAX, 0.0f64);
        for w in f.windows(2) {
            let gap = w[1] - w[0];
            if gap < worst.0 {
                worst = (gap, w[0]);
            }
        }
        // Judged against the spacing at that frequency, not against a fixed
        // number of hertz. A real board carries a quarter of a mode per hertz, so
        // its modes ARE a few hertz apart up high, and two of them 2 Hz apart at
        // 10 kHz beat at a rate nobody can hear. What must not happen is two
        // modes on top of each other relative to their neighbours, which is a
        // comb filter.
        let mean_gap: f64 = f.windows(2).map(|w| w[1] - w[0]).sum::<f64>() / (f.len() - 1) as f64;
        assert!(
            worst.0 > mean_gap * 0.25,
            "two modes only {:.2} Hz apart at {:.0} Hz, against a mean spacing of {mean_gap:.1} Hz",
            worst.0,
            worst.1
        );
    }

    /// The lowest modes are the ones computed for an actual Steinway D's board.
    ///
    /// These were Conklin's Chladni figures on a STRIPPED grand, starting at
    /// 49 Hz. A board on a bench and a board built into a piano — with its rim,
    /// its ribs, its bridges and the strings bearing down on it — do not start
    /// in the same place: Chabassier's finite-element model of a Steinway D puts
    /// mode 1 at 23 Hz.
    #[test]
    fn it_starts_on_the_measured_modes() {
        let b = Soundboard::new(SR, 1.0);
        let want = [23.0, 39.0, 52.0, 67.0, 89.0, 112.0, 139.0, 180.0, 252.0];
        for (i, w) in want.iter().enumerate() {
            assert!(
                (b.frequencies()[i] - w).abs() < 0.5,
                "mode {i}: {} vs the measured {w}",
                b.frequencies()[i]
            );
        }
    }

    /// And the spacing above them follows the measured density.
    ///
    /// About 22 Hz over the lowest modes, tightening towards the four hertz that
    /// a real board's density implies. This asserted ~17 Hz, from reading Ege's
    /// 0.06 modes/Hz as the plate's density — but that figure is what is seen AT
    /// ONE POINT above the transition, where the vibration localises between the
    /// ribs and a point sits inside some waveguides and not others. It is not how
    /// many modes the plate has. Chabassier's Steinway D needs 2400 modes to
    /// reach 10 kHz, and has 392 of them below 2693 Hz.
    #[test]
    fn modal_spacing_matches_the_measurement() {
        let b = Soundboard::new(SR, 1.0);
        // The tenor fill (161/204/229) is appended, not inserted, so the mode
        // list is not sorted; spacing is a property of the SET, so sort first.
        let mut f = b.frequencies().to_vec();
        f.sort_by(|a, c| a.partial_cmp(c).unwrap());
        let low: Vec<f64> = f.windows(2).take(20).map(|w| w[1] - w[0]).collect();
        let mean_low = low.iter().sum::<f64>() / low.len() as f64;
        assert!(
            mean_low > 16.0 && mean_low < 30.0,
            "mean spacing over the 21 lowest modes is {mean_low:.1} Hz, measured ≈ 22"
        );
        // Density just under the transition.
        let near: Vec<f64> = f
            .windows(2)
            .filter(|w| w[0] > 700.0 && w[1] < TRANSITION_HZ)
            .map(|w| w[1] - w[0])
            .collect();
        if near.len() > 3 {
            let mean = near.iter().sum::<f64>() / near.len() as f64;
            assert!(
                mean > 4.0 && mean < 12.0,
                "spacing below the transition is {mean:.1} Hz, and 2400 modes to 10 kHz says ~6"
            );
        }
    }

    /// Spruce's loss factor, and the rise where the board starts radiating.
    ///
    /// The first four modes are held apart here for the same reason the study
    /// holds them apart — "except for the first four low-frequency resonances,
    /// at which the energy losses at the rim are probably not negligible
    /// compared to those inside wood" — and are checked to be damped MORE, not
    /// checked against the wood's own figure.
    /// Every way out of the plate must give the same sound.
    ///
    /// There are four — `process`, `advance`, `radiate_pair` and the fused
    /// `drive_and_process` — and the fused one's whole licence is its promise to
    /// be identical to `drive_bridge` + `process` + `bridge_displacement`.
    /// Nothing checked that promise, and on 2026-08-23 it was broken by a change
    /// applied to `process` alone: the engine's hot loop takes another path, so
    /// the instrument shipped without a 3.27 dB compensation and a demo render
    /// came out 3.4 dB down before anyone noticed. Anything scaling the radiated
    /// sound belongs in `radiate`; this is what says so.
    #[test]
    fn every_way_out_of_the_plate_agrees() {
        let mut fused = Soundboard::new(SR, 1.0);
        let mut split = Soundboard::new(SR, 1.0);
        for i in 0..2000 {
            // Something with content at both ends, so a tilt cannot cancel.
            let f = if i == 0 { 1.0 } else { 0.3 * (i as f64 * 0.01).sin() };
            let (fl, fr, fd) = fused.drive_and_process(f);
            split.drive_bridge(f);
            let (sl, sr) = split.process();
            let sd = split.bridge_displacement();
            for (a, b, what) in [(fl, sl, "left"), (fr, sr, "right"), (fd, sd, "bridge")] {
                assert!(
                    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1e-12),
                    "sample {i}: the fused path's {what} is {a:e} and the split path's {b:e}"
                );
            }
        }
        // And the fused `advance` against the unfused `process`, driven the way
        // a voice drives — at its own attachment, not at the bridge point.
        // (`radiate_pair` needs no case of its own: it is one line around
        // `radiate` and holds no state, so it cannot drift from it.)
        let mut a = Soundboard::new(SR, 1.0);
        let mut b = Soundboard::new(SR, 1.0);
        let point: Vec<f64> = a.attachment_shared(60).to_vec();
        for i in 0..500 {
            let f = if i == 0 { 1.0 } else { 0.0 };
            a.drive_at(&point, f);
            let (al, ar) = a.advance();
            b.drive_at(&point, f);
            let (bl, br) = b.process();
            assert!(
                (al - bl).abs() < 1e-12 && (ar - br).abs() < 1e-12,
                "sample {i}: advance gives ({al:e}, {ar:e}), process ({bl:e}, {br:e})"
            );
        }
    }

    #[test]
    fn damping_matches_spruce_and_its_radiation_band() {
        let b = Soundboard::new(SR, 1.0);
        let modes = b.bank.modes();
        let mut low = Vec::new();
        for (i, m) in modes.iter().enumerate() {
            let f = m.w / std::f64::consts::TAU;
            let eta = 2.0 * m.sigma / m.w;
            // Ege sets the lowest modes aside because their losses at the rim
            // are not negligible against those inside the wood. That is a
            // statement about FREQUENCY, not about a count: on a board with a
            // realistic modal density there are far more than four of them below
            // the knee.
            if f < RIM_KNEE_HZ * 1.5 {
                assert!(
                    eta > ETA_SPRUCE * 1.5,
                    "mode {i} at {f:.0} Hz should carry rim losses on top of the wood's, has η = {eta:.3}"
                );
                continue;
            }
            if f < 1200.0 {
                // Ege gives TWO figures for this band and they are not the same
                // statement: a loss factor of 1-3%, and a damping RATE averaging
                // ~80 s⁻¹. A constant loss factor means σ = ½ηω, which falls away
                // in proportion to frequency and meets 80 s⁻¹ only around 1.1 kHz
                // — at 110 Hz it gives 8. Asserting the loss factor alone, as
                // this test used to, licensed a board five times too lively
                // through the whole bass, and a board that lively takes five
                // times too long to rise: the instrument's output peaked 148 ms
                // after the hammer struck instead of within a few milliseconds,
                // and a sound that swells rather than strikes is heard as a
                // plucked string, not a piano.
                //
                // So the RATE is what is held to here. The loss factor it implies
                // low down is large, and that is correct: down there the losses
                // are the rim, the bridge, the strings' downbearing and the air
                // being pushed, not the wood.
                //
                // This test used to assert the LOSS FACTOR instead (1% to 3%),
                // on the reading that Ege's two figures describe the same board
                // because the band's modes crowd towards its top, so their mean
                // rate lands near 80 with no floor imposed. The mean does. The
                // instrument does not: a chord excites its own fundamentals at
                // 80 to 300 Hz, and the mean averages precisely over those. The
                // plate took 50 to 100 ms to rise under every note and the user
                // heard the result as an organ. See `BOARD_RATE_LOW` for the
                // measurement that settled it.
                assert!(
                    m.sigma >= BOARD_RATE_LOW - 1e-9,
                    "{f:.0} Hz damps at {:.0} s⁻¹, under the floor Ege's mean rate sets",
                    m.sigma
                );
                low.push(m.sigma);
            }
        }
        let mean = low.iter().sum::<f64>() / low.len() as f64;
        assert!(
            mean > 30.0 && mean < 110.0,
            "mean damping below the transition is {mean:.0} s⁻¹, measured ≈ 80"
        );
    }

    /// The board is a body, not a reverb: struck, it is over well inside half a
    /// second, and its mid band decays on the ~80 s⁻¹ that was measured.
    #[test]
    fn the_board_is_a_body_and_not_a_reverb() {
        let mut b = Soundboard::new(SR, 1.0);
        b.drive_bridge(1.0);
        let mut env = Vec::new();
        for _ in 0..(SR as usize) {
            let (l, r) = b.process();
            env.push(l.abs().max(r.abs()));
        }
        let win = (SR as usize) / 200;
        let rms = |from: usize| -> f64 {
            let seg = &env[from..(from + win).min(env.len())];
            (seg.iter().map(|x| x * x).sum::<f64>() / seg.len() as f64).sqrt()
        };
        let early = rms((SR as usize) / 1000);
        let half = rms((SR as usize) / 2);
        let db = 20.0 * (half / early.max(1e-30)).log10();
        assert!(db < -45.0, "half a second on it is still only {db:.0} dB down");
        // And the modes that carry the measured 80 s⁻¹ must decay accordingly:
        // 80 s⁻¹ is a T60 of 86 ms, so about 35 dB over the first 50 ms.
        let mid_decay = 20.0 * (rms((SR as usize) / 20) / early.max(1e-30)).log10();
        assert!(
            (-60.0..=-12.0).contains(&mid_decay),
            "50 ms in, the board is {mid_decay:.0} dB down; the measured damping says roughly -35"
        );
    }

    /// Two points on a plate see different combinations of its modes, so the
    /// instrument is stereo by construction. A pan pot cannot do this: it leaves
    /// the two channels the same signal at different levels.
    #[test]
    fn the_two_sides_hear_genuinely_different_things() {
        let mut b = Soundboard::new(SR, 1.0);
        let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
        let mut seed = 7u64;
        for _ in 0..(SR as usize / 4) {
            b.drive_bridge(hashed(&mut seed) * 0.01);
            let (l, r) = b.process();
            ll += l * l;
            rr += r * r;
            lr += l * r;
        }
        let corr = lr / (ll.sqrt() * rr.sqrt()).max(1e-30);
        assert!(
            corr.abs() < 0.35,
            "the two sides correlate at {corr:.3}; a real piano measures around 0.2"
        );
    }

    /// And a narrow setting is a narrower version of the same image, not a
    /// different instrument.
    #[test]
    fn narrowing_the_listening_points_narrows_the_image() {
        let corr_of = |spread: f64| -> f64 {
            let mut b = Soundboard::new(SR, spread);
            let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
            let mut seed = 11u64;
            for _ in 0..(SR as usize / 4) {
                b.drive_bridge(hashed(&mut seed) * 0.01);
                let (l, r) = b.process();
                ll += l * l;
                rr += r * r;
                lr += l * r;
            }
            lr / (ll.sqrt() * rr.sqrt()).max(1e-30)
        };
        assert!(corr_of(0.0) > corr_of(1.0), "close points should correlate more");
    }

    /// The bridge has to be stiff enough to hold a string up and soft enough to
    /// take its energy: if it were rigid, nothing would ever decay.
    #[test]
    fn the_bridge_yields_but_not_much() {
        let b = Soundboard::new(SR, 1.0);
        let c = b.bridge_compliance();
        assert!(c > 0.0, "a rigid bridge would never let a note decay");
        // A newton, held for a second, would move it a few millimetres at most.
        let per_second = c * SR as f64;
        assert!(
            per_second < 0.05,
            "the bridge moves {per_second:e} m/s per newton, which is not a soundboard"
        );
    }

    /// The driving-point mobility, measured, against what was measured on real
    /// bridges — and against the shape a plate is obliged to have.
    ///
    /// Two published constraints, and this board was failing both quietly.
    ///
    /// The BAND: Wogram, and Giordano, put a piano bridge's driving-point
    /// mobility at roughly 1e-3 to 1e-2 m/s/N through the range the instrument
    /// lives in. That figure is already the reason `BRIDGE_COUPLING` has the value
    /// it has, so it has to hold at every note and not only at A4.
    ///
    /// The SHAPE: for a plate the mobility is asymptotically constant with
    /// frequency, `Y∞ = 1/(8√(Dρh))`, and neither Wogram's nor Giordano's
    /// measurements show a rise. It is not a detail. `α = T·Re{Y}/L` says the
    /// decay rate is proportional to it, so a mobility that climbs with pitch
    /// drains every note's upper partials first and empties the treble fastest —
    /// which is a string being plucked, and is what "harpe", "guitare" and "pincé"
    /// have all been describing.
    ///
    /// Measured the honest way: one newton-second into the point each note is
    /// pinned at, then the velocity that comes back IS the mobility's impulse
    /// response.
    #[test]
    fn the_bridge_mobility_is_flat_and_in_the_measured_band() {
        use rustfft::{num_complex::Complex, FftPlanner};
        const N: usize = 16_384;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(N);
        let mut seen: Vec<(u8, f64)> = Vec::new();
        for note in [33u8, 45, 57, 69, 81, 93, 105] {
            let mut b = Soundboard::new(SR, 0.7);
            let attach = b.attachment_shared(note);
            b.drive_at(&attach, SR as f64);
            let mut buf: Vec<Complex<f32>> = Vec::with_capacity(N);
            for _ in 0..N {
                buf.push(Complex { re: b.read_velocity_at(&attach) as f32, im: 0.0 });
                let _ = b.advance();
            }
            fft.process(&mut buf);
            let hz = SR as f64 / N as f64;
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let (lo, hi) = (((f0 * 0.6) / hz) as usize, (((f0 * 2.5) / hz) as usize).min(N / 2));
            // The DFT sums where the transform integrates, so the sampling
            // interval has to be put back or the answer is out by SR.
            let y = (lo..hi.max(lo + 1)).map(|k| buf[k].re as f64).sum::<f64>()
                / (hi.max(lo + 1) - lo) as f64
                / SR as f64;
            seen.push((note, y.abs()));
        }
        // The published band applies to the bridge, which is one thing, so it is
        // the average over the compass that has to sit inside it.
        let mean = (seen.iter().map(|s| s.1.ln()).sum::<f64>() / seen.len() as f64).exp();
        assert!(
            (1e-3..=1e-2).contains(&mean),
            "bridge mobility averages {mean:.2e} m/s/N, outside the 1e-3..1e-2 that Wogram \
             and Giordano measured — {seen:?}"
        );
        // A single note may sit under it, and legitimately: below the transition
        // the modes are 22 Hz apart with η = 1-3% (Ege), so their half-widths are
        // a couple of hertz and the overlap is about 0.07. A point mobility read
        // in that régime falls into the gaps BETWEEN modes, and a real board does
        // the same. What it may not do is fall out of the band by a factor.
        for &(note, y) in &seen {
            assert!(
                (5e-4..=1e-2).contains(&y),
                "note {note}: bridge mobility {y:.2e} m/s/N is a factor clear of the \
                 measured band — {seen:?}"
            );
        }
        // The shape, which is the part that decides the balance across the
        // keyboard: flat, with only the scatter low overlap explains.
        let lo = seen.iter().map(|s| s.1).fold(f64::MAX, f64::min);
        let hi = seen.iter().map(|s| s.1).fold(0.0, f64::max);
        assert!(
            hi / lo < 3.0,
            "the mobility spans {:.1}x across the compass where a plate's is flat \
             (Y∞ = 1/(8√(Dρh))) — {seen:?}",
            hi / lo
        );
        // And no TREND, which is the failure this test was written for: the top
        // of the compass must not be systematically more mobile than the bottom.
        let half = seen.len() / 2;
        let low: f64 = seen[..half].iter().map(|s| s.1).sum::<f64>() / half as f64;
        let high: f64 =
            seen[half..].iter().map(|s| s.1).sum::<f64>() / (seen.len() - half) as f64;
        assert!(
            (high / low) < 2.0 && (low / high) < 2.0,
            "the mobility rises {:.1}x from the bass to the treble; a plate's does not, \
             and α = T·Re(Y)/L turns that straight into a treble that empties first — {seen:?}",
            high / low
        );
    }
}

#[cfg(test)]
mod attach_probe {
    use super::*;
    /// How strongly each register is pinned to the plate's LOW modes.
    ///
    /// A treble string is anchored at the stiff, ribbed end of the board, a few
    /// centimetres from the rim it is glued to. The modes with one or two lobes
    /// across the whole plate are nearly still there, so a treble note should
    /// hardly wake them. If it wakes them as hard as a bass note does, every
    /// treble strike drags a low rumble behind it.
    /// The plate's driving-point mobility AT EACH NOTE'S OWN PARTIALS.
    ///
    /// A string only sounds through the plate, so if the modes the plate offers
    /// at that note's fundamental are ones its attachment point cannot see, the
    /// fundamental starves and the note comes out thin and bright: the file
    /// already names that failure for the tenor gap at 196 Hz. Above the
    /// transition the shapes localise between the ribs and each point sees only
    /// a fraction of the modes, so the same starvation can happen note by note.
    /// The plate's actual transfer, driven where a note is pinned and heard at
    /// the listening point: an impulse in, spectrum out. This is what multiplies
    /// the string's bridge force to make the sound.
    /// The plate's response as a CURVE, driven at the bridge and heard at the
    /// listening point. A real soundboard climbs steeply out of the bass (which
    /// is why a piano's lowest notes are all upper partials and almost no
    /// fundamental) and falls again above a few kilohertz.
    /// Two tones an octave apart, equal amplitude, driven into the bridge: what
    /// comes out tells us plainly whether the plate brightens or darkens a note.
    /// And what does a volume-velocity output do to the TIMBRE? The keyboard's
    /// levels were measured first; this asks the other half. At Eb6 the model's
    /// second partial sits 11.4 dB above what the force and the strike comb
    /// predict, and the stereo width was measured to add 9.7 of those — the same
    /// two-point lottery that digs the level holes. If reading the far field as
    /// the modes' volume velocity removes it, one change answers both faults.
    #[test]
    #[ignore]
    fn what_volume_velocity_does_to_the_timbre() {
        let sr = 48_000.0f32;
        eprintln!("  H2 against the fundamental, plate transfer at each note's partials:");
        for note in [84u8, 87, 91, 96] {
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let mut line = format!("    note {note:3} ");
            for which in 0..2 {
                let b = Soundboard::new(sr, 0.7);
                let att = b.attachment_shared(note);
                let n = b.bank.len();
                let ear: Vec<f64> = if which == 0 {
                    b.left.shape.clone()
                } else {
                    let r = (b.left.shape.iter().map(|a| a * a).sum::<f64>()
                        / b.left.shape.len().max(1) as f64)
                        .sqrt();
                    vec![r; n]
                };
                let amp = |hz: f64| -> f64 {
                    let mut bb = Soundboard::new(sr, 0.7);
                    let steps = (sr as usize) / 4;
                    let (mut re, mut im) = (0.0f64, 0.0f64);
                    for i in 0..steps {
                        let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                        bb.drive_at(&att, w.sin());
                        bb.bank.tick();
                        let o = bb.bank.read_velocity(&ear);
                        if i >= steps / 2 {
                            re += o * w.cos();
                            im -= o * w.sin();
                        }
                    }
                    re.hypot(im)
                };
                let (a1, a2) = (amp(f0), amp(2.0 * f0));
                line += &format!(
                    "{}: la table donne a H2 {:+6.1} dB   ",
                    if which == 0 { "deux points" } else { "debit volumique" },
                    20.0 * (a2 / a1.max(1e-30)).log10()
                );
            }
            eprintln!("{line}");
        }
        let _ = sr;
    }

    /// Would a VOLUME-VELOCITY output actually flatten the keyboard? Asked before
    /// rewriting the output model for it. The far field sums what the whole
    /// surface radiates, which per mode is its net volume displacement — a
    /// deterministic, same-sign weight — instead of two random samples of the
    /// plate. This drives each note's own attachment and reads the plate three
    /// ways: as shipped, with a constant positive weight on every mode (the
    /// crudest volume-velocity proxy, the frequency shaping being left to
    /// `radiate`), and with the plate's own bridge shape as the ear.
    #[test]
    #[ignore]
    fn would_volume_velocity_flatten_the_keyboard() {
        let sr = 48_000.0f32;
        let mut cols: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for note in 78u8..=99 {
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            for (which, col) in cols.iter_mut().enumerate() {
                let mut b = Soundboard::new(sr, 0.7);
                let att = b.attachment_shared(note);
                let n = b.bank.len();
                let ear: Vec<f64> = match which {
                    0 => b.left.shape.clone(),
                    1 => {
                        let r = (b.left.shape.iter().map(|a| a * a).sum::<f64>()
                            / b.left.shape.len().max(1) as f64)
                            .sqrt();
                        vec![r; n]
                    }
                    _ => b.bridge.shape.clone(),
                };
                let steps = (sr as usize) / 3;
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for i in 0..steps {
                    let w = std::f64::consts::TAU * f0 * i as f64 / sr as f64;
                    b.drive_at(&att, w.sin());
                    b.bank.tick();
                    let o = b.bank.read_velocity(&ear);
                    if i >= steps / 2 {
                        re += o * w.cos();
                        im -= o * w.sin();
                    }
                }
                col.push(20.0 * (re.hypot(im) / steps as f64).log10());
            }
        }
        let stat = |v: &[f64]| {
            let (mn, mx) = v.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
            let jump = (1..v.len()).map(|i| (v[i] - v[i - 1]).abs()).fold(0.0f64, f64::max);
            (mx - mn, jump)
        };
        for (name, v) in [
            ("deux points, comme livre", &cols[0]),
            ("debit volumique (poids constant positif)", &cols[1]),
            ("oreille = forme du chevalet", &cols[2]),
        ] {
            let (sp, jp) = stat(v);
            eprintln!("    {name:42} etendue {sp:5.1} dB   pire saut {jp:5.1} dB");
        }
    }

    /// Drive the board at EACH NOTE's own attachment point, with a sine at that
    /// note's fundamental, and read what it radiates. If the keyboard's level
    /// notches are the plate's coupling and not the string's, they will show up
    /// here with no hammer and no string in the way.
    #[test]
    #[ignore]
    fn what_the_plate_gives_each_note() {
        let sr = 48_000.0f32;
        eprintln!("  radiated level driving each note's own bridge point at its f0:");
        let mut prev = None;
        for note in 78u8..=99 {
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let mut b = Soundboard::new(sr, 0.7);
            let att = b.attachment_shared(note);
            let n = (sr as usize) / 3;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..n {
                let w = std::f64::consts::TAU * f0 * i as f64 / sr as f64;
                b.drive_at(&att, w.sin());
                let (l, r) = b.advance();
                if i >= n / 2 {
                    let o = 0.5 * (l + r);
                    re += o * w.cos();
                    im -= o * w.sin();
                }
            }
            let db = 20.0 * (re.hypot(im) / n as f64).log10();
            let step = prev.map_or(String::new(), |p: f64| format!("   ecart au demi-ton precedent {:+5.1}", db - p));
            eprintln!("    note {note:3} ({f0:6.0} Hz)  {db:7.1} dB{step}");
            prev = Some(db);
        }
    }

    /// The plate's own modes at the bottom: how many, how far apart, how damped.
    #[test]
    #[ignore]
    fn the_plates_lowest_modes() {
        let b = Soundboard::new(48_000.0, 0.7);
        eprintln!("  plate modes below 300 Hz. Ege measures ~80/s below 1.2 kHz,");
        eprintln!("  which is a half-power width of 25 Hz — wide enough that modes 23 Hz");
        eprintln!("  apart overlap and no deep valley can open between them.");
        let mut prev = 0.0f64;
        for m in b.bank.modes() {
            let f = m.w / std::f64::consts::TAU;
            if f > 300.0 { break; }
            eprintln!(
                "    {f:7.1} Hz   sigma {:6.1}/s   width {:5.1} Hz   gap below {:5.1} Hz",
                m.sigma,
                m.sigma / std::f64::consts::PI,
                f - prev
            );
            prev = f;
        }
    }

    /// Drive the board where a bass note meets it and sweep one tone across the
    /// bottom of the compass: how jagged is what a listener actually hears?
    #[test]
    #[ignore]
    fn bass_response_at_the_listener() {
        let sr = 48_000.0f32;
        let b = Soundboard::new(sr, 0.7);
        let att = b.attachment_shared(36);
        eprintln!("  radiated level driving the C2 bridge point, dB (65 Hz = P1, 131 = P2, 196 = P3):");
        let mut hz = 55.0f64;
        while hz <= 400.0 {
            let mut bb = Soundboard::new(sr, 0.7);
            let n = (sr as usize) / 2;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..n {
                let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                bb.drive_at(&att, w.sin());
                let (l, r) = bb.advance();
                if i >= n / 2 {
                    let o = 0.5 * (l + r);
                    re += o * w.cos();
                    im -= o * w.sin();
                }
            }
            eprintln!("    {hz:6.1} Hz  {:+6.1} dB", 20.0 * (re.hypot(im) / n as f64).log10());
            hz *= 1.09;
        }
        let _ = &b;
    }

    /// The radiation shelf on its own, no plate: does it level off where a real
    /// plate's radiation efficiency levels off (at the critical frequency)?
    #[test]
    #[ignore]
    fn radiation_shelf_alone() {
        let sr = 48_000.0f32;
        let mut b = Soundboard::new(sr, 0.7);
        eprintln!("  radiation shelf, dB relative to its high-frequency plateau:");
        for hz in [40.0f64, 65.0, 110.0, 220.0, 440.0, 880.0, 1200.0, 1760.0, 2500.0, 3520.0, 5000.0, 7040.0, 10000.0] {
            b.rad_l = [0.0; 2];
            b.rad_r = [0.0; 2];
            let n = (sr as usize) / 2;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for i in 0..n {
                let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                let (o, _) = b.radiate(w.sin(), 0.0);
                if i >= n / 2 {
                    re += o * w.cos();
                    im -= o * w.sin();
                }
            }
            eprintln!("    {hz:6.0} Hz  {:+6.1} dB", 20.0 * (2.0 * re.hypot(im) / n as f64).log10());
        }
    }

    #[test]
    #[ignore]
    fn plate_two_tone() {
        let sr = 48_000.0f32;
        for (f1, f2) in [(65.4f64, 130.8), (130.8, 261.6), (261.6, 523.2), (523.2, 1046.5), (1046.5, 2093.0), (1244.5, 2489.0), (2093.0, 4186.0)] {
            let mut b = Soundboard::new(sr, 0.7);
            let nn = (12.0 * (f1 / 440.0f64).log2() + 69.0).round() as u8;
            let att = b.attachment_shared(nn);
            let n = (sr as usize) / 4;
            let mut out = vec![0.0f64; n];
            for (i, v) in out.iter_mut().enumerate() {
                let t = i as f64 / sr as f64;
                let drive = (std::f64::consts::TAU * f1 * t).sin()
                    + (std::f64::consts::TAU * f2 * t).sin();
                b.drive_at(&att, drive);
                let (l, r) = b.advance();
                *v = 0.5 * (l + r);
            }
            // The same drive, read as raw bridge velocity: separates the plate's
            // mobility from the radiation shelf sitting after it.
            let mut b2 = Soundboard::new(sr, 0.7);
            let att2 = b2.attachment_shared(nn);
            let mut mob = vec![0.0f64; n];
            for (i, v) in mob.iter_mut().enumerate() {
                let t = i as f64 / sr as f64;
                let drive = (std::f64::consts::TAU * f1 * t).sin()
                    + (std::f64::consts::TAU * f2 * t).sin();
                b2.drive_at(&att2, drive);
                b2.bank.tick();
                *v = b2.bank.read_velocity(&b2.bridge.shape);
            }
            let amp = |hz: f64| -> f64 {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, &x) in out.iter().enumerate().skip(n / 2) {
                    let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                    re += x * w.cos();
                    im -= x * w.sin();
                }
                (re * re + im * im).sqrt()
            };
            let amp2 = |hz: f64| -> f64 {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, &x) in mob.iter().enumerate().skip(n / 2) {
                    let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                    re += x * w.cos();
                    im -= x * w.sin();
                }
                (re * re + im * im).sqrt()
            };
            eprintln!(
                "  {f1:.0} vs {f2:.0} Hz driven equally: bridge velocity {:+.1} dB/oct, radiated {:+.1} dB/oct",
                20.0 * (amp2(f2) / amp2(f1).max(1e-30)).log10(),
                20.0 * (amp(f2) / amp(f1).max(1e-30)).log10()
            );
        }
    }

    #[test]
    #[ignore]
    fn plate_response_curve() {
        let sr = 48_000.0f32;
        let mut b = Soundboard::new(sr, 0.7);
        let att = b.attachment_shared(60);
        let n = (sr as usize) / 2;
        let mut out = vec![0.0f64; n];
        b.drive_at(&att, 1.0);
        for v in out.iter_mut() {
            let (l, r) = b.process();
            *v = 0.5 * (l + r);
        }
        let dft = |hz: f64| -> f64 {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &x) in out.iter().enumerate() {
                let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                let win = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                re += x * w.cos() * win;
                im -= x * w.sin() * win;
            }
            (re * re + im * im).sqrt()
        };
        // third-octave averages, so single modes do not decide the shape
        let band = |c: f64| -> f64 {
            let mut acc = 0.0;
            let mut k = 0;
            while k < 15 {
                let f = c * (2f64).powf((k as f64 - 7.0) / 60.0);
                acc += dft(f).powi(2);
                k += 1;
            }
            (acc / 15.0).sqrt()
        };
        let mut r1k = 0.0;
        let mut rows = Vec::new();
        let mut f = 50.0;
        while f <= 12000.0 {
            let v = band(f);
            if (f - 1000.0).abs() < 1.0 {
                r1k = v;
            }
            rows.push((f, v));
            f *= 2f64.powf(1.0 / 3.0);
        }
        if r1k <= 0.0 {
            r1k = rows.iter().map(|r| r.1).fold(0.0f64, f64::max);
        }
        eprintln!("  the plate's response, third octaves, dB relative to 1 kHz:");
        for (f, v) in rows {
            let db = 20.0 * (v / r1k.max(1e-30)).log10();
            let bar = "#".repeat(((db + 60.0) / 2.0).max(0.0) as usize);
            eprintln!("   {f:7.0} Hz {db:+7.1} {bar}");
        }
    }

    #[test]
    #[ignore]
    fn plate_transfer_at_a_note() {
        let sr = 48_000.0f32;
        for note in [60u8, 87] {
            let mut b = Soundboard::new(sr, 0.7);
            let att = b.attachment_shared(note);
            let n = (sr as usize) / 4;
            let mut out = vec![0.0f64; n];
            b.drive_at(&att, 1.0);
            for v in out.iter_mut() {
                let (l, r) = b.process();
                *v = 0.5 * (l + r);
            }
            let dft = |hz: f64| -> f64 {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, &x) in out.iter().enumerate() {
                    let w = std::f64::consts::TAU * hz * i as f64 / sr as f64;
                    let win = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                    re += x * w.cos() * win;
                    im -= x * w.sin() * win;
                }
                (re * re + im * im).sqrt()
            };
            // average over a band, so a single mode does not decide it
            let band = |c: f64| -> f64 {
                let mut acc = 0.0;
                let mut k = -6;
                while k <= 6 {
                    acc += dft(c * (1.0 + 0.01 * k as f64)).powi(2);
                    k += 1;
                }
                (acc / 13.0).sqrt()
            };
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            let r1 = band(f0);
            eprintln!(
                "  note {note} (f0 {f0:.0}): plate response at P2 {:+6.1} dB, P3 {:+6.1} dB, P4 {:+6.1} dB (relative to P1)",
                20.0 * (band(2.0 * f0) / r1.max(1e-30)).log10(),
                20.0 * (band(3.0 * f0) / r1.max(1e-30)).log10(),
                20.0 * (band(4.0 * f0) / r1.max(1e-30)).log10()
            );
        }
    }

    #[test]
    #[ignore]
    fn mobility_at_each_notes_partials() {
        let b = Soundboard::new(48_000.0, 0.7);
        let modes = b.modes_for_audit().to_vec();
        for note in [84u8, 88, 90, 92, 94, 96, 99, 103] {
            let a = b.attachment(note);
            let f0 = 440.0 * 2f64.powf((note as f64 - 69.0) / 12.0);
            // Mobility at f: sum over modes of w^2 * (a resonance term at f).
            let mob = |f: f64| -> f64 {
                let wf = std::f64::consts::TAU * f;
                let mut acc = 0.0;
                for (i, w) in a.iter().enumerate() {
                    let m = modes[i];
                    let d = (m.w * m.w - wf * wf).powi(2) + (2.0 * m.sigma * wf).powi(2);
                    acc += w * w * wf * wf / d.max(1e-12);
                }
                acc.sqrt()
            };
            let m1 = mob(f0);
            let row: Vec<String> = (2..=4)
                .map(|k| format!("P{k} {:+6.1} dB", 20.0 * (mob(f0 * k as f64) / m1.max(1e-30)).log10()))
                .collect();
            eprintln!("  note {note:3} (f0 {f0:6.0}): mobility at its partials, relative to P1 -> {}", row.join("  "));
        }
    }

    #[test]
    #[ignore]
    fn where_each_register_is_pinned() {
        let b = Soundboard::new(48_000.0, 0.7);
        let modes = b.modes_for_audit().to_vec();
        for note in [28u8, 40, 60, 84, 96, 99, 108] {
            let a = b.attachment_shared(note);
            let mut lo = 0.0;
            let mut mid = 0.0;
            let mut hi = 0.0;
            for (i, w) in a.iter().enumerate() {
                let f = modes[i].w / std::f64::consts::TAU;
                let e = w * w;
                if f < 800.0 {
                    lo += e;
                } else if f < 3000.0 {
                    mid += e;
                } else {
                    hi += e;
                }
            }
            eprintln!(
                "  note {note:3}: coupling energy  <800Hz {:8.3e}   800-3k {:8.3e}   >3k {:8.3e}",
                lo, mid, hi
            );
        }
    }
}

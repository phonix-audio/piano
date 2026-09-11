//! Where every part of the piano is, in piano space.
//!
//! One coordinate system for the whole instrument: `(u, v)` over the unit
//! square, `u` from spine (left, bass) to bentside (right, treble), `v` from
//! tail (top) down to the keyboard edge (bottom). The scene maps it to pixels
//! once; everything else — the case outline, a single string, where a
//! microphone may sit — is stated here in those terms.
//!
//! Kept apart from the painting because these are the numbers the editor has
//! to *reason* about, not just draw: a sounding note lights its own string, a
//! dragged microphone lands somewhere real on the soundboard. An illustration
//! file could not answer those questions.
//!
//! The layout is a real grand seen from above: straight spine on the bass side,
//! the bentside curving in on the treble side, the bass strings crossing over
//! the tenor ones on their own bridge.

/// A point in piano space. Both components are in 0..1 for anything inside the
/// case.
pub type Uv = (f32, f32);

/// Lowest and highest note the instrument has. A0 to C8, the 88 keys.
pub const LOWEST: u8 = 21;
pub const HIGHEST: u8 = 108;

/// Above this note there are no dampers. Real grands stop damping around here
/// because the top strings ring so briefly that a damper would do nothing.
pub const LAST_DAMPED: u8 = 88;

/// The lowest note strung with copper-wound bass strings, which cross over the
/// tenor ones on their own bridge. Below this, everything is overstrung.
pub const BASS_TOP: u8 = 32;

/// Straight-line distance between two points in piano space.
fn dist(a: Uv, b: Uv) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// Quadratic through three points, sampled at `t`.
///
/// Curves are sampled to polylines everywhere in this codebase rather than
/// emitted as bezier shapes; it keeps stroking, hit-testing and glow passes on
/// one representation.
fn quad(p0: Uv, p1: Uv, p2: Uv, t: f32) -> Uv {
    let m = 1.0 - t;
    (
        m * m * p0.0 + 2.0 * m * t * p1.0 + t * t * p2.0,
        m * m * p0.1 + 2.0 * m * t * p1.1 + t * t * p2.1,
    )
}

/// The case, as a closed loop, walked clockwise from the front-left corner.
///
/// Front edge (where the keys are), up the treble cheek, in along the bentside,
/// around the tail, and back down the spine.
pub fn case_outline() -> Vec<Uv> {
    let mut pts = Vec::with_capacity(96);
    pts.push((0.00, 1.00));
    pts.push((1.00, 1.00));
    pts.push((1.00, 0.82));
    // Bentside: bows OUTWARD, hugging the right side before it turns over
    // towards the tail. Pull the control point in towards the middle and the
    // silhouette collapses into a wedge, which is the one shape a grand never
    // has.
    for i in 1..=44 {
        pts.push(quad((1.00, 0.82), (0.93, 0.30), (0.52, 0.05), i as f32 / 44.0));
    }
    // Tail, rounding over to the spine, which then runs dead straight down the
    // bass side to the keyboard.
    for i in 1..=18 {
        pts.push(quad((0.52, 0.05), (0.24, 0.00), (0.00, 0.10), i as f32 / 18.0));
    }
    pts
}

/// The same loop pulled inwards by `inset` (in piano-space units), used for the
/// inner rim line and for the soundboard.
///
/// Scaled about the centroid rather than offset per-edge: at these insets the
/// difference is invisible, and it cannot self-intersect on a concave curve the
/// way a true offset can.
pub fn case_inset(inset: f32) -> Vec<Uv> {
    let outline = case_outline();
    let n = outline.len() as f32;
    let cx = outline.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = outline.iter().map(|p| p.1).sum::<f32>() / n;
    let k = 1.0 - inset;
    outline
        .iter()
        .map(|&(u, v)| (cx + (u - cx) * k, cy + (v - cy) * k))
        .collect()
}

/// The long bridge, where the tenor and treble strings meet the soundboard.
pub fn long_bridge() -> Vec<Uv> {
    let mut pts = Vec::with_capacity(33);
    for i in 0..=16 {
        pts.push(quad((0.14, 0.18), (0.34, 0.34), (0.55, 0.55), i as f32 / 16.0));
    }
    for i in 1..=16 {
        pts.push(quad((0.55, 0.55), (0.72, 0.66), (0.84, 0.74), i as f32 / 16.0));
    }
    pts
}

/// The short bass bridge, set apart and higher up the board.
pub fn bass_bridge() -> [Uv; 2] {
    [(0.20, 0.24), (0.40, 0.44)]
}

/// The back length of a string: the stub that carries on past the bridge to
/// its hitch pin at the rim.
///
/// Not decoration. It is the reason the board behind the bridge is not bare in
/// a photograph of a grand, and leaving it out is what made this one look like
/// a diagram of a piano rather than a piano.
pub fn back_length(note: u8) -> [Uv; 2] {
    let [near, far] = string_line(note);
    let (dx, dy) = (far.0 - near.0, far.1 - near.1);
    let end = (far.0 + dx * 0.30, far.1 + dy * 0.30);
    [far, (end.0.clamp(0.02, 0.98), end.1.clamp(0.02, 0.98))]
}

/// A point along the long bridge at `t` in 0..1, by arc length.
///
/// Arc length rather than parameter, so string spacing along the bridge stays
/// even instead of bunching where the curve bends.
pub fn long_bridge_at(t: f32) -> Uv {
    let pts = long_bridge();
    let mut cum = Vec::with_capacity(pts.len());
    let mut total = 0.0;
    cum.push(0.0);
    for w in pts.windows(2) {
        total += dist(w[0], w[1]);
        cum.push(total);
    }
    let want = t.clamp(0.0, 1.0) * total;
    for i in 1..cum.len() {
        if cum[i] >= want {
            let seg = cum[i] - cum[i - 1];
            let f = if seg > 0.0 { (want - cum[i - 1]) / seg } else { 0.0 };
            let (a, b) = (pts[i - 1], pts[i]);
            return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        }
    }
    *pts.last().unwrap()
}

/// True when this note is strung as a wound bass string, crossing over the
/// tenor fan on the bass bridge.
pub fn is_bass(note: u8) -> bool {
    note <= BASS_TOP
}

/// One string, from its tuning pin at the front to where it crosses its bridge.
///
/// The bass strings start further left and run to the short bridge, which is
/// why they cross over the tenor ones: that diagonal is the single clearest
/// sign, from above, that this is a grand and not a diagram.
pub fn string_line(note: u8) -> [Uv; 2] {
    let n = note.clamp(LOWEST, HIGHEST);
    if is_bass(n) {
        let t = (n - LOWEST) as f32 / (BASS_TOP - LOWEST) as f32;
        let near = (0.06 + t * 0.10, PIN_ROW_V);
        let [b0, b1] = bass_bridge();
        let far = (b0.0 + (b1.0 - b0.0) * t, b0.1 + (b1.1 - b0.1) * t);
        [near, far]
    } else {
        let span = (HIGHEST - LOWEST) as f32;
        let near = (0.10 + (n - LOWEST) as f32 / span * 0.84, PIN_ROW_V);
        let t = (n - (BASS_TOP + 1)) as f32 / (HIGHEST - BASS_TOP - 1) as f32;
        [near, long_bridge_at(t)]
    }
}

/// Where this note's damper rests on its string, or `None` above the damped
/// range.
pub fn damper_at(note: u8) -> Option<Uv> {
    if note > LAST_DAMPED || note < LOWEST {
        return None;
    }
    let [near, far] = string_line(note);
    // A third of the way up the speaking length: the damper row sits well
    // forward, near the hammers, not out in the middle of the board.
    Some((near.0 + (far.0 - near.0) * 0.32, near.1 + (far.1 - near.1) * 0.32))
}

/// The pin block: the brass band across the front where every string is
/// tensioned. `(top v, bottom v)`.
pub const PIN_BLOCK_V: (f32, f32) = (0.83, 0.93);

/// The row the tuning pins sit on, and so where every string starts.
pub const PIN_ROW_V: f32 = (PIN_BLOCK_V.0 + PIN_BLOCK_V.1) * 0.5;

/// Centre lines of the three frame struts, front to tail.
pub const STRUTS: [[Uv; 2]; 3] = [
    [(0.22, 0.83), (0.09, 0.16)],
    [(0.50, 0.83), (0.31, 0.07)],
    [(0.78, 0.83), (0.60, 0.14)],
];

/// The line the microphones travel along, over the middle of the strings.
pub const MIC_V: f32 = 0.46;
/// Their midpoint, and the widest each may sit from it.
pub const MIC_CENTRE_U: f32 = 0.46;
/// The narrowest and the widest the pair may sit apart.
const MIC_MIN_SEP: f32 = 0.08;
const MIC_SPAN: f32 = 0.56;

/// Where the two microphones sit for a given `width`, as `(left u, right u)`.
///
/// `width` is the model's own parameter: how far apart the two points on the
/// soundboard are listened at. Even at zero they are not coincident, because
/// two capsules in the same place is not a thing a piano is ever recorded with.
pub fn mic_positions(width: f32) -> (f32, f32) {
    let sep = MIC_MIN_SEP + MIC_SPAN * width.clamp(0.0, 1.0);
    (MIC_CENTRE_U - sep * 0.5, MIC_CENTRE_U + sep * 0.5)
}

/// The `width` that would put a microphone at `u`, mirroring its partner.
pub fn width_for_mic_u(u: f32, is_left: bool) -> f32 {
    let half = if is_left { MIC_CENTRE_U - u } else { u - MIC_CENTRE_U };
    ((half * 2.0 - MIC_MIN_SEP) / MIC_SPAN).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_case_is_a_closed_shape_inside_the_unit_square() {
        let o = case_outline();
        assert!(o.len() > 50, "too coarse to look curved");
        for &(u, v) in &o {
            assert!((-0.001..=1.001).contains(&u), "u out of the box: {u}");
            assert!((-0.001..=1.001).contains(&v), "v out of the box: {v}");
        }
        // Front edge is the widest part; the tail is not.
        let front = o.iter().filter(|p| p.1 > 0.95).count();
        assert!(front >= 2, "no front edge");
    }

    /// Every string must start on the pin block and end on a bridge, and the
    /// treble ones must be shorter than the tenor ones — that taper is most of
    /// what makes the fan read as a piano.
    #[test]
    fn strings_run_from_the_pins_to_a_bridge_and_shorten_upwards() {
        let len = |n: u8| {
            let [a, b] = string_line(n);
            ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
        };
        for n in LOWEST..=HIGHEST {
            let [near, far] = string_line(n);
            assert!((near.1 - PIN_ROW_V).abs() < 1e-6, "note {n} does not start at the pins");
            for c in [near.0, near.1, far.0, far.1] {
                assert!((-0.001..=1.001).contains(&c), "note {n} leaves the box");
            }
        }
        assert!(len(40) > len(70), "the tenor is not longer than the treble");
        assert!(len(70) > len(108), "the treble does not taper");
    }

    /// Tuning pins march left to right with pitch. If this ever inverts, the
    /// keyboard and the string fan disagree and the illusion collapses.
    #[test]
    fn the_pins_march_up_with_pitch() {
        for n in (BASS_TOP + 1)..HIGHEST {
            assert!(
                string_line(n)[0].0 < string_line(n + 1)[0].0,
                "pins out of order at {n}"
            );
        }
        for n in LOWEST..BASS_TOP {
            assert!(string_line(n)[0].0 < string_line(n + 1)[0].0, "bass pins out of order at {n}");
        }
    }

    /// The bass strings have to actually cross the tenor fan: a bass string
    /// must start LEFT of a tenor string and end RIGHT of where that tenor
    /// string starts.
    #[test]
    fn the_bass_strings_cross_over_the_tenor_ones() {
        let bass = string_line(24);
        let tenor = string_line(45);
        assert!(bass[0].0 < tenor[0].0, "bass pin is not left of the tenor pin");
        assert!(bass[1].0 > bass[0].0, "the bass string does not run inward");
        assert!(bass[1].1 < tenor[0].1, "the bass string does not reach over the fan");
    }

    #[test]
    fn dampers_stop_where_a_real_piano_stops_damping() {
        assert!(damper_at(LOWEST).is_some());
        assert!(damper_at(LAST_DAMPED).is_some());
        assert!(damper_at(LAST_DAMPED + 1).is_none(), "the top octaves must be undamped");
        assert!(damper_at(HIGHEST).is_none());
    }

    /// Dragging a microphone and reading the width back has to be the identity,
    /// or the dot slides away from the pointer.
    #[test]
    fn the_microphones_and_the_width_agree_both_ways() {
        for i in 0..=10 {
            let w = i as f32 / 10.0;
            let (l, r) = mic_positions(w);
            assert!(l < r, "the microphones crossed over at width {w}");
            assert!((width_for_mic_u(l, true) - w).abs() < 1e-4, "left round trip at {w}");
            assert!((width_for_mic_u(r, false) - w).abs() < 1e-4, "right round trip at {w}");
            assert!((0.0..=1.0).contains(&l) && (0.0..=1.0).contains(&r), "off the board at {w}");
        }
    }

    #[test]
    fn the_bridge_is_sampled_evenly() {
        let a = long_bridge_at(0.0);
        let m = long_bridge_at(0.5);
        let b = long_bridge_at(1.0);
        assert!(a.0 < m.0 && m.0 < b.0, "the bridge doubles back");
        let d1 = ((m.0 - a.0).powi(2) + (m.1 - a.1).powi(2)).sqrt();
        let d2 = ((b.0 - m.0).powi(2) + (b.1 - m.1).powi(2)).sqrt();
        assert!((d1 - d2).abs() < 0.06, "halves differ: {d1} vs {d2}");
    }
}

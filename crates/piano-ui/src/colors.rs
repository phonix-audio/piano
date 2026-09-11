//! The instrument's palette: black lacquer, brass, aged ivory.
//!
//! Began as a verbatim copy of a shared token set, most of which named
//! colours for engines that do not exist here (Strata jade, Loquace teal, a
//! Moog blue). What is left is the five tokens the shared widgets actually
//! read, retuned from the DAW's cool grey to a grand piano's warm blacks, plus
//! the scene colours this editor paints with.
//!
//! Retuning the tokens rather than forking the widgets is deliberate:
//! `draw_knob_body` reads them, so the knobs re-skin themselves and stay
//! geometrically identical to the shared one.

use egui::Color32;

// ── Tokens the shared widgets read ───────────────────────────────────

/// Recesses, knob wells, the darkest surface. Warm near-black, not grey.
pub const BG_DARK: Color32 = Color32::from_rgb(22, 19, 16);
/// Cluster panels sitting on the lacquer.
pub const BG_PANEL: Color32 = Color32::from_rgb(28, 25, 21);
/// Knob bodies and raised elements — a warm umber shadow.
pub const BG_RAISED: Color32 = Color32::from_rgb(40, 35, 28);
/// Rims and dividers, brass-tinted so edges catch the light.
pub const BORDER: Color32 = Color32::from_rgb(70, 60, 44);
/// Labels and values.
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(236, 226, 205);
/// Units, captions, anything secondary.
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 140, 122);

// ── The instrument ───────────────────────────────────────────────────

/// The window itself: lacquer, just short of black.
pub const BG_LACQUER: Color32 = Color32::from_rgb(19, 17, 15);
/// Backdrop gradient, top and bottom. The sheen of a polished lid.
pub const LACQUER_TOP: Color32 = Color32::from_rgb(26, 23, 19);
pub const LACQUER_BOTTOM: Color32 = Color32::from_rgb(13, 12, 10);

/// Primary accent: knob arcs, headings, the wordmark.
pub const GOLD: Color32 = Color32::from_rgb(197, 160, 88);
/// Glows, lit strings, anything that should read as illuminated brass.
pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(232, 197, 120);

/// The engraved nameplate, three stops of a vertical gradient.
pub const PLATE_EDGE: Color32 = Color32::from_rgb(138, 109, 53);
pub const PLATE_FACE: Color32 = Color32::from_rgb(201, 167, 90);
/// Text engraved INTO the plate reads dark, never light.
pub const PLATE_ENGRAVED: Color32 = Color32::from_rgb(46, 36, 20);

/// Damper felt, the strip above the keys, clip indication. One red, used for
/// every piece of felt in the instrument.
pub const FELT_RED: Color32 = Color32::from_rgb(122, 40, 34);

// ── The scene ────────────────────────────────────────────────────────

/// Case and rim.
pub const CASE_LACQUER: Color32 = Color32::from_rgb(24, 22, 20);
pub const CASE_EDGE: Color32 = Color32::from_rgb(8, 8, 8);
/// The inset line that reads as the depth of an open case.
pub const CASE_INNER_RIM: Color32 = Color32::from_rgb(78, 68, 50);

/// Spruce soundboard, lit from the tail.
pub const WOOD_TOP: Color32 = Color32::from_rgb(96, 62, 33);
pub const WOOD_BOTTOM: Color32 = Color32::from_rgb(134, 91, 49);
pub const WOOD_GRAIN: Color32 = Color32::from_rgba_premultiplied(60, 38, 20, 14);

/// Cast-iron frame, gilded as they are.
pub const FRAME_DARK: Color32 = Color32::from_rgb(146, 116, 62);
pub const FRAME_LIGHT: Color32 = Color32::from_rgb(196, 164, 100);
pub const FRAME_HIGHLIGHT: Color32 = Color32::from_rgb(216, 180, 110);
/// The strip in front of the pin block, where the action lives under the keys.
pub const KEYBED: Color32 = Color32::from_rgb(38, 26, 18);

pub const TUNING_PIN: Color32 = Color32::from_rgb(210, 190, 140);

/// Bridges: hard maple, capped darker.
pub const BRIDGE: Color32 = Color32::from_rgb(74, 46, 26);
pub const BRIDGE_BASS: Color32 = Color32::from_rgb(58, 36, 20);

/// Plain steel strings, and the copper-wound bass that cross over them.
pub const STRING_STEEL: Color32 = Color32::from_rgb(203, 197, 183);
pub const STRING_BASS: Color32 = Color32::from_rgb(178, 108, 58);
/// A sounding string, and the warmer glow a wound bass string takes.
pub const STRING_LIT: Color32 = Color32::from_rgb(240, 210, 150);
pub const STRING_LIT_BASS: Color32 = Color32::from_rgb(230, 160, 90);

/// Damper felt block.
pub const DAMPER: Color32 = Color32::from_rgb(78, 68, 60);

// ── Microphones, and the meter needles that answer them ──────────────
//
// Deliberately the same two colours in both places: the left microphone on the
// soundboard and the left needle on the meter are one idea.

pub const MIC_LEFT: Color32 = Color32::from_rgb(212, 175, 55);
pub const MIC_RIGHT: Color32 = Color32::from_rgb(192, 196, 204);

// ── Keys ─────────────────────────────────────────────────────────────

/// Aged ivory, with the shade band that runs along the back of every key.
pub const KEY_WHITE: Color32 = Color32::from_rgb(233, 226, 212);
pub const KEY_WHITE_SHADE: Color32 = Color32::from_rgb(205, 196, 178);
pub const KEY_WHITE_HOVER: Color32 = Color32::from_rgb(244, 238, 224);
pub const KEY_BLACK: Color32 = Color32::from_rgb(22, 21, 19);
/// A key that is down, or whose string is sounding.
pub const KEY_LIT: Color32 = Color32::from_rgb(224, 168, 60);

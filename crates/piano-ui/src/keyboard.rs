//! The 88 keys, A0 to C8.
//!
//! Adapted from a shared keyboard rather than copied: that one builds
//! its keys octave by octave from a C, which cannot express a piano — a real
//! keyboard starts on A0 and ends on a lone C8. This walks the note range
//! instead, so the partial octave at each end comes out right.
//!
//! What is kept from the original, because it was learned the hard way: black
//! keys are hit-tested first, a drag across the keys glissandos, velocity comes
//! from how far down the key the click landed, and the press handler is gated
//! on `mouse_note.is_none()` — egui promotes a motionless held click into a
//! drag after 0.8 s, which otherwise fires a second note-on against a single
//! note-off.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::colors::*;
use crate::piano_geom::{HIGHEST, LOWEST};

/// A note the keyboard produced this frame. The caller maps it onto its own
/// channel; the widget knows nothing about the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyEvent {
    On { note: u8, velocity: u8 },
    Off { note: u8 },
}

/// Keyboard state that has to survive between frames.
#[derive(Clone, Default)]
pub struct KeyboardState {
    /// Notes held with the mouse, right here.
    pub held: Vec<u8>,
    /// Notes the engine reports sounding, so the keyboard reflects MIDI.
    pub active: Vec<u8>,
    /// The note under the current drag, which is what makes glissando work.
    mouse_note: Option<u8>,
}

/// Loudest a click can be. The top of a key is soft, the bottom is this.
const MAX_VELOCITY: f32 = 112.0;
const MIN_VELOCITY: f32 = 34.0;

fn is_black(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

/// How many white keys there are from A0 up to and including C8.
pub fn white_key_count() -> usize {
    (LOWEST..=HIGHEST).filter(|&n| !is_black(n)).count()
}

struct Key {
    rect: Rect,
    note: u8,
    black: bool,
}

/// Lay the keys out across `rect`, whites first, blacks after so they hit-test
/// on top.
fn keys(rect: Rect) -> Vec<Key> {
    let whites = white_key_count() as f32;
    let gap = 1.0;
    let w = (rect.width() - gap * (whites - 1.0)) / whites;
    let black_w = w * 0.62;
    let black_h = rect.height() * 0.62;

    let mut out = Vec::with_capacity(88);
    let mut x = rect.left();
    // The x of the white key just placed, so a black key can hang off it.
    let mut last_white_x = rect.left();
    let mut blacks: Vec<Key> = Vec::new();
    for n in LOWEST..=HIGHEST {
        if is_black(n) {
            blacks.push(Key {
                rect: Rect::from_min_size(
                    Pos2::new(last_white_x + w - black_w * 0.5 + gap * 0.5, rect.top()),
                    Vec2::new(black_w, black_h),
                ),
                note: n,
                black: true,
            });
        } else {
            out.push(Key {
                rect: Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, rect.height())),
                note: n,
                black: false,
            });
            last_white_x = x;
            x += w + gap;
        }
    }
    out.extend(blacks);
    out
}

/// Draw the keyboard and return what was played.
pub fn draw(ui: &mut Ui, rect: Rect, state: &mut KeyboardState) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let keys = keys(rect);
    let response = ui.interact(rect, ui.id().with("keys"), Sense::click_and_drag());

    // ── Which key is under the pointer, and how hard ──────────────────
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let mut hovered = None;
    let mut velocity = MAX_VELOCITY as u8;
    if let Some(pos) = pointer {
        for k in keys.iter().rev() {
            if k.rect.contains(pos) {
                hovered = Some(k.note);
                let t = ((pos.y - k.rect.top()) / k.rect.height()).clamp(0.0, 1.0);
                velocity = (MIN_VELOCITY + t * (MAX_VELOCITY - MIN_VELOCITY)).round() as u8;
                break;
            }
        }
    }

    // ── Press, glissando, release ─────────────────────────────────────
    if (response.drag_started() || response.is_pointer_button_down_on())
        && state.mouse_note.is_none()
    {
        if let Some(n) = hovered {
            state.mouse_note = Some(n);
            if !state.held.contains(&n) {
                state.held.push(n);
            }
            events.push(KeyEvent::On { note: n, velocity: velocity.max(1) });
        }
    } else if response.dragged() && state.mouse_note != hovered {
        if let Some(prev) = state.mouse_note.take() {
            state.held.retain(|&x| x != prev);
            events.push(KeyEvent::Off { note: prev });
        }
        if let Some(n) = hovered {
            state.mouse_note = Some(n);
            if !state.held.contains(&n) {
                state.held.push(n);
            }
            events.push(KeyEvent::On { note: n, velocity: velocity.max(1) });
        }
    }
    if response.drag_stopped()
        || (!response.is_pointer_button_down_on() && state.mouse_note.is_some() && !response.dragged())
    {
        if let Some(prev) = state.mouse_note.take() {
            state.held.retain(|&x| x != prev);
            events.push(KeyEvent::Off { note: prev });
        }
    }

    // ── Paint ─────────────────────────────────────────────────────────
    let p = ui.painter();
    // The strip of felt the keys come up against, as on the real instrument.
    let felt = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top() - 3.0),
        Pos2::new(rect.right(), rect.top()),
    );
    p.rect_filled(felt, 0.0, FELT_RED);

    let lit = |n: u8| state.held.contains(&n) || state.active.contains(&n);

    for k in keys.iter().filter(|k| !k.black) {
        let on = lit(k.note);
        let fill = if on {
            KEY_LIT
        } else if hovered == Some(k.note) {
            KEY_WHITE_HOVER
        } else {
            KEY_WHITE
        };
        p.rect_filled(k.rect, 2.0, fill);
        // A warm shade at the top and a shadow down the right edge: ivory keys
        // are not flat white, and flat white is what reads as a toy.
        if !on {
            let shade = Rect::from_min_size(k.rect.left_top(), Vec2::new(k.rect.width(), 5.0));
            p.rect_filled(shade, 2.0, KEY_WHITE_SHADE);
        }
        p.line_segment(
            [k.rect.right_top(), k.rect.right_bottom()],
            Stroke::new(1.0_f32, Color32::from_rgb(150, 142, 126)),
        );
        if k.note % 12 == 0 {
            p.text(
                Pos2::new(k.rect.center().x, k.rect.bottom() - 9.0),
                Align2::CENTER_CENTER,
                format!("C{}", k.note / 12 - 1),
                FontId::proportional(8.0),
                Color32::from_rgb(120, 112, 98),
            );
        }
    }

    for k in keys.iter().filter(|k| k.black) {
        let on = lit(k.note);
        let fill = if on {
            KEY_LIT.gamma_multiply(0.85)
        } else if hovered == Some(k.note) {
            Color32::from_rgb(52, 46, 40)
        } else {
            KEY_BLACK
        };
        p.rect_filled(k.rect, 2.0, fill);
        if !on {
            let hl = Rect::from_min_size(k.rect.left_top(), Vec2::new(k.rect.width(), 2.0));
            p.rect_filled(hl, 2.0, Color32::from_rgb(64, 58, 50));
        }
        p.rect_stroke(
            k.rect,
            2.0,
            Stroke::new(1.0_f32, Color32::from_rgb(12, 11, 10)),
            egui::StrokeKind::Outside,
        );
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A piano has 88 keys, 52 of them white, and it starts on A0 and ends on
    /// C8. Build them by whole octaves and both ends come out wrong.
    #[test]
    fn there_are_eighty_eight_keys_from_a0_to_c8() {
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 100.0));
        let k = keys(r);
        assert_eq!(k.len(), 88, "not a piano keyboard");
        assert_eq!(white_key_count(), 52, "wrong number of white keys");
        assert_eq!(k.iter().filter(|k| k.black).count(), 36);
        assert!(k.iter().any(|k| k.note == 21 && !k.black), "no A0");
        assert!(k.iter().any(|k| k.note == 108 && !k.black), "no C8");
    }

    /// The keys must span the strip exactly: a gap at the right edge is the
    /// visible tell of a layout built from octave counts.
    #[test]
    fn the_keys_fill_the_strip() {
        let r = Rect::from_min_size(Pos2::new(10.0, 0.0), Vec2::new(1240.0, 100.0));
        let k = keys(r);
        let left = k.iter().map(|k| k.rect.left()).fold(f32::MAX, f32::min);
        let right = k.iter().map(|k| k.rect.right()).fold(f32::MIN, f32::max);
        assert!((left - r.left()).abs() < 0.5, "left edge off by {}", left - r.left());
        assert!((right - r.right()).abs() < 0.5, "right edge off by {}", right - r.right());
    }

    /// Every black key must sit between its neighbours, never over a white key
    /// it does not belong to.
    #[test]
    fn black_keys_straddle_the_gap_they_belong_to() {
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 100.0));
        let k = keys(r);
        for b in k.iter().filter(|k| k.black) {
            let below = k
                .iter()
                .find(|w| !w.black && w.note == b.note - 1)
                .expect("a black key with no white key under its left shoulder");
            assert!(
                b.rect.center().x > below.rect.center().x,
                "black key {} sits left of its own white key",
                b.note
            );
            assert!(
                b.rect.center().x < below.rect.right() + below.rect.width(),
                "black key {} drifted a whole key away",
                b.note
            );
        }
    }
}

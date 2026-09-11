//! Piano — a grand piano, built from the physics.
//!
//! The hammer's force is computed against the string rather than assumed; the
//! strings of a unison are coupled through a real bridge; and the soundboard is
//! in the loop, loading the strings, rather than being a reverb hung on the
//! output. The decay of every note, the double decay, the beating of a unison,
//! sympathetic resonance, the sustain pedal and the stereo image are all
//! consequences of that chain, not settings inside it.
//!
//! Sources: Humbert (IRCAM/ATIAM 2002) for the modal formulation and the
//! real-time contact scheme; Chabassier (2012) for the string, the felt and the
//! measured stringing of a Steinway D; Ege (2009, 2013) for the soundboard's
//! measured modes and damping.

// DSP loops index several arrays by one counter, and a contact solve takes
// what it takes.
#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod fx;
pub mod patch;
pub mod modal_bank;
pub mod scale;
pub mod string;
pub mod hammer;
pub mod parallel;
pub mod soundboard;
pub mod voice;
pub mod mechanics;
pub mod engine;
#[cfg(test)]
mod chord_attack;
#[cfg(test)]
mod sympathy_ab;
pub mod piano_tables;
pub mod sympathy;

/// The engine-to-editor meter path and the flush-to-zero guard, from the
/// audio thread's toolbox.
pub use phonix_rt::{denormal, triple_buffer as state_buffer};

pub use engine::{PianoCommand, PianoEngine, PianoMeterState};
pub use patch::PianoPatch;

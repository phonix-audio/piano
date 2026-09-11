//! The Piano editor: a grand piano seen from above, in egui.
//!
//! The instrument is the interface. The two microphones are dots you drag on
//! the soundboard, because `width` is literally where the model listens to the
//! plate; the strings light as they sound; the dampers lift when the pedal
//! goes down. The technician's adjustments sit around the case.
//!
//! A separate crate from the engine, and not a feature of it. Cargo unifies
//! features across a resolved dependency graph, so an optional `egui` inside
//! `piano` would be switched on for the engine's own tests by any
//! `cargo test --workspace`. A crate boundary is the only thing that makes
//! "the engine never sees egui" true rather than merely intended.

pub mod app;
pub mod colors;
pub mod fx_page;
pub mod header;
pub mod keyboard;
pub mod piano_geom;
pub mod scene;
pub mod theme;
pub mod vu;

pub use app::PianoApp;

/// Where a saved patch goes on disk.
pub const PRESET_HOME: phonix_ui::preset_io::PresetHome =
    phonix_ui::preset_io::PresetHome { organisation: "Phonix Audio", application: "Piano" };

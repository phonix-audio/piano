//! The chain a patch describes, and the values each factory preset gives it.
//!
//! Described here, run nowhere: the engine owns no effects. The host builds a
//! live chain from this description.
//!
//! Four effects in a fixed order -- shelf, glue, room, ceiling. Which ones and
//! in which order is not a preset's business; what each one is set to is.

use phonix_fx::{ChainSpec, SlotSpec};

/// Slots the chain occupies. Frozen with the order below.
pub const FX_SLOTS: usize = 4;

/// The shelf: band 0 of the EQ, a low shelf under its automatic type.
const SHELF_HZ: f32 = 90.0;
const SHELF_Q: f32 = 0.7;

/// The room a preset is heard in.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    /// 0..1. How big the space is.
    pub size: f32,
    /// 0..1, not seconds: the reverb maps it per type.
    pub decay: f32,
    /// 0..1. How much of it is heard: the slot's mix.
    pub mix: f32,
}

/// Build the chain. Units are the effects' own: the EQ in Hz and dB, the
/// compressor's threshold in dB and its times in seconds, the reverb
/// normalised except a pre-delay in seconds, the limiter's ceiling in dB and
/// its release in milliseconds.
pub fn chain(shelf_db: f32, comp_thresh_db: f32, room: Room) -> ChainSpec {
    ChainSpec::new(vec![
        // Tone before anything reacts to level. A modelled string radiates
        // below what a real soundboard does; the shelf takes that back.
        SlotSpec::new("parametric-eq")
            .with("band.0.freq", SHELF_HZ)
            .with("band.0.q", SHELF_Q)
            .with("band.0.gain", shelf_db)
            .with("band.1.enabled", false)
            .with("band.2.enabled", false)
            .with("band.3.enabled", false),
        // Slow bus glue: hard knee, no lookahead, so it adds no latency.
        SlotSpec::new("compressor")
            .with("mode", "bus")
            .with("threshold", comp_thresh_db)
            .with("ratio", 2.0_f32)
            .with("attack", 0.020_f32)
            .with("release", 0.150_f32)
            .with("knee", 6.0_f32),
        // The room past the microphones. `width` places the mics on the
        // soundboard; the model has no boundary reflections at all, and their
        // absence is what reads as unreal.
        SlotSpec::new("reverb")
            .with("type", "room")
            .with("size", room.size)
            .with("decay", room.decay)
            .with("damping", 0.50_f32)
            .with("predelay", 0.008_f32)
            .with("width", 1.0_f32)
            .mix(room.mix),
        // Safety, not character. The three above can add gain, and every
        // preset gets the same ceiling: how loud is too loud is not a musical
        // choice.
        SlotSpec::new("brickwall-limiter")
            .with("ceiling", -0.3_f32)
            .with("release", 50.0_f32),
    ])
}

/// The chain for a piano heard from the usual distance in the usual hall.
pub fn concert_hall() -> ChainSpec {
    chain(-3.0, -18.0, Room { size: 0.35, decay: 0.40, mix: 0.22 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind and parameter the recipe names exists in the build, in
    /// range, under the name written.
    #[test]
    fn the_recipe_names_only_what_the_build_has() {
        let report = concert_hall().check(&phonix_fx::Registry::builtin());
        assert!(report.is_clean(), "{report}");
    }

    #[test]
    fn the_order_and_the_kinds_are_frozen() {
        let spec = concert_hall();
        assert_eq!(spec.kinds().collect::<Vec<_>>(), ["parametric-eq", "compressor", "reverb", "brickwall-limiter"]);
        assert_eq!(spec.len(), FX_SLOTS);
        assert!(spec.slots.iter().all(|s| s.enabled));
    }
}

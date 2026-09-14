//! What you can change about the piano.
//!
//! These are a piano's own adjustments, not a synthesiser's. Everything about
//! the tone that a player or a technician cannot alter — the scaling, the
//! hammer masses, the plate's modes — is not here, because it is not a setting:
//! it is what the instrument is, and it lives in `scale.rs`, `hammer.rs` and
//! `soundboard.rs`.
//!
//! What is left is what a technician actually does. Voicing the hammers by
//! needling or filing the felt. Setting the unison, which is the single largest
//! thing a tuner decides about how a piano sings. Regulating the dampers.
//! Choosing where to listen.

use serde::{Deserialize, Serialize};

fn default_mechanics() -> f32 {
    0.35
}

fn default_release_noise() -> f32 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoPatch {
    pub name: String,
    /// Hammer felt, 0 needled soft to 1 filed hard. Moves the stiffness and the
    /// exponent of the felt's force law together, which is what needling and
    /// filing physically do.
    pub voicing: f32,
    /// Spread across the strings of a unison, in cents.
    ///
    /// Small. A tuner sets a unison to beat as little as they can hear, and what
    /// is left is a fraction of a cent: measured against a real grand, one cent
    /// across the outer strings puts the wobble of the low partials where it
    /// belongs (1.7 dB against a measured 1.5). This was at 2.5, which is what a
    /// badly kept piano has — every note beating in step at a couple of hertz,
    /// heard as a flanger over the whole instrument rather than as life in the
    /// tone.
    ///
    /// Zero is a dead unison: technically perfect, and lifeless.
    pub unison_detune: f32,
    /// How far apart the two points on the soundboard are listened at. This is
    /// the stereo image, and it is a microphone placement, not a pan pot.
    pub width: f32,
    /// Damper regulation: how quickly the felts stop a string once the key is
    /// released.
    pub damper: f32,
    /// How loud the action, the dampers and the pedal are — the noises the
    /// machine makes that are not the strings. A real piano has them; how much
    /// of them you hear depends entirely on where the microphones are.
    #[serde(default = "default_mechanics")]
    pub mechanics: f32,
    /// How loud the key coming back up is, on its own.
    ///
    /// Separate from `mechanics` because it is a separate event and players
    /// treat it as one: it is the noise of the key returning and the jack
    /// resetting, and unlike the damper it happens with the pedal down too. On
    /// a close-miked recording it is a large part of what makes a released
    /// chord sound like an instrument rather than a fade-out.
    #[serde(default = "default_release_noise")]
    pub release_noise: f32,
    /// Fine tuning, in cents.
    pub tune: f32,
    pub gain: f32,
    /// The master chain this patch is heard through: shelf, glue, room,
    /// ceiling. Appended, and empty by default, so a patch written before it
    /// existed opens with no chain and sounds exactly as it did. A chain
    /// written in the form before names were the wire format is read and
    /// renamed on the way in.
    ///
    /// Described, never run. The host builds the live chain from it.
    #[serde(default, deserialize_with = "fx_field")]
    pub fx: phonix_fx::ChainSpec,
}

/// The two forms a chain was ever written in.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum FxField {
    Named(phonix_fx::ChainSpec),
    Ordinal { slots: Vec<phonix_legacy::FxSlotState> },
}

fn fx_field<'de, D: serde::Deserializer<'de>>(d: D) -> Result<phonix_fx::ChainSpec, D::Error> {
    Ok(match FxField::deserialize(d)? {
        FxField::Named(spec) => spec,
        FxField::Ordinal { slots } => phonix_legacy::convert_chain(&slots).0,
    })
}

impl Default for PianoPatch {
    fn default() -> Self {
        PianoPatch {
            name: "Concert Grand".to_string(),
            voicing: 0.5,
            unison_detune: 1.0,
            width: 0.7,
            damper: 0.5,
            mechanics: default_mechanics(),
            release_noise: default_release_noise(),
            tune: 0.0,
            gain: PRESET_GAIN,
            // The default patch IS the first factory preset -- same name,
            // same values -- so it carries the same chain. Distinct from the
            // `#[serde(default)]` above, which is per FIELD: a patch written
            // before `fx` existed still deserialises to an EMPTY chain and
            // keeps sounding as it did.
            fx: crate::fx::concert_hall(),
        }
    }
}

// `PianoPatch` deliberately implements no GUI trait. It used to carry an
// an editor-side `Preset` impl, which made the engine's data model
// depend on a DAW's widget layer. Each interface declares its own trait and
// implements it on this type instead: legal for them, free for us.

/// The output trim every factory preset ships at, on the 0..2 the knob spans.
///
/// The engine's own trim keeps a fortissimo six-note chord under full scale
/// with NO chain in the way, which leaves a single fortissimo note 7.6 dB
/// down. The plugin's fader holds full scale, so a preset can sit higher:
/// at 1.5 a single fortissimo note peaks 3.7 dB below full scale and the
/// chord is held by the fader, with a quarter of the knob still above.
/// Measured at 48 kHz, C4 and a C major chord, through the Concert Grand
/// chain.
pub const PRESET_GAIN: f32 = 1.5;

/// A short, honest bank. A piano is one instrument; what varies between these is
/// what varies between two well-kept pianos, or between the same piano on two
/// different days of a technician's work.
pub fn factory_presets() -> Vec<PianoPatch> {
    use crate::fx::{chain, Room};
    let mk = |name: &str, voicing: f32, detune: f32, width: f32, damper: f32,
              fx: phonix_fx::ChainSpec| PianoPatch {
        name: name.to_string(),
        voicing,
        unison_detune: detune,
        width,
        damper,
        mechanics: default_mechanics(),
        release_noise: default_release_noise(),
        tune: 0.0,
        gain: PRESET_GAIN,
        fx,
    };
    // The room each one is heard in. It follows the microphones: the closer
    // they are, the less of the room reaches them, and the more low end the
    // shelf has to take back.
    let hall = |mix: f32| chain(-3.0, -18.0, Room { size: 0.35, decay: 0.40, mix });
    vec![
        mk("Concert Grand", 0.5, 1.0, 0.7, 0.5, hall(0.22)),
        mk("Concert Grand, Bright", 0.78, 1.0, 0.7, 0.5, hall(0.24)),
        mk("Concert Grand, Mellow", 0.24, 0.9, 0.7, 0.5, hall(0.18)),
        // Close in: proximity puts more weight low, and almost no room.
        mk("Close Mics", 0.55, 1.0, 0.25, 0.5,
           chain(-5.0, -18.0, Room { size: 0.20, decay: 0.30, mix: 0.10 })),
        // Where the player sits: the lid, and the room behind it.
        mk("Player's Seat", 0.45, 1.2, 0.9, 0.5,
           chain(-3.0, -18.0, Room { size: 0.45, decay: 0.45, mix: 0.28 })),
        // A small bright room rather than a hall.
        mk("Salon", 0.4, 1.4, 0.55, 0.45,
           chain(-2.5, -16.0, Room { size: 0.30, decay: 0.35, mix: 0.26 })),
        // Dead means dead, including the room.
        mk("Tuned Dead", 0.5, 0.0, 0.7, 0.5,
           chain(-3.0, -18.0, Room { size: 0.25, decay: 0.30, mix: 0.06 })),
        mk("Wide Unison", 0.5, 3.0, 0.8, 0.5, hall(0.24)),
        // The strings ring on by themselves; the room would only crowd them.
        mk("Long Dampers", 0.45, 1.0, 0.7, 0.15, hall(0.16)),
        mk("Tight Dampers", 0.5, 1.0, 0.7, 0.9, hall(0.20)),
    ]
}

/// The name the rest of the codebase already imports.
pub use factory_presets as factory_presets_tagged;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_are_unique() {
        let bank = factory_presets();
        let mut names: Vec<&str> = bank.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate preset names break the picker");
    }

    #[test]
    fn every_preset_is_in_range() {
        for p in factory_presets() {
            assert!((0.0..=1.0).contains(&p.voicing), "{}: voicing", p.name);
            assert!((0.0..=20.0).contains(&p.unison_detune), "{}: detune", p.name);
            assert!((0.0..=1.0).contains(&p.width), "{}: width", p.name);
            assert!((0.0..=1.0).contains(&p.damper), "{}: damper", p.name);
            assert!((0.0..=1.0).contains(&p.mechanics), "{}: mechanics", p.name);
            assert!((0.0..=1.0).contains(&p.release_noise), "{}: release", p.name);
        }
    }

    /// The demo picks a preset by name and hands it to the track as JSON. If
    /// that blob does not come back as the patch that went in, every render has
    /// silently been played on the DEFAULT piano and the preset was decoration.
    #[test]
    fn a_serialised_preset_survives_the_track() {
        let mut p = factory_presets()
            .into_iter()
            .find(|p| p.name == "Concert Grand, Mellow")
            .expect("the preset the demo asks for");
        p.mechanics = 0.0;
        p.release_noise = 0.0;
        let blob = serde_json::to_vec(&p).unwrap();
        let back: PianoPatch = serde_json::from_slice(&blob)
            .expect("the track's engine_state must deserialise");
        assert_eq!(back.mechanics, 0.0, "mechanics did not survive");
        assert_eq!(back.release_noise, 0.0, "release noise did not survive");
        assert_eq!(back.voicing, p.voicing);
    }

    /// The bank is a wire format, and so is its ORDER.
    ///
    /// The names are looked up as strings by the demos, and the plugin's integer
    /// `preset` parameter indexes this vector — that integer is what a host's
    /// automation lane and every generated `.vstpreset` store. Reordering the
    /// bank silently repoints saved projects at a different piano. Append only.
    #[test]
    fn the_factory_bank_is_a_wire_format() {
        let bank = factory_presets();
        let names: Vec<&str> = bank.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Concert Grand",
                "Concert Grand, Bright",
                "Concert Grand, Mellow",
                "Close Mics",
                "Player's Seat",
                "Salon",
                "Tuned Dead",
                "Wide Unison",
                "Long Dampers",
                "Tight Dampers",
            ]
        );
    }

    /// The field names are a wire format too: this JSON sits inside
    /// the host's saved state for this plugin. Add fields, never
    /// rename them, and always behind `#[serde(default)]`.
    ///
    /// Checked as names in declaration order rather than as one golden string,
    /// so the test pins the contract and not `f32` formatting.
    #[test]
    fn the_patch_field_names_and_their_order_are_a_wire_format() {
        let json = serde_json::to_string(&PianoPatch::default()).unwrap();
        let mut at = 0;
        for key in [
            "name", "voicing", "unison_detune", "width", "damper", "mechanics",
            "release_noise", "tune", "gain",
        ] {
            let needle = format!("\"{key}\":");
            let found = json[at..]
                .find(&needle)
                .unwrap_or_else(|| panic!("{key} missing or out of order in {json}"));
            at += found + needle.len();
        }
    }

    /// Each factory preset carries its own chain: that is what "the presets
    /// have effects" means.
    #[test]
    fn every_factory_preset_carries_its_own_chain() {
        for p in factory_presets() {
            assert_eq!(p.fx.slots.len(), crate::fx::FX_SLOTS, "{}", p.name);
        }
    }

    /// A fresh instance shows the first preset's name, so it must sound like
    /// it, effects included. Anything else is a plugin that says one thing and
    /// plays another.
    #[test]
    fn the_default_patch_is_the_first_preset_chain_included() {
        let d = PianoPatch::default();
        let first = &factory_presets()[0];
        assert_eq!(d.name, first.name);
        assert_eq!(d.fx, first.fx);
    }

    /// The container's default and the field's serde default are NOT the same
    /// thing, and the difference is the whole compatibility story: a patch
    /// written before `fx` existed deserialises to an empty chain.
    #[test]
    fn a_patch_written_before_fx_existed_has_no_chain() {
        let legacy = r#"{"name":"Concert Grand","voicing":0.5,"unison_detune":1.0,
                         "width":0.7,"damper":0.5,"tune":0.0,"gain":0.9}"#;
        let p: PianoPatch = serde_json::from_str(legacy).unwrap();
        assert!(p.fx.slots.is_empty());
    }

    /// The rooms differ, or the presets would not be carrying anything of
    /// their own. Close Mics is the driest and Player's Seat the wettest.
    #[test]
    fn the_presets_are_not_all_heard_in_the_same_room() {
        let mix = |name: &str| -> f32 {
            let p = factory_presets().into_iter().find(|p| p.name == name).unwrap();
            p.fx.slots[2].mix
        };
        assert!(mix("Close Mics") < mix("Concert Grand"));
        assert!(mix("Concert Grand") < mix("Player's Seat"));
        assert!(mix("Tuned Dead") < mix("Close Mics"));
    }

    /// A patch written while the chain was ordinals and pids opens as the
    /// same chain under its names, with nothing left behind.
    #[test]
    fn a_patch_written_with_the_old_chain_opens_named() {
        let old = r#"{"name":"Concert Grand","voicing":0.5,"unison_detune":1.0,"width":0.7,
            "damper":0.5,"tune":0.0,"gain":1.5,"fx":{"slots":[
            {"effect_type":20,"enabled":true,"mix":1.0,"params":[[28,90.0],[38,0.7],[29,-3.0],[80,1.0],[79,1.0]]},
            {"effect_type":8,"enabled":true,"mix":1.0,"params":[[60,-18.0],[61,2.0],[62,0.02],[63,0.15],[65,6.0],[66,1.0]]},
            {"effect_type":2,"enabled":true,"mix":1.0,"params":[[16,1.0],[10,0.35],[11,0.4],[12,0.5],[13,0.008],[15,1.0],[14,0.22]]},
            {"effect_type":28,"enabled":true,"mix":1.0,"params":[[28,-0.3],[29,50.0],[38,1.0]]}]}}"#;
        let p: PianoPatch = serde_json::from_str(old).unwrap();
        assert_eq!(p.fx.kinds().collect::<Vec<_>>(), ["parametric-eq", "compressor", "reverb", "brickwall-limiter"]);
        assert_eq!(p.fx.slots[0].f32("band.0.gain"), Some(-3.0));
        assert_eq!(p.fx.slots[2].variant("type"), Some("room"));
        assert_eq!(p.fx.slots[2].mix, 0.22);
        // The old chain never wrote the compressor's mode; the plugin set it.
        // Its band mask named every band, on or off; the recipe names the
        // ones it turns off.
        // The fourth slot was a ceiling this build no longer carries: it is
        // kept as written and left alone, and the three before it are the
        // recipe's.
        let mut want = crate::fx::concert_hall();
        want.slots[1].params.remove("mode");
        want.slots[0].set("band.0.enabled", true);
        assert_eq!(p.fx.slots[..3], want.slots[..3]);
        assert_eq!(p.fx.slots[3].kind, "brickwall-limiter");
        let again: PianoPatch = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(again.fx, p.fx, "a named chain does not survive a round trip");
    }

    /// A session written before `mechanics` and `release_noise` existed must
    /// still open, and must land on the documented defaults rather than on zero.
    /// Removing either `#[serde(default)]` is a silent data change, not a
    /// compile error, which is why this is a test.
    #[test]
    fn a_session_written_before_those_fields_still_loads() {
        let legacy = r#"{"name":"Concert Grand","voicing":0.5,"unison_detune":1.0,
                         "width":0.7,"damper":0.5,"tune":0.0,"gain":0.9}"#;
        let p: PianoPatch = serde_json::from_str(legacy).expect("legacy engine_state must load");
        assert_eq!(p.mechanics, 0.35, "mechanics lost its default");
        assert_eq!(p.release_noise, 0.5, "release_noise lost its default");
    }

    /// The saved-state contract: what goes out has to come back unchanged.
    #[test]
    fn patch_serde_roundtrip() {
        let p = PianoPatch::default();
        let json = serde_json::to_vec(&p).unwrap();
        let back: PianoPatch = serde_json::from_slice(&json).unwrap();
        assert_eq!(serde_json::to_vec(&back).unwrap(), json);
    }
}

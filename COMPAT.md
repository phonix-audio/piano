# What must not change

Everything below is a wire format. It is written into files other programs read,
so it is not a naming choice and not refactorable. Each item has a test; the
tests exist because a comment alone loses to a rename.

## Identifiers a host resolves the plugin by

    VST3 class id   PxPhonixPiano001            (16 ASCII bytes, exactly)
    CLAP id         com.phonix-audio.piano
    Plugin NAME     Phonix Piano
    Plugin VENDOR   Phonix Audio

The class id goes into every exported DAWproject `Vst3Plugin` device and into the
header of every `.vstpreset`. NAME and VENDOR compose the preset directory
Cubase's MediaBay indexes, `<presets>/Phonix Audio/Phonix Piano/`. Change any of
them and existing projects and preset banks point at a plugin no host can find.

They have moved twice. The instrument shipped as "Phonix Marteau"
(`PxMarteauModl001`), then as "Cordis" (`PxCordisPiano001`), and carries the
name above now. Each move cost something, and this one cost the two tagged
releases, which were deleted: nothing versioned stands behind those older ids.
What keeps a saved session working across the moves is the sequencer's
`RENAMED_DEVICES` table, old id to current id, never to nothing. A `.vstpreset`
on disk does not follow: its bank is written under the directory above, and a
bank written under an older name stays where it is.

NAME repeats the vendor on purpose. "Piano" alone is the word every host lists a
dozen times over, and a plugin nobody can pick out of that list by name is worse
off than one that spends a little width saying whose it is.

A host application that resolves plugins by class id holds a copy of it in its
own DAWproject export. Both sides assert against the literal, so a drift fails a
build on each. That is a note for whoever maintains that side: its copy has to be
moved to
the value above in the same change, or its export names a plugin no host can
find. Nothing in this repository depends on it.

Test: `piano-plugin`, `frozen_identifiers`.

## Parameter ids and persistence keys

    persist   patch  editor-state
    params    preset voicing unison width damper action release tune gain

Three of the ids deliberately differ from what they drive: `unison` sets
`unison_detune`, `action` sets `mechanics`, and `width` is labelled "Spread" in
the editor. They are frozen as they are.

These strings are JSON keys inside every saved project's plugin state and inside
every generated `.vstpreset`.

Adding is always allowed; renaming and reordering are not. Two ids were
REMOVED, `hybrid` and `maxhold`, with the sample-cache preview they drove: a
project that stored them restores with those two keys skipped (nice-plug
logs an unknown id and moves on) and everything else intact. Neither id may
be reused for something new, or such a project would restore a stale value
into it.

## The patch's serde shape

    name voicing unison_detune width damper mechanics release_noise tune gain
    fx

In that order, and with the `#[serde(default)]` fallbacks intact: `mechanics`
defaults to 0.35, `release_noise` to 0.5, and `fx` to an empty chain. A session written before those two
fields existed relies on them, so removing a default is a silent data change
rather than a compile error.

This JSON is what a host's session file stores for a Piano track.

Tests: `piano`, `the_patch_field_names_and_their_order_are_a_wire_format` and
`a_session_written_before_those_fields_still_loads`.

## The factory bank, names AND order

    Concert Grand * Concert Grand, Bright * Concert Grand, Mellow * Close Mics
    Player's Seat * Salon * Tuned Dead * Wide Unison * Long Dampers * Tight Dampers

The names are looked up as strings. The ORDER matters too: the plugin's integer
`preset` parameter indexes this vector, and that integer is what a host's
automation lane and every generated `.vstpreset` store. Reordering the bank
silently repoints saved projects at a different piano.

Append only.

Test: `piano`, `the_factory_bank_is_a_wire_format`.

## The master chain a patch carries

    slots   parametric eq, glue compression, room, width

Every factory preset carries its own settings for those four, inside its patch:
the rooms differ, and so do the shelf and the glue where the microphones move.
What no preset and no user can change is WHICH effects run and in what order.
That is what curated means here, and it is why the order is listed above.

`PianoPatch::default()` is the first preset, name and values, and it carries
that preset's chain: a fresh instance plays what its window says. The field's
serde default is a different thing, and the difference is the whole
compatibility story: a patch written before `fx` existed deserialises to an
EMPTY chain, and an empty chain is a real no-op -- a `Chain` with zero slots
returns its input untouched. A project saved before this existed keeps
sounding as it did.

The chain is a `phonix_fx::ChainSpec`: every slot names its kind and its
parameters by string id (`"parametric-eq"`, `"band.0.gain"`, `"reverb"`,
`"type": "room"`), and the slot's mix is the slot's. Those ids are the wire
format. A patch written while the chain was an effect ordinal and `(pid,
value)` pairs is read by `phonix_legacy` and renamed on the way in; the next
save carries names. What the recipe writes for each kind, and the ranges and
units of every parameter, live with the effect in `phonix_fx::effects`.

Tests: `piano`, `every_factory_preset_carries_its_own_chain`,
`the_default_patch_is_the_first_preset_chain_included`,
`a_patch_written_before_fx_existed_has_no_chain`,
`a_patch_written_with_the_old_chain_opens_named` and
`the_recipe_names_only_what_the_build_has`; `piano-plugin`,
`an_empty_chain_is_bit_identical` and `a_fresh_instance_carries_the_first_preset_chain`;
`piano-ui`, `the_engine_mirror_does_not_erase_an_fx_edit`.

## The `.vstpreset` byte layout

48-byte header, `Comp`/`Info`/`List` chunk ids, uppercase-hex class id, and the
`MetaInfo` XML shape. Third-party hosts parse these.

On Windows the preset directory is
`%USERPROFILE%\Documents\VST3 Presets\<vendor>\<plugin>`, because that is where
MediaBay looks. That path is Windows-only: a backslash path on Linux is not
absolute but a single directory name, so a bank would be written into a junk
folder in the current directory. The Linux and macOS
branches were fixed; the Windows one was not touched, so nothing already indexed
is orphaned.

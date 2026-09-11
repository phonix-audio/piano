# What this is built on

Piano is an implementation of published work, not an invention. Every part of
the model comes from somewhere, and this file says where, so a reader can check
the code against the paper it claims to follow.

The citations are not decoration. The source quotes equation numbers, table
rows and figure panels, and several tests assert that a computed value falls
inside a published bound; `Chaigne 2016 eq. (18)`, `Chabassier Table III`,
`Chaigne & Askenfelt Fig. 17`. Where the implementation departs from a paper,
the comment at that line says so and why; those departures are collected in
LIMITATIONS.md.

## The papers

**Humbert, T.**, modal formulation of the piano string and a real-time contact
scheme (IRCAM / ATIAM, 2002). The reason a string here is a bank of modes
rather than a delay line: inharmonicity, the stiffness term and the two decay
rates fall out of the formulation instead of being dialled in afterwards.
`string.rs`, `modal_bank.rs`.

**Chabassier, J.**, *Modélisation et simulation numérique d'un piano par
modèles physiques* (PhD, 2012), and **Chabassier, Joly & Chaigne**, JASA 134(1)
2013. The string, the felt law and the measured stringing of a Steinway D:
Table III is the source of the string lengths, diameters and tensions, and of
the hammer masses. The most-cited work here; `string.rs`, `hammer.rs`,
`scale.rs`, `soundboard.rs`, `voice.rs`.

**Chaigne, A. & Askenfelt, A.**, *Numerical simulations of piano strings*,
JASA 95(2) 1994. The hammer felt as a nonlinear compressed spring, and the wave
packet the key sends into the string. `hammer.rs`, `mechanics.rs`.

**Chaigne, A.** (2016); contact duration against striking velocity across five
keyboards including a Steinway D. Equation (18) and Table I are what the
hammer's contact times are tested against, and the envelope at note 105 is
where the treble deficit was first measured. `hammer.rs`, `scale.rs`.

**Ege, K.** (2009, 2013); the soundboard's measured modes and their damping,
including its damping floor. `soundboard.rs`, `modal_bank.rs`,
`chord_attack.rs`.

**Boutillon, X. & Rébillat, M.**, *Vibroacoustics of the piano soundboard*,
JSV 2013. How the board radiates, and the frequency above which it stops
behaving as a single plate. `soundboard.rs`.

**Weinreich, G.**, coupled piano strings and the double decay. Why a unison
does not decay as one string does, and why the aftersound outlasts the prompt
sound. `soundboard.rs`, `voice.rs`.

**Stulov, A.**, hereditary felt: a hammer that has just struck is not the
hammer that struck a second ago. `hammer.rs`.

**Conklin, H. A.**, piano scaling and design, in three parts (JASA 1996). The
plan of the strings from bass bridge to treble. `scale.rs`, `string.rs`,
`soundboard.rs`.

**Giordano, N.**, soundboard impedance and the hammer-string interaction.
`hammer.rs`, `soundboard.rs`, `voice.rs`.

**Suzuki, H.**, soundboard impedance and radiation.
`soundboard.rs`.

**Fletcher, N. H. & Rossing, T. D.**, *The Physics of Musical Instruments*,
the standard reference behind the parts nobody writes a paper about.

## Reading the code against them

Start at `crates/piano/src/lib.rs`, which names the chain in
order. Each module
opens with what it implements and whose formulation it follows. Constants that
came from a table say which table; the ones that were set by ear say that
instead, and that distinction is what LIMITATIONS.md is built on.

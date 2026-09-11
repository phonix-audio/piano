//! What fraction of the plate is actually HEARD.
//!
//! In the decoupled live path the plate is a pure output filter: the strings
//! push it and never read it back, so a mode that reaches neither ear has no
//! effect on anything at all. This counts how many of the 3618 modes carry
//! the radiated energy — the question behind "what can be precomputed at
//! preset load to make the playing cheaper".
fn main() {
    let board = piano::soundboard::Soundboard::new(48_000.0, 0.7);
    let w = board.heard_weights();
    let total: f64 = w.iter().map(|x| x * x).sum();
    let mut e: Vec<f64> = w.iter().map(|x| x * x).collect();
    e.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let mut acc = 0.0;
    let (mut n90, mut n99, mut n999, mut n9999) = (0, 0, 0, 0);
    for (i, v) in e.iter().enumerate() {
        acc += v;
        if n90 == 0 && acc >= 0.90 * total { n90 = i + 1; }
        if n99 == 0 && acc >= 0.99 * total { n99 = i + 1; }
        if n999 == 0 && acc >= 0.999 * total { n999 = i + 1; }
        if n9999 == 0 && acc >= 0.9999 * total { n9999 = i + 1; }
    }
    println!("modes de la table : {}", w.len());
    println!("  90%   de ce qui est rayonne : {n90} modes");
    println!("  99%                          : {n99}");
    println!("  99.9%                        : {n999}");
    println!("  99.99%                       : {n9999}");

    // And the same question weighted by what a NOTE actually drives: the
    // product of the note's attachment weight and the mode's ear weight is
    // the mode's contribution to what that note sounds like.
    println!("\npar note (attache x oreille, 99.9% du rayonne) :");
    for note in [28u8, 40, 52, 64, 76, 88, 100] {
        let a = board.attachment(note);
        let mut e: Vec<f64> = a.iter().zip(w.iter()).map(|(x, y)| (x * y) * (x * y)).collect();
        let tot: f64 = e.iter().sum();
        e.sort_by(|p, q| q.partial_cmp(p).unwrap());
        let mut acc = 0.0;
        let mut n = 0;
        for (i, v) in e.iter().enumerate() {
            acc += v;
            if acc >= 0.999 * tot { n = i + 1; break; }
        }
        println!("  note {note:3} : {n:4} modes sur {}", a.len());
    }
}

//! Radiated magnitude response of the FULL soundboard, driven at a bridge point,
//! log-spaced 100 Hz..16 kHz. This is the target the reduced-order HF section
//! must reproduce above the transition.
use piano::soundboard::Soundboard;
fn main() {
    piano::denormal::enable_flush_to_zero();
    let sr = 48_000.0f32;
    let mut b = Soundboard::new(sr, 0.25);
    let n = 1 << 16;
    let mut rad = vec![0.0f64; n];
    for i in 0..n {
        let f = if i == 0 { 1.0 } else { 0.0 };
        let (l, r, _d) = b.drive_and_process(f);
        rad[i] = 0.5 * (l + r);
    }
    let pi = std::f64::consts::PI;
    let dft = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in rad.iter().enumerate() {
            let w = 2.0 * pi * f * i as f64 / sr as f64;
            re += x * w.cos(); im -= x * w.sin();
        }
        20.0 * ((re * re + im * im).sqrt() + 1e-12).log10()
    };
    // reference at 1 kHz, print relative dB so the shape is clear
    let refdb = dft(1000.0);
    println!("freq(Hz)  radiated(dB rel 1kHz)");
    let mut f = 100.0f64;
    while f <= 16000.0 {
        println!("{f:7.0}   {:+7.1}", dft(f) - refdb);
        f *= 1.15;
    }
}

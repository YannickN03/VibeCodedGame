//! Procedural in-memory WAV synthesis (16-bit mono PCM, 44-byte RIFF header).
//!
//! Everything is tuned to the C-minor pentatonic scale. Harmony grows with
//! the flow tier: tier 0 = root, 1 = + sub-octave, 2 = + fifth,
//! 3 = + arpeggiated chord. This module is pure (no audio device needed).

use std::f32::consts::TAU;

use crate::core::rng::SplitMix64;

/// Output sample rate (matches the mixer, so no resampling happens).
pub const SAMPLE_RATE: u32 = 44_100;

/// C-minor pentatonic: C4, Eb4, F4, G4, Bb4, C5, Eb5, G5.
pub const SCALE: [f32; 8] = [
    261.63, 311.13, 349.23, 392.00, 466.16, 523.25, 622.25, 783.99,
];

/// Number of flow tiers we synthesize harmony layers for.
pub const TIERS: usize = 4;

/// Encodes `[-1, 1]` float samples as a complete WAV file.
pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Parsed header facts of a WAV produced by [`encode_wav`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub samples: usize,
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Strict validation of a canonical 44-byte-header PCM WAV.
pub fn validate_wav(b: &[u8]) -> Result<WavInfo, &'static str> {
    if b.len() < 44 {
        return Err("too short");
    }
    if &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not RIFF/WAVE");
    }
    if u32_at(b, 4) as usize != b.len() - 8 {
        return Err("bad RIFF size");
    }
    if &b[12..16] != b"fmt " || u32_at(b, 16) != 16 {
        return Err("bad fmt chunk");
    }
    if u16_at(b, 20) != 1 {
        return Err("not PCM");
    }
    let channels = u16_at(b, 22);
    let sample_rate = u32_at(b, 24);
    let byte_rate = u32_at(b, 28);
    let block_align = u16_at(b, 32);
    let bits = u16_at(b, 34);
    if channels != 1 || bits != 16 || block_align != 2 || byte_rate != sample_rate * 2 {
        return Err("unexpected format");
    }
    if &b[36..40] != b"data" || u32_at(b, 40) as usize != b.len() - 44 {
        return Err("bad data chunk");
    }
    Ok(WavInfo {
        sample_rate,
        channels,
        bits,
        samples: (b.len() - 44) / 2,
    })
}

// ---------------------------------------------------------------- building blocks

fn buffer(seconds: f32) -> Vec<f32> {
    vec![0.0; (seconds * SAMPLE_RATE as f32) as usize]
}

/// Attack/exponential-decay envelope.
fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        t / attack
    } else {
        (-(t - attack) * decay).exp()
    }
}

/// Warm plucked voice (sine + decaying overtones), mixed into `buf`.
fn add_pluck(buf: &mut [f32], start: f32, freq: f32, amp: f32, decay: f32) {
    let sr = SAMPLE_RATE as f32;
    let first = (start * sr) as usize;
    for (i, s) in buf.iter_mut().enumerate().skip(first) {
        let t = (i - first) as f32 / sr;
        let e = env(t, 0.004, decay);
        if e < 1e-4 && t > 0.01 {
            break;
        }
        let ph = TAU * freq * t;
        let v = ph.sin() + 0.35 * (2.0 * ph).sin() * (-t * 9.0).exp() + 0.12 * (3.0 * ph).sin();
        *s += amp * e * v;
    }
}

/// Soft triangle (sub-octave layer).
fn add_triangle(buf: &mut [f32], start: f32, freq: f32, amp: f32, decay: f32) {
    let sr = SAMPLE_RATE as f32;
    let first = (start * sr) as usize;
    for (i, s) in buf.iter_mut().enumerate().skip(first) {
        let t = (i - first) as f32 / sr;
        let p = (freq * t).fract();
        let tri = 4.0 * (p - 0.5).abs() - 1.0;
        *s += amp * env(t, 0.01, decay) * tri;
    }
}

/// Sine with exponential pitch glide `f0 -> f1`.
fn add_glide(buf: &mut [f32], f0: f32, f1: f32, amp: f32, decay: f32) {
    let sr = SAMPLE_RATE as f32;
    let len = buf.len() as f32 / sr;
    let mut phase = 0.0f32;
    for (i, s) in buf.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let f = f0 * (f1 / f0).powf((t / len).min(1.0));
        phase += TAU * f / sr;
        *s += amp * env(t, 0.002, decay) * phase.sin();
    }
}

fn add_noise(buf: &mut [f32], amp: f32, decay: f32, seed: u64) {
    let sr = SAMPLE_RATE as f32;
    let mut rng = SplitMix64::new(seed);
    let mut lp = 0.0;
    for (i, s) in buf.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let n = rng.next_f32() * 2.0 - 1.0;
        lp += (n - lp) * 0.25; // gentle low-pass
        *s += amp * env(t, 0.001, decay) * lp;
    }
}

/// Normalizes to `peak` and soft-clips; also adds 4 ms fade-out (no clicks).
fn finish(mut buf: Vec<f32>, peak: f32) -> Vec<f32> {
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        let g = peak / max;
        for s in buf.iter_mut() {
            *s = (*s * g).tanh();
        }
    }
    let fade = (0.004 * SAMPLE_RATE as f32) as usize;
    let n = buf.len();
    for i in 0..fade.min(n) {
        buf[n - 1 - i] *= i as f32 / fade as f32;
    }
    buf
}

// ---------------------------------------------------------------- instruments

/// Scale note `idx` (0..8) with the harmony layers of `tier` (0..4).
pub fn tier_note(idx: usize, tier: usize) -> Vec<f32> {
    let f = SCALE[idx % SCALE.len()];
    let mut b = buffer(0.55);
    add_pluck(&mut b, 0.0, f, 1.0, 7.0);
    if tier >= 1 {
        add_triangle(&mut b, 0.0, f * 0.5, 0.45, 5.0);
    }
    if tier >= 2 {
        add_pluck(&mut b, 0.0, f * 1.5, 0.45, 8.0);
    }
    if tier >= 3 {
        // Arpeggiated minor chord on top: minor third, fifth, octave.
        for (k, ratio) in [1.2f32, 1.5, 2.0].iter().enumerate() {
            add_pluck(&mut b, 0.055 * (k + 1) as f32, f * ratio, 0.38, 9.0);
        }
    }
    finish(b, 0.85)
}

fn arpeggio(notes: &[f32], step: f32, len: f32, decay: f32) -> Vec<f32> {
    let mut b = buffer(len);
    for (k, &f) in notes.iter().enumerate() {
        add_pluck(&mut b, step * k as f32, f, 0.8, decay);
    }
    finish(b, 0.8)
}

/// All sounds of the game, as `(name, wav bytes)`.
pub fn build_bank() -> Vec<(String, Vec<u8>)> {
    let mut bank: Vec<(String, Vec<u8>)> = Vec::new();
    for tier in 0..TIERS {
        for idx in 0..SCALE.len() {
            bank.push((note_name(tier, idx), encode_wav(&tier_note(idx, tier))));
        }
    }

    let mut push =
        |name: &str, samples: Vec<f32>| bank.push((name.to_string(), encode_wav(&samples)));

    let mut mv = buffer(0.035);
    add_pluck(&mut mv, 0.0, SCALE[7] * 2.0, 1.0, 120.0);
    push("move", finish(mv, 0.35));

    let mut rot = buffer(0.08);
    add_glide(&mut rot, SCALE[5], SCALE[7], 1.0, 45.0);
    push("rotate", finish(rot, 0.45));

    let mut hold = buffer(0.16);
    add_pluck(&mut hold, 0.0, SCALE[6], 0.8, 22.0);
    add_pluck(&mut hold, 0.05, SCALE[5], 0.8, 22.0);
    push("hold", finish(hold, 0.5));

    let mut hd = buffer(0.22);
    add_glide(&mut hd, 110.0, 42.0, 1.0, 16.0);
    add_noise(&mut hd, 0.6, 30.0, 7);
    push("harddrop", finish(hd, 0.8));

    let mut lock = buffer(0.09);
    add_glide(&mut lock, 190.0, 120.0, 1.0, 40.0);
    push("lock", finish(lock, 0.5));

    let mut sparkle = buffer(0.5);
    for (k, f) in [SCALE[5], SCALE[6], SCALE[7], SCALE[7] * 2.0]
        .iter()
        .enumerate()
    {
        add_pluck(&mut sparkle, 0.04 * k as f32, *f, 0.6, 10.0);
    }
    push("sparkle", finish(sparkle, 0.6));

    push(
        "levelup",
        arpeggio(
            &[SCALE[0], SCALE[1], SCALE[3], SCALE[5], SCALE[6], SCALE[7]],
            0.07,
            0.9,
            6.0,
        ),
    );
    let mut od = arpeggio(
        &[
            SCALE[3],
            SCALE[5],
            SCALE[6],
            SCALE[7],
            SCALE[7] * 1.5,
            SCALE[7] * 2.0,
        ],
        0.05,
        1.1,
        4.0,
    );
    let mut sweep = buffer(1.1);
    add_glide(&mut sweep, 130.0, 1046.5, 0.5, 2.5);
    for (o, s) in od.iter_mut().zip(sweep) {
        *o += s * 0.4;
    }
    push("overdrive", finish(od, 0.85));
    push(
        "gameover",
        arpeggio(
            &[SCALE[4], SCALE[3], SCALE[1], SCALE[0], SCALE[0] * 0.5],
            0.16,
            1.4,
            3.5,
        ),
    );
    push("finish", arpeggio(&SCALE, 0.06, 1.2, 4.0));
    push("select", arpeggio(&[SCALE[5]], 0.0, 0.08, 40.0));
    push(
        "zen",
        arpeggio(&[SCALE[0] * 0.5, SCALE[3] * 0.5, SCALE[0]], 0.2, 1.4, 2.5),
    );
    bank
}

/// Name of a tier/scale note sound in the bank.
pub fn note_name(tier: usize, idx: usize) -> String {
    format!("note_t{}_{}", tier.min(TIERS - 1), idx % SCALE.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riff_header_is_canonical() {
        let wav = encode_wav(&[0.0, 0.5, -0.5, 1.0]);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32_at(&wav, 4), 44);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16_at(&wav, 20), 1);
        assert_eq!(u16_at(&wav, 22), 1);
        assert_eq!(u32_at(&wav, 24), 44_100);
        assert_eq!(u16_at(&wav, 34), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32_at(&wav, 40), 8);
        let info = validate_wav(&wav).unwrap();
        assert_eq!(info.samples, 4);
        assert_eq!(i16::from_le_bytes([wav[50], wav[51]]), i16::MAX);
    }

    #[test]
    fn bank_is_complete_and_valid() {
        let bank = build_bank();
        assert!(bank.len() >= 32 + 10);
        for (name, wav) in &bank {
            let info = validate_wav(wav).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(info.samples > 100, "{name} too short");
        }
        assert!(bank.iter().any(|(n, _)| n == &note_name(3, 7)));
    }

    #[test]
    fn higher_tiers_add_harmony() {
        // More layers => more spectral energy away from the root: compare
        // zero-crossing counts (higher partials cross zero more often).
        let zc = |v: &[f32]| {
            v.windows(2)
                .filter(|w| w[0].signum() != w[1].signum())
                .count()
        };
        let t0 = tier_note(0, 0);
        let t3 = tier_note(0, 3);
        assert_ne!(t0, t3);
        assert!(zc(&t3) != zc(&t0));
    }

    #[test]
    fn pentatonic_scale_is_c_minor() {
        // Ratios to C4 match equal-tempered semitones 0,3,5,7,10,12,15,19.
        let semis = [0, 3, 5, 7, 10, 12, 15, 19];
        for (f, s) in SCALE.iter().zip(semis) {
            let expect = 261.63 * 2f32.powf(s as f32 / 12.0);
            assert!((f - expect).abs() < 0.5, "{f} vs {expect}");
        }
    }

    #[test]
    fn samples_never_clip() {
        for (_, wav) in build_bank() {
            let info = validate_wav(&wav).unwrap();
            assert_eq!(info.sample_rate, SAMPLE_RATE);
        }
        let n = tier_note(7, 3);
        assert!(n.iter().all(|s| s.abs() <= 1.0));
    }
}

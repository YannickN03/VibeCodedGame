//! Procedural, seamlessly looping background music (pure, no audio device).
//!
//! One calm track for the menu and five game stages. Each stage is faster and
//! denser than the one before: tempo rises from 112 to 158 BPM and drums,
//! sidechain pumping, 16th-note arpeggios and a lead melody join in. Every
//! loop is exactly [`BARS`] bars long and voices that ring past the loop end
//! wrap around to its start, so playback loops without clicks.

use std::f32::consts::TAU;

use crate::core::GameMode;
use crate::core::rng::SplitMix64;

use super::synth::{SAMPLE_RATE, encode_wav};

/// Number of game stages (intensity levels).
pub const GAME_STAGES: usize = 5;
/// Length of every loop in 4/4 bars.
pub const BARS: usize = 8;

/// Tempo of each game stage.
const STAGE_BPM: [f32; GAME_STAGES] = [112.0, 122.0, 132.0, 144.0, 158.0];
const MENU_BPM: f32 = 84.0;

/// A background music loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Track {
    Menu,
    /// Game stage `0..GAME_STAGES`; higher = faster and more intense.
    Game(usize),
}

impl Track {
    /// All tracks in synthesis order (menu first: it is needed immediately).
    pub fn all() -> Vec<Track> {
        let mut v = vec![Track::Menu];
        v.extend((0..GAME_STAGES).map(Track::Game));
        v
    }

    pub fn bpm(self) -> f32 {
        match self {
            Track::Menu => MENU_BPM,
            Track::Game(s) => STAGE_BPM[s.min(GAME_STAGES - 1)],
        }
    }
}

/// Music stage for the current game situation.
///
/// Levelling modes follow the gravity level (the game gets faster, so does
/// the music). Sprint has constant gravity, so it follows the race progress
/// and goes all-out on the final stretch. Zen stays relaxed.
pub fn stage_for(mode: GameMode, level: u32, lines: u32) -> usize {
    let stage = match mode {
        GameMode::Zen => 0,
        GameMode::Sprint => match lines {
            0..10 => 0,
            10..20 => 1,
            20..30 => 2,
            30..35 => 3,
            _ => 4,
        },
        GameMode::Marathon | GameMode::DailyVibe => match level {
            0..=2 => 0,
            3..=5 => 1,
            6..=9 => 2,
            10..=14 => 3,
            _ => 4,
        },
    };
    stage.min(GAME_STAGES - 1)
}

// ---------------------------------------------------------------- loop buffer

fn midi(m: i32) -> f32 {
    440.0 * 2f32.powf((m - 69) as f32 / 12.0)
}

/// A loop-length buffer; everything written past the end wraps to the start.
struct Loop {
    buf: Vec<f32>,
}

impl Loop {
    fn new(bpm: f32) -> Loop {
        let secs = BARS as f32 * 4.0 * 60.0 / bpm;
        Loop {
            buf: vec![0.0; (secs * SAMPLE_RATE as f32).round() as usize],
        }
    }

    /// Mixes `voice(t)` for `t` in `0..dur` seconds starting at `start`.
    fn add(&mut self, start: f32, dur: f32, mut voice: impl FnMut(f32) -> f32) {
        let sr = SAMPLE_RATE as f32;
        let n = self.buf.len();
        let first = (start * sr).round() as usize;
        let count = ((dur * sr) as usize).min(n);
        for k in 0..count {
            self.buf[(first + k) % n] += voice(k as f32 / sr);
        }
    }
}

// ---------------------------------------------------------------- voices

fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) * decay).exp()
    }
}

/// Soft detuned pad: attack, sustain for `hold` seconds, then release.
fn pad(freq: f32, hold: f32, amp: f32) -> impl FnMut(f32) -> f32 {
    move |t| {
        let e = if t < 0.3 {
            t / 0.3
        } else if t < hold {
            1.0
        } else {
            (-(t - hold) * 6.0).exp()
        };
        let mut v = 0.0;
        for detune in [0.997f32, 1.003] {
            let ph = TAU * freq * detune * t;
            // Band-limited, darkened saw: harmonics fall off steeply.
            for k in 1..=4 {
                let kf = k as f32;
                v += (kf * ph).sin() / (kf * kf);
            }
        }
        amp * e * v * 0.5
    }
}

/// Plucked voice with decaying overtones.
fn pluck(freq: f32, amp: f32, decay: f32, bright: f32) -> impl FnMut(f32) -> f32 {
    move |t| {
        let ph = TAU * freq * t;
        let v = ph.sin()
            + bright * 0.5 * (2.0 * ph).sin() * (-t * 8.0).exp()
            + bright * 0.25 * (3.0 * ph).sin() * (-t * 12.0).exp();
        amp * env(t, 0.003, decay) * v
    }
}

/// Square-ish lead (odd harmonics) with a little vibrato.
fn lead(freq: f32, amp: f32, decay: f32) -> impl FnMut(f32) -> f32 {
    move |t| {
        let vib = 1.0 + 0.004 * (TAU * 5.5 * t).sin() * (t * 3.0).min(1.0);
        let ph = TAU * freq * vib * t;
        let v = ph.sin() + (3.0 * ph).sin() / 3.0 * 0.6 + (5.0 * ph).sin() / 5.0 * 0.3;
        amp * env(t, 0.008, decay) * v
    }
}

/// Bass: sub sine plus a darkened saw.
fn bass(freq: f32, amp: f32, decay: f32, grit: f32) -> impl FnMut(f32) -> f32 {
    move |t| {
        let ph = TAU * freq * t;
        let mut saw = 0.0;
        for k in 1..=6 {
            let kf = k as f32;
            saw += (kf * ph).sin() / kf.powf(1.6);
        }
        let e = env(t, 0.006, decay);
        amp * e * (0.8 * ph.sin() + grit * saw)
    }
}

fn kick(amp: f32) -> impl FnMut(f32) -> f32 {
    let mut phase = 0.0f32;
    move |t| {
        let f = 45.0 + 115.0 * (-t * 30.0).exp();
        phase += TAU * f / SAMPLE_RATE as f32;
        amp * (-t * 9.0).exp() * phase.sin()
    }
}

/// High-passed noise burst (`decay` small = open hat, large = closed hat).
fn hat(amp: f32, decay: f32, seed: u64) -> impl FnMut(f32) -> f32 {
    let mut rng = SplitMix64::new(seed);
    let mut lp = 0.0f32;
    move |t| {
        let n = rng.next_f32() * 2.0 - 1.0;
        lp += (n - lp) * 0.35;
        amp * (-t * decay).exp() * (n - lp)
    }
}

fn snare(amp: f32, seed: u64) -> impl FnMut(f32) -> f32 {
    let mut rng = SplitMix64::new(seed);
    let mut lp = 0.0f32;
    move |t| {
        let n = rng.next_f32() * 2.0 - 1.0;
        lp += (n - lp) * 0.5;
        let tone = (TAU * 185.0 * t).sin() * (-t * 22.0).exp();
        amp * (0.6 * lp * (-t * 15.0).exp() + 0.5 * tone)
    }
}

// ---------------------------------------------------------------- arrangement

struct Chord {
    bass: i32,
    tones: [i32; 3],
}

const fn chord(bass: i32, tones: [i32; 3]) -> Chord {
    Chord { bass, tones }
}

/// i – VI – III – VII in C minor (Cm, Ab, Eb, Bb).
const GAME_PROG: [Chord; 4] = [
    chord(36, [60, 63, 67]),
    chord(32, [60, 63, 68]),
    chord(39, [58, 63, 67]),
    chord(34, [58, 62, 65]),
];

/// Dreamier menu progression (Ab, Fm, Cm, Bb).
const MENU_PROG: [Chord; 4] = [
    chord(44, [60, 63, 68]),
    chord(41, [60, 65, 68]),
    chord(48, [60, 63, 67]),
    chord(46, [58, 62, 65]),
];

/// Lead melody: 8 eighth notes per bar, `0` = rest. Bars 4..8 repeat with a
/// rising variation at the end of the loop.
const MELODY: [[i32; 8]; BARS] = [
    [72, 0, 75, 0, 79, 77, 75, 0],
    [72, 0, 0, 75, 72, 0, 68, 0],
    [70, 0, 75, 0, 79, 0, 82, 79],
    [77, 0, 74, 0, 70, 0, 72, 0],
    [72, 0, 75, 0, 79, 77, 75, 0],
    [72, 0, 0, 75, 72, 0, 68, 0],
    [70, 0, 75, 0, 79, 0, 82, 79],
    [77, 79, 82, 0, 79, 77, 74, 0],
];

/// Arpeggio pattern over the chord tones (3 = root an octave up).
const ARP: [usize; 8] = [0, 1, 2, 3, 2, 1, 2, 3];

fn arp_note(c: &Chord, step: usize) -> i32 {
    let i = ARP[step % ARP.len()];
    if i == 3 { c.tones[0] + 12 } else { c.tones[i] }
}

/// Normalizes and soft-clips. No fade: the loop seam must stay continuous.
fn master(mut buf: Vec<f32>) -> Vec<f32> {
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        let g = 1.1 / max;
        for s in buf.iter_mut() {
            *s = (*s * g).tanh();
        }
    }
    buf
}

fn render_menu() -> Vec<f32> {
    let bpm = MENU_BPM;
    let beat = 60.0 / bpm;
    let bar = 4.0 * beat;
    let mut out = Loop::new(bpm);
    let mut rng = SplitMix64::new(0x00B1_0CC5);
    // Pentatonic bell notes high above the pad.
    let bells = [72, 75, 77, 79, 82, 84];

    for b in 0..BARS {
        let c = &MENU_PROG[b % 4];
        let t0 = b as f32 * bar;
        for &m in &c.tones {
            out.add(t0, bar + 0.8, pad(midi(m), bar, 0.22));
        }
        out.add(t0, bar + 0.5, bass(midi(c.bass - 12), 0.35, 0.9, 0.15));
        for step in 0..8 {
            let m = arp_note(c, step) + 12;
            out.add(
                t0 + step as f32 * beat * 0.5,
                1.6,
                pluck(midi(m), 0.07, 3.0, 0.4),
            );
        }
        // Sparse bells: a note on beat 1, sometimes another on beat 3.
        let n = bells[rng.next_u64() as usize % bells.len()];
        out.add(t0, 2.5, pluck(midi(n), 0.12, 1.8, 0.8));
        if rng.next_u64().is_multiple_of(2) {
            let n = bells[rng.next_u64() as usize % bells.len()];
            out.add(t0 + 2.0 * beat, 2.5, pluck(midi(n), 0.09, 1.8, 0.8));
        }
    }
    master(out.buf)
}

fn render_stage(stage: usize) -> Vec<f32> {
    let s = stage.min(GAME_STAGES - 1);
    let bpm = STAGE_BPM[s];
    let beat = 60.0 / bpm;
    let bar = 4.0 * beat;
    let eighth = beat / 2.0;
    let sixteenth = beat / 4.0;
    let mut drums = Loop::new(bpm);
    let mut music = Loop::new(bpm);
    let mut seed = 1u64;
    let mut next_seed = || {
        seed += 1;
        seed
    };

    for b in 0..BARS {
        let c = &GAME_PROG[b % 4];
        let t0 = b as f32 * bar;
        let last_bar = b == BARS - 1;

        // ---- harmony
        let pad_amp = [0.20, 0.17, 0.15, 0.13, 0.12][s];
        for &m in &c.tones {
            music.add(t0, bar + 0.4, pad(midi(m), bar - 0.05, pad_amp));
        }

        // ---- bass
        let root = midi(c.bass);
        match s {
            0 => {
                for half in 0..2 {
                    music.add(
                        t0 + half as f32 * 2.0 * beat,
                        2.0 * beat,
                        bass(root, 0.5, 2.5, 0.3),
                    );
                }
            }
            1 | 2 => {
                music.add(t0, eighth, bass(root, 0.5, 6.0, 0.45));
                for i in 0..4 {
                    let t = t0 + (2 * i + 1) as f32 * eighth;
                    music.add(t, eighth, bass(root, 0.5, 7.0, 0.45));
                }
            }
            _ => {
                for i in 0..8 {
                    let f = if i % 2 == 0 { root } else { root * 2.0 };
                    let grit = if s == 4 { 0.8 } else { 0.6 };
                    music.add(t0 + i as f32 * eighth, eighth, bass(f, 0.48, 9.0, grit));
                }
            }
        }

        // ---- arpeggio
        let (steps, step_len, arp_amp) = if s >= 2 {
            (16, sixteenth, 0.10)
        } else {
            (8, eighth, 0.11)
        };
        for i in 0..steps {
            let m = arp_note(c, i) + if s >= 3 && i % 4 == 3 { 24 } else { 12 };
            let bright = 0.4 + 0.15 * s as f32;
            music.add(
                t0 + i as f32 * step_len,
                0.5,
                pluck(midi(m), arp_amp, 9.0, bright),
            );
        }

        // ---- lead melody
        if s >= 3 {
            for (i, &m) in MELODY[b].iter().enumerate() {
                if m == 0 {
                    continue;
                }
                let t = t0 + i as f32 * eighth;
                music.add(t, 0.8, lead(midi(m), 0.13, 5.0));
                if s == 4 {
                    music.add(t, 0.6, lead(midi(m + 12), 0.05, 7.0));
                }
            }
        }

        // ---- drums
        for q in 0..4 {
            let tq = t0 + q as f32 * beat;
            if s >= 1 || q % 2 == 0 {
                drums.add(tq, 0.4, kick(0.9));
            }
            if s >= 1 && q % 2 == 1 && !(s == 4 && last_bar && q == 3) {
                let amp = if s >= 3 { 0.6 } else { 0.45 };
                drums.add(tq, 0.3, snare(amp, next_seed()));
            }
            if s == 4 && q == 2 {
                drums.add(tq + 3.0 * sixteenth, 0.3, kick(0.6));
            }
            // Hats: offbeat 8ths, then 16ths; open hats on the offbeat at stage 3+.
            if s >= 2 {
                for k in 0..4 {
                    let t = tq + k as f32 * sixteenth;
                    if k == 2 && s >= 3 {
                        drums.add(t, 0.3, hat(0.22, 12.0, next_seed()));
                    } else {
                        let amp = if k == 2 { 0.2 } else { 0.1 };
                        drums.add(t, 0.08, hat(amp, 60.0, next_seed()));
                    }
                }
            } else {
                drums.add(tq + eighth, 0.08, hat(0.16, 55.0, next_seed()));
            }
        }
        // Snare roll into the loop restart at the top stage.
        if s == 4 && last_bar {
            for k in 0..4 {
                let t = t0 + 3.0 * beat + k as f32 * sixteenth;
                drums.add(t, 0.2, snare(0.3 + 0.1 * k as f32, next_seed()));
            }
        }
    }

    // Sidechain pump: harmony ducks under every kick from stage 1 on.
    let depth = [0.0, 0.25, 0.35, 0.45, 0.5][s];
    if depth > 0.0 {
        let sr = SAMPLE_RATE as f32;
        for (i, v) in music.buf.iter_mut().enumerate() {
            let tb = (i as f32 / sr) % beat;
            *v *= 1.0 - depth * (-tb * 9.0).exp();
        }
    }

    let mix = drums
        .buf
        .iter()
        .zip(&music.buf)
        .map(|(d, m)| d + m)
        .collect();
    master(mix)
}

/// Raw samples of `track`, exactly [`BARS`] bars long.
pub fn render(track: Track) -> Vec<f32> {
    match track {
        Track::Menu => render_menu(),
        Track::Game(s) => render_stage(s),
    }
}

/// `track` encoded as a WAV file.
pub fn render_wav(track: Track) -> Vec<u8> {
    encode_wav(&render(track))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::synth::validate_wav;

    #[test]
    fn tempo_rises_with_stage() {
        for s in 1..GAME_STAGES {
            assert!(Track::Game(s).bpm() > Track::Game(s - 1).bpm());
        }
        assert!(Track::Menu.bpm() < Track::Game(0).bpm());
    }

    #[test]
    fn stage_follows_level_and_progress() {
        assert_eq!(stage_for(GameMode::Marathon, 1, 0), 0);
        assert_eq!(stage_for(GameMode::Marathon, 3, 25), 1);
        assert_eq!(stage_for(GameMode::DailyVibe, 7, 60), 2);
        assert_eq!(stage_for(GameMode::Marathon, 12, 115), 3);
        assert_eq!(stage_for(GameMode::Marathon, 40, 400), 4);
        assert_eq!(stage_for(GameMode::Sprint, 1, 0), 0);
        assert_eq!(stage_for(GameMode::Sprint, 1, 36), 4);
        assert_eq!(stage_for(GameMode::Zen, 1, 500), 0);
        // Monotonic in level.
        let mut prev = 0;
        for lvl in 1..30 {
            let s = stage_for(GameMode::Marathon, lvl, 0);
            assert!(s >= prev);
            prev = s;
        }
    }

    #[test]
    fn loops_are_exact_valid_and_seamless() {
        for track in Track::all() {
            let samples = render(track);
            let expect = BARS as f32 * 4.0 * 60.0 / track.bpm() * SAMPLE_RATE as f32;
            assert!(
                (samples.len() as f32 - expect).abs() <= 1.0,
                "{track:?} length"
            );
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.5, "{track:?} too quiet");
            // The seam must not jump more than an ordinary sample step does.
            let max_step = samples
                .windows(2)
                .fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
            let seam = (samples[0] - samples[samples.len() - 1]).abs();
            assert!(seam <= max_step + 1e-6, "{track:?} clicks at the seam");
            let info = validate_wav(&encode_wav(&samples)).unwrap();
            assert_eq!(info.samples, samples.len());
        }
    }
}

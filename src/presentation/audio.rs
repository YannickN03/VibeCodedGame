//! Audio playback of the in-memory synthesized bank, with a graceful silent
//! fallback when no audio device exists.
//!
//! macroquad's Windows backend opens the device on a background thread and
//! `assert!`s on failure. That panic only kills the audio thread; we detect it
//! through a panic hook and switch to silent mode instead of crashing.

use std::collections::HashMap;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::task::{Context, Poll, Waker};

use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume, stop_sound,
};

use super::music::{self, Track};
use super::synth;

/// Music level relative to the master volume (sound effects stay on top).
const MUSIC_GAIN: f32 = 0.42;
/// Seconds to fade a track in / out (tracks crossfade on stage changes).
const FADE_IN_SECS: f32 = 1.5;
const FADE_OUT_SECS: f32 = 2.0;
/// Music level while the game is paused.
const PAUSE_DUCK: f32 = 0.35;

static AUDIO_THREAD_FAILED: AtomicBool = AtomicBool::new(false);

/// Installs a panic hook that turns panics on non-main threads (the audio
/// device thread) into a "no audio" flag instead of noisy crashes.
pub fn install_audio_panic_guard() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() != Some("main") {
            AUDIO_THREAD_FAILED.store(true, Ordering::SeqCst);
            return;
        }
        default_hook(info);
    }));
}

/// Polls a future that is known to complete synchronously on desktop.
fn poll_once<F: Future>(fut: F) -> Option<F::Output> {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

/// Loaded sound bank.
pub struct Audio {
    sounds: HashMap<String, Sound>,
    pub muted: bool,
    /// `0.0..=1.0`
    pub volume: f32,
    load_failed: bool,
    music: Music,
}

impl Audio {
    /// Synthesizes and uploads all sounds. Never panics.
    pub fn load(muted: bool, volume: f32) -> Audio {
        let mut sounds = HashMap::new();
        let mut load_failed = false;
        for (name, wav) in synth::build_bank() {
            let result =
                panic::catch_unwind(AssertUnwindSafe(|| poll_once(load_sound_from_bytes(&wav))));
            match result {
                Ok(Some(Ok(sound))) => {
                    sounds.insert(name, sound);
                }
                _ => {
                    load_failed = true;
                    break;
                }
            }
        }
        if load_failed {
            sounds.clear();
        }
        let music = if load_failed {
            Music::default()
        } else {
            Music::spawn_synthesis()
        };
        Audio {
            sounds,
            muted,
            volume,
            load_failed,
            music,
        }
    }

    /// Selects the background track (`None` fades the music out) and whether
    /// it should be ducked (pause screen).
    pub fn set_music(&mut self, track: Option<Track>, ducked: bool) {
        self.music.target = track;
        self.music.ducked = ducked;
    }

    /// Advances crossfades and uploads freshly synthesized tracks.
    pub fn update_music(&mut self, dt: f32) {
        let available = self.available();
        let master = if self.muted { 0.0 } else { self.volume };
        self.music.update(dt, available, master);
    }

    /// `true` when sounds are loaded and the device thread is alive.
    pub fn available(&self) -> bool {
        !self.load_failed && !AUDIO_THREAD_FAILED.load(Ordering::SeqCst) && !self.sounds.is_empty()
    }

    /// Plays `name` at relative volume `gain`.
    pub fn play(&self, name: &str, gain: f32) {
        if self.muted || self.volume <= 0.0 || !self.available() {
            return;
        }
        if let Some(s) = self.sounds.get(name) {
            play_sound(
                s,
                PlaySoundParams {
                    looped: false,
                    volume: (gain * self.volume).clamp(0.0, 1.0),
                },
            );
        }
    }

    /// Plays a pentatonic note with the harmony layers of `tier`.
    pub fn note(&self, tier: usize, idx: usize, gain: f32) {
        self.play(&synth::note_name(tier, idx), gain);
    }

    /// Plays a cue reported by an arcade title.
    pub fn play_cue(&self, cue: &crate::arcade::AudioCue) {
        match cue {
            crate::arcade::AudioCue::Sound { name, gain } => self.play(name, *gain),
            crate::arcade::AudioCue::Note { tier, idx, gain } => self.note(*tier, *idx, *gain),
        }
    }

    pub fn status(&self) -> &'static str {
        if !self.available() {
            "NO DEVICE"
        } else if self.muted {
            "MUTED"
        } else {
            "ON"
        }
    }
}

/// One playing music loop and its fade level.
struct Channel {
    track: Track,
    fade: f32,
    applied: f32,
}

/// Background music: tracks are synthesized on a worker thread (so startup
/// stays instant), uploaded as they arrive and crossfaded on track changes.
#[derive(Default)]
struct Music {
    incoming: Option<Receiver<(Track, Vec<u8>)>>,
    tracks: HashMap<Track, Sound>,
    channels: Vec<Channel>,
    target: Option<Track>,
    ducked: bool,
    duck: f32,
}

impl Music {
    fn spawn_synthesis() -> Music {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("music-synth".into())
            .spawn(move || {
                for track in Track::all() {
                    if tx.send((track, music::render_wav(track))).is_err() {
                        break; // game closed
                    }
                }
            });
        Music {
            incoming: spawned.ok().map(|_| rx),
            duck: 1.0,
            ..Default::default()
        }
    }

    fn receive(&mut self) {
        let Some(rx) = &self.incoming else {
            return;
        };
        // At most one upload per frame keeps frame times smooth.
        match rx.try_recv() {
            Ok((track, wav)) => {
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    poll_once(load_sound_from_bytes(&wav))
                }));
                if let Ok(Some(Ok(sound))) = result {
                    self.tracks.insert(track, sound);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.incoming = None,
        }
    }

    fn update(&mut self, dt: f32, available: bool, master: f32) {
        self.receive();
        if !available {
            for ch in self.channels.drain(..) {
                if let Some(s) = self.tracks.get(&ch.track) {
                    stop_sound(s);
                }
            }
            return;
        }

        // Start the target loop silently; it fades in below.
        if let Some(target) = self.target
            && !self.channels.iter().any(|c| c.track == target)
            && let Some(sound) = self.tracks.get(&target)
        {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: true,
                    volume: 0.0,
                },
            );
            self.channels.push(Channel {
                track: target,
                fade: 0.0,
                applied: 0.0,
            });
        }

        let duck_goal = if self.ducked { PAUSE_DUCK } else { 1.0 };
        self.duck += (duck_goal - self.duck) * (dt * 6.0).min(1.0);

        let target = self.target;
        let duck = self.duck;
        let tracks = &self.tracks;
        self.channels.retain_mut(|ch| {
            let Some(sound) = tracks.get(&ch.track) else {
                return false;
            };
            if Some(ch.track) == target {
                ch.fade = (ch.fade + dt / FADE_IN_SECS).min(1.0);
            } else {
                ch.fade = (ch.fade - dt / FADE_OUT_SECS).max(0.0);
                if ch.fade <= 0.0 {
                    stop_sound(sound);
                    return false;
                }
            }
            // Equal-power curve: crossfades keep a constant loudness.
            let shaped = (ch.fade * std::f32::consts::FRAC_PI_2).sin();
            let vol = (shaped * duck * master * MUSIC_GAIN).clamp(0.0, 1.0);
            if (vol - ch.applied).abs() > 0.002 {
                set_sound_volume(sound, vol);
                ch.applied = vol;
            }
            true
        });
    }
}

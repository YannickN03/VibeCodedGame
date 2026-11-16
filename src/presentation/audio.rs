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
use std::task::{Context, Poll, Waker};

use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};

use super::synth;

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
        Audio {
            sounds,
            muted,
            volume,
            load_failed,
        }
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

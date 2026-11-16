//! Pure navigation state machine of the arcade:
//! `Menu -> Playing <-> Paused`, `Playing -> Ending -> Results -> Menu`.
//!
//! No rendering, no input devices: the controller feeds it events and the
//! headless smoke test drives the very same transitions.

/// Seconds the final board stays visible before the results overlay.
pub const END_DELAY: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Playing,
    Paused,
    /// Session over; the last frame stays visible for [`END_DELAY`].
    Ending,
    Results,
}

/// Which game and mode (indices into the registry / its mode list) run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub game: usize,
    pub mode: usize,
}

#[derive(Clone, Debug)]
pub struct Nav {
    pub screen: Screen,
    pub active: Option<Selection>,
    /// Seconds spent on the current screen.
    pub screen_time: f32,
    end_timer: f32,
}

impl Default for Nav {
    fn default() -> Self {
        Nav {
            screen: Screen::Menu,
            active: None,
            screen_time: 0.0,
            end_timer: 0.0,
        }
    }
}

impl Nav {
    fn go(&mut self, s: Screen) {
        self.screen = s;
        self.screen_time = 0.0;
    }

    /// Advances time; `Ending` turns into `Results` after [`END_DELAY`].
    pub fn tick(&mut self, dt: f32) {
        self.screen_time += dt;
        if self.screen == Screen::Ending {
            self.end_timer += dt;
            if self.end_timer >= END_DELAY {
                self.go(Screen::Results);
            }
        }
    }

    pub fn start(&mut self, sel: Selection) {
        self.active = Some(sel);
        self.end_timer = 0.0;
        self.go(Screen::Playing);
    }

    pub fn restart(&mut self) {
        if let Some(sel) = self.active {
            self.start(sel);
        }
    }

    /// Returns `true` if the pause took effect.
    pub fn pause(&mut self) -> bool {
        if self.screen == Screen::Playing {
            self.go(Screen::Paused);
            return true;
        }
        false
    }

    pub fn resume(&mut self) -> bool {
        if self.screen == Screen::Paused {
            self.go(Screen::Playing);
            return true;
        }
        false
    }

    /// The running game reported its end.
    pub fn session_ended(&mut self) {
        if self.screen == Screen::Playing {
            self.end_timer = 0.0;
            self.go(Screen::Ending);
        }
    }

    pub fn to_menu(&mut self) {
        self.active = None;
        self.go(Screen::Menu);
    }

    /// `true` while a game is on screen (running, paused, ending, results).
    pub fn in_game(&self) -> bool {
        self.screen != Screen::Menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cycle_returns_to_menu() {
        let mut n = Nav::default();
        assert_eq!(n.screen, Screen::Menu);
        n.start(Selection { game: 1, mode: 2 });
        assert_eq!(n.screen, Screen::Playing);
        assert!(n.pause());
        assert!(!n.pause(), "cannot pause twice");
        assert!(n.resume());
        n.session_ended();
        assert_eq!(n.screen, Screen::Ending);
        n.tick(END_DELAY * 0.5);
        assert_eq!(n.screen, Screen::Ending);
        n.tick(END_DELAY);
        assert_eq!(n.screen, Screen::Results);
        n.restart();
        assert_eq!(n.screen, Screen::Playing);
        assert_eq!(n.active, Some(Selection { game: 1, mode: 2 }));
        n.session_ended();
        n.tick(2.0);
        n.to_menu();
        assert_eq!(n.screen, Screen::Menu);
        assert_eq!(n.active, None);
    }

    #[test]
    fn session_end_is_ignored_while_paused_or_in_menu() {
        let mut n = Nav::default();
        n.session_ended();
        assert_eq!(n.screen, Screen::Menu);
        n.start(Selection { game: 0, mode: 0 });
        n.pause();
        n.session_ended();
        assert_eq!(n.screen, Screen::Paused);
    }
}

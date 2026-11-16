//! Delayed Auto Shift / Auto Repeat Rate with SOCD last-input priority.

/// Horizontal direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
}

impl Dir {
    pub fn dx(self) -> i32 {
        match self {
            Dir::Left => -1,
            Dir::Right => 1,
        }
    }
}

/// Handling configuration (seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handling {
    pub das: f32,
    pub arr: f32,
}

impl Default for Handling {
    fn default() -> Self {
        Handling {
            das: 0.130,
            arr: 0.030,
        }
    }
}

/// Shift request produced for one simulation step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shift {
    pub dir: Dir,
    /// Number of cells; `u32::MAX` means "slide to the wall" (ARR = 0).
    pub cells: u32,
}

/// DAS/ARR state machine.
#[derive(Clone, Debug, Default)]
pub struct DasHandler {
    /// Direction currently driving the piece (last input wins).
    active: Option<Dir>,
    left_held: bool,
    right_held: bool,
    das_timer: f32,
    arr_timer: f32,
    charged: bool,
}

impl DasHandler {
    pub fn active(&self) -> Option<Dir> {
        self.active
    }

    fn start(&mut self, dir: Dir) -> Shift {
        self.active = Some(dir);
        self.das_timer = 0.0;
        self.arr_timer = 0.0;
        self.charged = false;
        Shift { dir, cells: 1 }
    }

    /// Advances the handler.
    ///
    /// * `left`/`right` — currently held
    /// * `left_pressed`/`right_pressed` — rising edges in this step
    pub fn update(
        &mut self,
        left: bool,
        right: bool,
        left_pressed: bool,
        right_pressed: bool,
        dt: f32,
        handling: Handling,
    ) -> Option<Shift> {
        let prev_left = self.left_held;
        let prev_right = self.right_held;
        self.left_held = left;
        self.right_held = right;

        // New presses: last input wins (if both in the same step, prefer right
        // only when left was already held, else left — deterministic order).
        let new_left = left && (left_pressed || !prev_left);
        let new_right = right && (right_pressed || !prev_right);
        if new_left && new_right {
            return Some(self.start(Dir::Right));
        }
        if new_right {
            return Some(self.start(Dir::Right));
        }
        if new_left {
            return Some(self.start(Dir::Left));
        }

        // Releases: fall back to the other direction if still held.
        match self.active {
            Some(Dir::Left) if !left => {
                return if right {
                    Some(self.start(Dir::Right))
                } else {
                    self.active = None;
                    None
                };
            }
            Some(Dir::Right) if !right => {
                return if left {
                    Some(self.start(Dir::Left))
                } else {
                    self.active = None;
                    None
                };
            }
            None => return None,
            _ => {}
        }

        let dir = self.active?;
        if !self.charged {
            self.das_timer += dt;
            if self.das_timer < handling.das {
                return None;
            }
            self.charged = true;
            // Carry over the overshoot into ARR.
            self.arr_timer = self.das_timer - handling.das;
            if handling.arr <= 0.0 {
                return Some(Shift {
                    dir,
                    cells: u32::MAX,
                });
            }
            // First auto-repeat move happens right when DAS is charged.
            return Some(Shift { dir, cells: 1 });
        }
        if handling.arr <= 0.0 {
            return Some(Shift {
                dir,
                cells: u32::MAX,
            });
        }
        self.arr_timer += dt;
        let mut cells = 0;
        while self.arr_timer >= handling.arr {
            self.arr_timer -= handling.arr;
            cells += 1;
        }
        (cells > 0).then_some(Shift { dir, cells })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 240.0;

    fn run(h: &mut DasHandler, l: bool, r: bool, secs: f32) -> i32 {
        let mut moved = 0;
        let steps = (secs / DT).round() as usize;
        for _ in 0..steps {
            if let Some(s) = h.update(l, r, false, false, DT, Handling::default()) {
                moved += s.dir.dx() * s.cells.min(10) as i32;
            }
        }
        moved
    }

    #[test]
    fn tap_moves_once() {
        let mut h = DasHandler::default();
        let s = h.update(true, false, true, false, DT, Handling::default());
        assert_eq!(
            s,
            Some(Shift {
                dir: Dir::Left,
                cells: 1
            })
        );
        assert_eq!(run(&mut h, true, false, 0.12), 0, "DAS not charged yet");
    }

    #[test]
    fn das_then_arr() {
        let mut h = DasHandler::default();
        h.update(false, true, false, true, DT, Handling::default());
        // after 130 ms: 1 move, then every 30 ms
        let moved = run(&mut h, false, true, 0.130 + 0.090);
        assert_eq!(moved, 1 + 3);
    }

    #[test]
    fn socd_last_input_priority() {
        let mut h = DasHandler::default();
        h.update(true, false, true, false, DT, Handling::default());
        // Right pressed while left held: right wins.
        let s = h.update(true, true, false, true, DT, Handling::default());
        assert_eq!(s.map(|s| s.dir), Some(Dir::Right));
        assert_eq!(h.active(), Some(Dir::Right));
        // Release right while left still held: left takes over.
        let s = h.update(true, false, false, false, DT, Handling::default());
        assert_eq!(s.map(|s| s.dir), Some(Dir::Left));
        assert_eq!(h.active(), Some(Dir::Left));
        // Release all.
        assert_eq!(
            h.update(false, false, false, false, DT, Handling::default()),
            None
        );
        assert_eq!(h.active(), None);
    }

    #[test]
    fn arr_zero_slides_to_wall() {
        let mut h = DasHandler::default();
        let hd = Handling {
            das: 0.05,
            arr: 0.0,
        };
        h.update(true, false, true, false, DT, hd);
        let mut instant = false;
        for _ in 0..20 {
            if let Some(s) = h.update(true, false, false, false, DT, hd) {
                instant |= s.cells == u32::MAX;
            }
        }
        assert!(instant);
    }
}

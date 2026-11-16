//! `GameState`: the complete, deterministic rules engine.
//!
//! It knows nothing about keys, frames or rendering. The engine layer calls
//! the action methods (`move_horizontal`, `rotate`, `hold`, `hard_drop`, …)
//! and `tick(dt)`; the presentation layer reads state and drains
//! [`GameEvent`]s for juice and audio.

use super::bag::SevenBag;
use super::board::{Board, VISIBLE_TOP, drop_position, is_valid_position};
use super::piece::{Piece, PieceKind, RotateDir};
use super::scoring::{ClearResult, ScoreTracker, clear_label};
use super::srs::try_rotate;
use crate::core::flow::{FlowMeter, FlowTier, LockFlowInput};

/// Lock delay in seconds.
pub const LOCK_DELAY: f32 = 0.5;
/// Maximum number of lock-timer resets by movement/rotation.
pub const MAX_LOCK_RESETS: u32 = 15;
/// Soft drop speed factor relative to normal gravity.
pub const SOFT_DROP_FACTOR: f32 = 20.0;
/// Minimum soft drop speed in cells per second.
pub const SOFT_DROP_MIN: f32 = 20.0;
/// Lines per level in Marathon / Daily.
pub const LINES_PER_LEVEL: u32 = 10;
/// Level at which the gravity curve stops getting faster.
pub const MAX_GRAVITY_LEVEL: u32 = 20;

/// The four game modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameMode {
    Marathon,
    Sprint,
    DailyVibe,
    Zen,
}

impl GameMode {
    pub const ALL: [GameMode; 4] = [
        GameMode::Marathon,
        GameMode::Sprint,
        GameMode::DailyVibe,
        GameMode::Zen,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GameMode::Marathon => "MARATHON",
            GameMode::Sprint => "40-LINE SPRINT",
            GameMode::DailyVibe => "DAILY VIBE",
            GameMode::Zen => "ZEN",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            GameMode::Marathon => "Endless. Level up every 10 lines.",
            GameMode::Sprint => "Clear 40 lines as fast as you can.",
            GameMode::DailyVibe => "200-line flow run. One seed for everyone.",
            GameMode::Zen => "No game over. Just vibes.",
        }
    }

    /// Stable key for persistence.
    pub fn key(self) -> &'static str {
        match self {
            GameMode::Marathon => "marathon",
            GameMode::Sprint => "sprint",
            GameMode::DailyVibe => "daily",
            GameMode::Zen => "zen",
        }
    }

    /// Line target that finishes the run.
    pub fn line_goal(self) -> Option<u32> {
        match self {
            GameMode::Sprint => Some(40),
            GameMode::DailyVibe => Some(200),
            _ => None,
        }
    }

    /// Whether levels (and thus gravity) increase with cleared lines.
    pub fn levels_up(self) -> bool {
        matches!(self, GameMode::Marathon | GameMode::DailyVibe)
    }

    /// Sprint is ranked by time, everything else by score.
    pub fn ranked_by_time(self) -> bool {
        self == GameMode::Sprint
    }
}

/// Why the game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopOut {
    /// A new piece overlaps blocks at its spawn position.
    BlockOut,
    /// A piece locked entirely inside the hidden buffer zone.
    LockOut,
}

/// Overall status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStatus {
    Playing,
    GameOver(TopOut),
    /// Line goal reached (Sprint / Daily Vibe).
    Finished,
}

/// Things that happened, for presentation (juice + audio).
#[derive(Clone, Debug, PartialEq)]
pub enum GameEvent {
    Spawned(PieceKind),
    Moved,
    Rotated {
        kick_index: usize,
    },
    Held(PieceKind),
    SoftDropped(u32),
    HardDropped {
        from: Piece,
        to: Piece,
        distance: u32,
    },
    Locked {
        piece: Piece,
        tspin: bool,
    },
    LinesCleared {
        rows: Vec<usize>,
        result: ClearResult,
        label: Option<&'static str>,
    },
    /// T-spin without lines (still a callout).
    TSpinNoLines {
        points: u64,
    },
    LevelUp(u32),
    TierChanged {
        from: FlowTier,
        to: FlowTier,
    },
    TopOut(TopOut),
    /// Zen mode swallowed a top-out and cleared the field. Carries the
    /// removed blocks so the presentation can dissolve them gently.
    ZenReset {
        cleared: Vec<(i32, i32, PieceKind)>,
    },
    Finished,
}

/// The complete game.
#[derive(Clone, Debug)]
pub struct GameState {
    pub mode: GameMode,
    pub seed: u64,
    pub board: Board,
    pub active: Piece,
    pub hold: Option<PieceKind>,
    pub can_hold: bool,
    pub status: GameStatus,

    pub lock_timer: f32,
    pub lock_reset_count: u32,
    pub lowest_y_reached: i32,
    /// `true` when the last successful action on the active piece was a rotation.
    pub last_action_was_rotation: bool,
    pub soft_drop: bool,
    gravity_acc: f32,

    pub score: ScoreTracker,
    pub flow: FlowMeter,
    pub level: u32,
    pub lines: u32,
    pub pieces_placed: u32,
    pub quads: u32,
    pub tspins: u32,
    /// Seconds of active play.
    pub elapsed: f64,
    /// Seconds since the active piece spawned.
    pub piece_time: f32,
    pub zen_resets: u32,

    bag: SevenBag,
    events: Vec<GameEvent>,
}

impl GameState {
    pub fn new(mode: GameMode, seed: u64) -> Self {
        let mut bag = SevenBag::new(seed);
        let first = bag.next_piece();
        let mut g = GameState {
            mode,
            seed,
            board: Board::new(),
            active: Piece::spawn(first),
            hold: None,
            can_hold: true,
            status: GameStatus::Playing,
            lock_timer: 0.0,
            lock_reset_count: 0,
            lowest_y_reached: 0,
            last_action_was_rotation: false,
            soft_drop: false,
            gravity_acc: 0.0,
            score: ScoreTracker::new(),
            flow: FlowMeter::new(),
            level: 1,
            lines: 0,
            pieces_placed: 0,
            quads: 0,
            tspins: 0,
            elapsed: 0.0,
            piece_time: 0.0,
            zen_resets: 0,
            bag,
            events: Vec::new(),
        };
        g.spawn_piece(first);
        g
    }

    // ------------------------------------------------------------------ queries

    pub fn is_playing(&self) -> bool {
        self.status == GameStatus::Playing
    }

    /// Takes all pending events.
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    /// Upcoming pieces.
    pub fn next_queue(&self, n: usize) -> Vec<PieceKind> {
        self.bag.peek(n)
    }

    /// The piece is resting on something.
    pub fn is_grounded(&self) -> bool {
        !is_valid_position(&self.board, &self.active.shifted(0, 1))
    }

    /// Ghost piece: the active piece dropped as far as possible.
    pub fn ghost(&self) -> Piece {
        drop_position(&self.board, &self.active)
    }

    pub fn tier(&self) -> FlowTier {
        self.flow.tier()
    }

    /// Normal gravity in cells per second (guideline curve).
    ///
    /// Depends on mode and level only — **never** on flow.
    pub fn gravity_cells_per_second(&self) -> f32 {
        let lvl = if self.mode.levels_up() {
            self.level.clamp(1, MAX_GRAVITY_LEVEL)
        } else {
            1
        };
        let l = (lvl - 1) as f32;
        let seconds_per_row = (0.8 - l * 0.007).powf(l);
        1.0 / seconds_per_row
    }

    fn current_fall_rate(&self) -> f32 {
        let g = self.gravity_cells_per_second();
        if self.soft_drop {
            (g * SOFT_DROP_FACTOR).max(SOFT_DROP_MIN)
        } else {
            g
        }
    }

    // ------------------------------------------------------------------ spawn

    fn spawn_next(&mut self) {
        let kind = self.bag.next_piece();
        self.spawn_piece(kind);
    }

    /// Spawns `kind` at the spawn position (state 0) and applies block-out.
    fn spawn_piece(&mut self, kind: PieceKind) {
        let mut piece = Piece::spawn(kind);
        if !is_valid_position(&self.board, &piece) {
            self.active = piece;
            self.top_out(TopOut::BlockOut);
            return;
        }
        // Drop one row immediately if possible (into the visible field).
        if is_valid_position(&self.board, &piece.shifted(0, 1)) {
            piece = piece.shifted(0, 1);
        }
        self.active = piece;
        self.lock_timer = 0.0;
        self.lock_reset_count = 0;
        self.lowest_y_reached = piece.y;
        self.last_action_was_rotation = false;
        self.gravity_acc = 0.0;
        self.piece_time = 0.0;
        self.events.push(GameEvent::Spawned(kind));
    }

    fn top_out(&mut self, reason: TopOut) {
        if self.mode == GameMode::Zen {
            // Zen: gently clear the field and keep going.
            let cleared = self.board.filled_cells();
            self.board.clear_all();
            self.zen_resets += 1;
            self.events.push(GameEvent::ZenReset { cleared });
            let kind = self.active.kind;
            self.spawn_piece(kind);
            return;
        }
        self.status = GameStatus::GameOver(reason);
        self.events.push(GameEvent::TopOut(reason));
    }

    // ------------------------------------------------------------------ lock delay helpers

    /// Successful horizontal move or rotation: move-reset (anti-stalling).
    fn on_move_reset(&mut self) {
        if self.lock_reset_count < MAX_LOCK_RESETS {
            self.lock_timer = 0.0;
            self.lock_reset_count += 1;
        }
    }

    /// Called after any change of `y`: new lowest row resets the counter.
    fn on_vertical_progress(&mut self) {
        if self.active.y > self.lowest_y_reached {
            self.lowest_y_reached = self.active.y;
            self.lock_reset_count = 0;
            self.lock_timer = 0.0;
        }
    }

    // ------------------------------------------------------------------ actions

    /// Moves the active piece by `dx` columns (±1). Returns whether it moved.
    pub fn move_horizontal(&mut self, dx: i32) -> bool {
        if !self.is_playing() {
            return false;
        }
        let moved = self.active.shifted(dx, 0);
        if is_valid_position(&self.board, &moved) {
            self.active = moved;
            self.last_action_was_rotation = false;
            self.on_move_reset();
            self.events.push(GameEvent::Moved);
            true
        } else {
            false
        }
    }

    /// Rotates with SRS kicks. Returns whether the rotation happened.
    pub fn rotate(&mut self, dir: RotateDir) -> bool {
        if !self.is_playing() {
            return false;
        }
        match try_rotate(&self.board, &self.active, dir) {
            Some(r) => {
                self.active = r.piece;
                self.last_action_was_rotation = true;
                self.on_move_reset();
                self.on_vertical_progress();
                self.events.push(GameEvent::Rotated {
                    kick_index: r.kick_index,
                });
                true
            }
            None => false,
        }
    }

    /// Swaps with the hold slot (once per drop).
    pub fn hold(&mut self) -> bool {
        if !self.is_playing() || !self.can_hold {
            return false;
        }
        let current = self.active.kind;
        let swapped = self.hold.replace(current);
        self.events.push(GameEvent::Held(current));
        match swapped {
            Some(kind) => self.spawn_piece(kind),
            None => self.spawn_next(),
        }
        self.can_hold = false;
        true
    }

    pub fn set_soft_drop(&mut self, on: bool) {
        self.soft_drop = on;
    }

    /// Drops and locks immediately.
    pub fn hard_drop(&mut self) {
        if !self.is_playing() {
            return;
        }
        let from = self.active;
        let to = self.ghost();
        let distance = (to.y - from.y).max(0) as u32;
        if distance > 0 {
            self.last_action_was_rotation = false;
        }
        self.active = to;
        self.score.add_drop(distance, true);
        self.events
            .push(GameEvent::HardDropped { from, to, distance });
        self.lock_piece();
    }

    /// Advances gravity, lock delay, flow and clocks by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        if !self.is_playing() {
            return;
        }
        self.elapsed += dt as f64;
        self.piece_time += dt;

        let before = self.flow.tier();
        self.flow.update(dt);
        self.emit_tier_change(before);

        // Gravity
        self.gravity_acc += self.current_fall_rate() * dt;
        let mut soft_cells = 0;
        let mut steps = 0;
        while self.gravity_acc >= 1.0 && steps < 64 {
            steps += 1;
            let down = self.active.shifted(0, 1);
            if is_valid_position(&self.board, &down) {
                self.active = down;
                self.gravity_acc -= 1.0;
                self.last_action_was_rotation = false;
                self.on_vertical_progress();
                if self.soft_drop {
                    soft_cells += 1;
                }
            } else {
                self.gravity_acc = 0.0;
                break;
            }
        }
        if soft_cells > 0 {
            self.score.add_drop(soft_cells, false);
            self.events.push(GameEvent::SoftDropped(soft_cells));
        }

        // Lock delay: runs only while grounded, pauses otherwise.
        if self.is_grounded() {
            self.gravity_acc = 0.0;
            self.lock_timer += dt;
            if self.lock_timer >= LOCK_DELAY {
                self.lock_piece();
            }
        }
    }

    // ------------------------------------------------------------------ locking

    /// 3-corner rule around the T centre (box cell (1, 1)).
    fn is_tspin(&self) -> bool {
        if self.active.kind != PieceKind::T || !self.last_action_was_rotation {
            return false;
        }
        let cx = self.active.x + 1;
        let cy = self.active.y + 1;
        let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
        corners
            .iter()
            .filter(|(dx, dy)| self.board.is_blocked(cx + dx, cy + dy))
            .count()
            >= 3
    }

    fn emit_tier_change(&mut self, before: FlowTier) {
        let after = self.flow.tier();
        if after != before {
            self.events.push(GameEvent::TierChanged {
                from: before,
                to: after,
            });
        }
    }

    /// Locks the active piece into the matrix and resolves the consequences.
    fn lock_piece(&mut self) {
        let piece = self.active;
        let tspin = self.is_tspin();
        self.board.place(&piece);
        self.pieces_placed += 1;
        self.events.push(GameEvent::Locked { piece, tspin });

        // Lock out: entirely above the visible field.
        if piece.minos().iter().all(|m| m.1 < VISIBLE_TOP as i32) {
            self.can_hold = true;
            self.top_out(TopOut::LockOut);
            return;
        }

        let rows = self.board.clear_full_rows();
        let lines = rows.len() as u32;
        let tier_before = self.flow.tier();
        let result = self
            .score
            .register_lock(lines, tspin, self.level, self.flow.multiplier());

        self.flow.on_lock(LockFlowInput {
            seconds_since_spawn: self.piece_time,
            lines,
            tspin,
            combo: result.combo.max(0) as u32,
        });
        self.emit_tier_change(tier_before);

        if tspin {
            self.tspins += 1;
        }
        if lines == 4 {
            self.quads += 1;
        }
        if lines > 0 {
            self.events.push(GameEvent::LinesCleared {
                rows,
                result,
                label: clear_label(lines, tspin),
            });
            self.lines += lines;
            let new_level = 1 + self.lines / LINES_PER_LEVEL;
            if new_level > self.level {
                self.level = new_level;
                self.events.push(GameEvent::LevelUp(new_level));
            }
        } else if tspin {
            self.events.push(GameEvent::TSpinNoLines {
                points: result.points,
            });
        }

        if let Some(goal) = self.mode.line_goal()
            && self.lines >= goal
        {
            self.status = GameStatus::Finished;
            self.events.push(GameEvent::Finished);
            return;
        }

        self.can_hold = true;
        self.spawn_next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::board::{Cell, WIDTH};
    use crate::core::piece::Rotation;

    fn game_with(kind: PieceKind) -> GameState {
        let mut g = GameState::new(GameMode::Marathon, 1);
        g.spawn_piece(kind);
        g.drain_events();
        g
    }

    fn fill_row(g: &mut GameState, y: i32, except: &[i32]) {
        for x in 0..WIDTH as i32 {
            if !except.contains(&x) {
                g.board.set(x, y, Cell::Filled(PieceKind::Z));
            }
        }
    }

    #[test]
    fn spawn_drops_one_row_into_view() {
        let g = game_with(PieceKind::T);
        assert_eq!(g.active.y, 19);
        assert_eq!(g.active.rot, Rotation::Spawn);
    }

    #[test]
    fn lock_delay_locks_after_500ms_grounded() {
        let mut g = game_with(PieceKind::O);
        g.active = g.ghost();
        g.lowest_y_reached = g.active.y;
        let placed = g.pieces_placed;
        for _ in 0..4 {
            g.tick(0.1);
        }
        assert_eq!(g.pieces_placed, placed, "not yet locked at 0.4 s");
        g.tick(0.11);
        assert_eq!(g.pieces_placed, placed + 1, "locked at >= 0.5 s");
    }

    #[test]
    fn move_reset_limited_to_15() {
        let mut g = game_with(PieceKind::T);
        g.active = g.ghost();
        g.lowest_y_reached = g.active.y;
        let placed = g.pieces_placed;
        // 15 successful moves keep resetting the timer.
        for i in 0..15 {
            g.tick(0.4);
            assert_eq!(g.pieces_placed, placed);
            let dx = if i % 2 == 0 { 1 } else { -1 };
            assert!(g.move_horizontal(dx));
            assert_eq!(g.lock_timer, 0.0);
        }
        assert_eq!(g.lock_reset_count, 15);
        // 16th move no longer resets.
        g.tick(0.4);
        assert!(g.move_horizontal(1));
        assert!(g.lock_timer > 0.39);
        g.tick(0.11);
        assert_eq!(
            g.pieces_placed,
            placed + 1,
            "locks after reset budget is spent"
        );
    }

    #[test]
    fn pushing_against_wall_does_not_reset() {
        let mut g = game_with(PieceKind::O);
        g.active = g.ghost();
        g.lowest_y_reached = g.active.y;
        while g.move_horizontal(-1) {}
        let resets = g.lock_reset_count;
        g.tick(0.3);
        assert!(!g.move_horizontal(-1));
        assert_eq!(g.lock_reset_count, resets);
        assert!(g.lock_timer >= 0.3 - 1e-6);
    }

    #[test]
    fn new_lowest_row_resets_counter() {
        let mut g = game_with(PieceKind::T);
        for _ in 0..5 {
            g.move_horizontal(1);
            g.move_horizontal(-1);
        }
        assert_eq!(g.lock_reset_count, 10);
        g.tick(1.0); // gravity level 1 = 1 cell/s
        assert_eq!(g.lock_reset_count, 0);
    }

    #[test]
    fn timer_pauses_while_airborne() {
        let mut g = game_with(PieceKind::T);
        g.tick(0.2);
        assert_eq!(g.lock_timer, 0.0);
    }

    #[test]
    fn tspin_double_detected_and_scored() {
        let mut g = game_with(PieceKind::T);
        // Classic TSD slot at the bottom: rows 38/39, hole at x=4 (row 39), x=3..5 (row 38)
        fill_row(&mut g, 39, &[4]);
        fill_row(&mut g, 38, &[3, 4, 5]);
        g.board.set(3, 37, Cell::Filled(PieceKind::Z)); // overhang corner
        // T pointing down (Reverse) placed into the slot via rotation.
        g.active = Piece::new(PieceKind::T, Rotation::Right, 3, 37);
        assert!(is_valid_position(&g.board, &g.active));
        assert!(g.rotate(RotateDir::Cw));
        assert_eq!(g.active.rot, Rotation::Reverse);
        assert_eq!((g.active.x, g.active.y), (3, 37));
        g.hard_drop();
        let ev = g.drain_events();
        let cleared = ev.iter().find_map(|e| match e {
            GameEvent::LinesCleared { result, .. } => Some(*result),
            _ => None,
        });
        let r = cleared.expect("lines cleared");
        assert!(r.tspin);
        assert_eq!(r.lines, 2);
        assert_eq!(r.points, 1200);
        assert_eq!(g.lines, 2);
    }

    #[test]
    fn no_tspin_without_rotation() {
        let mut g = game_with(PieceKind::T);
        fill_row(&mut g, 39, &[4]);
        fill_row(&mut g, 38, &[3, 4, 5]);
        g.active = Piece::new(PieceKind::T, Rotation::Reverse, 3, 30);
        g.hard_drop();
        let ev = g.drain_events();
        let r = ev
            .iter()
            .find_map(|e| match e {
                GameEvent::LinesCleared { result, .. } => Some(*result),
                _ => None,
            })
            .unwrap();
        assert!(!r.tspin);
        assert_eq!(r.points, 300);
    }

    #[test]
    fn hold_only_once_per_drop() {
        let mut g = game_with(PieceKind::T);
        assert!(g.hold());
        assert_eq!(g.hold, Some(PieceKind::T));
        let current = g.active.kind;
        assert!(!g.hold(), "second hold in the same drop is rejected");
        assert_eq!(g.active.kind, current);
        g.hard_drop();
        assert!(g.can_hold);
        let next_kind = g.active.kind;
        g.move_horizontal(1);
        assert!(g.hold());
        assert_eq!(g.active.kind, PieceKind::T);
        assert_eq!(g.active.rot, Rotation::Spawn);
        assert_eq!(g.active.x, 3);
        assert_eq!(g.hold, Some(next_kind));
    }

    #[test]
    fn block_out_ends_game() {
        let mut g = game_with(PieceKind::T);
        for y in 18..40 {
            fill_row(&mut g, y, &[0]);
        }
        g.hard_drop();
        assert!(matches!(
            g.status,
            GameStatus::GameOver(TopOut::BlockOut) | GameStatus::GameOver(TopOut::LockOut)
        ));
    }

    #[test]
    fn block_out_on_spawn() {
        let mut g = game_with(PieceKind::T);
        g.board.set(4, 18, Cell::Filled(PieceKind::I));
        g.spawn_piece(PieceKind::T);
        assert_eq!(g.status, GameStatus::GameOver(TopOut::BlockOut));
    }

    #[test]
    fn lock_out_when_entirely_in_buffer() {
        let mut g = game_with(PieceKind::O);
        // Column stack under the O up to row 20 (visible top), leaving O in buffer.
        for y in 20..40 {
            g.board.set(4, y, Cell::Filled(PieceKind::I));
            g.board.set(5, y, Cell::Filled(PieceKind::I));
        }
        g.active = Piece::new(PieceKind::O, Rotation::Spawn, 3, 18);
        g.hard_drop();
        assert_eq!(g.status, GameStatus::GameOver(TopOut::LockOut));
    }

    #[test]
    fn zen_never_tops_out() {
        let mut g = GameState::new(GameMode::Zen, 3);
        g.board.set(4, 18, Cell::Filled(PieceKind::I));
        g.board.set(4, 30, Cell::Filled(PieceKind::I));
        g.spawn_piece(PieceKind::T);
        assert_eq!(g.status, GameStatus::Playing);
        assert_eq!(g.board.occupied_count(), 0);
        assert_eq!(g.zen_resets, 1);
    }

    #[test]
    fn flow_never_changes_gravity() {
        let mut g = game_with(PieceKind::T);
        let base = g.gravity_cells_per_second();
        g.flow.add(100.0);
        assert_eq!(g.tier(), FlowTier::Overdrive);
        assert_eq!(g.gravity_cells_per_second(), base);
    }

    #[test]
    fn gravity_increases_with_level() {
        let mut g = game_with(PieceKind::T);
        let g1 = g.gravity_cells_per_second();
        assert!((g1 - 1.0).abs() < 1e-6);
        g.level = 10;
        assert!(g.gravity_cells_per_second() > g1 * 10.0);
    }

    #[test]
    fn soft_drop_is_twenty_times_and_scores() {
        let mut g = game_with(PieceKind::T);
        g.set_soft_drop(true);
        let y = g.active.y;
        g.tick(0.5); // 20 cells/s * 0.5 = 10 cells
        assert_eq!(g.active.y, y + 10);
        assert_eq!(g.score.score, 10);
    }

    #[test]
    fn hard_drop_scores_two_per_cell() {
        let mut g = game_with(PieceKind::O);
        let dist = g.ghost().y - g.active.y;
        g.hard_drop();
        assert_eq!(g.score.score, 2 * dist as u64);
    }

    #[test]
    fn sprint_finishes_at_40_lines() {
        let mut g = GameState::new(GameMode::Sprint, 5);
        g.lines = 39;
        g.spawn_piece(PieceKind::I);
        fill_row(&mut g, 39, &[3, 4, 5, 6]);
        g.hard_drop();
        assert_eq!(g.lines, 40);
        assert_eq!(g.status, GameStatus::Finished);
    }

    #[test]
    fn quad_and_level_up() {
        let mut g = game_with(PieceKind::I);
        g.lines = 9;
        for y in 36..40 {
            fill_row(&mut g, y, &[0]);
        }
        g.active = Piece::new(PieceKind::I, Rotation::Left, -1, 25);
        assert!(is_valid_position(&g.board, &g.active));
        g.hard_drop();
        let ev = g.drain_events();
        assert!(ev.iter().any(|e| matches!(
            e,
            GameEvent::LinesCleared {
                label: Some("QUAD!"),
                ..
            }
        )));
        assert!(ev.iter().any(|e| matches!(e, GameEvent::LevelUp(2))));
        assert_eq!(g.level, 2);
    }

    #[test]
    fn quick_locks_build_flow() {
        let mut g = game_with(PieceKind::O);
        g.hard_drop();
        assert_eq!(g.flow.flow, 5.0);
    }
}

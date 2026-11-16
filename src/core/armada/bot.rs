//! A small defensive bot for the headless smoke test and the attract mode:
//! dodge falling bolts and beams, dash when it gets tight, otherwise line
//! up under the nearest target and keep firing.

use super::game::{ArmadaGame, ArmadaInput, Laser, SHIP_MAX_X, SHIP_MIN_X, SHIP_Y};

/// Chooses the input for the next frame.
pub fn choose(g: &ArmadaGame) -> ArmadaInput {
    let sx = g.ship.x;
    let mut move_x = 0.0;
    let mut dash = false;

    // Threats: bolts that are about to cross the ship's lane.
    let threat = g
        .enemy_bolts
        .iter()
        .filter(|b| b.y > SHIP_Y - 190.0 && b.y < SHIP_Y + 10.0)
        .filter(|b| {
            // Where will it be when it reaches the ship?
            let t = ((SHIP_Y - b.y) / b.vy.max(1.0)).max(0.0);
            (b.x + b.vx * t - sx).abs() < 34.0
        })
        .min_by(|a, b| (SHIP_Y - a.y).total_cmp(&(SHIP_Y - b.y)));

    let beam_x = g.boss.as_ref().and_then(|b| match b.laser {
        Laser::Telegraph { x, .. } | Laser::Fire { x, .. } => Some(x),
        Laser::Idle(_) => None,
    });

    if let Some(b) = threat {
        let away = if b.x >= sx { -1.0 } else { 1.0 };
        // Do not dodge into a wall.
        move_x = if (away < 0.0 && sx < SHIP_MIN_X + 60.0) || (away > 0.0 && sx > SHIP_MAX_X - 60.0)
        {
            -away
        } else {
            away
        };
        dash = g.dash_ready() && (SHIP_Y - b.y) < 80.0;
    } else if let Some(bx) = beam_x.filter(|bx| (bx - sx).abs() < 60.0) {
        move_x = if bx >= sx { -1.0 } else { 1.0 };
        if (sx < SHIP_MIN_X + 70.0 && move_x < 0.0) || (sx > SHIP_MAX_X - 70.0 && move_x > 0.0) {
            move_x = -move_x;
        }
    } else {
        // Line up under the nearest living target.
        let target = if let Some(b) = &g.boss {
            Some(b.x)
        } else {
            g.enemies
                .iter()
                .filter(|e| e.alive)
                .map(|e| g.enemy_pos(e).0)
                .min_by(|a, b| (a - sx).abs().total_cmp(&(b - sx).abs()))
        };
        if let Some(tx) = target {
            let d = tx - sx;
            if d.abs() > 6.0 {
                move_x = d.signum();
            }
        }
    }
    ArmadaInput {
        move_x,
        fire_held: true,
        fire_pressed: false,
        dash_pressed: dash,
    }
}

#[cfg(test)]
mod tests {
    use super::super::game::{ArmadaMode, STEP};
    use super::*;

    #[test]
    fn bot_fights_and_survives_in_every_mode() {
        for mode in ArmadaMode::ALL {
            let mut g = ArmadaGame::new(mode, 31337);
            for i in 0..(30.0 / STEP) as usize {
                let input = choose(&g);
                g.step(&input);
                assert!(
                    g.is_playing(),
                    "{mode:?}: bot died at step {i}: {:?}",
                    g.status
                );
            }
            assert!(g.kills >= 10, "{mode:?}: only {} kills", g.kills);
            assert!(g.shots > 20);
        }
    }

    #[test]
    fn bot_can_take_on_the_boss() {
        let mut g = ArmadaGame::new_at_wave(ArmadaMode::Zen, 5, 5);
        for _ in 0..(40.0 / STEP) as usize {
            let input = choose(&g);
            g.step(&input);
        }
        let boss_hurt = g
            .boss
            .as_ref()
            .map(|b| b.core_hp < b.core_max || b.turrets.iter().any(|t| !t.alive))
            .unwrap_or(true);
        assert!(boss_hurt, "the bot damages the Dreadnought");
    }
}

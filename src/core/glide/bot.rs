//! A small autopilot for the headless smoke test and the attract mode: it
//! steers towards the next gap (or its Vibe ring) and flaps when the glider
//! is about to drop below the target height.

use super::course::{LIP_OVERHANG, PIPE_W};
use super::game::{FIXED_DT, FLAP_VY, GLIDER_R, GLIDER_X, GRAVITY, GlideGame, GlideInput};

/// The input for the next step.
pub fn choose(g: &GlideGame) -> GlideInput {
    if !g.started {
        return GlideInput {
            flap_pressed: true,
            ..Default::default()
        };
    }
    let t = g.world_time();
    let kinetic = g.kinetic();
    let speed = g.speed();
    // The first pipe the glider has not completely passed yet.
    let (target, lower_limit) = g
        .pipes
        .iter()
        .find(|p| p.x + PIPE_W + LIP_OVERHANG + GLIDER_R > GLIDER_X)
        .map(|p| {
            // Moving gaps: aim where the gap WILL be when the pipe arrives.
            let eta = ((p.center_x() - GLIDER_X) / speed).max(0.0);
            let ta = t + eta;
            let centre = p.gap_center(ta, kinetic);
            // Never let the glider sink below the lower lip before flapping.
            let lower = centre + p.gap_h() * 0.5 - GLIDER_R - 6.0;
            let target = match (p.spec.ring, p.ring_resolved) {
                // Go for the ring, but stay well inside the gap.
                (Some(_), false) => {
                    let ry = p.ring_y(ta, kinetic).unwrap_or(centre);
                    // The hover band is [target - 26, target + 42]; keep all
                    // of it inside the gap (2 units of margin).
                    let m = p.gap_h() * 0.5 - GLIDER_R - 2.0;
                    ry.clamp(centre - m + 26.0, centre + m - 42.0)
                }
                _ => centre,
            };
            (target, lower)
        })
        .unwrap_or((300.0, 560.0));
    // A flap lifts the glider by v^2 / (2 * a) from wherever it happens, with
    // a = gravity + wind. Flapping at `target - 26 + apex` keeps the apex at
    // `target - 26`: inside even the narrowest (138) gap with room to spare.
    let accel = (GRAVITY + g.lift_now()).max(500.0);
    let apex = FLAP_VY * FLAP_VY / (2.0 * accel);
    let flap_line = (target - 26.0 + apex).min(lower_limit);
    let next_y = g.y + g.vy * FIXED_DT;
    GlideInput {
        flap_pressed: next_y >= flap_line && g.y > 70.0,
        dive_held: false,
        dash_pressed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::game::{FIXED_DT, GlideMode};
    use super::*;

    #[test]
    fn the_bot_survives_every_mode_for_a_long_time() {
        // Static modes: 30 s. Kinetic Shift (swaying pipes and sudden gusts)
        // is much harder for a simple bot, so it only has to last 15 s.
        for mode in GlideMode::ALL {
            let seconds = if mode == GlideMode::KineticShift {
                15.0
            } else {
                30.0
            };
            for seed in [1u64, 20261116, 777] {
                let mut g = GlideGame::new(mode, seed);
                for i in 0..(seconds / FIXED_DT) as usize {
                    let input = choose(&g);
                    g.step(&input);
                    assert!(
                        g.is_playing(),
                        "{mode:?}/{seed}: died at step {i}: {:?}",
                        g.status
                    );
                }
                assert!(g.passed >= 6, "{mode:?}/{seed}: only {} pipes", g.passed);
            }
        }
    }

    #[test]
    fn the_bot_collects_rings_and_passes_the_first_pipe_early() {
        let mut g = GlideGame::new(GlideMode::Classic, 5);
        let mut first_pass = None;
        for i in 0..(30.0 / FIXED_DT) as usize {
            let input = choose(&g);
            g.step(&input);
            if first_pass.is_none() && g.passed > 0 {
                first_pass = Some(i);
            }
        }
        let first = first_pass.expect("passes a pipe");
        assert!(
            first < 330,
            "first pipe within 2.8 s of play (step {first})"
        );
        assert!(g.rings >= 3, "the bot collects rings: {}", g.rings);
    }
}

//! Procedurally rendered window icon: four bevelled neon minos on a dark,
//! rounded tile. No image files needed.

use macroquad::miniquad::conf::Icon;

const BG: [u8; 3] = [0x0B, 0x0E, 0x14];
const COLORS: [[u8; 3]; 4] = [
    [0x3E, 0xE6, 0xF5], // cyan
    [0xB4, 0x6C, 0xFF], // purple
    [0xFF, 0x4D, 0x7A], // pink
    [0xFF, 0xD8, 0x4A], // yellow
];

fn shade(c: [u8; 3], f: f32) -> [u8; 3] {
    let m = |v: u8| {
        if f >= 0.0 {
            (v as f32 + (255.0 - v as f32) * f) as u8
        } else {
            (v as f32 * (1.0 + f)) as u8
        }
    };
    [m(c[0]), m(c[1]), m(c[2])]
}

/// RGBA pixels of an `n × n` icon.
pub fn render(n: usize) -> Vec<u8> {
    let mut px = vec![0u8; n * n * 4];
    let radius = n as f32 / 5.0;
    let margin = (n / 8).max(1);
    let cell = (n - 2 * margin) / 2;
    let bevel = (cell / 6).max(1);
    for y in 0..n {
        for x in 0..n {
            // rounded-square mask
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(radius, n as f32 - radius);
            let cy = fy.clamp(radius, n as f32 - radius);
            let inside = (fx - cx).powi(2) + (fy - cy).powi(2) <= radius * radius;
            if !inside {
                continue;
            }
            let mut rgb = BG;
            let (gx, gy) = (x as isize - margin as isize, y as isize - margin as isize);
            if gx >= 0 && gy >= 0 && (gx as usize) < 2 * cell && (gy as usize) < 2 * cell {
                let (gx, gy) = (gx as usize, gy as usize);
                let col = COLORS[(gy / cell) * 2 + gx / cell];
                let (lx, ly) = (gx % cell, gy % cell);
                rgb = if lx == 0 || ly == 0 || lx == cell - 1 || ly == cell - 1 {
                    shade(col, -0.8) // dark frame
                } else if ly <= bevel || lx <= bevel {
                    shade(col, 0.45) // light bevel
                } else if ly >= cell - 1 - bevel || lx >= cell - 1 - bevel {
                    shade(col, -0.4) // shadow bevel
                } else {
                    col
                };
            }
            let i = (y * n + x) * 4;
            px[i..i + 3].copy_from_slice(&rgb);
            px[i + 3] = 255;
        }
    }
    px
}

/// The window icon in all three sizes miniquad expects.
pub fn window_icon() -> Option<Icon> {
    Some(Icon {
        small: render(16).try_into().ok()?,
        medium: render(32).try_into().ok()?,
        big: render(64).try_into().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_transparent_corners() {
        for n in [16, 32, 64] {
            let px = render(n);
            assert_eq!(px.len(), n * n * 4);
            assert_eq!(px[3], 0, "corner is transparent");
            let mid = (n / 2 * n + n / 2) * 4;
            assert_eq!(px[mid + 3], 255, "centre is opaque");
        }
        assert!(window_icon().is_some());
    }
}

//! Fixed virtual 1280×720 stage with letterboxing and DPI-correct mouse
//! mapping. Everything is drawn in virtual coordinates.

use macroquad::prelude::*;

pub const VIRTUAL_W: f32 = 1280.0;
pub const VIRTUAL_H: f32 = 720.0;

/// Pure letterbox math: scale and top-left offset of the stage inside a
/// window of `w × h` logical pixels.
pub fn letterbox(w: f32, h: f32) -> (f32, f32, f32) {
    let scale = (w / VIRTUAL_W).min(h / VIRTUAL_H).max(0.0001);
    let ox = (w - VIRTUAL_W * scale) * 0.5;
    let oy = (h - VIRTUAL_H * scale) * 0.5;
    (scale, ox, oy)
}

/// Maps logical window coordinates into stage coordinates.
pub fn window_to_stage(p: (f32, f32), w: f32, h: f32) -> (f32, f32) {
    let (s, ox, oy) = letterbox(w, h);
    ((p.0 - ox) / s, (p.1 - oy) / s)
}

/// Per-frame stage transform.
#[derive(Clone, Copy, Debug)]
pub struct Stage {
    /// Logical pixels per virtual unit.
    pub scale: f32,
    pub offset: Vec2,
    /// Physical pixels per logical pixel.
    pub dpi: f32,
}

impl Stage {
    pub fn compute() -> Stage {
        let (scale, ox, oy) = letterbox(screen_width(), screen_height());
        Stage {
            scale,
            offset: vec2(ox, oy),
            dpi: screen_dpi_scale(),
        }
    }

    /// Activates the stage camera (viewport in physical pixels).
    pub fn apply(&self) {
        let d = self.dpi;
        let vw = VIRTUAL_W * self.scale;
        let vh = VIRTUAL_H * self.scale;
        let cam = Camera2D {
            target: vec2(VIRTUAL_W * 0.5, VIRTUAL_H * 0.5),
            zoom: vec2(2.0 / VIRTUAL_W, 2.0 / VIRTUAL_H),
            viewport: Some((
                (self.offset.x * d).round() as i32,
                (self.offset.y * d).round() as i32,
                (vw * d).round() as i32,
                (vh * d).round() as i32,
            )),
            ..Default::default()
        };
        set_camera(&cam);
    }

    /// Mouse position in stage coordinates.
    pub fn mouse(&self) -> Vec2 {
        let (x, y) = window_to_stage(mouse_position(), screen_width(), screen_height());
        vec2(x, y)
    }

    /// Font size / scale pair rendering `world_size` units with a 1:1 texel
    /// mapping on screen (crisp text at any window size or DPI).
    pub fn font(&self, world_size: f32) -> (u16, f32) {
        let px = (world_size * self.scale).round().max(1.0);
        (px as u16, world_size / px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_wide_window() {
        let (s, ox, oy) = letterbox(2560.0, 1080.0);
        assert!((s - 1.5).abs() < 1e-6);
        assert!((ox - 320.0).abs() < 1e-3);
        assert_eq!(oy, 0.0);
    }

    #[test]
    fn mouse_maps_through_letterbox_at_150_percent() {
        // 1920x1080 physical at 150% DPI => 1280x720 logical: identity.
        let p = window_to_stage((640.0, 360.0), 1280.0, 720.0);
        assert_eq!(p, (640.0, 360.0));
        // Tall window: 1280x1000 => letterbox bars top/bottom of 140 px.
        let p = window_to_stage((0.0, 140.0), 1280.0, 1000.0);
        assert!((p.0).abs() < 1e-3 && (p.1).abs() < 1e-3);
        // Small window: half scale.
        let p = window_to_stage((320.0, 180.0), 640.0, 360.0);
        assert_eq!(p, (640.0, 360.0));
    }
}

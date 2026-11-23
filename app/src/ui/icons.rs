//! Transport icons drawn as rounded shapes (font glyphs like ▶ have sharp corners).

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Polygon points with each corner rounded by a quadratic curve of radius r.
pub fn rounded_polygon(corners: &[Pos2], r: f32, steps: usize) -> Vec<Pos2> {
    let n = corners.len();
    let steps = steps.max(1);
    let mut out = Vec::with_capacity(n * (steps + 1));
    for i in 0..n {
        let prev = corners[(i + n - 1) % n];
        let cur = corners[i];
        let next = corners[(i + 1) % n];
        let r1 = r.min((prev - cur).length() * 0.5);
        let r2 = r.min((next - cur).length() * 0.5);
        let s = cur + (prev - cur).normalized() * r1;
        let e = cur + (next - cur).normalized() * r2;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            out.push(pos2(
                u * u * s.x + 2.0 * u * t * cur.x + t * t * e.x,
                u * u * s.y + 2.0 * u * t * cur.y + t * t * e.y,
            ));
        }
    }
    out
}

fn triangle(
    p: &Painter,
    left: f32,
    right: f32,
    cy: f32,
    half_h: f32,
    pointing_right: bool,
    color: Color32,
) {
    let pts = if pointing_right {
        [
            pos2(left, cy - half_h),
            pos2(right, cy),
            pos2(left, cy + half_h),
        ]
    } else {
        [
            pos2(right, cy - half_h),
            pos2(right, cy + half_h),
            pos2(left, cy),
        ]
    };
    let r = half_h * 0.35;
    p.add(Shape::convex_polygon(
        rounded_polygon(&pts, r, 6),
        color,
        Stroke::NONE,
    ));
}

/// Play: a triangle with rounded corners, fitted in rect.
pub fn play(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.height().min(rect.width());
    let c = rect.center();
    let h = s * 0.42;
    let w = h * 1.75;
    // Shift right a little so it looks centred.
    let left = c.x - w * 0.45;
    triangle(p, left, left + w, c.y, h, true, color);
}

/// Pause: two rounded bars.
pub fn pause(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.height().min(rect.width());
    let c = rect.center();
    let bar = vec2(s * 0.26, s * 0.8);
    let gap = s * 0.2;
    for dx in [-(gap + bar.x) / 2.0, (gap + bar.x) / 2.0] {
        let r = Rect::from_center_size(pos2(c.x + dx, c.y), bar);
        p.rect_filled(r, s * 0.08, color);
    }
}

/// Next (forward = true) or previous: rounded triangle plus a rounded bar.
pub fn skip(p: &Painter, rect: Rect, color: Color32, forward: bool) {
    let s = rect.height().min(rect.width());
    let c = rect.center();
    let h = s * 0.34;
    let w = h * 1.6;
    let bar_w = s * 0.14;
    let total = w + bar_w * 1.4;
    let x0 = c.x - total / 2.0;
    if forward {
        triangle(p, x0, x0 + w, c.y, h, true, color);
        let bar = Rect::from_min_size(pos2(x0 + total - bar_w, c.y - h), vec2(bar_w, h * 2.0));
        p.rect_filled(bar, bar_w * 0.5, color);
    } else {
        let bar = Rect::from_min_size(pos2(x0, c.y - h), vec2(bar_w, h * 2.0));
        p.rect_filled(bar, bar_w * 0.5, color);
        triangle(p, x0 + total - w, x0 + total, c.y, h, false, color);
    }
}

/// A clickable icon of the given size; when disabled it is dimmed and ignores clicks.
pub fn icon_button(
    ui: &mut egui::Ui,
    size: f32,
    enabled: bool,
    draw: impl Fn(&Painter, Rect, Color32),
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), sense);
    let v = ui.visuals();
    let color = if !enabled {
        v.weak_text_color()
    } else if resp.hovered() {
        v.strong_text_color()
    } else {
        v.text_color()
    };
    draw(ui.painter(), rect, color);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_polygon_points() {
        let sq = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
        ];
        let pts = rounded_polygon(&sq, 2.0, 4);
        assert_eq!(pts.len(), 4 * 5);
        assert!(
            pts.iter()
                .all(|p| (0.0..=10.0).contains(&p.x) && (0.0..=10.0).contains(&p.y))
        );
        let is_corner = |p: &Pos2| (p.x - 10.0).abs() < 1e-3 && p.y.abs() < 1e-3;
        assert!(!pts.iter().any(is_corner), "corner should be cut");
        let sharp = rounded_polygon(&sq, 0.0, 4);
        assert!(sharp.iter().any(is_corner), "r = 0 keeps the corner");
    }
}

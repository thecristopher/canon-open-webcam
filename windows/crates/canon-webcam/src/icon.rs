//! The app's icon, shared with build.rs so the .exe, tray and windows all wear the same one.

/// The app's badge: a white camera on a blue rounded square, readable on light and dark taskbars alike.
/// RGBA rows, drawn here so there's no icon file to ship. build.rs draws the .exe's icon with it too.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32 / 32.0;
    let inside_round_rect = |x: f32, y: f32, (l, t, r, b): (f32, f32, f32, f32), radius: f32| {
        let (l, t, r, b, radius) = (l * s, t * s, r * s, b * s, radius * s);
        let cx = x.clamp(l + radius, r - radius);
        let cy = y.clamp(t + radius, b - radius);
        (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius && x >= l && x <= r && y >= t && y <= b
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let i = ((y * size + x) * 4) as usize;
            if !inside_round_rect(fx, fy, (0.0, 0.0, 32.0, 32.0), 7.0) {
                continue;
            }
            // A gentle top to bottom blue
            let shade = fy / size as f32;
            let mut color =
                [(52.0 - 18.0 * shade) as u8, (120.0 - 40.0 * shade) as u8, (246.0 - 40.0 * shade) as u8, 255];
            let body = inside_round_rect(fx, fy, (6.0, 11.0, 26.0, 25.0), 3.0)
                || inside_round_rect(fx, fy, (12.0, 8.0, 20.0, 13.0), 1.5);
            let distance = ((fx - 16.0 * s).powi(2) + (fy - 18.0 * s).powi(2)).sqrt() / s;
            if body && !(3.2..5.2).contains(&distance) {
                color = [255, 255, 255, 255];
            }
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    pixels
}

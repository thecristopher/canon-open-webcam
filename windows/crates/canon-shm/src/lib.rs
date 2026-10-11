//! The frame buffer the engine fills and the virtual camera reads.
//!
//! One named section holds a small header and a single NV12 frame. The virtual camera lives in
//! Windows' camera service (Frame Server, running as LOCAL SERVICE) and the engine lives in the
//! user's session, so the section sits in the Global namespace with a DACL both sides can use.
//! Whoever is allowed to create it does; a normal user can't create Global objects, the service can.
//!
//! The header doubles as the on demand signal: the camera side stamps `consumer_tick` every time
//! an app asks for a frame, and the engine only wakes the real camera while that stamp is fresh.

/// What apps list the virtual camera as.
pub const CAMERA_NAME: &str = "Canon Webcam";
/// The class id of the virtual camera's media source, registered in HKLM\SOFTWARE\Classes\CLSID.
pub const CLSID_STRING: &str = "{B1495EF3-1D7F-4ADA-8E74-2C92B1B1F3E3}";
pub const CLSID_U128: u128 = 0xB1495EF3_1D7F_4ADA_8E74_2C92B1B1F3E3;

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const FPS: u32 = 30;
pub const FRAME_BYTES: usize = (WIDTH * HEIGHT * 3 / 2) as usize;

const MAGIC: u32 = u32::from_le_bytes(*b"CWCF");
const VERSION: u32 = 1;
// The frame starts one page in, so the header can grow without moving it
const FRAME_OFFSET: usize = 4096;
const SECTION_BYTES: usize = FRAME_OFFSET + FRAME_BYTES;

#[cfg(windows)]
mod section;
#[cfg(windows)]
pub use section::{SharedFrames, tick_ms};

/// Turns a top down BGRA image into NV12 (BT.709, limited range), the format the camera offers.
pub fn bgra_to_nv12(bgra: &[u8], width: usize, height: usize, nv12: &mut [u8]) {
    to_nv12(bgra, 4, [2, 1, 0], width, height, nv12);
}

/// Same for tightly packed RGB, which is what the JPEG decoder hands us.
pub fn rgb_to_nv12(rgb: &[u8], width: usize, height: usize, nv12: &mut [u8]) {
    to_nv12(rgb, 3, [0, 1, 2], width, height, nv12);
}

fn to_nv12(pixels: &[u8], step: usize, [r, g, b]: [usize; 3], width: usize, height: usize, nv12: &mut [u8]) {
    let (luma, chroma) = nv12.split_at_mut(width * height);
    // Fixed point BT.709 limited range, coefficients scaled by 256 and rounded so each chroma row sums to zero
    for y in 0..height {
        let row = &pixels[y * width * step..(y + 1) * width * step];
        let out = &mut luma[y * width..(y + 1) * width];
        for (x, px) in row.chunks_exact(step).enumerate() {
            let (pr, pg, pb) = (px[r] as i32, px[g] as i32, px[b] as i32);
            out[x] = (((47 * pr + 157 * pg + 16 * pb + 128) >> 8) + 16) as u8;
        }
    }
    for cy in 0..height / 2 {
        let out = &mut chroma[cy * width..(cy + 1) * width];
        for cx in 0..width / 2 {
            // Average the 2x2 block so edges don't shimmer
            let (mut sr, mut sg, mut sb) = (0i32, 0i32, 0i32);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let i = ((cy * 2 + dy) * width + cx * 2 + dx) * step;
                sr += pixels[i + r] as i32;
                sg += pixels[i + g] as i32;
                sb += pixels[i + b] as i32;
            }
            let (pr, pg, pb) = (sr / 4, sg / 4, sb / 4);
            out[cx * 2] = (((-26 * pr - 87 * pg + 113 * pb + 128) >> 8) + 128).clamp(0, 255) as u8;
            out[cx * 2 + 1] = (((112 * pr - 102 * pg - 10 * pb + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv12_of(rgb: [u8; 3]) -> (u8, u8, u8) {
        let pixels: Vec<u8> = rgb.iter().copied().cycle().take(4 * 4 * 3).collect();
        let mut nv12 = vec![0u8; 4 * 4 * 3 / 2];
        rgb_to_nv12(&pixels, 4, 4, &mut nv12);
        (nv12[0], nv12[16], nv12[17])
    }

    #[test]
    fn black_and_white_land_on_the_limited_range_ends() {
        assert_eq!(nv12_of([0, 0, 0]), (16, 128, 128));
        assert_eq!(nv12_of([255, 255, 255]), (235, 128, 128));
    }

    #[test]
    fn red_pushes_v_up_and_u_down() {
        let (y, u, v) = nv12_of([255, 0, 0]);
        assert!((60..66).contains(&y), "y {y}");
        assert!(u < 110 && v > 230, "u {u} v {v}");
    }

    #[test]
    fn bgra_reads_the_channels_in_its_own_order() {
        let mut nv12 = vec![0u8; 4 * 4 * 3 / 2];
        let blue_bgra: Vec<u8> = [255u8, 0, 0, 255].iter().copied().cycle().take(4 * 4 * 4).collect();
        bgra_to_nv12(&blue_bgra, 4, 4, &mut nv12);
        assert!(nv12[16] > 230, "blue should push u up, got {}", nv12[16]);
    }
}

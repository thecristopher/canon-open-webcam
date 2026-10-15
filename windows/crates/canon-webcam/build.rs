//! Gives CanonWebcam.exe its icon for Explorer, the taskbar and Installed apps.
//! Draws the same badge as the tray at every size Windows asks for, packs them into a .res file
//! and hands that to the linker, so no icon file or resource compiler is needed.

#[allow(dead_code)]
#[path = "src/icon.rs"]
mod icon;

use std::path::PathBuf;

/// What Explorer, the taskbar, Alt+Tab and the Start menu pick from at 100% to 200% scaling
const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
/// Drawn this many times bigger and shrunk, so the edges come out smooth instead of stair stepped
const SUPERSAMPLE: u32 = 4;
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    // link.exe takes a .res as is, other linkers would need it converted first
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    let mut res = Vec::new();
    // A .res file opens with an empty entry
    push_resource(&mut res, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    for (index, &size) in SIZES.iter().enumerate() {
        let image = encode(size, &smooth_icon(size));
        let id = index as u16 + 1;
        push_resource(&mut res, RT_ICON, id, 0x1010, &image);
        // 0 stands for 256
        group.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(image.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    // Explorer shows the first icon group, and this is the only one
    push_resource(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);

    let path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icon.res");
    std::fs::write(&path, res).unwrap();
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

/// The badge drawn big and averaged down, weighting by alpha so the corners don't go dark
fn smooth_icon(size: u32) -> Vec<u8> {
    let big = icon::icon_rgba(size * SUPERSAMPLE);
    let row = (size * SUPERSAMPLE) as usize;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size as usize {
        for x in 0..size as usize {
            let mut sum = [0u32; 4];
            for dy in 0..SUPERSAMPLE as usize {
                for dx in 0..SUPERSAMPLE as usize {
                    let i = ((y * SUPERSAMPLE as usize + dy) * row + x * SUPERSAMPLE as usize + dx) * 4;
                    let alpha = big[i + 3] as u32;
                    for c in 0..3 {
                        sum[c] += big[i + c] as u32 * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let i = (y * size as usize + x) * 4;
            for c in 0..3 {
                pixels[i + c] = sum[c].checked_div(sum[3]).unwrap_or(0) as u8;
            }
            pixels[i + 3] = (sum[3] / (SUPERSAMPLE * SUPERSAMPLE)) as u8;
        }
    }
    pixels
}

/// The big size goes in as a PNG to keep the .exe small, the rest as the 32 bit bitmaps every Windows reads
fn encode(size: u32, rgba: &[u8]) -> Vec<u8> {
    if size >= 256 {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(rgba).unwrap();
        return out;
    }
    let mut out = Vec::new();
    // BITMAPINFOHEADER, twice as tall because the AND mask follows the colors
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    // Bottom up BGRA rows
    for y in (0..size as usize).rev() {
        for x in 0..size as usize {
            let i = (y * size as usize + x) * 4;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // The AND mask, left clear since alpha does its job, with rows padded to 4 bytes
    let mask_row = size.div_ceil(32) as usize * 4;
    out.resize(out.len() + mask_row * size as usize, 0);
    out
}

/// One entry of a .res file: a header naming it by number, then its data, both padded to 4 bytes
fn push_resource(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    for number in [kind, id] {
        res.extend_from_slice(&0xFFFFu16.to_le_bytes());
        res.extend_from_slice(&number.to_le_bytes());
    }
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&flags.to_le_bytes());
    // US English, like rc.exe uses by default
    res.extend_from_slice(&(if kind == 0 { 0u16 } else { 0x0409 }).to_le_bytes());
    res.extend_from_slice(&[0; 8]);
    res.extend_from_slice(data);
    res.resize(res.len().next_multiple_of(4), 0);
}

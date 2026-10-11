use anyhow::{Context, Result, bail};
use canon_shm::{FRAME_BYTES, HEIGHT, WIDTH, bgra_to_nv12, rgb_to_nv12};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::options::{ADJUSTMENT_RANGE, FrameOptions, ZOOM_RANGE};

/// Turns live view JPEGs into 1080p NV12 with the person's picture and framing applied,
/// reusing its buffers frame after frame.
pub struct Picture {
    resizer: Resizer,
    rgb: Vec<u8>,
    scaled: Image<'static>,
    nv12: Vec<u8>,
    blur: Vec<u16>,
}

impl Picture {
    pub fn new() -> Self {
        Self {
            resizer: Resizer::new(),
            rgb: Vec::new(),
            scaled: Image::new(WIDTH, HEIGHT, PixelType::U8x3),
            nv12: vec![0; FRAME_BYTES],
            blur: Vec::new(),
        }
    }

    pub fn jpeg_to_nv12(&mut self, jpeg: &[u8], options: &FrameOptions) -> Result<&[u8]> {
        // A frame the camera cut off decodes into garbage, so drop it here instead
        if !looks_complete(jpeg) {
            bail!("live view frame was cut off");
        }
        let decoder_options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(jpeg), decoder_options);
        decoder.decode_headers().context("live view frame isn't a JPEG")?;
        let info = decoder.info().context("JPEG without a size")?;
        let (width, height) = (info.width as u32, info.height as u32);
        self.rgb.resize(decoder.output_buffer_size().context("JPEG size overflow")?, 0);
        decoder.decode_into(&mut self.rgb).context("couldn't decode the live view JPEG")?;

        // Color work happens on the small camera frame, it's a fraction of the pixels
        adjust_colors(&mut self.rgb, options);
        let (left, top, crop_width, crop_height) = visible_region(width as f64, height as f64, options);
        let source = ImageRef::new(width, height, &self.rgb, PixelType::U8x3)?;
        let resize = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)).crop(
            left,
            top,
            crop_width,
            crop_height,
        );
        self.resizer.resize(&source, &mut self.scaled, &resize)?;
        rgb_to_nv12(self.scaled.buffer(), WIDTH as usize, HEIGHT as usize, &mut self.nv12);
        if options.sharpness > 0.0 {
            sharpen_luma(&mut self.nv12, &mut self.blur, options.sharpness);
        }
        if options.mirror {
            mirror_nv12(&mut self.nv12);
        }
        Ok(&self.nv12)
    }
}

/// Every live view frame starts with FFD8 and ends with FFD9, a cut off one doesn't.
pub fn looks_complete(jpeg: &[u8]) -> bool {
    jpeg.len() > 4 && jpeg.starts_with(&[0xFF, 0xD8]) && jpeg.ends_with(&[0xFF, 0xD9])
}

/// The part of the camera frame that ends up on screen: cropped to fill 16:9, then zoomed and panned.
/// Panning slides across whatever overflows the frame, never past its edges.
pub fn visible_region(width: f64, height: f64, options: &FrameOptions) -> (f64, f64, f64, f64) {
    let zoom = options.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
    let fill = (WIDTH as f64 / width).max(HEIGHT as f64 / height);
    let scale = fill * zoom;
    let (visible_width, visible_height) = (WIDTH as f64 / scale, HEIGHT as f64 / scale);
    let pan = |value: f64| value.clamp(*ADJUSTMENT_RANGE.start(), *ADJUSTMENT_RANGE.end());
    let left = (width - visible_width) / 2.0 * (1.0 + pan(options.pan_x));
    // Pan y of 1 means the top edge, and image rows count down from the top
    let top = (height - visible_height) / 2.0 * (1.0 - pan(options.pan_y));
    (left, top, visible_width, visible_height)
}

/// Brightness, contrast, warmth and tint as one lookup table per channel, then saturation per pixel.
/// Tuned by eye to land close to the Mac app's Core Image filters.
pub fn adjust_colors(rgb: &mut [u8], options: &FrameOptions) {
    if !options.has_color_adjustments() {
        return;
    }
    // Warm pushes red up and blue down, tint toward magenta pulls green down
    let gains = [1.0 + 0.18 * options.warmth, 1.0 - 0.12 * options.tint, 1.0 - 0.18 * options.warmth];
    let contrast = 1.0 + options.contrast * 0.5;
    let brightness = options.brightness * 0.25 * 255.0;
    let tables: Vec<[u8; 256]> = gains
        .iter()
        .map(|gain| {
            let mut table = [0u8; 256];
            for (value, out) in table.iter_mut().enumerate() {
                let v = value as f64 * gain + brightness;
                *out = ((v - 127.5) * contrast + 127.5).round().clamp(0.0, 255.0) as u8;
            }
            table
        })
        .collect();
    let saturation = ((1.0 + options.saturation) * 256.0) as i32;
    for pixel in rgb.as_chunks_mut::<3>().0 {
        let [r, g, b] = [tables[0][pixel[0] as usize], tables[1][pixel[1] as usize], tables[2][pixel[2] as usize]];
        if saturation == 256 {
            *pixel = [r, g, b];
            continue;
        }
        let luma = (54 * r as i32 + 183 * g as i32 + 19 * b as i32) >> 8;
        let saturate = |c: u8| (luma + (((c as i32 - luma) * saturation) >> 8)).clamp(0, 255) as u8;
        *pixel = [saturate(r), saturate(g), saturate(b)];
    }
}

/// An unsharp mask on brightness only, which brings back crispness lost to upscaling without color fringes.
pub fn sharpen_luma(nv12: &mut [u8], scratch: &mut Vec<u16>, amount: f64) {
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let luma = &mut nv12[..w * h];
    scratch.resize(w * h, 0);
    // A 3x3 box blur, done as two passes, stands in for Core Image's 1.5px radius
    for y in 0..h {
        let row = &luma[y * w..(y + 1) * w];
        let out = &mut scratch[y * w..(y + 1) * w];
        for x in 0..w {
            let (l, r) = (row[x.saturating_sub(1)], row[(x + 1).min(w - 1)]);
            out[x] = l as u16 + row[x] as u16 + r as u16;
        }
    }
    let strength = (amount * 1.5 * 256.0) as i32;
    for y in 0..h {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            let sum = scratch[up * w + x] as i32 + scratch[y * w + x] as i32 + scratch[down * w + x] as i32;
            let original = luma[y * w + x] as i32;
            let detail = original - sum / 9;
            luma[y * w + x] = (original + ((detail * strength) >> 8)).clamp(16, 235) as u8;
        }
    }
}

/// Flips the frame left to right: luma byte by byte, chroma as U,V pairs.
pub fn mirror_nv12(nv12: &mut [u8]) {
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let (luma, chroma) = nv12.split_at_mut(w * h);
    for row in luma.chunks_exact_mut(w) {
        row.reverse();
    }
    for row in chroma.chunks_exact_mut(w) {
        row.as_chunks_mut::<2>().0.reverse();
    }
}

/// A full screen message card, drawn with GDI so it uses the system's own font.
#[cfg(windows)]
pub fn card(title: &str, detail: &str) -> Vec<u8> {
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::*;
    use windows_core::{HSTRING, w};

    let (w, h) = (WIDTH as i32, HEIGHT as i32);
    let mut nv12 = vec![0u8; FRAME_BYTES];
    unsafe {
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return nv12;
        };
        let previous = SelectObject(dc, bitmap.into());

        let background = CreateSolidBrush(COLORREF(0x0026_2220));
        FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, background);
        let _ = DeleteObject(background.into());
        SetBkMode(dc, TRANSPARENT);

        let draw = |text: &str, size: i32, weight: i32, color: u32, area: RECT, flags: DRAW_TEXT_FORMAT| {
            let font = CreateFontW(
                -size,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            );
            let old = SelectObject(dc, font.into());
            SetTextColor(dc, COLORREF(color));
            let mut wide: Vec<u16> = HSTRING::from(text).to_vec();
            let mut area = area;
            DrawTextW(dc, &mut wide, &mut area, flags);
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
        };
        let title_area = RECT { left: 120, top: h / 2 - 140, right: w - 120, bottom: h / 2 - 20 };
        draw(title, 72, 600, 0x00F2_F2F2, title_area, DT_CENTER | DT_SINGLELINE | DT_BOTTOM);
        let detail_area = RECT { left: 260, top: h / 2 + 10, right: w - 260, bottom: h - 120 };
        draw(detail, 40, 400, 0x00B4_AEA8, detail_area, DT_CENTER | DT_WORDBREAK);
        let footer_area = RECT { left: 0, top: h - 110, right: w, bottom: h - 50 };
        draw("Canon Webcam", 30, 600, 0x0070_6A66, footer_area, DT_CENTER | DT_SINGLELINE);
        let _ = GdiFlush();

        let pixels = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        bgra_to_nv12(pixels, w as usize, h as usize, &mut nv12);

        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
    nv12
}

#[cfg(test)]
mod tests {
    use super::*;

    // 96x54 mid grey with a white bar down the left, made with Windows' own JPEG encoder
    const JPEG: &[u8] = include_bytes!("../fixtures/grey-16x9.jpg");

    fn luma(nv12: &[u8], x: usize, y: usize) -> u8 {
        nv12[y * WIDTH as usize + x]
    }

    #[test]
    fn a_small_jpeg_fills_the_whole_frame() {
        let mut picture = Picture::new();
        let nv12 = picture.jpeg_to_nv12(JPEG, &FrameOptions { sharpness: 0.0, ..Default::default() }).unwrap();
        assert_eq!(nv12.len(), FRAME_BYTES);
        let middle = luma(nv12, WIDTH as usize / 2, HEIGHT as usize / 2);
        assert!((115..135).contains(&middle), "mid grey should stay mid grey, got luma {middle}");
        assert!(luma(nv12, 100, HEIGHT as usize / 2) > 220, "the white bar should survive the upscale");
    }

    #[test]
    fn mirror_moves_the_white_bar_to_the_right() {
        let mut picture = Picture::new();
        let nv12 = picture.jpeg_to_nv12(JPEG, &FrameOptions { mirror: true, ..Default::default() }).unwrap();
        assert!(luma(nv12, WIDTH as usize - 100, HEIGHT as usize / 2) > 220);
        assert!(luma(nv12, 100, HEIGHT as usize / 2) < 160);
    }

    #[test]
    fn zooming_in_pushes_the_bar_out_of_view() {
        let mut picture = Picture::new();
        let nv12 = picture.jpeg_to_nv12(JPEG, &FrameOptions { zoom: 2.0, ..Default::default() }).unwrap();
        assert!(luma(nv12, 10, HEIGHT as usize / 2) < 160, "at 2x the left sixth of the frame is cropped away");
    }

    #[test]
    fn panning_left_brings_it_back() {
        let mut picture = Picture::new();
        let options = FrameOptions { zoom: 2.0, pan_x: -1.0, ..Default::default() };
        let nv12 = picture.jpeg_to_nv12(JPEG, &options).unwrap();
        assert!(luma(nv12, 10, HEIGHT as usize / 2) > 220);
    }

    #[test]
    fn brightness_lifts_the_grey() {
        let mut picture = Picture::new();
        let plain = luma(picture.jpeg_to_nv12(JPEG, &FrameOptions::default()).unwrap(), 960, 540);
        let brighter = luma(
            picture.jpeg_to_nv12(JPEG, &FrameOptions { brightness: 0.8, ..Default::default() }).unwrap(),
            960,
            540,
        );
        assert!(brighter > plain + 20, "{plain} -> {brighter}");
    }

    #[test]
    fn warmth_tips_red_over_blue() {
        let mut rgb = vec![128u8; 3 * 4];
        adjust_colors(&mut rgb, &FrameOptions { warmth: 1.0, ..Default::default() });
        assert!(rgb[0] > rgb[2] + 30, "{rgb:?}");
    }

    #[test]
    fn no_saturation_means_grey() {
        let mut rgb = vec![200, 50, 50];
        adjust_colors(&mut rgb, &FrameOptions { saturation: -1.0, ..Default::default() });
        assert!(rgb[0] == rgb[1] && rgb[1] == rgb[2], "{rgb:?}");
    }

    #[test]
    fn the_visible_region_stays_inside_the_frame() {
        for pan in [-1.0, 0.0, 1.0] {
            let options = FrameOptions { zoom: 3.0, pan_x: pan, pan_y: pan, ..Default::default() };
            let (left, top, w, h) = visible_region(1024.0, 576.0, &options);
            assert!(left >= 0.0 && top >= 0.0 && left + w <= 1024.0 + 1e-6 && top + h <= 576.0 + 1e-6);
        }
    }

    #[test]
    fn a_three_by_two_frame_is_cropped_not_letterboxed() {
        let (_, top, w, h) = visible_region(960.0, 640.0, &FrameOptions::default());
        assert!((w / h - 16.0 / 9.0).abs() < 1e-6 && top > 0.0);
    }

    #[test]
    fn cut_off_frames_are_dropped() {
        assert!(Picture::new().jpeg_to_nv12(&JPEG[..JPEG.len() - 10], &FrameOptions::default()).is_err());
        assert!(Picture::new().jpeg_to_nv12(&[1, 2, 3, 4], &FrameOptions::default()).is_err());
    }
}

#[cfg(test)]
mod speed {
    use super::*;

    // Times a real sized live view frame: CANON_WEBCAM_BENCH_JPEG=frame.jpg cargo test --release -- --ignored speed
    #[test]
    #[ignore]
    fn speed_of_one_frame() {
        let Some(path) = std::env::var_os("CANON_WEBCAM_BENCH_JPEG") else { return };
        let jpeg = std::fs::read(path).unwrap();
        let mut picture = Picture::new();
        let busy = FrameOptions {
            brightness: 0.2,
            saturation: 0.3,
            warmth: 0.2,
            zoom: 1.4,
            mirror: true,
            ..Default::default()
        };
        for (label, options) in [("defaults", FrameOptions::default()), ("everything on", busy)] {
            picture.jpeg_to_nv12(&jpeg, &options).unwrap();
            let started = std::time::Instant::now();
            for _ in 0..50 {
                picture.jpeg_to_nv12(&jpeg, &options).unwrap();
            }
            println!("{label}: {:.1} ms per frame", started.elapsed().as_secs_f64() * 1000.0 / 50.0);
        }
    }
}

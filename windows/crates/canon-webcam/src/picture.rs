use anyhow::{Context, Result};
use canon_shm::{FRAME_BYTES, HEIGHT, WIDTH, bgra_to_nv12, rgb_to_nv12};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

/// Turns live view JPEGs into 1080p NV12, reusing its buffers frame after frame.
pub struct Picture {
    resizer: Resizer,
    options: ResizeOptions,
    rgb: Vec<u8>,
    scaled: Image<'static>,
    nv12: Vec<u8>,
}

impl Picture {
    pub fn new() -> Self {
        Self {
            resizer: Resizer::new(),
            // Lanczos keeps edges crisp on the way up, and cropping to fill means 3:2 bodies get no black bars
            options: ResizeOptions::new()
                .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
                .fit_into_destination(Some((0.5, 0.5))),
            rgb: Vec::new(),
            scaled: Image::new(WIDTH, HEIGHT, PixelType::U8x3),
            nv12: vec![0; FRAME_BYTES],
        }
    }

    pub fn jpeg_to_nv12(&mut self, jpeg: &[u8]) -> Result<&[u8]> {
        let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(jpeg), options);
        decoder
            .decode_headers()
            .context("live view frame isn't a JPEG")?;
        let info = decoder.info().context("JPEG without a size")?;
        let (width, height) = (info.width as u32, info.height as u32);
        self.rgb.resize(
            decoder.output_buffer_size().context("JPEG size overflow")?,
            0,
        );
        decoder
            .decode_into(&mut self.rgb)
            .context("couldn't decode the live view JPEG")?;

        let source = ImageRef::new(width, height, &self.rgb, PixelType::U8x3)?;
        self.resizer
            .resize(&source, &mut self.scaled, &self.options)?;
        rgb_to_nv12(
            self.scaled.buffer(),
            WIDTH as usize,
            HEIGHT as usize,
            &mut self.nv12,
        );
        Ok(&self.nv12)
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
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(dc);
            return nv12;
        };
        let previous = SelectObject(dc, bitmap.into());

        let background = CreateSolidBrush(COLORREF(0x0026_2220));
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            background,
        );
        let _ = DeleteObject(background.into());
        SetBkMode(dc, TRANSPARENT);

        let draw = |text: &str,
                    size: i32,
                    weight: i32,
                    color: u32,
                    area: RECT,
                    flags: DRAW_TEXT_FORMAT| {
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
        draw(
            title,
            72,
            600,
            0x00F2_F2F2,
            RECT {
                left: 120,
                top: h / 2 - 140,
                right: w - 120,
                bottom: h / 2 - 20,
            },
            DT_CENTER | DT_SINGLELINE | DT_BOTTOM,
        );
        draw(
            detail,
            40,
            400,
            0x00B4_AEA8,
            RECT {
                left: 260,
                top: h / 2 + 10,
                right: w - 260,
                bottom: h - 120,
            },
            DT_CENTER | DT_WORDBREAK,
        );
        draw(
            "Canon Webcam",
            30,
            600,
            0x0070_6A66,
            RECT {
                left: 0,
                top: h - 110,
                right: w,
                bottom: h - 50,
            },
            DT_CENTER | DT_SINGLELINE,
        );
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

    #[test]
    fn a_small_jpeg_fills_the_whole_frame() {
        // 96x54 mid grey with a white bar down the left, made with Windows' own JPEG encoder
        let jpeg = include_bytes!("../fixtures/grey-16x9.jpg");
        let mut picture = Picture::new();
        let nv12 = picture.jpeg_to_nv12(jpeg).expect("decodes");
        assert_eq!(nv12.len(), FRAME_BYTES);
        let luma = |x: usize, y: usize| nv12[y * WIDTH as usize + x];
        let middle = luma(WIDTH as usize / 2, HEIGHT as usize / 2);
        assert!(
            (115..135).contains(&middle),
            "mid grey should stay mid grey, got luma {middle}"
        );
        assert!(
            luma(100, HEIGHT as usize / 2) > 220,
            "the white bar should survive the upscale"
        );
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(Picture::new().jpeg_to_nv12(&[1, 2, 3, 4]).is_err());
    }
}

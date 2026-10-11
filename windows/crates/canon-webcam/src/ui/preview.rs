use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use eframe::egui::ColorImage;
use windows::Win32::Media::MediaFoundation::{IMFSourceReader, MF_MT_FRAME_SIZE, MF_SOURCE_READER_FIRST_VIDEO_STREAM};

use crate::diagnose::{MediaFoundation, open_virtual_camera};

/// Watches "Canon Webcam" exactly the way a call app does, so the window shows what everyone sees.
/// Opening it also counts as an app watching, which is what wakes the camera up.
pub struct Preview {
    latest: Arc<Mutex<Option<ColorImage>>>,
    stop: Arc<AtomicBool>,
}

impl Preview {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (frames, stopping) = (latest.clone(), stop.clone());
        std::thread::Builder::new()
            .name("preview".into())
            .spawn(move || {
                let mut last_problem = String::new();
                while !stopping.load(Ordering::Relaxed) {
                    // Retrying every couple of seconds shouldn't write the same line to the log forever
                    if let Err(e) = watch(&frames, &stopping, &ctx)
                        && format!("{e:#}") != last_problem
                    {
                        last_problem = format!("{e:#}");
                        crate::log::record(&format!("preview: {last_problem}"));
                    }
                    // Not installed yet, or Frame Server restarted: try again shortly
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
            .ok();
        Self { latest, stop }
    }

    /// The newest frame since the last call, if there is one.
    pub fn take(&self) -> Option<ColorImage> {
        self.latest.lock().unwrap().take()
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn watch(latest: &Mutex<Option<ColorImage>>, stop: &AtomicBool, ctx: &eframe::egui::Context) -> Result<()> {
    let _mf = MediaFoundation::start()?;
    let (source, reader) = open_virtual_camera()?;
    let result = read_frames(&reader, latest, stop, ctx);
    unsafe {
        let _ = source.Shutdown();
    }
    result
}

fn read_frames(
    reader: &IMFSourceReader,
    latest: &Mutex<Option<ColorImage>>,
    stop: &AtomicBool,
    ctx: &eframe::egui::Context,
) -> Result<()> {
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let size = unsafe { reader.GetCurrentMediaType(stream)?.GetUINT64(&MF_MT_FRAME_SIZE)? };
    let (width, height) = ((size >> 32) as usize, (size & 0xFFFF_FFFF) as usize);
    while !stop.load(Ordering::Relaxed) {
        // The flags are required even though we don't look at them, or Media Foundation answers E_POINTER
        let mut flags = 0u32;
        let mut sample = None;
        unsafe { reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))? };
        let Some(sample) = sample else { continue };
        let image = unsafe {
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer.Lock(&mut data, None, Some(&mut length))?;
            let nv12 = std::slice::from_raw_parts(data, length as usize);
            let image = (nv12.len() >= width * height * 3 / 2).then(|| half_size_rgba(nv12, width, height));
            buffer.Unlock()?;
            image
        };
        if let Some(image) = image {
            *latest.lock().unwrap() = Some(image);
            ctx.request_repaint();
        }
    }
    Ok(())
}

/// NV12 to RGBA at half size, plenty for a preview and a quarter of the work.
fn half_size_rgba(nv12: &[u8], width: usize, height: usize) -> ColorImage {
    let (w, h) = (width / 2, height / 2);
    let chroma = &nv12[width * height..];
    let mut rgba = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let luma = (nv12[(y * 2) * width + x * 2] as i32 - 16) * 298;
            let c = y * width + x * 2;
            let (u, v) = (chroma[c] as i32 - 128, chroma[c + 1] as i32 - 128);
            let i = (y * w + x) * 4;
            rgba[i] = ((luma + 459 * v + 128) >> 8).clamp(0, 255) as u8;
            rgba[i + 1] = ((luma - 55 * u - 136 * v + 128) >> 8).clamp(0, 255) as u8;
            rgba[i + 2] = ((luma + 541 * u + 128) >> 8).clamp(0, 255) as u8;
        }
    }
    ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

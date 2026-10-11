//! Commands for checking things from a terminal: list, probe and check.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use canon_ptp::{CanonCamera, CanonProperty};
use canon_shm::CAMERA_NAME;
use canon_wpd::{ComApartment, WpdTransport, list_devices};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows_core::PWSTR;

pub fn list() -> Result<()> {
    let _com = ComApartment::enter()?;
    let devices = list_devices().context("couldn't ask Windows for portable devices")?;
    if devices.is_empty() {
        println!("No portable devices. Is the camera on and plugged in?");
    }
    for device in devices {
        let tag = if device.is_canon() { "canon" } else { "     " };
        println!(
            "{tag}  {}  ({}, {})\n       {}",
            device.name, device.manufacturer, device.description, device.id
        );
    }
    Ok(())
}

/// Talks to the first Canon directly, without the virtual camera, and saves one live view JPEG.
pub fn probe(out: &Path) -> Result<()> {
    let _com = ComApartment::enter()?;
    let devices = list_devices().context("couldn't ask Windows for portable devices")?;
    let Some(entry) = devices.iter().find(|d| d.is_canon()) else {
        bail!(
            "No Canon found. Turn the camera on, plug it in, and quit Canon Webcam and anything else using it"
        );
    };
    println!("Opening {}", entry.name);
    let transport = WpdTransport::open(entry).context("Windows wouldn't open the camera")?;
    match transport.vendor_operations() {
        Ok(ops) => {
            let missing: Vec<String> = [0x9110u32, 0x9114, 0x9115, 0x9116, 0x9153]
                .iter()
                .filter(|op| !ops.contains(op))
                .map(|op| format!("0x{op:04X}"))
                .collect();
            let note = if missing.is_empty() {
                String::new()
            } else {
                format!(", missing {}", missing.join(" "))
            };
            println!("Camera offers {} vendor operations{note}", ops.len());
        }
        Err(e) => println!("Couldn't list vendor operations ({e}), trying anyway"),
    }

    let mut camera = CanonCamera::new(transport);
    camera
        .connect()
        .context("camera wouldn't enter remote mode")?;
    println!("Connected to {}", camera.model());
    if let Some(battery) = camera.state().value(CanonProperty::Battery) {
        println!("Battery level code {battery}");
    }
    if let Some(lens) = &camera.state().lens_name {
        println!("Lens {lens}");
    }
    camera
        .start_live_view()
        .context("camera wouldn't start live view")?;
    let started = Instant::now();
    let result = loop {
        match camera.next_frame() {
            Ok(Some(jpeg)) => break Ok(jpeg),
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(30))
            }
            Ok(None) => {
                break Err(anyhow::anyhow!(
                    "camera never produced a frame in 5 seconds"
                ));
            }
            Err(e) => break Err(e.into()),
        }
    };
    camera.disconnect();
    let jpeg = result?;
    std::fs::write(out, &jpeg).with_context(|| format!("couldn't write {}", out.display()))?;
    println!(
        "Saved a {} KB frame to {} after {} ms",
        jpeg.len() / 1024,
        out.display(),
        started.elapsed().as_millis()
    );
    Ok(())
}

/// Opens "Canon Webcam" the way Zoom would, reads frames for a few seconds and saves the last one.
pub fn check(out: &Path) -> Result<()> {
    let _com = ComApartment::enter()?;
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL)? };
    let result = unsafe { read_virtual_camera(out) };
    unsafe {
        let _ = MFShutdown();
    }
    result
}

unsafe fn read_virtual_camera(out: &Path) -> Result<()> {
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1)?;
        let attributes = attributes.context("no attributes")?;
        attributes.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )?;
        let mut devices = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attributes, &mut devices, &mut count)?;
        let list = std::slice::from_raw_parts(devices, count as usize);

        let mut names = Vec::new();
        let mut ours = None;
        for device in list.iter().flatten() {
            let mut name = PWSTR::null();
            let mut length = 0;
            if device
                .GetAllocatedString(
                    &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                    &mut name,
                    &mut length,
                )
                .is_ok()
            {
                let text = name.to_string().unwrap_or_default();
                CoTaskMemFree(Some(name.0 as _));
                // Windows adds " (Windows Virtual Camera)" to the name we registered
                if text.starts_with(CAMERA_NAME) {
                    ours = Some(device.clone());
                }
                names.push(text);
            }
        }
        CoTaskMemFree(Some(devices as _));
        println!("Cameras Windows sees: {}", names.join(", "));
        let device =
            ours.with_context(|| format!("\"{CAMERA_NAME}\" isn't among them. Is it installed?"))?;

        let source: IMFMediaSource = device.ActivateObject()?;
        let reader = MFCreateSourceReaderFromMediaSource(&source, None)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let media_type = reader.GetCurrentMediaType(stream)?;
        let size = media_type.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (width, height) = ((size >> 32) as usize, (size & 0xFFFF_FFFF) as usize);
        let subtype = media_type.GetGUID(&MF_MT_SUBTYPE)?;
        if subtype != MFVideoFormat_NV12 {
            bail!("expected NV12, the camera offers {subtype:?}");
        }
        println!("Opened {CAMERA_NAME}: {width}x{height} NV12");

        let started = Instant::now();
        let mut frames = 0;
        let mut last = Vec::new();
        while started.elapsed() < Duration::from_secs(4) {
            let mut flags = 0u32;
            let mut sample = None;
            reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            let Some(sample) = sample else { continue };
            frames += 1;
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer.Lock(&mut data, None, Some(&mut length))?;
            last = std::slice::from_raw_parts(data, length as usize).to_vec();
            buffer.Unlock()?;
        }
        let _ = source.Shutdown();
        let fps = frames as f64 / started.elapsed().as_secs_f64();
        println!("Read {frames} frames at {fps:.1} fps");
        if last.len() < width * height * 3 / 2 {
            bail!(
                "frame was {} bytes, expected {}",
                last.len(),
                width * height * 3 / 2
            );
        }
        save_nv12_as_bmp(&last, width, height, out)?;
        println!("Saved the last frame to {}", out.display());
    }
    Ok(())
}

/// NV12 back to RGB, written as a plain 24 bit BMP so any viewer opens it.
fn save_nv12_as_bmp(nv12: &[u8], width: usize, height: usize, out: &Path) -> Result<()> {
    let row = (width * 3).div_ceil(4) * 4;
    let mut file = Vec::with_capacity(54 + row * height);
    let image_bytes = (row * height) as u32;
    file.extend(b"BM");
    file.extend((54 + image_bytes).to_le_bytes());
    file.extend([0u8; 4]);
    file.extend(54u32.to_le_bytes());
    file.extend(40u32.to_le_bytes());
    file.extend((width as i32).to_le_bytes());
    file.extend((height as i32).to_le_bytes());
    file.extend(1u16.to_le_bytes());
    file.extend(24u16.to_le_bytes());
    file.extend([0u8; 4]);
    file.extend(image_bytes.to_le_bytes());
    file.extend([0u8; 16]);
    let chroma = &nv12[width * height..];
    // BMP rows go bottom up
    for y in (0..height).rev() {
        let start = file.len();
        for x in 0..width {
            let luma = nv12[y * width + x] as f32 - 16.0;
            let c = (y / 2) * width + (x / 2) * 2;
            let (u, v) = (chroma[c] as f32 - 128.0, chroma[c + 1] as f32 - 128.0);
            let r = 1.164 * luma + 1.793 * v;
            let g = 1.164 * luma - 0.213 * u - 0.533 * v;
            let b = 1.164 * luma + 2.112 * u;
            file.extend([b, g, r].map(|c| c.clamp(0.0, 255.0) as u8));
        }
        file.resize(start + row, 0);
    }
    std::fs::write(out, file).with_context(|| format!("couldn't write {}", out.display()))
}

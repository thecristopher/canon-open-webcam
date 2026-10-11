//! The terminal commands that talk to a running engine, plus doctor and bench.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use canon_ptp::CanonCamera;
use canon_wpd::{ComApartment, WpdTransport, list_devices};

use crate::control::{self, ControlRequest, EngineStatus};
use crate::setup;
use crate::timing::FramePacer;

pub fn status() -> Result<()> {
    let response = control::send(&ControlRequest::status())?;
    let status = response.status.with_context(|| response.message.unwrap_or_else(|| "no status".into()))?;
    println!("{}", describe(&status));
    Ok(())
}

pub fn describe(status: &EngineStatus) -> String {
    let options = &status.options;
    let mut lines = vec![format!("State:   {}", status.state)];
    if let Some(model) = &status.model {
        lines.push(format!("Camera:  {model}"));
    }
    if let Some(lens) = &status.lens {
        lines.push(format!("Lens:    {lens}"));
    }
    if status.frames_per_second > 0.0 {
        lines.push(format!("Speed:   {:.1} fps", status.frames_per_second));
    }
    lines.push(format!("Watched: {}", if status.apps_watching { "yes, an app has the camera open" } else { "no" }));
    lines.push(format!("Mirror:  {}", if options.mirror { "on" } else { "off" }));
    lines.push(format!("Framing: zoom {:.1}x, pan {:.1}, {:.1}", options.zoom, options.pan_x, options.pan_y));
    lines.push(format!(
        "Picture: brightness {:.2}, contrast {:.2}, saturation {:.2}, warmth {:.2}, tint {:.2}, sharpness {:.2}",
        options.brightness, options.contrast, options.saturation, options.warmth, options.tint, options.sharpness
    ));
    let output =
        if options.frame_rate == 0 { "every camera frame".to_string() } else { format!("{} fps", options.frame_rate) };
    lines.push(format!("Output:  {output}"));
    for (name, setting) in &status.settings {
        let choices =
            if setting.choices.is_empty() { "locked by the camera's mode".into() } else { setting.choices.join(" ") };
        lines.push(format!("{name}: {}  ({choices})", setting.value));
    }
    lines.join("\n")
}

/// `set` and `focus`: hand the request to the engine and print what it said.
pub fn send(request: ControlRequest) -> Result<()> {
    let response = control::send(&request)?;
    if !response.ok {
        bail!(response.message.unwrap_or_else(|| "the engine said no".into()));
    }
    println!("{}", response.message.unwrap_or_else(|| "Done.".into()));
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Info,
    Warning,
    Missing,
}

pub struct Check {
    pub verdict: Verdict,
    pub message: String,
}

impl Check {
    fn new(verdict: Verdict, message: impl Into<String>) -> Self {
        Self { verdict, message: message.into() }
    }
}

/// Everything that has to line up for the webcam to work, in the order you'd fix it.
pub fn checks() -> Vec<Check> {
    let mut checks = Vec::new();
    let build = setup::windows_build();
    checks.push(if build >= 22000 {
        Check::new(Verdict::Ok, format!("Windows 11 (build {build}), which has virtual cameras built in"))
    } else {
        Check::new(
            Verdict::Missing,
            format!("This is Windows build {build}. Canon Webcam needs Windows 11 (build 22000 or later)."),
        )
    });
    checks.push(if setup::is_installed() {
        Check::new(Verdict::Ok, format!("Canon Webcam is installed in {}", setup::install_dir().display()))
    } else {
        Check::new(
            Verdict::Missing,
            "Canon Webcam isn't installed. Double click CanonWebcam.exe, or run `CanonWebcam install`.",
        )
    });
    checks.push(match setup::virtual_camera_visible() {
        Some(name) => Check::new(Verdict::Ok, format!("Apps can see the camera as \"{name}\"")),
        None => Check::new(
            Verdict::Missing,
            "Windows doesn't list the virtual camera. Reinstall with `CanonWebcam install`.",
        ),
    });
    checks.push(match (canon_plugged_in(), setup::canon_on_generic_usb_driver()) {
        (Some(name), _) => Check::new(Verdict::Ok, format!("{name} is plugged in")),
        (None, Some(camera)) => Check::new(
            Verdict::Missing,
            format!(
                "Your Canon is on the {} driver (Zadig installed it as {}), which hides it from Windows' camera system. \
                 Run `CanonWebcam fix-driver` to switch it back.",
                camera.driver, camera.package
            ),
        ),
        (None, None) => Check::new(Verdict::Missing, "No Canon camera on USB. Plug it in and turn it on."),
    });
    checks.push(if !setup::canon_utility_present() {
        Check::new(Verdict::Ok, "Canon's EOS Webcam Utility isn't installed, so nothing else grabs the camera")
    } else if setup::canon_services_disabled() {
        Check::new(Verdict::Ok, "Canon's EOS Webcam Utility services are switched off")
    } else {
        Check::new(
            Verdict::Warning,
            "Canon's EOS Webcam Utility services may grab the camera. `CanonWebcam install` switches them off.",
        )
    });
    checks.push(if setup::starts_at_sign_in() {
        Check::new(Verdict::Info, format!("Starts when you sign in (log: {})", crate::log::path().display()))
    } else {
        Check::new(Verdict::Info, "Doesn't start when you sign in yet. Run `CanonWebcam install` for that.")
    });
    checks.push(match control::send(&ControlRequest::status()).ok().and_then(|r| r.status) {
        Some(status) => Check::new(Verdict::Info, format!("The engine is running and {}", status.state)),
        None => Check::new(Verdict::Info, "The engine isn't running right now"),
    });
    checks
}

pub fn doctor() -> Result<()> {
    let checks = checks();
    for check in &checks {
        let label = match check.verdict {
            Verdict::Ok => "ok",
            Verdict::Info => "INFO",
            Verdict::Warning => "WARNING",
            Verdict::Missing => "MISSING",
        };
        println!("{label:<8} {}", check.message);
    }
    if checks.iter().any(|c| c.verdict == Verdict::Missing) {
        bail!("something above needs fixing");
    }
    Ok(())
}

pub fn canon_plugged_in() -> Option<String> {
    let _com = ComApartment::enter().ok()?;
    list_devices().ok()?.into_iter().find(|d| d.is_canon()).map(|d| d.name)
}

/// Measures the raw live view rate of whatever Canon is plugged in, with no virtual camera in the way.
pub fn bench(seconds: f64) -> Result<()> {
    let _com = ComApartment::enter()?;
    let devices = list_devices()?;
    let entry = devices
        .iter()
        .find(|d| d.is_canon())
        .context("No Canon found. Quit Canon Webcam first, then plug in and turn on the camera.")?;
    let mut camera = CanonCamera::new(WpdTransport::open(entry).context("Windows wouldn't open the camera")?);
    camera.connect()?;
    camera.start_live_view()?;
    println!("Measuring live view from {} for {seconds:.0} seconds...", camera.model());

    let pacer = FramePacer::default();
    let (mut frames, mut not_ready, mut bytes) = (0u32, 0u32, 0usize);
    let mut transfer = Duration::ZERO;
    // Live view takes a second or two to produce its first frame, so the clock starts there
    let warmup = Instant::now();
    let mut first_frame: Option<Instant> = None;
    let mut last_frame = Instant::now();
    let result = (|| -> Result<()> {
        loop {
            match first_frame {
                Some(first) if first.elapsed().as_secs_f64() >= seconds => return Ok(()),
                None if warmup.elapsed() > Duration::from_secs(10) => bail!("the camera never sent a frame"),
                _ => {}
            }
            std::thread::sleep(pacer.wait_before_asking(last_frame.elapsed()));
            let asked = Instant::now();
            match camera.next_frame()? {
                Some(jpeg) => {
                    last_frame = Instant::now();
                    if first_frame.is_none() {
                        println!("First frame after {} ms", warmup.elapsed().as_millis());
                        first_frame = Some(last_frame);
                        continue;
                    }
                    transfer += asked.elapsed();
                    frames += 1;
                    bytes += jpeg.len();
                }
                None => {
                    if first_frame.is_some() {
                        not_ready += 1;
                    }
                    std::thread::sleep(pacer.retry_delay);
                }
            }
        }
    })();
    camera.disconnect();
    result?;
    let elapsed = first_frame.map_or(0.0, |first| (last_frame - first).as_secs_f64());
    println!("{frames} frames in {elapsed:.1}s: {:.1} fps", frames as f64 / elapsed.max(0.001));
    if frames > 0 {
        println!(
            "Average frame {} KB, {:.1} ms per transfer, {not_ready} not ready answers",
            bytes / frames as usize / 1024,
            transfer.as_secs_f64() * 1000.0 / frames as f64
        );
    }
    Ok(())
}

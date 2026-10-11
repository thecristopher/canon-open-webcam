use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use canon_ptp::{CanonCamera, CanonProperty};
use canon_wpd::{ComApartment, WpdTransport, list_devices};

const USAGE: &str = "\
canon-webcam: use a Canon EOS camera as a webcam

  canon-webcam list              show every portable device Windows sees
  canon-webcam probe [out.jpg]   talk to the first Canon and save one live view frame";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _com = ComApartment::enter().context("couldn't start COM")?;
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("probe") => probe(
            args.get(1)
                .map_or_else(|| PathBuf::from("frame.jpg"), PathBuf::from),
        ),
        _ => {
            println!("{USAGE}");
            Ok(())
        }
    }
}

fn list() -> Result<()> {
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

fn probe(out: PathBuf) -> Result<()> {
    let devices = list_devices().context("couldn't ask Windows for portable devices")?;
    let Some(entry) = devices.iter().find(|d| d.is_canon()) else {
        bail!(
            "No Canon found. Turn the camera on, plug it in, and close anything else using it (EOS Utility, Photos)"
        );
    };
    println!("Opening {}", entry.name);
    let transport = WpdTransport::open(entry).context("Windows wouldn't open the camera")?;
    match transport.vendor_operations() {
        Ok(ops) => {
            let wanted = [0x9110u32, 0x9114, 0x9115, 0x9116, 0x9153];
            let missing: Vec<String> = wanted
                .iter()
                .filter(|op| !ops.contains(op))
                .map(|op| format!("0x{op:04X}"))
                .collect();
            println!(
                "Camera offers {} vendor operations{}",
                ops.len(),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(", missing {}", missing.join(" "))
                }
            );
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
    std::fs::write(&out, &jpeg).with_context(|| format!("couldn't write {}", out.display()))?;
    println!(
        "Saved a {} KB frame to {} after {} ms",
        jpeg.len() / 1024,
        out.display(),
        started.elapsed().as_millis()
    );
    Ok(())
}

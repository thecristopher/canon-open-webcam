use canon_shm::{FPS, FRAME_BYTES, HEIGHT, SharedFrames, WIDTH, tick_ms};

use crate::log::log;

// If the engine hasn't written for this long, it's not running and we show our own frame
const ENGINE_GONE_MS: u64 = 3000;
const RETRY_OPEN_MS: u64 = 1000;

/// Where the stream gets its pictures: the engine's latest frame, or a quiet fallback.
pub struct Feed {
    shared: Option<SharedFrames>,
    last_attempt: u64,
    sequence: u64,
    frame: Vec<u8>,
    showing_fallback: bool,
}

impl Feed {
    pub fn new() -> Self {
        let mut frame = vec![0u8; FRAME_BYTES];
        paint_fallback(&mut frame);
        Self { shared: None, last_attempt: 0, sequence: 0, frame, showing_fallback: true }
    }

    /// How fast to hand frames out: 30 or 60, or 0 for each camera frame as it lands.
    /// Until the engine is up and has said, a steady 30.
    pub fn frame_rate(&self) -> u32 {
        match &self.shared {
            Some(shared) if shared.writer_idle_ms().is_some_and(|idle| idle <= ENGINE_GONE_MS) => {
                match shared.frame_rate() {
                    rate @ (0 | 30 | 60) => rate,
                    _ => FPS,
                }
            }
            _ => FPS,
        }
    }

    /// True when the engine has finished a frame we haven't handed out yet.
    pub fn fresh_frame_waiting(&self) -> bool {
        self.shared.as_ref().is_some_and(|shared| {
            let sequence = shared.sequence();
            sequence != self.sequence && sequence & 1 == 0
        })
    }

    /// The frame to hand out right now. Also tells the engine someone is watching.
    pub fn next(&mut self) -> &[u8] {
        if self.shared.is_none() && tick_ms().saturating_sub(self.last_attempt) >= RETRY_OPEN_MS {
            self.last_attempt = tick_ms();
            match SharedFrames::open_or_create() {
                Ok(shared) => {
                    log!("frame buffer {}", if shared.created() { "created" } else { "opened" });
                    self.shared = Some(shared);
                }
                Err(e) => log!("couldn't open the frame buffer: {e}"),
            }
        }
        let Some(shared) = &self.shared else {
            return &self.frame;
        };
        shared.touch_consumer();

        if let Some(sequence) = shared.read_frame(self.sequence, &mut self.frame) {
            self.sequence = sequence;
            self.showing_fallback = false;
        } else if !self.showing_fallback && shared.writer_idle_ms().is_none_or(|idle| idle > ENGINE_GONE_MS) {
            log!("engine stopped writing, showing the fallback frame");
            paint_fallback(&mut self.frame);
            self.showing_fallback = true;
        }
        &self.frame
    }
}

/// A dark slate with a soft ring in the middle, so a call shows something calm instead of black.
fn paint_fallback(frame: &mut [u8]) {
    let (w, h) = (WIDTH as i64, HEIGHT as i64);
    let (luma, chroma) = frame.split_at_mut((w * h) as usize);
    let (cx, cy) = (w / 2, h / 2);
    for y in 0..h {
        for x in 0..w {
            let d2 = (x - cx).pow(2) + (y - cy).pow(2);
            let ring = (110i64.pow(2)..=124i64.pow(2)).contains(&d2);
            luma[(y * w + x) as usize] = if ring { 70 } else { 34 };
        }
    }
    // Slightly blue grey
    for pair in chroma.as_chunks_mut::<2>().0 {
        pair[0] = 134;
        pair[1] = 124;
    }
}

//! Minimal, thread-driven spinner/progress-bar replacement for the small
//! slice of `indicatif::ProgressBar` this crate used. `indicatif`'s own
//! terminal drawing is replaced with a local thread plus `\r` overwrite —
//! the same pattern already used for CLI spinners elsewhere in this
//! workspace (deckhand, deliver, bart) — with styling from `form3` instead
//! of `console`.

use form3::ansi::Color;
use form3::compat::Colorize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const TICK_CHARS: &[char] = &['⠁', '⠂', '⠄', '⡀', '⢀', '⠠', '⠐', '⠈'];
const BAR_WIDTH: usize = 40;

fn clear_line() {
    eprint!("\r\x1b[2K");
    let _ = io::stderr().flush();
}

/// An indefinite spinner, ticking on its own thread until finished.
pub struct Spinner {
    active: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

pub fn spinner(msg: &str) -> Spinner {
    let msg = msg.to_string();
    let active = Arc::new(AtomicBool::new(true));
    let worker_active = Arc::clone(&active);
    let handle = thread::spawn(move || {
        let mut tick = 0usize;
        while worker_active.load(Ordering::Relaxed) {
            let glyph = TICK_CHARS[tick % TICK_CHARS.len()].to_string().color(Color::Cyan);
            eprint!("\r\x1b[2K{glyph} {msg}");
            let _ = io::stderr().flush();
            tick += 1;
            thread::sleep(Duration::from_millis(80));
        }
    });
    Spinner {
        active,
        handle: Some(handle),
    }
}

impl Spinner {
    fn stop(&mut self) {
        self.active.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    pub fn finish_with_message(mut self, message: impl AsRef<str>) {
        self.stop();
        let glyph = TICK_CHARS[0].to_string().color(Color::Cyan);
        eprintln!("\r\x1b[2K{glyph} {}", message.as_ref());
    }

    pub fn finish_and_clear(mut self) {
        self.stop();
        clear_line();
    }
}

/// A finished/simulated bar, kept only so callers can `finish_and_clear()` it.
pub struct HunkProgress;

impl HunkProgress {
    pub fn finish_and_clear(self) {
        clear_line();
    }
}

/// Plays a fixed-duration progress animation over `len` steps, then returns.
/// Mirrors the prior behaviour exactly: for `len == 0` this performs no
/// animation at all (no iterations, nothing drawn).
pub async fn hunk_progress(len: usize, msg: &str) -> HunkProgress {
    let denom = len.max(1);
    for step in 0..len {
        let glyph = TICK_CHARS[step % TICK_CHARS.len()].to_string().color(Color::Cyan);
        let filled = ((step + 1) as f64 / denom as f64 * BAR_WIDTH as f64).round() as usize;
        let filled = filled.min(BAR_WIDTH);
        let bar = format!(
            "{}{}",
            "#".repeat(filled).color(Color::Cyan),
            "-".repeat(BAR_WIDTH - filled).color(Color::Blue)
        );
        eprint!("\r\x1b[2K{glyph} {msg} [{bar}] {}/{len}", step + 1);
        let _ = io::stderr().flush();
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    HunkProgress
}

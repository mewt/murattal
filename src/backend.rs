//! Audio output backends.
//!
//! Desktop (macOS/Linux) decodes and plays in-process with rodio. Android/Termux
//! has no ALSA device and can't build cpal, so there we shell out to
//! `termux-media-player` (from the termux-api package), which plays through
//! Android's audio system. The right backend is chosen at runtime.

use std::path::Path;

/// A thing that can play one local audio file at a time.
pub trait Backend {
    /// Start playing `path` from the beginning (replacing anything current).
    fn play_file(&mut self, path: &Path);
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    /// Volume as 0..=100.
    fn set_volume(&mut self, vol: u8);
    /// True when the current track has finished (or nothing is loaded).
    fn finished(&self) -> bool;
}

#[allow(dead_code)] // used only on non-Android targets
fn is_termux() -> bool {
    std::env::var_os("TERMUX_VERSION").is_some()
}

/// Pick a backend for this machine.
pub fn make_backend() -> Box<dyn Backend> {
    #[cfg(target_os = "android")]
    {
        Box::new(termux::TermuxBackend::new())
    }
    #[cfg(not(target_os = "android"))]
    {
        if is_termux() {
            return Box::new(termux::TermuxBackend::new());
        }
        match desktop::RodioBackend::new() {
            Some(b) => Box::new(b),
            None => {
                eprintln!(
                    "murattal: no audio output device; trying termux-media-player"
                );
                Box::new(termux::TermuxBackend::new())
            }
        }
    }
}

// ---- Termux (Android) ------------------------------------------------------

mod termux {
    use super::Backend;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    pub struct TermuxBackend {
        started: Option<Instant>,
        stopped: bool,
    }

    impl TermuxBackend {
        pub fn new() -> Self {
            Self {
                started: None,
                stopped: true,
            }
        }

        fn run(args: &[&str]) {
            let _ = Command::new("termux-media-player").args(args).output();
        }
    }

    impl Backend for TermuxBackend {
        fn play_file(&mut self, path: &Path) {
            let _ = Command::new("termux-media-player")
                .arg("play")
                .arg(path)
                .output();
            self.started = Some(Instant::now());
            self.stopped = false;
        }

        fn pause(&mut self) {
            Self::run(&["pause"]);
        }

        fn resume(&mut self) {
            // `play` with no file resumes a paused track.
            Self::run(&["play"]);
        }

        fn stop(&mut self) {
            Self::run(&["stop"]);
            self.stopped = true;
        }

        fn set_volume(&mut self, vol: u8) {
            // termux-media-player has no volume; use the Android music stream (0..=15).
            let lvl = ((vol as u32 * 15 + 50) / 100).min(15);
            let _ = Command::new("termux-volume")
                .arg("music")
                .arg(lvl.to_string())
                .output();
        }

        fn finished(&self) -> bool {
            if self.stopped {
                return true;
            }
            // Grace window: `info` can report "stopped" for a beat right after we
            // start, so don't let that trigger a false auto-advance.
            if let Some(t) = self.started {
                if t.elapsed() < Duration::from_millis(2000) {
                    return false;
                }
            }
            match Command::new("termux-media-player").arg("info").output() {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                    // Playing/paused -> still going; anything else -> track ended.
                    !(s.contains("playing") || s.contains("paused"))
                }
                // Player missing/unavailable: can't tell, so don't spin-advance.
                Err(_) => false,
            }
        }
    }
}

// ---- Desktop (rodio) -------------------------------------------------------

#[cfg(not(target_os = "android"))]
mod desktop {
    use super::Backend;
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;

    use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

    pub struct RodioBackend {
        _stream: OutputStream, // keep the output device alive
        handle: OutputStreamHandle,
        sink: Option<Sink>,
        volume: f32,
    }

    impl RodioBackend {
        pub fn new() -> Option<Self> {
            let (stream, handle) = OutputStream::try_default().ok()?;
            Some(Self {
                _stream: stream,
                handle,
                sink: None,
                volume: 1.0,
            })
        }
    }

    impl Backend for RodioBackend {
        fn play_file(&mut self, path: &Path) {
            if let Some(old) = self.sink.take() {
                old.stop();
            }
            let decoded = File::open(path)
                .map_err(|e| e.to_string())
                .and_then(|f| Decoder::new(BufReader::new(f)).map_err(|e| e.to_string()));
            match decoded {
                Ok(dec) => {
                    if let Ok(sink) = Sink::try_new(&self.handle) {
                        sink.set_volume(self.volume);
                        sink.append(dec);
                        self.sink = Some(sink);
                    }
                }
                Err(e) => eprintln!("murattal: decode failed: {e}"),
            }
        }

        fn pause(&mut self) {
            if let Some(s) = &self.sink {
                s.pause();
            }
        }

        fn resume(&mut self) {
            if let Some(s) = &self.sink {
                s.play();
            }
        }

        fn stop(&mut self) {
            if let Some(s) = self.sink.take() {
                s.stop();
            }
        }

        fn set_volume(&mut self, vol: u8) {
            self.volume = (vol as f32 / 100.0).clamp(0.0, 2.0);
            if let Some(s) = &self.sink {
                s.set_volume(self.volume);
            }
        }

        fn finished(&self) -> bool {
            match &self.sink {
                Some(s) => s.empty(),
                None => true,
            }
        }
    }
}

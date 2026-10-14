//! Wire protocol between the `quran` client and the background playback daemon.

use serde::{Deserialize, Serialize};

/// One surah's remote audio file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub surah: u32,
    pub url: String,
}

/// What happens when a surah finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Continue to the next surah through 114.
    Advance,
    /// Play only the requested surah, then stop.
    Once,
    /// Repeat the requested surah forever.
    Loop,
}

/// Command sent from the client to the daemon.
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Play {
        tracks: Vec<Track>,
        reciter_id: u32,
        reciter_name: String,
        mode: Mode,
    },
    TogglePause,
    Stop,
    Next,
    Prev,
    Vol(u8),
    /// Report current playback state.
    Status,
    /// Liveness probe used when spawning the daemon.
    Ping,
}

/// Current playback state, reported back to the client.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StatusData {
    pub playing: bool,
    pub surah: u32,
    pub surah_name: String,
    pub reciter: String,
    pub paused: bool,
}

/// Daemon's reply to a [`Request`].
#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Status(StatusData),
    Error(String),
}

//! Background playback daemon: owns the audio output and a control socket.
//!
//! One daemon per machine. The client (`quran play`, `quran pause`, …) spawns it
//! on demand and talks to it over a Unix domain socket, so playback persists in
//! the background and is controllable from any terminal or session.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::backend::{make_backend, Backend};
use crate::ipc::{Mode, Request, Response, StatusData, Track};
use crate::surahs;

const UA: &str = concat!("murattal/", env!("CARGO_PKG_VERSION"));

// ---- shared paths ----------------------------------------------------------

pub fn runtime_dir() -> PathBuf {
    let d = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("murattal");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("daemon.sock")
}

fn audio_dir(reciter_id: u32) -> PathBuf {
    let d = runtime_dir().join("audio").join(reciter_id.to_string());
    let _ = fs::create_dir_all(&d);
    d
}

fn track_path(reciter_id: u32, surah: u32) -> PathBuf {
    audio_dir(reciter_id).join(format!("{surah:03}.mp3"))
}

/// Download a surah to the cache (if not already there) and return its path.
fn ensure_downloaded(reciter_id: u32, t: &Track) -> Result<PathBuf, String> {
    let path = track_path(reciter_id, t.surah);
    if path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(path);
    }
    let resp = ureq::get(&t.url)
        .set("User-Agent", UA)
        .call()
        .map_err(|e| e.to_string())?;
    let tmp = path.with_file_name(format!("{:03}.part{}", t.surah, std::process::id()));
    {
        let mut f = File::create(&tmp).map_err(|e| e.to_string())?;
        io::copy(&mut resp.into_reader(), &mut f).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

// ---- audio thread ----------------------------------------------------------

enum AudioCmd {
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
}

/// Result of an off-thread download: the generation it belongs to and the
/// local path (or `None` if the download failed).
type Loaded = (u64, Option<PathBuf>);

struct AudioState {
    backend: Box<dyn Backend>,
    tracks: Vec<Track>,
    idx: usize,
    reciter_id: u32,
    reciter_name: String,
    mode: Mode,
    playing: bool,
    paused: bool,
    /// A track is being fetched off-thread; suppress auto-advance until it lands.
    loading: bool,
    /// Bumped on every load so a slow download for a surah we've moved past is ignored.
    gen: u64,
    ready_tx: Sender<Loaded>,
    status: Arc<Mutex<StatusData>>,
}

impl AudioState {
    fn apply(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::Play {
                tracks,
                reciter_id,
                reciter_name,
                mode,
            } => {
                self.tracks = tracks;
                self.idx = 0;
                self.reciter_id = reciter_id;
                self.reciter_name = reciter_name;
                self.mode = mode;
                self.playing = !self.tracks.is_empty();
                self.paused = false;
                if self.playing {
                    self.load_current();
                }
            }
            AudioCmd::TogglePause => {
                self.paused = !self.paused;
                if self.paused {
                    self.backend.pause();
                } else {
                    self.backend.resume();
                }
            }
            AudioCmd::Stop => self.stop_playback(),
            AudioCmd::Next => {
                if self.playing && self.idx + 1 < self.tracks.len() {
                    self.idx += 1;
                    self.load_current();
                }
            }
            AudioCmd::Prev => {
                if self.playing && self.idx > 0 {
                    self.idx -= 1;
                    self.load_current();
                }
            }
            AudioCmd::Vol(v) => self.backend.set_volume(v),
        }
    }

    fn stop_playback(&mut self) {
        self.backend.stop();
        self.playing = false;
        self.paused = false;
        self.loading = false;
        self.gen += 1; // invalidate any download still in flight
    }

    /// Begin loading the current track. The download runs on its own thread so
    /// the daemon keeps handling commands (switch reciter, stop, skip) instead
    /// of blocking for the whole of a multi-MB surah like Al-Baqarah.
    fn load_current(&mut self) {
        let Some(t) = self.tracks.get(self.idx).cloned() else {
            self.stop_playback();
            return;
        };
        self.gen += 1;
        let gen = self.gen;
        self.loading = true;
        // Silence the current track immediately; the next one plays once fetched.
        self.backend.stop();
        // Reflect the target surah right away so `now`/`bar` aren't blank while
        // the download that follows is still in flight.
        self.write_status();
        let rid = self.reciter_id;
        let tx = self.ready_tx.clone();
        thread::spawn(move || {
            let res = match ensure_downloaded(rid, &t) {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("murattal: download failed for surah {}: {e}", t.surah);
                    None
                }
            };
            let _ = tx.send((gen, res));
        });
        self.prefetch(self.idx + 1);
    }

    /// Handle a finished off-thread download.
    fn on_loaded(&mut self, gen: u64, res: Option<PathBuf>) {
        if gen != self.gen || !self.playing {
            return; // stale: the user has since switched, skipped, or stopped
        }
        self.loading = false;
        match res {
            Some(path) => {
                self.backend.play_file(&path);
                if self.paused {
                    self.backend.pause();
                }
            }
            // Download failed: skip on to the next track.
            None => self.advance(),
        }
    }

    /// Fetch an upcoming track into the cache in the background, so advancing is instant.
    fn prefetch(&self, idx: usize) {
        if let Some(t) = self.tracks.get(idx).cloned() {
            let rid = self.reciter_id;
            thread::spawn(move || {
                let _ = ensure_downloaded(rid, &t);
            });
        }
    }

    fn advance(&mut self) {
        match self.mode {
            Mode::Loop => self.load_current(),
            Mode::Once => self.stop_playback(),
            Mode::Advance => {
                if self.idx + 1 < self.tracks.len() {
                    self.idx += 1;
                    self.load_current();
                } else {
                    self.stop_playback();
                }
            }
            Mode::Cycle => {
                if self.tracks.is_empty() {
                    self.stop_playback();
                } else {
                    self.idx = (self.idx + 1) % self.tracks.len();
                    self.load_current();
                }
            }
        }
    }

    fn tick(&mut self) {
        // Don't auto-advance while a track is still downloading: the backend
        // reports `finished()` (nothing is playing yet) but we're not done.
        if self.playing && !self.paused && !self.loading && self.backend.finished() {
            self.advance();
        }
        self.write_status();
    }

    fn write_status(&self) {
        let mut st = self.status.lock().unwrap();
        if self.playing {
            if let Some(t) = self.tracks.get(self.idx) {
                *st = StatusData {
                    playing: true,
                    surah: t.surah,
                    surah_name: surahs::name(t.surah).to_string(),
                    reciter: self.reciter_name.clone(),
                    paused: self.paused,
                };
                return;
            }
        }
        *st = StatusData::default();
    }
}

fn audio_thread(rx: mpsc::Receiver<AudioCmd>, status: Arc<Mutex<StatusData>>) {
    let (ready_tx, ready_rx) = mpsc::channel::<Loaded>();
    let mut state = AudioState {
        backend: make_backend(),
        tracks: Vec::new(),
        idx: 0,
        reciter_id: 0,
        reciter_name: String::new(),
        mode: Mode::Advance,
        playing: false,
        paused: false,
        loading: false,
        gen: 0,
        ready_tx,
        status,
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(cmd) => state.apply(cmd),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Start playing any track whose off-thread download just completed.
        while let Ok((gen, res)) = ready_rx.try_recv() {
            state.on_loaded(gen, res);
        }
        state.tick();
    }
}

// ---- socket server ---------------------------------------------------------

fn handle_conn(stream: UnixStream, tx: &Sender<AudioCmd>, status: &Arc<Mutex<StatusData>>) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let req: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(_) => return,
    };
    let resp = match req {
        Request::Ping => Response::Ok,
        Request::Status => Response::Status(status.lock().unwrap().clone()),
        Request::Play {
            tracks,
            reciter_id,
            reciter_name,
            mode,
        } => {
            let _ = tx.send(AudioCmd::Play {
                tracks,
                reciter_id,
                reciter_name,
                mode,
            });
            Response::Ok
        }
        Request::TogglePause => send_ok(tx, AudioCmd::TogglePause),
        Request::Stop => send_ok(tx, AudioCmd::Stop),
        Request::Next => send_ok(tx, AudioCmd::Next),
        Request::Prev => send_ok(tx, AudioCmd::Prev),
        Request::Vol(v) => send_ok(tx, AudioCmd::Vol(v)),
    };
    let mut w = &stream;
    let _ = writeln!(w, "{}", serde_json::to_string(&resp).unwrap());
}

fn send_ok(tx: &Sender<AudioCmd>, cmd: AudioCmd) -> Response {
    let _ = tx.send(cmd);
    Response::Ok
}

/// Daemon entry point (`quran __daemon`).
pub fn run() {
    let sock = socket_path();
    // Another daemon already listening? Then we are redundant — exit quietly.
    if UnixStream::connect(&sock).is_ok() {
        return;
    }
    let _ = fs::remove_file(&sock);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(_) => return,
    };

    let status = Arc::new(Mutex::new(StatusData::default()));
    let (tx, rx) = mpsc::channel::<AudioCmd>();
    {
        let status = status.clone();
        thread::spawn(move || audio_thread(rx, status));
    }

    for conn in listener.incoming() {
        if let Ok(stream) = conn {
            handle_conn(stream, &tx, &status);
        }
    }
}

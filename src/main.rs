//! `quran` — listen to Quran recitation from your terminal.
//!
//! The CLI is a thin client: it resolves the reciter/surah, fetches the audio
//! URLs, and hands a playlist to a background daemon (spawned on demand) that
//! owns the audio output. State is global, so playback started anywhere is
//! controllable everywhere.

mod api;
mod daemon;
mod ipc;
mod surahs;

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};

use daemon::socket_path;
use ipc::{Mode, Request, Response, StatusData, Track};

#[derive(Parser)]
#[command(name = "quran", version, about = "Listen to Quran recitation in your terminal")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Play a surah (number or name). Auto-advances through 114 by default.
    Play {
        /// Surah number (1-114) or name, e.g. 27 or "an-naml"
        surah: String,
        /// Reciter id or name (default: Hani ar-Rifai)
        #[arg(long)]
        reciter: Option<String>,
        /// Play only this surah, then stop
        #[arg(long)]
        once: bool,
        /// Repeat this surah continuously
        #[arg(long = "loop")]
        loop_: bool,
    },
    /// Toggle pause / resume
    Pause,
    /// Stop playback
    Stop,
    /// Skip to the next surah
    Next,
    /// Back to the previous surah
    Prev,
    /// Show what's playing
    Now,
    /// Compact one-line status for a status bar (nothing when stopped)
    Bar,
    /// Set volume (0-100)
    Vol { level: u8 },
    /// List all 114 surahs
    List,
    /// List available reciters
    Reciters,
    /// Run the background playback daemon (internal)
    #[command(name = "__daemon", hide = true)]
    Daemon,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Daemon => {
            daemon::run();
            0
        }
        Cmd::Play {
            surah,
            reciter,
            once,
            loop_,
        } => cmd_play(&surah, reciter.as_deref(), once, loop_),
        Cmd::Pause => cmd_control(Request::TogglePause),
        Cmd::Next => cmd_control(Request::Next),
        Cmd::Prev => cmd_control(Request::Prev),
        Cmd::Stop => cmd_stop(),
        Cmd::Vol { level } => cmd_vol(level),
        Cmd::Now => cmd_now(),
        Cmd::Bar => cmd_bar(),
        Cmd::List => cmd_list(),
        Cmd::Reciters => cmd_reciters(),
    };
    std::process::exit(code);
}

// ---- commands --------------------------------------------------------------

fn cmd_play(surah_arg: &str, reciter: Option<&str>, once: bool, loop_: bool) -> i32 {
    if once && loop_ {
        return fail("choose only one of --once / --loop");
    }
    let mode = if once {
        Mode::Once
    } else if loop_ {
        Mode::Loop
    } else {
        Mode::Advance
    };

    let surah = match surahs::resolve(surah_arg) {
        Ok(n) => n,
        Err(e) => return fail(&e),
    };
    let (rid, rname) = match reciter {
        Some(q) => match api::resolve_reciter(q) {
            Ok(x) => x,
            Err(e) => return fail(&e),
        },
        None => (api::DEFAULT_RECITER.0, api::DEFAULT_RECITER.1.to_string()),
    };

    eprintln!("Fetching recitation list ({rname})…");
    let all = match api::fetch_playlist(rid) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let tracks: Vec<Track> = match mode {
        Mode::Advance => all.into_iter().filter(|t| t.surah >= surah).collect(),
        Mode::Once | Mode::Loop => all.into_iter().filter(|t| t.surah == surah).collect(),
    };
    if tracks.is_empty() {
        return fail(&format!("no audio for surah {surah}"));
    }

    if let Err(e) = ensure_daemon() {
        return fail(&e);
    }
    if let Err(e) = send(&Request::Play {
        tracks,
        reciter_id: rid,
        reciter_name: rname.clone(),
        mode,
    }) {
        return fail(&e);
    }

    let label = match mode {
        Mode::Advance => "auto-advancing to 114",
        Mode::Once => "single surah",
        Mode::Loop => "looping",
    };
    println!("▶ Playing {surah} {} — {rname} ({label})", surahs::name(surah));
    println!("  control: quran pause | next | now | stop");
    0
}

fn cmd_control(req: Request) -> i32 {
    if !daemon_up() {
        return fail("nothing playing");
    }
    if let Err(e) = send(&req) {
        return fail(&e);
    }
    thread::sleep(Duration::from_millis(250)); // let the daemon settle so `now` is accurate
    cmd_now()
}

fn cmd_stop() -> i32 {
    if daemon_up() {
        let _ = send(&Request::Stop);
    }
    println!("⏹ stopped");
    0
}

fn cmd_vol(level: u8) -> i32 {
    if !daemon_up() {
        return fail("nothing playing");
    }
    if let Err(e) = send(&Request::Vol(level.min(100))) {
        return fail(&e);
    }
    println!("volume: {}", level.min(100));
    0
}

fn cmd_now() -> i32 {
    match status() {
        Some(s) if s.playing => {
            let flag = if s.paused { "⏸" } else { "▶" };
            println!("{flag} {} {} — {}", s.surah, s.surah_name, s.reciter);
        }
        _ => println!("Nothing playing."),
    }
    0
}

fn cmd_bar() -> i32 {
    if let Some(s) = status() {
        if s.playing {
            let suffix = if s.paused { " (paused)" } else { "" };
            println!("{} {}{}", s.surah, s.surah_name, suffix);
        }
    }
    0
}

fn cmd_list() -> i32 {
    for (i, name) in surahs::NAMES.iter().enumerate().skip(1) {
        println!("{i:>3}  {name}");
    }
    0
}

fn cmd_reciters() -> i32 {
    for (id, name) in api::RECITERS {
        println!("{id:>3}  {name}");
    }
    0
}

// ---- daemon client ---------------------------------------------------------

fn daemon_up() -> bool {
    send(&Request::Ping).is_ok()
}

fn ensure_daemon() -> Result<(), String> {
    if daemon_up() {
        return Ok(());
    }
    spawn_daemon().map_err(|e| format!("could not start player: {e}"))?;
    for _ in 0..40 {
        if daemon_up() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("player daemon did not start".into())
}

fn spawn_daemon() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("__daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach into a new session so the daemon outlives this process (and any
    // Claude Code tool call), which would otherwise kill its process group.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}

fn status() -> Option<StatusData> {
    match send(&Request::Status) {
        Ok(Response::Status(s)) => Some(s),
        _ => None,
    }
}

fn send(req: &Request) -> Result<Response, String> {
    let stream = UnixStream::connect(socket_path()).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    {
        let mut w = &stream;
        let data = serde_json::to_string(req).map_err(|e| e.to_string())?;
        w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        w.write_all(b"\n").map_err(|e| e.to_string())?;
        w.flush().ok();
    }
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| e.to_string())
}

fn fail(msg: &str) -> i32 {
    eprintln!("quran: {msg}");
    1
}

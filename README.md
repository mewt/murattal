# murattal

Listen to Quran recitation from your terminal. Streams full surahs in the
background so your shell stays free — **no mpv or external player required**,
just one self-contained binary.

Installs a command called `quran`.

```sh
cargo install murattal

quran play 27            # An-Naml, then 28, 29, … (auto-advances to 114)
quran play yasin --once  # just Ya-Sin
quran pause              # toggle pause / resume
quran next               # skip to the next surah
quran now                # ▶ 27 An-Naml — Hani ar-Rifai
quran stop
```

## Features

- Streams from [QuranicAudio](https://quranicaudio.com) via the Quran.com API.
- Background playback via a small on-demand daemon — start/stop/skip from **any**
  terminal or session; there's only ever one player (the latest `play` wins).
- Surahs are cached on first play, so replays are instant and work offline.
- Auto-advances through the mushaf, or `--once` / `--loop`.
- Pick a reciter with `--reciter <id|name>` (default: Hani ar-Rifai).
- `quran bar` prints a compact one-liner (empty when stopped) for status bars.

## Commands

| Command | Description |
|---|---|
| `quran play <surah> [--reciter <id\|name>] [--once\|--loop]` | Play a surah (number or name) |
| `quran pause` | Toggle pause / resume |
| `quran next` / `quran prev` | Skip within the playlist |
| `quran now` | What's playing |
| `quran bar` | Compact status-bar line (empty when stopped) |
| `quran vol <0-100>` | Set volume |
| `quran stop` | Stop playback |
| `quran list` | List all 114 surahs |
| `quran reciters` | List available reciters |

## Requirements

- **macOS**: works out of the box.
- **Linux**: needs ALSA at build time — `sudo apt install libasound2-dev`
  (Debian/Ubuntu) or the equivalent.

## How it works

The `quran` command is a thin client. It resolves the reciter and surah, fetches
the audio URLs, and hands a playlist to a background daemon (spawned on demand,
detached into its own session) that owns the audio output and a Unix-socket
control channel. State lives in your cache dir, so every invocation — from any
terminal — talks to the same single player.

## License

MIT

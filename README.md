# murattal

Listen to Quran recitation from your terminal. Streams full surahs in the
background so your shell stays free — **no mpv or external player required**,
just one self-contained binary.

Installs a command called `quran`.

```sh
cargo install murattal

quran play 27            # An-Naml, then 28, 29, … (auto-advances to 114)
quran play yasin --once  # just Ya-Sin
quran juz                # the Juz 'Amma short surahs (Al-Ikhlas … Quraysh)
quran reciter alafasy    # pick your reciter once; it sticks for future plays
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
- `quran juz` plays the ten Juz 'Amma short surahs (great for daily review).
- Pick a reciter once with `quran reciter <id|name>` — the choice persists for
  every future play. Override a single play with `--reciter` (default: Hani ar-Rifai).
- `quran bar` prints a compact one-liner (empty when stopped) for status bars.

## Commands

| Command | Description |
|---|---|
| `quran play <surah> [--reciter <id\|name>] [--once\|--loop]` | Play a surah (number or name) |
| `quran juz [--reciter <id\|name>] [--loop]` | Play the Juz 'Amma short surahs, then stop |
| `quran reciter [<id\|name>]` | Show, or persistently set, the reciter for future plays |
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
- **Android / Termux**: murattal detects Termux and plays through
  `termux-media-player` (Android's audio) instead of ALSA — so it builds and
  runs on your phone with no NDK. Volume maps to the Android music stream.
  (Pause/resume/stop/skip work; sub-second seeking isn't available there.)

  Setup — **do not use the Play Store** (its Termux is deprecated and broken on
  Android 11+). Install both the **Termux** app and the **Termux:API** app from
  **[F-Droid](https://f-droid.org)** (or GitHub releases), from the *same*
  source so their signatures match, then:

  ```sh
  pkg install rust termux-api     # termux-api = the CLI bridge package
  cargo install murattal
  ```

  `pkg install termux-api` only provides the `termux-media-player` command; it
  still needs the separately-installed Termux:API **app** to actually play.

## How it works

The `quran` command is a thin client. It resolves the reciter and surah, fetches
the audio URLs, and hands a playlist to a background daemon (spawned on demand,
detached into its own session) that owns the audio output and a Unix-socket
control channel. State lives in your cache dir, so every invocation — from any
terminal — talks to the same single player.

## License

MIT

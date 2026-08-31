<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
<!-- Copyright (c) 2026 Enzo Costa Fuke -->

# Architecture

## How it's organized

- `src/yt.rs` — runs `yt-dlp --dump-json --flat-playlist` and *streams*
  the JSON lines as they arrive (the list fills in incrementally). No
  longer resolves the stream URL itself — that's now mpv's job (see
  below).
- `src/mpv.rs` — keeps a single mpv instance in `--idle` mode with IPC
  over a Unix socket (`--input-ipc-server`). The YouTube URL (not a
  resolved stream URL) is sent straight through via
  `loadfile <url> append-play` (Enter/`a`, the default behavior when
  selecting — joins mpv's native queue without reopening the
  process/window) or `loadfile <url> replace` (`r`, plays now and clears
  the queue) — mpv itself resolves the format using its built-in yt-dlp
  hook (`ytdl_hook`). `set_property vid auto|no` toggles the video track
  at runtime without restarting the stream. `get_property
  time-pos/duration` drives the progress bar.
- `src/history.rs` — persists played/queued videos to
  `~/.config/yt-tui/history.toml` (or `$XDG_CONFIG_HOME/yt-tui/...`),
  loaded on startup; lets you browse without searching again (`H`). Each
  selection is saved sequentially (`await`, not `tokio::spawn`) to avoid
  the risk of concurrent out-of-order writes, and the app also saves once
  more on exit as a safety net.
- `src/app.rs` — central state: search results, history, queue (mirroring
  mpv's playlist), current position/duration, and the `mpsc` channel that
  connects the async search back to the main loop.
- `src/ui.rs` — draws the screen with ratatui: search bar, main list
  (results or history) plus a queue panel side by side, progress bar
  (`Gauge`), and status.
- `src/main.rs` — main loop with `tokio::select!` combining keyboard
  events (`crossterm::EventStream`, async) with a 250ms tick that syncs
  position/duration/pause with the real mpv over IPC.

## Notes and known limitations

- mpv's IPC socket also emits spontaneous events (pause, seek, etc.) to
  every connected client; `Mpv::send` already filters those lines out and
  only treats the first non-event line as the command's response.
- One-off IPC errors during use (busy socket, no-op command, etc.) never
  bring down the TUI — they show up in the status bar. Only a failure
  during mpv's startup (see [TROUBLESHOOTING.md](TROUBLESHOOTING.md)) is
  fatal, because there's nothing to play without it.
- Video mode (`v`) depends on a display being available for mpv to open a
  window (X11/Wayland). Without a display, turning on video can fail —
  mpv's error shows up in the status bar instead of crashing the app.
- The queue (`queue` in the app) mirrors the order items were sent to
  mpv; it assumes nothing reorders the playlist from outside the app.
- `history.toml` keeps the 300 most recent entries (no duplicates by id);
  adjustable via `MAX_ENTRIES` in `src/history.rs`.
- Format resolution happens inside mpv itself (via `ytdl_hook`), so we
  don't cache any URL ourselves — every `loadfile` decides the format
  again. That's intentional: see
  [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for why manual resolution was
  dropped.

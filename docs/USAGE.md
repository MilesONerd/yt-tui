<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
<!-- Copyright (c) 2026 Enzo Costa Fuke -->

# Usage

## External dependencies (not crates — need to be on the PATH)

- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) — search and stream resolution
- [`mpv`](https://mpv.io/) — audio/video playback

```bash
# Debian/Ubuntu
sudo apt install mpv
pip install -U yt-dlp   # or: sudo apt install yt-dlp (tends to be outdated)
```

## Running

```bash
cargo run --release
```

## Controls

| Key           | Action                                       |
|---------------|-----------------------------------------------|
| `/`           | Focus the search box                          |
| Enter (search)| Search                                        |
| `H`           | Toggle between search results and history     |
| ↑ / ↓ / j / k | Navigate the list                             |
| Enter / `a`   | Enqueue (append-play, doesn't interrupt)      |
| `r`           | Play now, replacing the whole queue           |
| `c`           | Clear the queue and stop playback             |
| Space         | Pause / resume                                |
| ← / → / h / l | Seek back / forward 10s                       |
| `n` / `p`     | Next / previous track in mpv's queue          |
| `v`           | Toggle video on / audio-only                  |
| `q` / Esc     | Quit                                          |

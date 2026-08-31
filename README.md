# yt-tui

Terminal YouTube player: search via `yt-dlp`, playback via `mpv` (single
instance, IPC-controlled), interface with `ratatui`, all orchestrated
asynchronously with `tokio`.

## Quick start

```bash
# dependencies (see docs/USAGE.md for details)
sudo apt install mpv
pip install -U yt-dlp

cargo run --release
```

## Documentation

- [docs/USAGE.md](docs/USAGE.md) — external dependencies, running the
  app, and the full controls table.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the code is
  organized, module by module, plus known limitations.
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) — postmortems for
  bugs hit during development (silent audio, queue/history bugs, mpv
  startup failures) and what to check if something similar comes up.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.

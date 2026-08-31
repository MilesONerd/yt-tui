<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
<!-- Copyright (c) 2026 Enzo Costa Fuke -->

# Troubleshooting

Postmortems for bugs hit (and fixed) during development, kept here in
case something similar resurfaces.

## Video with no audio / "mpv doesn't play anything"

The first version resolved the stream URL by calling `yt-dlp -f
bestvideo+bestaudio/best -g` and sent only the first line of the output to
mpv. The problem: when YouTube serves video and audio as separate streams
(common at higher quality), that command prints **two URLs**, one per
line — and using only the first one means grabbing the video URL *with no
audio at all*. Since the player starts in audio-only mode (`--vid=no`),
the result was total silence. On top of that, Google Video's signed URLs
sometimes require the same headers yt-dlp used to obtain them, which
weren't being passed along to mpv.

The fix: `app.rs` no longer resolves anything itself — it sends the
YouTube URL (`https://www.youtube.com/watch?v=...`) straight to mpv via
`loadfile`, and mpv's built-in yt-dlp hook (`--ytdl`, on by default)
takes care of picking and combining the formats correctly, with the right
headers. It's simpler and more robust than reimplementing that logic.

## Queue clearing itself / history losing entries

Two versions ago, `Enter` played the video by replacing mpv's entire
playlist (clearing the queue), and only `a` actually enqueued. That had
things backwards: selecting a video (the natural action, Enter) should
*enqueue*, not discard what was already queued. Fixed: now Enter/`a`
always enqueue (`append-play`), and only the new `r` key ("replace")
clears the queue and plays right away. `c` clears the queue manually.

History also had a data race: each selection saved `history.toml` in a
detached task (`tokio::spawn`) without waiting for it to finish.
Selecting several videos quickly fired off multiple concurrent writes
with no guaranteed order — a more complete one could finish writing
*before* an older (incomplete) one, which would then overwrite the file
last and lose entries. Closing the app right after selecting something
could also kill a pending write before it finished. Fixed: saving is now
sequential (`await`, not `spawn`), and the app saves once more when you
press `q`/Esc as an extra safeguard.

## "mpv didn't open the IPC socket in time"

This error only happens during startup (`Mpv::spawn`) and now comes with
mpv's actual `stderr` attached, so the full message already tells you the
cause. The most common cause used to be the app forcing
`--force-window=yes` at startup, which makes mpv try to initialize video
before any file is even loaded — and that breaks in environments without
a display ready (`DISPLAY` / `WAYLAND_DISPLAY` / `XDG_RUNTIME_DIR`
missing or not passed through to the terminal, common over SSH without X
forwarding). That's been removed: the app now starts mpv in `--vid=no`
(audio) mode only, and the video window is only created on demand when
you press `v`. If the error persists, the `stderr` shown in the message
should point to the real cause (mpv not installed, no permission on the
socket path, etc.).

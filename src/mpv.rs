// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Controls a single `mpv` instance over its JSON IPC socket.
//!
//! [`Mpv::spawn`] starts one `mpv --idle` process that stays alive for the
//! lifetime of the app, controlled entirely through its
//! [JSON IPC protocol](https://mpv.io/manual/stable/#json-ipc) over a Unix
//! socket. This avoids opening/closing the process (and the window, in
//! video mode) on every track change, and lets [`crate::app::App`] use
//! mpv's own playlist as a queue via [`LoadMode::AppendPlay`].

use anyhow::{Context, Result};
use serde_json::json;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

/// How to load a new file into mpv.
pub enum LoadMode {
    /// Replaces the entire playlist and plays immediately.
    Replace,
    /// Appends to the end; if mpv is idle (nothing playing), starts
    /// playing right away — otherwise it just joins the queue.
    AppendPlay,
}

impl LoadMode {
    fn as_str(&self) -> &'static str {
        match self {
            LoadMode::Replace => "replace",
            LoadMode::AppendPlay => "append-play",
        }
    }
}

/// Handle to a single, long-lived mpv process, controlled via its JSON
/// IPC socket. Killed automatically when dropped.
pub struct Mpv {
    _child: Child,
    socket_path: String,
}

impl Mpv {
    /// Starts `mpv --idle` in the background and waits for its IPC socket
    /// to come up.
    ///
    /// # Errors
    ///
    /// Returns an error if the `mpv` binary can't be spawned (e.g. not on
    /// the `PATH`), or if it exits or fails to open the socket before the
    /// startup timeout — in which case mpv's captured `stderr`, if any, is
    /// included in the error message.
    pub async fn spawn() -> Result<Self> {
        let socket_path = format!("/tmp/yt-tui-mpv-{}.sock", std::process::id());

        let mut child = Command::new("mpv")
            .args([
                "--idle",
                "--no-terminal",
                // Starts in audio-only mode; toggleable at runtime via
                // set_video_enabled (set_property vid). We do NOT force a
                // window here — --force-window=yes would try to initialize
                // video right at startup and crash mpv in environments
                // without a display available (SSH without X forwarding,
                // a plain TTY, etc). The window is created on its own once
                // video is enabled and something with a video track loads.
                "--vid=no",
                &format!("--input-ipc-server={socket_path}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true) // makes sure mpv dies together with the app
            .spawn()
            .context("failed to start mpv — is it installed and on the PATH?")?;

        // The socket takes a few ms to become available after spawn.
        let mut attempts = 0;
        loop {
            if UnixStream::connect(&socket_path).await.is_ok() {
                break;
            }

            // If mpv has already died, there's no point waiting for the
            // socket to show up — capture stderr to give a useful error
            // instead of the generic timeout.
            if let Ok(Some(status)) = child.try_wait() {
                let mut stderr_text = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = stderr.read_to_string(&mut stderr_text).await;
                }
                anyhow::bail!(
                    "mpv exited before opening the IPC socket (status: {status}).{}",
                    if stderr_text.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" stderr: {}", stderr_text.trim())
                    }
                );
            }

            attempts += 1;
            if attempts > 150 {
                anyhow::bail!(
                    "mpv didn't open the IPC socket within {}s — it's stuck somewhere (process still running)",
                    150 * 20 / 1000
                );
            }
            sleep(Duration::from_millis(20)).await;
        }

        Ok(Self {
            _child: child,
            socket_path,
        })
    }

    /// Opens a new connection per command (simpler than multiplexing a
    /// persistent connection) and discards spontaneous "event" lines that
    /// mpv sends by default to every connected client — only the first
    /// non-event line is treated as our command's response.
    async fn send(&self, command: serde_json::Value) -> Result<serde_json::Value> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .context("failed to connect to mpv's socket")?;

        let payload = serde_json::to_string(&json!({ "command": command }))?;
        stream.write_all(payload.as_bytes()).await?;
        stream.write_all(b"\n").await?;

        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                anyhow::bail!("mpv closed the socket before responding");
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("event").is_some() {
                continue; // async event (pause, seek, etc), not our response
            }
            return Ok(value);
        }
    }

    /// Loads `stream_url` (typically a YouTube watch URL — mpv resolves
    /// the actual media via its own `yt-dlp` hook) using the given
    /// [`LoadMode`].
    pub async fn load(&self, stream_url: &str, mode: LoadMode) -> Result<()> {
        self.send(json!(["loadfile", stream_url, mode.as_str()]))
            .await?;
        Ok(())
    }

    /// Toggles play/pause on the current track.
    pub async fn toggle_pause(&self) -> Result<()> {
        self.send(json!(["cycle", "pause"])).await?;
        Ok(())
    }

    /// Seeks by `seconds` relative to the current position (negative
    /// seeks backward).
    pub async fn seek(&self, seconds: f64) -> Result<()> {
        self.send(json!(["seek", seconds, "relative"])).await?;
        Ok(())
    }

    /// Advances to the next entry in mpv's playlist. A no-op if already on
    /// the last entry (`"weak"` mode — doesn't error out at the end).
    pub async fn playlist_next(&self) -> Result<()> {
        // "weak": doesn't error out if we're already on the last track.
        self.send(json!(["playlist-next", "weak"])).await?;
        Ok(())
    }

    /// Moves to the previous entry in mpv's playlist. A no-op if already
    /// on the first entry (`"weak"` mode).
    pub async fn playlist_prev(&self) -> Result<()> {
        self.send(json!(["playlist-prev", "weak"])).await?;
        Ok(())
    }

    /// Sets mpv's output volume (0–100, or higher to amplify).
    pub async fn set_volume(&self, volume: f64) -> Result<()> {
        self.send(json!(["set_property", "volume", volume])).await?;
        Ok(())
    }

    /// Enables/disables the video track without restarting mpv or the
    /// stream — flips `vid` (video track) on the fly via IPC.
    pub async fn set_video_enabled(&self, enabled: bool) -> Result<()> {
        let value = if enabled { json!("auto") } else { json!("no") };
        self.send(json!(["set_property", "vid", value])).await?;
        Ok(())
    }

    /// Reads a numeric (floating point) mpv property, e.g. `"time-pos"` or
    /// `"duration"`. Returns `Ok(None)` if the property currently has no
    /// value (e.g. nothing loaded) rather than treating that as an error.
    pub async fn get_property_f64(&self, name: &str) -> Result<Option<f64>> {
        let resp = self.send(json!(["get_property", name])).await?;
        Ok(resp.get("data").and_then(|d| d.as_f64()))
    }

    /// Reads an integer mpv property, e.g. `"playlist-pos"`.
    pub async fn get_property_i64(&self, name: &str) -> Result<Option<i64>> {
        let resp = self.send(json!(["get_property", name])).await?;
        Ok(resp.get("data").and_then(|d| d.as_i64()))
    }

    /// Reads a boolean mpv property, e.g. `"pause"`.
    pub async fn get_property_bool(&self, name: &str) -> Result<Option<bool>> {
        let resp = self.send(json!(["get_property", name])).await?;
        Ok(resp.get("data").and_then(|d| d.as_bool()))
    }

    /// Stops playback and clears mpv's playlist — this is what
    /// [`crate::app::App::clear_queue`] uses to empty the queue.
    pub async fn stop(&self) -> Result<()> {
        self.send(json!(["stop"])).await?;
        Ok(())
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

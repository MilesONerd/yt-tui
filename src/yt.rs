// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Searches YouTube via `yt-dlp` and streams results back incrementally.
//!
//! This module only handles *search* — resolving a [`Video`] into an
//! actual playable stream is left entirely to `mpv` (see
//! [`crate::mpv`]), which uses its own built-in `yt-dlp` hook.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// A YouTube video — a yt-dlp search result, and also the format saved in
/// history.toml (hence deriving Serialize in addition to Deserialize).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct Video {
    /// The YouTube video id (the `v=` query parameter of its watch URL).
    pub id: String,
    /// The video's title, as reported by yt-dlp.
    pub title: String,
    /// Duration in whole seconds, if yt-dlp reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    /// Uploader/channel name, if yt-dlp reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    /// View count, if yt-dlp reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_count: Option<u64>,
}

impl Video {
    /// The canonical `https://www.youtube.com/watch?v=...` URL for this
    /// video — what gets handed to mpv to play.
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }

    /// Formats [`Video::duration`] as `MM:SS`, or `--:--` if unknown.
    pub fn duration_fmt(&self) -> String {
        match self.duration {
            Some(secs) => format!("{:02}:{:02}", secs / 60, secs % 60),
            None => "--:--".to_string(),
        }
    }
}

/// Parses manually instead of a pure serde_json derive because yt-dlp
/// sometimes sends `duration` as a float and other fields can be missing —
/// this way we silently ignore whatever doesn't match instead of dropping
/// the whole line.
fn parse_video(raw: &serde_json::Value) -> Option<Video> {
    Some(Video {
        id: raw.get("id")?.as_str()?.to_string(),
        title: raw
            .get("title")?
            .as_str()
            .unwrap_or("(untitled)")
            .to_string(),
        duration: raw
            .get("duration")
            .and_then(|d| d.as_f64())
            .map(|d| d.round() as u64),
        uploader: raw
            .get("uploader")
            .and_then(|u| u.as_str())
            .map(String::from),
        view_count: raw.get("view_count").and_then(|v| v.as_u64()),
    })
}

/// Kicks off `yt-dlp ytsearchN:query --dump-json --flat-playlist` and sends
/// each video over `tx` as soon as its JSON line arrives on stdout,
/// instead of waiting for the whole process to finish.
///
/// Malformed or partial JSON lines are skipped rather than treated as a
/// fatal error. Returns once yt-dlp exits or `tx`'s receiver is dropped
/// (e.g. because a newer search superseded this one).
///
/// # Errors
///
/// Returns an error if the `yt-dlp` binary can't be spawned (e.g. not on
/// the `PATH`) or if reading its stdout fails.
pub async fn search_stream(
    query: String,
    limit: u32,
    tx: mpsc::UnboundedSender<Video>,
) -> Result<()> {
    let search_term = format!("ytsearch{}:{}", limit, query);

    let mut child = Command::new("yt-dlp")
        .args([
            "--dump-json",
            "--flat-playlist",
            "--no-warnings",
            "--ignore-errors",
            &search_term,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to start yt-dlp — is it installed and on the PATH?")?;

    let stdout = child.stdout.take().context("no stdout from yt-dlp")?;
    let mut lines = BufReader::new(stdout).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(video) = parse_video(&raw) {
                if tx.send(video).is_err() {
                    break; // receiver dropped (new search started); stop sending
                }
            }
        }
    }

    let _ = child.wait().await;
    Ok(())
}

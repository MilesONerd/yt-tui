// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Persists watched/queued videos to `history.toml` so they can be
//! browsed again without re-searching YouTube.
//!
//! Entries are kept most-recent-first, deduplicated by video id, and
//! capped at `MAX_ENTRIES`. The file lives under the user's config
//! directory — see `path()` — and every write is a full rewrite of the
//! file (there's no append-only log or versioning).

use crate::yt::Video;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum number of entries kept — keeps the file from growing unbounded.
const MAX_ENTRIES: usize = 300;

/// The full watch/queue history, as persisted to `history.toml`.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct History {
    /// Entries, most recently played/queued first.
    #[serde(default)]
    pub entries: Vec<Video>,
}

/// `$XDG_CONFIG_HOME/yt-tui/history.toml`, falling back to `~/.config/...`
/// and, if $HOME isn't set either, to `./yt-tui-history.toml` in the
/// current directory.
fn path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("yt-tui").join("history.toml");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("yt-tui")
            .join("history.toml");
    }
    PathBuf::from("yt-tui-history.toml")
}

/// Loads history from disk; any problem (missing file, corrupted, etc.)
/// just results in an empty history instead of blocking app startup.
pub async fn load() -> History {
    let p = path();
    match tokio::fs::read_to_string(&p).await {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => History::default(),
    }
}

/// Writes `history` to disk as TOML, creating the config directory if
/// needed. Callers should `await` this directly rather than spawning it
/// as a detached task — see `docs/TROUBLESHOOTING.md` for why concurrent
/// detached writes can silently lose entries.
pub async fn save(history: &History) -> Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("creating config directory")?;
    }
    let content = toml::to_string_pretty(history).context("serializing history.toml")?;
    tokio::fs::write(&p, content)
        .await
        .context("writing history.toml")?;
    Ok(())
}

/// Inserts/updates an entry at the top (most recent first), removing any
/// duplicate by id and truncating to `MAX_ENTRIES`.
pub fn push_entry(history: &mut History, video: Video) {
    history.entries.retain(|v| v.id != video.id);
    history.entries.insert(0, video);
    history.entries.truncate(MAX_ENTRIES);
}

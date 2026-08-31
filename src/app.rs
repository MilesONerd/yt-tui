// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Central application state: search results, history, the mpv-backed
//! playback queue, and the glue between them.
//!
//! [`App`] is the state machine the binary's key handler mutates and that
//! [`crate::ui::draw`] renders on every frame. It owns the single
//! [`crate::mpv::Mpv`] instance for the lifetime of the process.

use crate::history::{self, History};
use crate::mpv::{LoadMode, Mpv};
use crate::yt::{self, Video};
use anyhow::Result;
use tokio::sync::mpsc;

/// Whether the user is currently typing a search query or browsing a list.
#[derive(PartialEq)]
pub enum Mode {
    /// The search box is focused; keystrokes are appended to [`App::query`].
    Searching,
    /// The main list (search results or history) is focused; keystrokes
    /// navigate and act on it.
    Browsing,
}

/// Which list is currently shown/navigated in the main panel.
#[derive(PartialEq, Clone, Copy)]
pub enum ListSource {
    /// Showing [`App::results`], the most recent search.
    Search,
    /// Showing [`App::history`], the persisted watch/queue history.
    History,
}

/// Whether selecting a video should replace mpv's playlist or just join
/// the queue (loadfile ... append-play).
#[derive(Clone, Copy)]
enum PlayIntent {
    Replace,
    Enqueue,
}

/// All state needed to drive the TUI: what's on screen, what's playing,
/// and the handle to the single long-lived `mpv` process.
pub struct App {
    /// Whether the search box or the list is currently focused.
    pub mode: Mode,
    /// Which list ([`App::results`] or [`App::history`]) is being shown.
    pub list_source: ListSource,
    /// Current contents of the search box.
    pub query: String,
    /// Results of the most recent search, filled in incrementally as
    /// `yt-dlp` streams them back.
    pub results: Vec<Video>,
    /// Persisted history of played/queued videos, loaded on startup and
    /// saved back to disk after every selection.
    pub history: History,
    /// Index of the highlighted item in the currently active list.
    pub selected: usize,

    /// Videos sent to mpv so far, in the order mpv's own playlist should
    /// have them (mirrors mpv's internal playlist).
    pub queue: Vec<Video>,
    /// mpv's current `playlist-pos`, polled every tick; `None` before the
    /// first successful poll.
    pub playlist_pos: Option<i64>,
    /// The video mpv is currently on, if any.
    pub now_playing: Option<Video>,

    /// Current playback position in seconds, polled from mpv every tick.
    pub position: Option<f64>,
    /// Duration of the current track in seconds, polled from mpv every tick.
    pub duration_secs: Option<f64>,
    /// Whether mpv is currently paused.
    pub paused: bool,
    /// Whether the video track is currently enabled (vs. audio-only).
    pub video_enabled: bool,

    /// Free-form message shown in the status bar.
    pub status: String,
    /// Set to `true` to make the main loop exit on the next iteration.
    pub should_quit: bool,

    /// The single, long-lived mpv instance this app controls.
    pub mpv: Mpv,

    search_tx: mpsc::UnboundedSender<Video>,
    /// Receiving end of the channel `yt-dlp` search results stream over;
    /// drained into [`App::results`] by [`App::drain_search_results`].
    pub search_rx: mpsc::UnboundedReceiver<Video>,
}

impl App {
    /// Loads the saved history and spawns the single mpv instance used
    /// for the lifetime of the app. Fails only if mpv itself can't start
    /// (see [`crate::mpv::Mpv::spawn`]).
    pub async fn new() -> Result<Self> {
        let (search_tx, search_rx) = mpsc::unbounded_channel();
        let history = history::load().await;

        Ok(Self {
            mode: Mode::Searching,
            list_source: ListSource::Search,
            query: String::new(),
            results: Vec::new(),
            history,
            selected: 0,
            queue: Vec::new(),
            playlist_pos: None,
            now_playing: None,
            position: None,
            duration_secs: None,
            paused: false,
            video_enabled: false,
            status: "Type something and press Enter to search (H to view history)".to_string(),
            should_quit: false,
            mpv: Mpv::spawn().await?,
            search_tx,
            search_rx,
        })
    }

    /// List currently visible for navigation (search results OR history).
    fn active_list(&self) -> &[Video] {
        match self.list_source {
            ListSource::Search => &self.results,
            ListSource::History => &self.history.entries,
        }
    }

    /// Drains the search channel; called on every turn of the main loop.
    pub fn drain_search_results(&mut self) {
        while let Ok(video) = self.search_rx.try_recv() {
            self.results.push(video);
        }
    }

    /// Kicks off a new search for [`App::query`] in the background,
    /// clearing any previous results and switching to
    /// [`ListSource::Search`]. Does nothing if the query is blank.
    pub fn start_search(&mut self) {
        if self.query.trim().is_empty() {
            return;
        }
        self.results.clear();
        self.list_source = ListSource::Search;
        self.selected = 0;
        self.status = format!("Searching \"{}\"...", self.query);
        self.mode = Mode::Browsing;

        let query = self.query.clone();
        let tx = self.search_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = yt::search_stream(query, 15, tx).await {
                eprintln!("search error: {e:#}");
            }
        });
    }

    /// Toggles between viewing the last search's results and the saved
    /// history, without needing to search YouTube again.
    pub fn toggle_history_view(&mut self) {
        self.list_source = match self.list_source {
            ListSource::Search => ListSource::History,
            ListSource::History => ListSource::Search,
        };
        self.selected = 0;
        self.mode = Mode::Browsing;
        self.status = match self.list_source {
            ListSource::Search => "Showing search results".to_string(),
            ListSource::History => format!("History ({} items)", self.history.entries.len()),
        };
    }

    /// Moves the selection cursor in the active list by `delta`,
    /// clamped to the list's bounds. Negative values move up.
    pub fn move_selection(&mut self, delta: i32) {
        let len = self.active_list().len() as i32;
        if len == 0 {
            return;
        }
        let new_index = (self.selected as i32 + delta).clamp(0, len - 1);
        self.selected = new_index as usize;
    }

    /// Enter/'a': joins mpv's queue (append-play) without interrupting
    /// whatever's already playing; if mpv is idle, starts playing right
    /// away. This is the default behavior when selecting a video.
    pub async fn enqueue_selected(&mut self) {
        self.dispatch_selected(PlayIntent::Enqueue).await;
    }

    /// 'r': plays now, replacing mpv's entire playlist (clears the queue).
    pub async fn replace_selected(&mut self) {
        self.dispatch_selected(PlayIntent::Replace).await;
    }

    async fn dispatch_selected(&mut self, intent: PlayIntent) {
        let Some(video) = self.active_list().get(self.selected).cloned() else {
            return;
        };

        // Only reset position/"loading" status if this selection is about
        // to start playing immediately — an 'a' behind something already
        // playing shouldn't touch the current track's progress.
        let will_start_now = matches!(intent, PlayIntent::Replace) || self.now_playing.is_none();

        let mode = match intent {
            PlayIntent::Replace => {
                self.queue.clear();
                self.queue.push(video.clone());
                self.playlist_pos = Some(0);
                self.now_playing = Some(video.clone());
                LoadMode::Replace
            }
            PlayIntent::Enqueue => {
                self.queue.push(video.clone());
                if self.now_playing.is_none() {
                    self.now_playing = Some(video.clone());
                }
                LoadMode::AppendPlay
            }
        };

        if will_start_now {
            self.paused = false;
            self.position = None;
            self.duration_secs = None;
            self.status = format!("Loading \"{}\"...", video.title);
        } else {
            self.status = format!("Added to queue: \"{}\"", video.title);
        }

        // Saved sequentially (await, not spawn): if we fired this off as a
        // detached task, several quick 'a' presses would launch concurrent
        // writes with no guaranteed order — a more complete one could
        // finish writing BEFORE an older (incomplete) one, which would
        // then overwrite the file last and silently drop entries.
        // Awaiting here serializes the writes in the same order the keys
        // were pressed.
        history::push_entry(&mut self.history, video.clone());
        if let Err(e) = history::save(&self.history).await {
            self.status = format!("{} (warning: failed to save history: {e})", self.status);
        }

        // We pass the YouTube URL straight to mpv, which resolves it via
        // its built-in yt-dlp hook (ytdl_hook). This avoids reimplementing
        // format selection/combination ourselves — the hook already knows
        // how to mux video+audio when they come as separate streams and
        // use the right headers, which manually resolving with
        // `yt-dlp -g` didn't guarantee.
        if let Err(e) = self.mpv.load(&video.url(), mode).await {
            self.status = format!("Error loading into mpv: {e}");
        }
    }

    /// Clears the entire queue and stops playback.
    pub async fn clear_queue(&mut self) -> Result<()> {
        self.mpv.stop().await?;
        self.queue.clear();
        self.playlist_pos = None;
        self.now_playing = None;
        self.position = None;
        self.duration_secs = None;
        self.status = "Queue cleared".to_string();
        Ok(())
    }

    /// Toggles play/pause on the currently loaded track.
    pub async fn toggle_pause(&mut self) -> Result<()> {
        self.mpv.toggle_pause().await?;
        self.paused = !self.paused;
        Ok(())
    }

    /// Seeks by `seconds` relative to the current position (negative
    /// seeks backward).
    pub async fn seek(&mut self, seconds: f64) -> Result<()> {
        self.mpv.seek(seconds).await
    }

    /// Skips to the next track in mpv's playlist, if any.
    pub async fn next_track(&mut self) -> Result<()> {
        self.mpv.playlist_next().await
    }

    /// Skips to the previous track in mpv's playlist, if any.
    pub async fn prev_track(&mut self) -> Result<()> {
        self.mpv.playlist_prev().await
    }

    /// Toggles video on/off at runtime (without reopening mpv or the
    /// stream) — works because the format resolved by mpv's hook already
    /// includes the video track by default; this just enables/disables it.
    pub async fn toggle_video(&mut self) -> Result<()> {
        self.video_enabled = !self.video_enabled;
        self.mpv.set_video_enabled(self.video_enabled).await?;
        self.status = if self.video_enabled {
            "Video enabled".to_string()
        } else {
            "Audio-only mode".to_string()
        };
        Ok(())
    }

    /// Called on every tick of the main loop to keep position, duration,
    /// playlist index, and pause state synced with the real mpv (via
    /// get_property over the IPC socket).
    pub async fn refresh_progress(&mut self) {
        if self.now_playing.is_none() {
            return;
        }

        let had_position = self.position.is_some();
        if let Ok(pos) = self.mpv.get_property_f64("time-pos").await {
            if !had_position && pos.is_some() {
                self.status = "Playing".to_string();
            }
            self.position = pos;
        }
        if let Ok(dur) = self.mpv.get_property_f64("duration").await {
            self.duration_secs = dur;
        }
        if let Ok(idx) = self.mpv.get_property_i64("playlist-pos").await {
            self.playlist_pos = idx;
            // Keeps `now_playing` aligned with the queue's current track.
            if let Some(i) = idx {
                if let Some(v) = self.queue.get(i as usize) {
                    if self.now_playing.as_ref() != Some(v) {
                        self.now_playing = Some(v.clone());
                    }
                }
            }
        }
        if let Ok(Some(paused)) = self.mpv.get_property_bool("pause").await {
            self.paused = paused;
        }
    }
}

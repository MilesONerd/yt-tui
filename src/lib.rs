// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! `yt-tui` is a terminal YouTube player: search via `yt-dlp`, playback
//! via a single long-lived `mpv` instance controlled over its IPC socket,
//! interface with `ratatui`, all orchestrated asynchronously with
//! `tokio`.
//!
//! This crate is published primarily as the `yt-tui` binary (see
//! `src/main.rs` and `docs/USAGE.md` in the repository for the controls
//! and how to run it). The modules below are also exposed as a library in
//! case you want to reuse the YouTube-search / mpv-IPC / history plumbing
//! for a different frontend.
//!
//! # Modules
//!
//! - [`yt`] — searches YouTube via `yt-dlp` and streams results back.
//! - [`mpv`] — controls a single `mpv` instance over its JSON IPC socket.
//! - [`history`] — persists watched/queued videos to `history.toml`.
//! - [`app`] — ties the above together into the [`app::App`] state machine
//!   that drives the TUI.
//! - [`ui`] — renders [`app::App`]'s state with `ratatui`.
//!
//! # External dependencies
//!
//! This crate shells out to two binaries that must be installed and on
//! the `PATH`:
//!
//! - [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) — search and stream
//!   resolution.
//! - [`mpv`](https://mpv.io/) — audio/video playback, controlled via its
//!   [JSON IPC protocol](https://mpv.io/manual/stable/#json-ipc).
//!
//! See `docs/USAGE.md` in the repository for installation instructions.

#![warn(missing_docs)]

pub mod app;
pub mod history;
pub mod mpv;
pub mod ui;
pub mod yt;

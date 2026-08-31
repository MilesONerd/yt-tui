// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Binary entry point: terminal setup and the key-handling event loop.
//!
//! All the actual application logic lives in the `yt_tui` library crate
//! (see [`yt_tui`] for the module overview) — this file only wires up
//! `crossterm`'s raw-mode terminal, `ratatui`'s render loop, and keyboard
//! dispatch.

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::time::Duration;
use tokio::time::interval;
use yt_tui::app::{App, Mode};
use yt_tui::{history, ui};

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    let mut app = App::new().await?;
    let mut events = EventStream::new();
    // Redraws and syncs with mpv (position, duration, pause) 4x/second.
    let mut tick = interval(Duration::from_millis(250));

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if app.should_quit {
            // Safety net: makes sure history is flushed to disk before
            // exiting, even if no per-action save has run yet (e.g. the
            // app closed right after selecting something).
            let _ = history::save(&app.history).await;
            return Ok(());
        }

        tokio::select! {
            // Keyboard/mouse events from the terminal
            maybe_event = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_event {
                    if key.kind == KeyEventKind::Press {
                        handle_key(&mut app, key.code).await;
                    }
                }
            }

            // Syncs playback state and drains pending results
            _ = tick.tick() => {
                app.drain_search_results();
                app.refresh_progress().await;
            }
        }
    }
}

/// Never returns Err: a one-off mpv IPC hiccup (busy socket, no-op
/// command, etc.) shouldn't take down the whole TUI — it just becomes a
/// status bar message.
async fn handle_key(app: &mut App, code: KeyCode) {
    match app.mode {
        Mode::Searching => match code {
            KeyCode::Enter => app.start_search(),
            KeyCode::Char(c) => app.query.push(c),
            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Esc => app.should_quit = true,
            _ => {}
        },
        Mode::Browsing => match code {
            KeyCode::Char('/') => app.mode = Mode::Searching,
            KeyCode::Char('H') => app.toggle_history_view(),
            KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
            KeyCode::Enter | KeyCode::Char('a') => app.enqueue_selected().await,
            KeyCode::Char('r') => app.replace_selected().await,
            KeyCode::Char('c') => {
                let result = app.clear_queue().await;
                report(app, result);
            }
            KeyCode::Char(' ') => {
                let result = app.toggle_pause().await;
                report(app, result);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let result = app.seek(10.0).await;
                report(app, result);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let result = app.seek(-10.0).await;
                report(app, result);
            }
            KeyCode::Char('n') => {
                let result = app.next_track().await;
                report(app, result);
            }
            KeyCode::Char('p') => {
                let result = app.prev_track().await;
                report(app, result);
            }
            KeyCode::Char('v') => {
                let result = app.toggle_video().await;
                report(app, result);
            }
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            _ => {}
        },
    }
}

/// If the command failed, shows the error in the status bar instead of propagating it.
fn report(app: &mut App, result: Result<()>) {
    if let Err(e) = result {
        app.status = format!("Error: {e:#}");
    }
}

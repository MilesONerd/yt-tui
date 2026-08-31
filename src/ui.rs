// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Enzo Costa Fuke

//! Renders [`crate::app::App`]'s state to the terminal with `ratatui`.
//!
//! Everything here is a pure, stateless view over [`App`]: search bar on
//! top, the active list ([`crate::app::ListSource::Search`] or
//! [`crate::app::ListSource::History`]) next to the playback queue in the
//! middle, then a progress gauge and a status line at the bottom.

use crate::app::{App, ListSource, Mode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
};

/// Draws one full frame of the UI from the current `app` state. Called
/// once per iteration of the main loop, before any state-changing await.
pub fn draw(frame: &mut Frame, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(5),    // main list + queue
            Constraint::Length(3), // progress bar
            Constraint::Length(3), // status
        ])
        .split(frame.area());

    draw_search_bar(frame, app, rows[0]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(rows[1]);

    draw_main_list(frame, app, cols[0]);
    draw_queue(frame, app, cols[1]);

    draw_progress(frame, app, rows[2]);
    draw_status(frame, app, rows[3]);
}

fn draw_search_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let style = if app.mode == Mode::Searching {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let search = Paragraph::new(app.query.as_str()).style(style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Search (/ to focus, Enter to search)"),
    );
    frame.render_widget(search, area);
}

fn draw_main_list(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (list_data, title): (&[crate::yt::Video], &str) = match app.list_source {
        ListSource::Search => (&app.results, "Results (H: history | Enter/a: enqueue | r: play now)"),
        ListSource::History => (&app.history.entries, "History (H: back to search | Enter/a: enqueue | r: play now)"),
    };

    let items: Vec<ListItem> = list_data
        .iter()
        .map(|v| {
            let mut spans = vec![
                Span::styled(
                    format!("[{}] ", v.duration_fmt()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(v.title.clone()),
            ];
            if let Some(uploader) = &v.uploader {
                spans.push(Span::styled(
                    format!("  — {uploader}"),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default();
    if !list_data.is_empty() {
        list_state.select(Some(app.selected.min(list_data.len().saturating_sub(1))));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Green),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_queue(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let current_idx = app.playlist_pos.and_then(|i| usize::try_from(i).ok());

    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let is_current = current_idx == Some(i);
            let marker = if is_current { "▶ " } else { "  " };
            let style = if is_current {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{marker}{}", v.title),
                style,
            )))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Queue ({}) — n/p change track, c clears", app.queue.len())),
    );
    frame.render_widget(list, area);
}

fn draw_progress(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (position, duration) = (app.position.unwrap_or(0.0), app.duration_secs.unwrap_or(0.0));
    let ratio = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = format!("{} / {}", fmt_time(position), fmt_time(duration));

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio(ratio)
        .label(label);

    frame.render_widget(gauge, area);
}

fn draw_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let playback = match &app.now_playing {
        Some(v) => {
            let state = if app.paused { "paused" } else { "playing" };
            format!("{state}: {}", v.title)
        }
        None => "nothing playing".to_string(),
    };
    let audio_mode = if app.video_enabled { "video" } else { "audio-only" };

    let text = format!("{playback} [{audio_mode}, v: toggle] | {}", app.status);
    let status = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Status"));
    frame.render_widget(status, area);
}

fn fmt_time(secs: f64) -> String {
    let secs = secs.max(0.0) as u64;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

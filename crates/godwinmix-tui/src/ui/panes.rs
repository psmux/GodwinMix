//! The three lists: sources, destinations, alerts.

use super::{colour_of, display_name, meter, pane_block, BROKEN, LIVE, WAITING};
use crate::app::{App, Pane};
use crate::model::{clock_ms, gain_to_db, Meters, OutputState, OutputStatus, SourceState, SourceStatus};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub fn sources(frame: &mut Frame, app: &App, area: Rect) {
    let view = app.view();
    let visible = app.visible_sources();
    let title = match (view.ready, app.filter.is_empty()) {
        (false, _) => "sources (waiting for the mixer)".to_string(),
        (true, true) => format!("sources ({})", visible.len()),
        (true, false) => format!("sources ({} of {}, filter '{}')", visible.len(), view.status.sources.len(), app.filter),
    };
    let block = pane_block(&title, app.pane == Pane::Sources);
    if visible.is_empty() {
        let text = if view.ready {
            "no sources. Add one with the web UI, gmx ctl, or curl: see docs/how-to/control-the-mixer.md"
        } else {
            "waiting for event/snapshot"
        };
        frame.render_widget(Paragraph::new(text).block(block), area);
        return;
    }
    let width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, source)| ListItem::new(source_row(app, i, source, width)))
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("");
    let selected = if app.pane == Pane::Sources { Some(app.sel_source) } else { None };
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
}

/// One source: its slot number, its colour, its name, its state, its tally,
/// its meter, and where it has got to if it is a clip.
fn source_row<'a>(app: &App, index: usize, source: &'a SourceStatus, width: usize) -> Line<'a> {
    let view = app.view();
    let slot = if index < 9 { format!("{} ", index + 1) } else { "  ".to_string() };
    let tally = view.tally_of(&source.id);
    let name_width = width.saturating_sub(34).max(8);
    let mut spans = vec![
        Span::styled(slot, Style::default().fg(Color::DarkGray)),
        Span::styled("● ", Style::default().fg(colour_of(source))),
        Span::styled(pad(&display_name(source), name_width), name_style(source, tally)),
        Span::styled(pad(source.state.label(), 11), state_style(source.state)),
        Span::styled(pad(tally_label(tally), 4), tally_style(tally)),
    ];
    spans.extend(meter(Meters::peak(view.meters.sources.get(&source.id).map(Vec::as_slice).unwrap_or(&[])), 8));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(audio_note(source), audio_style(source)));
    if source.seekable {
        spans.push(Span::raw(format!(" {}", position(source))));
    }
    Line::from(spans)
}

fn position(source: &SourceStatus) -> String {
    match (source.position_ms, source.duration_ms) {
        (Some(at), Some(of)) => format!("{}/{}", clock_ms(at), clock_ms(of)),
        (Some(at), None) => clock_ms(at),
        _ => "--:--:--".to_string(),
    }
}

fn audio_note(source: &SourceStatus) -> String {
    if !source.has_audio {
        return "  no audio".to_string();
    }
    if source.muted {
        return "     muted".to_string();
    }
    format!("{:>+7.0} dB", gain_to_db(source.gain))
}

fn audio_style(source: &SourceStatus) -> Style {
    if source.muted {
        Style::default().fg(BROKEN)
    } else if !source.has_audio {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    }
}

fn name_style(source: &SourceStatus, tally: &str) -> Style {
    let base = Style::default();
    if tally == "program" {
        base.fg(BROKEN).add_modifier(Modifier::BOLD)
    } else if source.state == SourceState::Live {
        base
    } else {
        base.fg(Color::DarkGray)
    }
}

fn state_style(state: SourceState) -> Style {
    Style::default().fg(match state {
        SourceState::Live => LIVE,
        SourceState::Connecting => WAITING,
        SourceState::Stalled => WAITING,
        SourceState::Failed => BROKEN,
    })
}

fn tally_label(tally: &str) -> &'static str {
    match tally {
        "program" => "PGM",
        "preview" => "PVW",
        _ => "",
    }
}

fn tally_style(tally: &str) -> Style {
    match tally {
        "program" => Style::default().fg(Color::Black).bg(BROKEN),
        "preview" => Style::default().fg(Color::Black).bg(LIVE),
        _ => Style::default(),
    }
}

pub fn outputs(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_outputs();
    let block = pane_block(&format!("destinations ({})", visible.len()), app.pane == Pane::Outputs);
    if visible.is_empty() {
        frame.render_widget(
            Paragraph::new("no destinations. The programme is being mixed and going nowhere.")
                .block(block),
            area,
        );
        return;
    }
    let width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> =
        visible.iter().map(|output| ListItem::new(output_row(output, width))).collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let selected = if app.pane == Pane::Outputs { Some(app.sel_output) } else { None };
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
}

fn output_row(output: &OutputStatus, width: usize) -> Line<'_> {
    let name_width = width.saturating_sub(32).max(8);
    Line::from(vec![
        Span::raw(pad(&output.id, name_width)),
        Span::styled(pad(output.state.label(), 14), output_style(output.state)),
        Span::raw(format!("{:>3} retries", output.reconnects)),
        Span::styled(
            format!("{:>6.1} s queued", output.queue_secs),
            if output.queue_secs > 2.0 {
                Style::default().fg(WAITING)
            } else {
                Style::default()
            },
        ),
    ])
}

fn output_style(state: OutputState) -> Style {
    Style::default().fg(match state {
        OutputState::Live => LIVE,
        OutputState::Connecting | OutputState::Reconnecting => WAITING,
        OutputState::Failed => BROKEN,
    })
}

pub fn alerts(frame: &mut Frame, app: &App, area: Rect) {
    let view = app.view();
    let block = pane_block(
        &format!("alerts ({}, newest first)", view.alerts.len()),
        app.pane == Pane::Alerts,
    );
    if view.alerts.is_empty() {
        frame.render_widget(Paragraph::new("nothing has gone wrong yet.").block(block), area);
        return;
    }
    let items: Vec<ListItem> = view
        .alerts
        .iter()
        .map(|alert| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", alert.at), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    pad(&alert.severity, 8),
                    Style::default().fg(severity_colour(&alert.severity)),
                ),
                Span::raw(alert.message.clone()),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let selected = if app.pane == Pane::Alerts { Some(app.sel_alert) } else { None };
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
}

fn severity_colour(severity: &str) -> Color {
    match severity {
        "critical" | "error" => BROKEN,
        "warning" => WAITING,
        _ => Color::Gray,
    }
}

/// Pad or cut to a column width, counting characters. Good enough for names
/// and ids, which the core keeps to slugs and short labels.
fn pad(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count >= width {
        let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
        out.push('…');
        out
    } else {
        format!("{text}{}", " ".repeat(width - count))
    }
}

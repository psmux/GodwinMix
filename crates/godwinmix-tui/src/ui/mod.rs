//! The screen.
//!
//! One layout: the programme across the top, sources down the left, the
//! destinations and the alerts down the right, one log line at the bottom.
//! Everything is painted from `store.view()`, which only moves at
//! `event/flush`.

mod help;
mod panes;

use crate::app::{App, Link, Mode};
use crate::model::{clock_ms, clock_secs};
use crate::picture::Picture;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// Green for live, yellow for waiting, red for broken. The same three
/// everywhere, so a glance at any pane means the same thing.
pub const LIVE: Color = Color::Green;
pub const WAITING: Color = Color::Yellow;
pub const BROKEN: Color = Color::Red;

/// Draw one frame. Answers with the rectangle the mosaic was given, when the
/// operator asked for a picture and the terminal draws it with escapes rather
/// than with cells.
pub fn draw(frame: &mut Frame, app: &App, picture: Picture) -> Option<Rect> {
    let [top, body, footer] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(3), Constraint::Length(1)])
            .areas(frame.area());
    programme(frame, app, top);
    let reserved = middle(frame, app, body, picture);
    footer_line(frame, app, footer);
    if app.mode == Mode::Help {
        help::draw(frame, app);
    }
    reserved
}

/// The programme block: what is on air, how long it has been up, the running
/// time, the encoder behind it, and whether the link is there at all.
fn programme(frame: &mut Frame, app: &App, area: Rect) {
    let view = app.view();
    let on_air = view.status.program.clone();
    let name = on_air
        .as_ref()
        .and_then(|id| view.source(id))
        .map(|s| format!("{} ({})", display_name(s), s.id))
        .or_else(|| on_air.clone())
        .unwrap_or_else(|| "black (the slate)".to_string());
    let backend = &view.status.backend;
    let mut lines = vec![
        Line::from(vec![
            Span::styled("ON AIR  ", Style::default().fg(Color::Black).bg(BROKEN)),
            Span::raw(" "),
            Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw(format!("running {}  ", clock_ms(view.status.running_time_ms))),
            Span::raw(format!("up {}  ", clock_secs(view.status.uptime_secs))),
            Span::raw(format!(
                "{} / {}{}",
                short_element(&backend.video_encoder),
                short_element(&backend.audio_encoder),
                if backend.hardware_accelerated { " (hardware)" } else { "" }
            )),
        ]),
        Line::from(link_spans(app)),
    ];
    if let Some(ad) = &view.status.ad {
        lines.push(Line::from(vec![Span::styled(
            format!(
                "ad break {}: {}{}",
                if ad.on_air { "on air" } else { "armed" },
                ad.uri,
                ad.return_to.as_ref().map(|r| format!(", back to {r}")).unwrap_or_default()
            ),
            Style::default().fg(WAITING),
        )]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" programme ")),
        area,
    );
}

fn link_spans(app: &App) -> Vec<Span<'static>> {
    let meter = crate::model::Meters::peak(&app.view().meters.program);
    let program_meter = match meter {
        Some(db) if db > -100.0 => format!("programme peak {db:>6.1} dBFS"),
        _ => "programme peak  --".to_string(),
    };
    match &app.link {
        Link::Live => vec![
            Span::styled("linked", Style::default().fg(LIVE)),
            Span::raw(format!("  seq {}  ", app.store.seq)),
            Span::raw(program_meter),
        ],
        Link::Connecting => vec![
            Span::styled("connecting", Style::default().fg(WAITING)),
            Span::raw("  waiting for the snapshot"),
        ],
        Link::Retrying { reason, in_secs } => vec![
            Span::styled("no link", Style::default().fg(BROKEN)),
            Span::raw(format!("  {reason}. Trying again in {in_secs} s")),
        ],
    }
}

/// Sources on the left; the picture, destinations and alerts on the right.
fn middle(frame: &mut Frame, app: &App, area: Rect, picture: Picture) -> Option<Rect> {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);
    panes::sources(frame, app, left);
    let picture_rows = if app.wants_picture { picture_height(right) } else { 0 };
    let [image, outputs, alerts] = Layout::vertical([
        Constraint::Length(picture_rows),
        Constraint::Length(outputs_height(app, right, picture_rows)),
        Constraint::Min(3),
    ])
    .areas(right);
    panes::outputs(frame, app, outputs);
    panes::alerts(frame, app, alerts);
    if picture_rows == 0 {
        return None;
    }
    crate::picture::draw(frame, app, image, picture)
}

/// Half the right column, at most, and never less than six rows or there is
/// no picture worth looking at.
fn picture_height(right: Rect) -> u16 {
    (right.height / 2).clamp(0, 24).max(6).min(right.height.saturating_sub(8))
}

fn outputs_height(app: &App, right: Rect, picture_rows: u16) -> u16 {
    let wanted = app.view().status.outputs.len() as u16 + 2;
    let room = right.height.saturating_sub(picture_rows).saturating_sub(5);
    wanted.clamp(3, room.max(3))
}

/// The log line: the last thing that happened, and what to do about it.
fn footer_line(frame: &mut Frame, app: &App, area: Rect) {
    let text = match app.mode {
        Mode::Filter => format!("filter: {}_  (Enter keeps it, Escape clears it)", app.input),
        Mode::AdUri => format!("ad break clip: {}_  (Enter rolls it, Escape gives up)", app.input),
        _ => app.footer.text.clone(),
    };
    let style = if app.footer.bad && app.mode == Mode::Normal {
        Style::default().fg(Color::Black).bg(BROKEN)
    } else {
        Style::default().fg(Color::Black).bg(Color::Gray)
    };
    frame.render_widget(Paragraph::new(Line::from(text)).style(style), area);
}

/// A source's name, or its id when it has not got one.
pub fn display_name(source: &crate::model::SourceStatus) -> String {
    if source.name.is_empty() {
        source.id.clone()
    } else {
        source.name.clone()
    }
}

/// The tile colour. A core that grows `source.set {color}` sends one; until
/// then it comes off the id, so a source keeps its colour between runs and
/// between clients.
pub fn colour_of(source: &crate::model::SourceStatus) -> Color {
    if let Some(name) = &source.color {
        if let Some(colour) = named_colour(name) {
            return colour;
        }
    }
    const WHEEL: [Color; 8] = [
        Color::Cyan,
        Color::Magenta,
        Color::Blue,
        Color::Green,
        Color::Yellow,
        Color::LightCyan,
        Color::LightMagenta,
        Color::LightBlue,
    ];
    let hash = source.id.bytes().fold(17u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    WHEEL[(hash % WHEEL.len() as u32) as usize]
}

fn named_colour(name: &str) -> Option<Color> {
    if let Some(hex) = name.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        return None;
    }
    match name.to_lowercase().as_str() {
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "blue" => Some(Color::Blue),
        "cyan" => Some(Color::Cyan),
        "magenta" | "purple" => Some(Color::Magenta),
        "yellow" => Some(Color::Yellow),
        "orange" => Some(Color::LightRed),
        "grey" | "gray" => Some(Color::Gray),
        _ => None,
    }
}

/// `nvh264enc` rather than the whole element description, because the header
/// has one line for it.
fn short_element(name: &str) -> &str {
    if name.is_empty() {
        "?"
    } else {
        name
    }
}

/// One meter, as a bar of blocks. Peak dBFS in, so -60 is empty and 0 is full.
pub fn meter(peak_db: Option<f64>, width: usize) -> Vec<Span<'static>> {
    let Some(db) = peak_db else {
        return vec![Span::styled("─".repeat(width), Style::default().fg(Color::DarkGray))];
    };
    let filled = (((db + 60.0) / 60.0).clamp(0.0, 1.0) * width as f64).round() as usize;
    let colour = if db > -3.0 {
        BROKEN
    } else if db > -18.0 {
        WAITING
    } else {
        LIVE
    };
    vec![
        Span::styled("█".repeat(filled), Style::default().fg(colour)),
        Span::styled("─".repeat(width.saturating_sub(filled)), Style::default().fg(Color::DarkGray)),
    ]
}

/// The block a focused pane gets, so it is obvious which list the arrow keys
/// are moving.
pub fn pane_block(title: &str, focused: bool) -> Block<'static> {
    let block = Block::bordered().title(format!(" {title} "));
    if focused {
        block.border_style(Style::default().fg(Color::White).bold())
    } else {
        block.border_style(Style::default().fg(Color::DarkGray))
    }
}

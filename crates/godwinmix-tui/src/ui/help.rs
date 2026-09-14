//! The `?` sheet: the key map, and what this build could not do.

use crate::app::App;
use crate::keys::HELP;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = centred(frame.area(), 62, HELP.len() as u16 + 6);
    let mut lines: Vec<Line> = HELP
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("  {key:<12}"), Style::default().fg(Color::Cyan)),
                Span::raw(*what),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    if app.revert_missing {
        lines.push(Line::styled(
            "  this core has no program.revert, so r says so and does nothing",
            Style::default().fg(Color::Yellow),
        ));
    }
    if !app.ignored_ext.is_empty() {
        lines.push(Line::styled(
            format!("  this core ignored ext: {}", app.ignored_ext.join(", ")),
            Style::default().fg(Color::Yellow),
        ));
    }
    lines.push(Line::raw("  every key here is one call any other client can make"));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" keys (? closes this) ")),
        area,
    );
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let [middle] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [centre] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(middle);
    centre
}

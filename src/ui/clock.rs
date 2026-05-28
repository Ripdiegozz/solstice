use chrono::DateTime;
use chrono_tz::Tz;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::config::Config;

/// Clock widget showing current time, date, and timezone
pub struct ClockWidget<'a> {
    pub now: DateTime<Tz>,
    pub config: &'a Config,
}

impl<'a> ClockWidget<'a> {
    pub fn new(now: DateTime<Tz>, config: &'a Config) -> Self {
        Self { now, config }
    }
}

impl<'a> Widget for ClockWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let time_str = self.now.format(&self.config.time_format).to_string();
        let date_str = self.now.format(&self.config.date_format).to_string();
        let tz_label = format!("({})", self.config.timezone);

        let accent = parse_color(&self.config.theme.accent);
        let text_color = parse_color(&self.config.theme.text);
        let muted = parse_color(&self.config.theme.muted);

        let lines = vec![
            Line::from(Span::styled(
                &time_str,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(&date_str, Style::default().fg(text_color)),
                Span::raw("  "),
                Span::styled(&tz_label, Style::default().fg(muted)),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::NONE);

        Paragraph::new(lines)
            .block(block)
            .render(area, buf);
    }
}

/// Parse a hex color string (#RRGGBB) into a ratatui Color
pub fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::White;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color::Rgb(r, g, b)
}

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

        let lines = vec![
            Line::from(Span::styled(
                &time_str,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(&date_str, Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(&tz_label, Style::default().fg(Color::DarkGray)),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::NONE);

        Paragraph::new(lines)
            .block(block)
            .render(area, buf);
    }
}

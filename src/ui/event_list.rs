use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

use crate::config::Config;
use crate::events::Event;
use crate::ui::clock::parse_color;

/// Event list panel showing events for the selected day
pub struct EventListWidget<'a> {
    pub events: &'a [Event],
    pub selected_date_label: String,
    pub config: &'a Config,
    pub selected_index: Option<usize>,
}

impl<'a> EventListWidget<'a> {
    pub fn new(events: &'a [Event], selected_date_label: String, config: &'a Config) -> Self {
        Self { events, selected_date_label, config, selected_index: None }
    }

    pub fn with_selected_index(mut self, index: Option<usize>) -> Self {
        self.selected_index = index;
        self
    }
}

impl<'a> Widget for EventListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let _text_color = parse_color(&self.config.theme.text);
        let muted = parse_color(&self.config.theme.muted);
        let accent = parse_color(&self.config.theme.accent);
        let surface = parse_color(&self.config.theme.surface);

        let block = Block::default()
            .title(format!(" {} ", self.selected_date_label))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(surface));

        if self.events.is_empty() {
            let empty_msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No events for this day",
                    Style::default().fg(muted),
                )),
            ])
            .block(block);
            empty_msg.render(area, buf);
            return;
        }

        let items: Vec<ListItem> = self.events.iter().enumerate().map(|(idx, event)| {
            let time_label = match (&event.start_time, &event.end_time) {
                (Some(start), Some(end)) => format!("{}-{} ", start, end),
                (Some(start), None) => format!("{} ", start),
                _ => String::new(),
            };

            let source_marker = if event.source == crate::events::EventSource::GoogleCalendar {
                " [gcal]"
            } else {
                ""
            };

            let is_selected = self.selected_index == Some(idx);

            let style = if is_selected {
                Style::default()
                    .fg(accent)
                    .bg(surface)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(time_label, if is_selected { Style::default().fg(accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(accent) }),
                Span::styled(event.title.clone(), style),
                Span::styled(source_marker, Style::default().fg(muted)),
            ]))
        }).collect();

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

use ratatui::widgets::Paragraph;

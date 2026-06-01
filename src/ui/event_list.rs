use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

use crate::config::Config;
use crate::events::Event;

/// Hit-test an event list: return the event index under y if any
pub fn hit_test_event_list(y: u16, rect: Rect, event_count: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(rect);
    if inner.height < 1 || event_count == 0 {
        return None;
    }
    if y < inner.y || y >= inner.y + inner.height {
        return None;
    }
    let idx = (y - inner.y) as usize;
    if idx < event_count {
        Some(idx)
    } else {
        None
    }
}

/// Event list panel showing events for the selected day
pub struct EventListWidget<'a> {
    pub events: &'a [Event],
    pub selected_date_label: String,
    pub config: &'a Config,
    pub selected_index: Option<usize>,
    pub focused: bool,
}

impl<'a> EventListWidget<'a> {
    pub fn new(events: &'a [Event], selected_date_label: String, config: &'a Config) -> Self {
        Self { events, selected_date_label, config, selected_index: None, focused: false }
    }

    pub fn with_selected_index(mut self, index: Option<usize>) -> Self {
        self.selected_index = index;
        self
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for EventListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" [2]-Events ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { Color::Magenta } else { Color::DarkGray }));

        if self.events.is_empty() {
            let empty_msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No events for this day",
                    Style::default().fg(Color::DarkGray),
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
                    .fg(Color::Magenta)
                    .bg(Color::Reset)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(time_label, if is_selected { Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Magenta) }),
                Span::styled(event.title.clone(), style),
                Span::styled(source_marker, Style::default().fg(Color::DarkGray)),
            ]))
        }).collect();

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn test_hit_test_event_list_correct() {
        let rect = Rect::new(10, 5, 20, 10);
        // Inner: x=11..28, y=6..13
        // idx = y - inner.y
        assert_eq!(hit_test_event_list(6, rect, 5), Some(0));
        assert_eq!(hit_test_event_list(7, rect, 5), Some(1));
        assert_eq!(hit_test_event_list(8, rect, 5), Some(2));
    }

    #[test]
    fn test_hit_test_event_list_empty_returns_none() {
        let rect = Rect::new(10, 5, 20, 10);
        assert_eq!(hit_test_event_list(6, rect, 0), None);
    }

    #[test]
    fn test_hit_test_event_list_outside_returns_none() {
        let rect = Rect::new(10, 5, 20, 10);
        assert_eq!(hit_test_event_list(4, rect, 5), None); // above
        assert_eq!(hit_test_event_list(15, rect, 5), None); // below
    }
}

use ratatui::widgets::Paragraph;

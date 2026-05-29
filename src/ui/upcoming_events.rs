use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

use crate::config::Config;
use crate::events::Event;
use crate::ui::clock::parse_color;

/// Hit-test an upcoming events list: return the event index under y if any
pub fn hit_test_upcoming(y: u16, rect: Rect, event_count: usize) -> Option<usize> {
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

/// Upcoming events panel
pub struct UpcomingEventsWidget<'a> {
    events: &'a [Event],
    config: &'a Config,
    focused: bool,
    selected_index: Option<usize>,
}

impl<'a> UpcomingEventsWidget<'a> {
    pub fn new(events: &'a [Event], config: &'a Config) -> Self {
        Self { events, config, focused: false, selected_index: None }
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn with_selected_index(mut self, index: Option<usize>) -> Self {
        self.selected_index = index;
        self
    }
}

impl<'a> Widget for UpcomingEventsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text_color = parse_color(&self.config.theme.text);
        let muted = parse_color(&self.config.theme.muted);
        let accent = parse_color(&self.config.theme.accent);
        let surface = parse_color(&self.config.theme.surface);

        let block = Block::default()
            .title(" [3]-Upcoming ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { accent } else { surface }));

        if self.events.is_empty() {
            let empty_msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No upcoming events",
                    Style::default().fg(muted),
                )),
            ])
            .block(block);
            empty_msg.render(area, buf);
            return;
        }

        let items: Vec<ListItem> = self.events.iter().map(|event| {
            let date_label = event.date.format("%b %d").to_string();

            let time_label = match (&event.start_time, &event.end_time) {
                (Some(start), Some(end)) => format!("{}-{} ", start, end),
                (Some(start), None) => format!("{} ", start),
                _ => String::new(),
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", date_label), Style::default().fg(muted)),
                Span::styled(time_label, Style::default().fg(accent)),
                Span::styled(event.title.clone(), Style::default().fg(text_color)),
            ]))
        }).collect();

        let list = List::new(items).block(block);
        list.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::events::{EventSource, Recurrence};
    use chrono::NaiveDate;
    use ratatui::layout::Rect;

    /// Helper: scan a buffer range for a substring
    fn buffer_contains(buf: &Buffer, x_start: u16, x_end: u16, y_start: u16, y_end: u16, needle: &str) -> bool {
        for y in y_start..y_end {
            let mut line = String::new();
            for x in x_start..x_end {
                line.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            if line.contains(needle) {
                return true;
            }
        }
        false
    }

    fn make_event(id: i64, title: &str, date: NaiveDate, start: Option<&str>, end: Option<&str>) -> Event {
        Event {
            id,
            title: title.to_string(),
            description: None,
            date,
            start_time: start.map(String::from),
            end_time: end.map(String::from),
            source: EventSource::Local,
            recurrence: Recurrence::None,
        }
    }

    #[test]
    fn test_upcoming_renders_events() {
        let config = Config::default();
        let today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let events = vec![
            make_event(1, "Team Standup", today, Some("09:00"), Some("09:30")),
            make_event(2, "Lunch", today, Some("12:00"), Some("13:00")),
        ];

        let widget = UpcomingEventsWidget::new(&events, &config);
        let area = Rect::new(0, 0, 40, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Block borders take y=0 and y=7. Inner area is y=1..6. List items start at y=1.
        assert!(buffer_contains(&buf, 1, 35, 1, 7, "Team Standup"), "Should show event title");
        assert!(buffer_contains(&buf, 1, 35, 1, 7, "Lunch"), "Should show second event");
    }

    #[test]
    fn test_upcoming_renders_date_and_time() {
        let config = Config::default();
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let events = vec![
            make_event(1, "Standup", date, Some("09:00"), Some("09:30")),
        ];

        let widget = UpcomingEventsWidget::new(&events, &config);
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        assert!(buffer_contains(&buf, 1, 35, 1, 5, "Jun 15"), "Should show formatted date");
        assert!(buffer_contains(&buf, 1, 35, 1, 5, "09:00-09:30"), "Should show time range");
    }

    #[test]
    fn test_upcoming_empty_state() {
        let config = Config::default();
        let events: Vec<Event> = vec![];

        let widget = UpcomingEventsWidget::new(&events, &config);
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        assert!(buffer_contains(&buf, 1, 35, 1, 5, "No upcoming events"), "Should show empty message");
    }

    #[test]
    fn test_upcoming_event_without_time() {
        let config = Config::default();
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let events = vec![
            make_event(1, "All Day Event", date, None, None),
        ];

        let widget = UpcomingEventsWidget::new(&events, &config);
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        assert!(buffer_contains(&buf, 1, 35, 1, 5, "All Day Event"), "Should show title");
        assert!(buffer_contains(&buf, 1, 35, 1, 5, "Jun 15"), "Should show date");
        // Should NOT contain a time pattern
        let found_time = buffer_contains(&buf, 1, 35, 1, 5, ":");
        assert!(!found_time, "Should NOT show time for event without start_time");
    }

    #[test]
    fn test_upcoming_date_format_at_month_boundary() {
        let config = Config::default();
        let events = vec![
            make_event(1, "End of June", NaiveDate::from_ymd_opt(2026, 6, 30).unwrap(), None, None),
            make_event(2, "July Start", NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(), None, None),
        ];

        let widget = UpcomingEventsWidget::new(&events, &config);
        let area = Rect::new(0, 0, 40, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        assert!(buffer_contains(&buf, 1, 35, 1, 7, "Jun 30"), "First event should show Jun 30");
        assert!(buffer_contains(&buf, 1, 35, 1, 7, "Jul 01"), "Second event should show Jul 01 (leading zero)");
    }

    #[test]
    fn test_upcoming_respects_limit() {
        let config = Config::default();
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        // Build events beyond config.upcoming_event_count (default 5)
        let events: Vec<Event> = (0..10)
            .map(|i| make_event(i as i64, &format!("Event {}", i), date, None, None))
            .collect();

        let widget = UpcomingEventsWidget::new(&events, &config);
        // Area tall enough to show all items (inner height = 10)
        let area = Rect::new(0, 0, 50, 14);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // All events passed should render (truncation is data-layer responsibility)
        assert!(buffer_contains(&buf, 1, 45, 1, 12, "Event 0"), "First event rendered");
        assert!(buffer_contains(&buf, 1, 45, 1, 12, "Event 9"), "Last event rendered");
    }

    #[test]
    fn test_hit_test_upcoming_correct() {
        let rect = Rect::new(10, 5, 20, 10);
        // Inner: x=11..28, y=6..13
        assert_eq!(hit_test_upcoming(6, rect, 5), Some(0));
        assert_eq!(hit_test_upcoming(7, rect, 5), Some(1));
        assert_eq!(hit_test_upcoming(8, rect, 5), Some(2));
    }

    #[test]
    fn test_hit_test_upcoming_empty_returns_none() {
        let rect = Rect::new(10, 5, 20, 10);
        assert_eq!(hit_test_upcoming(6, rect, 0), None);
    }

    #[test]
    fn test_hit_test_upcoming_outside_returns_none() {
        let rect = Rect::new(10, 5, 20, 10);
        assert_eq!(hit_test_upcoming(4, rect, 5), None);
        assert_eq!(hit_test_upcoming(15, rect, 5), None);
    }
}

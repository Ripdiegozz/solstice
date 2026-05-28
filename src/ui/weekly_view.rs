use chrono::{Datelike, NaiveDate};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::HashMap;

use crate::app::App;
use crate::config::Config;
use crate::events::Event;
use crate::ui::clock::parse_color;

/// Hit-test a weekly calendar view: return the date under (x, y) if any
pub fn hit_test_weekly(x: u16, y: u16, rect: Rect, app: &App) -> Option<NaiveDate> {
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(rect);

    if inner.height < 2 || inner.width < 25 {
        return None;
    }
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }

    let row = (y as i64) - (inner.y as i64 + 1);
    if !(0..7).contains(&row) {
        return None;
    }
    let ws = crate::calendar::grid::week_start_date(app.selected_date, app.config.first_day_of_week);
    Some(ws + chrono::Duration::days(row))
}

/// Weekly calendar view widget showing 7 days with event summaries
pub struct WeeklyView<'a> {
    pub week_start: NaiveDate,
    pub selected_date: NaiveDate,
    pub today: NaiveDate,
    pub holidays: &'a HashMap<NaiveDate, String>,
    pub week_events: &'a HashMap<NaiveDate, Vec<Event>>,
    pub config: &'a Config,
    pub focused: bool,
}

impl<'a> WeeklyView<'a> {
    pub fn new(
        week_start: NaiveDate,
        selected_date: NaiveDate,
        today: NaiveDate,
        holidays: &'a HashMap<NaiveDate, String>,
        week_events: &'a HashMap<NaiveDate, Vec<Event>>,
        config: &'a Config,
    ) -> Self {
        Self { week_start, selected_date, today, holidays, week_events, config, focused: false }
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for WeeklyView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text_color = parse_color(&self.config.theme.text);
        let _muted = parse_color(&self.config.theme.muted);
        let accent = parse_color(&self.config.theme.accent);
        let today_color = parse_color(&self.config.theme.today);
        let holiday_color = parse_color(&self.config.theme.holiday);
        let surface = parse_color(&self.config.theme.surface);

        let week_end = self.week_start + chrono::Duration::days(6);
        let block = Block::default()
            .title(format!(" Week of {} ", self.week_start.format("%b %-d")))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { accent } else { surface }));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 25 {
            return;
        }

        // Day name abbreviations
        let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let day_start = match self.config.first_day_of_week {
            crate::config::FirstDayOfWeek::Sunday => 6, // offset: Sun is last
            crate::config::FirstDayOfWeek::Monday => 0,
        };

        let _ = week_end; // suppress unused warning

        // Render 7 rows, one per day
        for i in 0..7 {
            let date = self.week_start + chrono::Duration::days(i);
            let y = inner.y + 1 + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let day_idx = (i as usize + day_start) % 7;
            let day_name = day_names[day_idx];

            let is_today = date == self.today;
            let is_selected = date == self.selected_date;
            let is_holiday = self.holidays.contains_key(&date);
            let events = self.week_events.get(&date).map(|v| v.as_slice()).unwrap_or(&[]);

            // Compute style for this row
            let date_style = if is_today {
                Style::default()
                    .fg(today_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if is_selected {
                Style::default()
                    .fg(accent)
                    .add_modifier(Modifier::BOLD)
            } else if is_holiday {
                Style::default().fg(holiday_color)
            } else {
                Style::default().fg(text_color)
            };

            // Day name + date
            let date_label = format!("{} {:>2}", day_name, date.day());
            buf.set_string(inner.x, y, &date_label, date_style);

            // Event summary
            let summary_x = inner.x + 7;
            if summary_x < inner.x + inner.width && !events.is_empty() {
                let event_color = parse_color(&self.config.theme.event);
                let max_len = (inner.x + inner.width - summary_x) as usize;

                let summaries: Vec<String> = events.iter()
                    .take(3)
                    .map(|e| {
                        if let Some(ref t) = e.start_time {
                            format!("{} {}", t, e.title)
                        } else {
                            e.title.clone()
                        }
                    })
                    .collect();

                let mut summary_text = summaries.join(", ");
                if events.len() > 3 {
                    summary_text.push_str(&format!(" +{} more", events.len() - 3));
                }

                if summary_text.len() > max_len {
                    summary_text.truncate(max_len.saturating_sub(1));
                    summary_text.push('…');
                }

                buf.set_string(summary_x, y, &summary_text, Style::default().fg(event_color));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::config::FirstDayOfWeek;
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

    #[test]
    fn test_weekly_view_renders_day_names() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let selected = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config);
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Content area: inner.x=1, inner.y+1=2 through inner.y+7=8
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Mon"), "Should show Monday in first row");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Tue"), "Should show Tuesday");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Wed"), "Should show Wednesday");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Thu"), "Should show Thursday");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Fri"), "Should show Friday");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Sat"), "Should show Saturday");
        assert!(buffer_contains(&buf, 1, 6, 2, 9, "Sun"), "Should show Sunday");
    }

    #[test]
    fn test_weekly_view_shows_event_summaries() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = week_start;
        let today = week_start;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        let mon = week_start;
        week_events.insert(mon, vec![
            Event {
                id: 1,
                title: "Standup".to_string(),
                description: None,
                date: mon,
                start_time: Some("09:00".to_string()),
                end_time: Some("09:30".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
            Event {
                id: 2,
                title: "Lunch".to_string(),
                description: None,
                date: mon,
                start_time: Some("12:00".to_string()),
                end_time: Some("13:00".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config);
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Event summaries are at x >= 8 (inner.x + 7), y in 2..9 (inner.y+1..inner.y+8)
        assert!(buffer_contains(&buf, 8, 55, 2, 9, "Standup"), "Standup should appear in event summary");
        assert!(buffer_contains(&buf, 8, 55, 2, 9, "Lunch"), "Lunch should appear in event summary");
    }

    #[test]
    fn test_weekly_view_shows_overflow_indicator() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = week_start;
        let today = week_start;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        let mon = week_start;
        week_events.insert(mon, (1..=5).map(|i| Event {
            id: i as i64,
            title: format!("Event {}", i),
            description: None,
            date: mon,
            start_time: None,
            end_time: None,
            source: crate::events::EventSource::Local,
            recurrence: crate::events::Recurrence::None,
        }).collect());

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config);
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        assert!(buffer_contains(&buf, 8, 55, 2, 9, "+2 more"), "Should show '+2 more' for 5 events with 3 displayed");
    }

    #[test]
    fn test_weekly_view_respects_sunday_start() {
        let mut config = Config::default();
        config.first_day_of_week = FirstDayOfWeek::Sunday;
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap(); // Sunday
        let selected = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let today = selected;
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config);
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // First day row (inner.y+1 = 2) should be "Sun 14"
        assert!(buffer_contains(&buf, 1, 7, 2, 3, "Sun"), "First row should be Sun");
        assert!(buffer_contains(&buf, 1, 7, 2, 3, "14"), "First row should show date 14");
    }

    #[test]
    fn test_weekly_view_respects_monday_start() {
        let mut config = Config::default();
        config.first_day_of_week = FirstDayOfWeek::Monday;
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let selected = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let today = selected;
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config);
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // First day row (inner.y+1 = 2) should be "Mon 15"
        assert!(buffer_contains(&buf, 1, 7, 2, 3, "Mon"), "First row should be Mon");
        assert!(buffer_contains(&buf, 1, 7, 2, 3, "15"), "First row should show date 15");
    }

    #[test]
    fn test_hit_test_weekly_correct() {
        use crate::app::{App, FocusedPanel};
        use crate::config::Config;

        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        app.view_mode = crate::app::ViewMode::Weekly;
        app.focused_panel = FocusedPanel::Calendar;

        let rect = Rect::new(0, 0, 40, 10);
        // Inner: x=1..38, y=1..8
        // Day rows: y=2..8 (inner.y + 1 + i)
        // Default config: Sunday start. week_start_date(June 15, Sunday) = June 14.
        // Row 0: June 14 (y=2), Row 1: June 15 (y=3), Row 3: June 17 (y=5)
        let date = hit_test_weekly(5, 3, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));

        let date = hit_test_weekly(5, 5, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 17).unwrap()));
    }

    #[test]
    fn test_hit_test_weekly_outside_returns_none() {
        use crate::app::{App, FocusedPanel};
        use crate::config::Config;

        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        app.view_mode = crate::app::ViewMode::Weekly;
        app.focused_panel = FocusedPanel::Calendar;

        let rect = Rect::new(0, 0, 40, 10);
        assert_eq!(hit_test_weekly(0, 0, rect, &app), None);
        assert_eq!(hit_test_weekly(100, 100, rect, &app), None);
    }
}

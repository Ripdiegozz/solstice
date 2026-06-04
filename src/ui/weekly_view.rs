use chrono::NaiveDate;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::HashMap;

use crate::app::App;
use crate::config::Config;
use crate::events::Event;

/// Hit-test a weekly grid view: return (date, Option<hour>).
/// Header clicks return hour=None; grid cells return hour=Some(hour).
pub fn hit_test_weekly(x: u16, y: u16, rect: Rect, app: &App) -> Option<(NaiveDate, Option<u8>)> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(rect);

    if inner.height < 3 || inner.width < 32 {
        return None;
    }

    let label_width: u16 = 6;
    let col_width = (inner.width.saturating_sub(label_width)) / 7;
    if col_width == 0 {
        return None;
    }

    // Check if click is within the inner area
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }

    let ws = crate::calendar::grid::week_start_date(app.selected_date, app.config.first_day_of_week);

    // Determine day column
    let rel_x = x.saturating_sub(inner.x).saturating_sub(label_width);
    let day_col = (rel_x / col_width).min(6);
    let date = ws + chrono::Duration::days(day_col as i64);

    // Header row?
    if y == inner.y {
        return Some((date, None));
    }

    // Grid rows: inner.y + 1 is first hour row (06:00)
    let row = y.saturating_sub(inner.y).saturating_sub(1);
    let hour = (6 + row as u8).clamp(6, 22);

    Some((date, Some(hour)))
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
    pub selected_hour: u8,
}

impl<'a> WeeklyView<'a> {
    pub fn new(
        week_start: NaiveDate,
        selected_date: NaiveDate,
        today: NaiveDate,
        holidays: &'a HashMap<NaiveDate, String>,
        week_events: &'a HashMap<NaiveDate, Vec<Event>>,
        config: &'a Config,
        selected_hour: u8,
    ) -> Self {
        Self { week_start, selected_date, today, holidays, week_events, config, focused: false, selected_hour }
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for WeeklyView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" Week of {} ", self.week_start.format("%b %-d")))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { Color::Magenta } else { Color::DarkGray }));

        let inner = block.inner(area);
        block.render(area, buf);

        // Minimum size check
        if inner.height < 3 || inner.width < 32 {
            return;
        }

        let label_width: u16 = 6;
        let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let day_start = match self.config.first_day_of_week {
            crate::config::FirstDayOfWeek::Sunday => 6,
            crate::config::FirstDayOfWeek::Monday => 0,
        };

        let col_width = (inner.width.saturating_sub(label_width)) / 7;
        if col_width == 0 {
            return;
        }

        // ── Header row: day names + all-day events ──
        let header_y = inner.y;
        for col in 0..7 {
            let date = self.week_start + chrono::Duration::days(col as i64);
            let day_idx = ((col as usize) + day_start) % 7;
            let day_name = day_names[day_idx];

            let col_x = inner.x + label_width + col as u16 * col_width;

            // Day name
            let is_today = date == self.today;
            let header_style = if is_today {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(col_x, header_y, day_name, header_style);

            // All-day events for this day
            let events = self.week_events.get(&date).map(|v| v.as_slice()).unwrap_or(&[]);
            let all_day: Vec<&Event> = events.iter().filter(|e| e.start_time.is_none()).collect();

            if !all_day.is_empty() {
                let max_title_len = col_width.saturating_sub(1) as usize;
                let first_title = &all_day[0].title;
                let title = crate::utils::truncate_str(first_title, max_title_len);

                let event_x = col_x;
                let remaining = max_title_len;
                // Render first title (truncated)
                if remaining > 0 {
                    buf.set_string(event_x, header_y, &title, Style::default().fg(Color::Green));
                }
                // Overflow indicator if more than 1
                if all_day.len() > 1 && max_title_len >= 4 {
                    let overflow = format!("+{}", all_day.len() - 1);
                    let ox = event_x + title.chars().count() as u16 + 1;
                    if ox < col_x + col_width {
                        buf.set_string(ox, header_y, &overflow, Style::default().fg(Color::Green));
                    }
                }
            }
        }

        // ── Hour rows: 06:00 through 22:00 (16 rows) ──
        for i in 0..16 {
            let hour = 6 + i as u8;
            let row_y = inner.y + 1 + i as u16;
            if row_y >= inner.y + inner.height {
                break;
            }

            // Hour label
            let label = format!("{:02}:00 ", hour);
            buf.set_string(inner.x, row_y, &label, Style::default().fg(Color::DarkGray));

            // For each day column
            for col in 0..7 {
                let date = self.week_start + chrono::Duration::days(col as i64);
                let col_x = inner.x + label_width + col as u16 * col_width;

                let is_selected = date == self.selected_date && hour == self.selected_hour;

                // Get events for this date
                let events = self.week_events.get(&date).map(|v| v.as_slice()).unwrap_or(&[]);

                // Collect events that start at this hour and multi-hour continuations
                let mut starts_here: Vec<&Event> = Vec::new();
                let mut continues_here: Vec<&Event> = Vec::new();

                for event in events.iter().filter(|e| e.start_time.is_some()) {
                    let start_time = event.start_time.as_deref().unwrap();
                    if let Some((sh, _)) = crate::events::parse_hhmm(start_time) {
                        // Start hour (floor :30 → hour)
                        if sh == hour {
                            starts_here.push(event);
                            continue;
                        }
                        // Check if this is a continuation row
                        if let Some(end_time) = event.end_time.as_deref() {
                            if let Some((eh, _)) = crate::events::parse_hhmm(end_time) {
                                let end_hour = if eh == 0 { 24 } else { eh };
                                if sh < hour && hour < end_hour {
                                    continues_here.push(event);
                                }
                            }
                        }
                    }
                }

                // Render cell style
                let cell_style = if is_selected && self.focused {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };

                let max_len = col_width.saturating_sub(1) as usize;

                if !starts_here.is_empty() {
                    let first_title = &starts_here[0].title;
                    let title = crate::utils::truncate_str(first_title, max_len);
                    buf.set_string(col_x, row_y, &title, cell_style.fg(Color::Cyan));

                    if starts_here.len() > 1 && max_len >= 4 {
                        let overflow = format!("+{}", starts_here.len() - 1);
                        let ox = col_x + title.chars().count() as u16 + 1;
                        if ox < col_x + col_width {
                            buf.set_string(ox, row_y, &overflow, cell_style.fg(Color::Cyan));
                        }
                    }
                } else if !continues_here.is_empty() {
                    buf.set_string(col_x, row_y, "│", cell_style.fg(Color::Cyan));
                } else if is_selected && self.focused {
                    // Empty selected cell: fill with a space with REVERSED to make highlight visible
                    buf.set_string(col_x, row_y, " ", cell_style);
                }
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
    use ratatui::style::Modifier;

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
        let mut config = Config::default();
        config.first_day_of_week = FirstDayOfWeek::Monday;
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let selected = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // inner.x=1, inner.y=1. Day names in header row y=1, col starts at x=7
        // With Monday start, Monday is col 0 at x=7
        assert!(buffer_contains(&buf, 7, 15, 1, 2, "Mon"), "Should show Monday in header");
        assert!(buffer_contains(&buf, 17, 25, 1, 2, "Tue"), "Should show Tuesday");
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

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // 09:00 is row idx 3 → y = inner.y + 1 + 3 = 5, col starts at 7
        assert!(buffer_contains(&buf, 7, 60, 5, 6, "Standup"), "Standup should appear at 09:00 slot");
        // 12:00 is row idx 6 → y = 8
        assert!(buffer_contains(&buf, 7, 60, 8, 9, "Lunch"), "Lunch should appear at 12:00 slot");
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
        // All all-day events (no start_time) — overflow in header row
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

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // All-day events in header row, overflow for 5 events
        assert!(buffer_contains(&buf, 7, 60, 1, 2, "+4"), "Should show '+4' for 5 all-day events");
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

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // First day col in header (x=7) should be "Sun"
        assert!(buffer_contains(&buf, 7, 15, 1, 2, "Sun"), "First col should be Sun");
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

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // First day col in header (x=7) should be "Mon"
        assert!(buffer_contains(&buf, 7, 15, 1, 2, "Mon"), "First col should be Mon");
    }

    // ── hit_test_weekly tests (new signature) ──

    #[test]
    fn test_hit_test_weekly_grid_cell_returns_date_and_hour() {
        use crate::app::{App, FocusedPanel};
        use crate::config::Config;

        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.view_mode = crate::app::ViewMode::Weekly;
        app.focused_panel = FocusedPanel::Calendar;

        // Week start: Sunday June 14. First day col at x = inner.x + 6 = 7
        // 09:00 row = inner.y + 1 + 3 = 5
        let rect = Rect::new(0, 0, 80, 24);
        // Click on Monday June 15, 09:00 cell
        let result = hit_test_weekly(17, 5, rect, &app);
        let expected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(result, Some((expected_date, Some(9))));
    }

    #[test]
    fn test_hit_test_weekly_header_returns_date_only() {
        use crate::app::{App, FocusedPanel};
        use crate::config::Config;

        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.view_mode = crate::app::ViewMode::Weekly;
        app.focused_panel = FocusedPanel::Calendar;

        let rect = Rect::new(0, 0, 80, 24);
        // Header row at inner.y = 1. Click on Monday column
        let result = hit_test_weekly(17, 1, rect, &app);
        let expected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(result, Some((expected_date, None)));
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

        let rect = Rect::new(0, 0, 80, 24);
        assert_eq!(hit_test_weekly(0, 0, rect, &app), None);
        assert_eq!(hit_test_weekly(200, 200, rect, &app), None);
    }

    #[test]
    fn test_hit_test_weekly_clamps_hour_at_bounds() {
        use crate::app::{App, FocusedPanel};
        use crate::config::Config;

        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        app.view_mode = crate::app::ViewMode::Weekly;
        app.focused_panel = FocusedPanel::Calendar;

        let rect = Rect::new(0, 0, 80, 24);
        // First hour row at inner.y + 1 = 2 → hour = 6
        let result = hit_test_weekly(10, 2, rect, &app);
        assert!(result.is_some());
        assert_eq!(result.unwrap().1, Some(6));

        // Last hour row at inner.y + 1 + 15 = 17 → hour = 21
        let result = hit_test_weekly(10, 17, rect, &app);
        assert!(result.is_some());
        assert_eq!(result.unwrap().1, Some(21));
    }

    #[test]
    fn test_weekly_view_stores_selected_hour() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(
            week_start, selected, today,
            &holidays, &week_events, &config, 9,
        );
        assert_eq!(view.selected_hour, 9);
    }

    // ── new grid render tests ──

    /// Helper: get cell style from buffer at (x, y)
    fn cell_style(buf: &Buffer, x: u16, y: u16) -> Option<Style> {
        buf.cell((x, y)).map(|c| c.style())
    }

    /// Helper: collect a horizontal string from the buffer
    fn row_string(buf: &Buffer, x_start: u16, x_end: u16, y: u16) -> String {
        (x_start..x_end)
            .map(|x| buf.cell((x, y)).map_or(' ', |c| c.symbol().chars().next().unwrap_or(' ')))
            .collect()
    }

    #[test]
    fn test_grid_renders_hour_labels() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = week_start;
        let today = week_start;
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // inner.x = 1, inner.y = 1. Label column at x=1, width 6
        // Hour rows start at inner.y + 1 = y=2
        let label = row_string(&buf, 1, 7, 3); // row 3 = 07:00 (06+i where i=1 => 07)
        assert!(label.contains("07:00"), "Expected 07:00 label, got: '{}'", label);

        let label = row_string(&buf, 1, 7, 10); // row 10 = 14:00
        assert!(label.contains("14:00"), "Expected 14:00 label, got: '{}'", label);
    }

    #[test]
    fn test_grid_renders_day_columns() {
        let config = Config::default();
        // Monday June 15, 2026
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = week_start;
        let today = week_start;
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // inner.x = 1, inner.y = 1. Label col = 6 chars at x=1..7.
        // col_width = (inner.width - 6) / 7 = (78 - 6) / 7 = 10
        // Day cols start at x = 1 + 6 = 7
        // Header row = inner.y = y=1
        // Day names appear in the header row
        let header = row_string(&buf, 7, 80, 1);
        assert!(header.contains("Mon"), "Header should contain Mon, got: '{}'", header);
        assert!(header.contains("Tue"), "Header should contain Tue, got: '{}'", header);
    }

    #[test]
    fn test_grid_places_timed_event_in_correct_hour_slot() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let mon = week_start;
        let selected = mon;
        let today = mon;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
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
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // 09:00 is hour row index 3 (06,07,08,09). Row y = inner.y + 1 + 3 = inner.y + 4 = 5
        // Col starts at inner.x + 6 = 7. Monday is col 0.
        let row = row_string(&buf, 7, 60, 5);
        assert!(row.contains("Standup"), "Expected 'Standup' in 09:00 row, got: '{}'", row);
    }

    #[test]
    fn test_grid_all_day_event_in_header() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let mon = week_start;
        let selected = mon;
        let today = mon;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        week_events.insert(mon, vec![
            Event {
                id: 1,
                title: "All-Day Conference".to_string(),
                description: None,
                date: mon,
                start_time: None,
                end_time: None,
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Header row is at inner.y = y=1. Day col for Monday is at x=7
        let header = row_string(&buf, 7, 60, 1);
        assert!(header.contains("All-Day"), "Header should contain all-day event, got: '{}'", header);
    }

    #[test]
    fn test_grid_multi_hour_event_shows_pipe_in_continuation() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let mon = week_start;
        let selected = mon;
        let today = mon;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        week_events.insert(mon, vec![
            Event {
                id: 1,
                title: "Workshop".to_string(),
                description: None,
                date: mon,
                start_time: Some("10:00".to_string()),
                end_time: Some("12:00".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // 10:00 is row index 4, y = inner.y + 1 + 4 = 6. Should have "Workshop"
        let row_10 = row_string(&buf, 7, 60, 6);
        assert!(row_10.contains("Workshop"), "10:00 row should contain Workshop, got: '{}'", row_10);

        // 11:00 is row index 5, y = 7. Should have "│" pipe
        let row_11 = row_string(&buf, 7, 60, 7);
        assert!(row_11.contains('│'), "11:00 continuation should contain │, got: '{}'", row_11);
    }

    #[test]
    fn test_grid_overflow_shows_plus_n() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let mon = week_start;
        let selected = mon;
        let today = mon;
        let holidays = HashMap::new();

        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        week_events.insert(mon, vec![
            Event {
                id: 1, title: "Meeting A".to_string(),
                description: None, date: mon,
                start_time: Some("10:00".to_string()), end_time: Some("11:00".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
            Event {
                id: 2, title: "Meeting B".to_string(),
                description: None, date: mon,
                start_time: Some("10:30".to_string()), end_time: Some("11:00".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Both events start at hour 10. Row y = inner.y + 1 + 4 = 6
        let row_10 = row_string(&buf, 7, 60, 6);
        assert!(row_10.contains("+1"), "Overflow should show +1, got: '{}'", row_10);
    }

    #[test]
    fn test_grid_selected_cell_has_reversed_modifier() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Monday
        let mon = week_start;
        // selected_date = Monday, selected_hour = 9
        let selected = mon;
        let today = mon;
        let holidays = HashMap::new();
        let mut week_events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        week_events.insert(mon, vec![
            Event {
                id: 1, title: "Standup".to_string(),
                description: None, date: mon,
                start_time: Some("09:00".to_string()), end_time: Some("09:30".to_string()),
                source: crate::events::EventSource::Local,
                recurrence: crate::events::Recurrence::None,
            },
        ]);

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9)
            .with_focused(true);
        let area = Rect::new(0, 0, 100, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // 09:00 = row idx 3, y = inner.y + 1 + 3 = 5. Mon col starts at x=7.
        // The cell content should have REVERSED modifier
        let style = cell_style(&buf, 7, 5);
        assert!(style.is_some(), "Cell at (7,5) should exist");
        assert!(style.unwrap().add_modifier(Modifier::REVERSED) == style.unwrap(),
                "Selected cell should have REVERSED modifier");
    }

    #[test]
    fn test_grid_short_terminal_still_renders() {
        let config = Config::default();
        let week_start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let selected = week_start;
        let today = week_start;
        let holidays = HashMap::new();
        let week_events = HashMap::new();

        let view = WeeklyView::new(week_start, selected, today, &holidays, &week_events, &config, 9);
        // Narrow terminal (50 cols)
        let area = Rect::new(0, 0, 50, 24);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Should still have hour labels
        let label = row_string(&buf, 1, 7, 3);
        assert!(label.contains("07:00"), "Narrow terminal should still show hour labels");
    }
}

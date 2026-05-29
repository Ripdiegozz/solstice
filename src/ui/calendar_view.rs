use chrono::{Datelike, NaiveDate};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::{HashMap, HashSet};

use crate::app::App;
use crate::calendar::grid::CalendarGrid;
use crate::config::Config;
use crate::ui::clock::parse_color;

/// Hit-test a monthly calendar view: return the date under (x, y) if any
pub fn hit_test_monthly(x: u16, y: u16, rect: Rect, app: &App) -> Option<NaiveDate> {
    let grid = CalendarGrid::new(app.view_year, app.view_month, app.config.first_day_of_week);
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(rect);

    if inner.height < 2 || inner.width < 20 {
        return None;
    }
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }

    // Header row is at inner.y
    if y == inner.y {
        return None; // weekday header
    }

    let col_width = inner.width / 7;
    let col = ((x - inner.x) / col_width) as usize;
    if col >= 7 {
        return None;
    }

    // Week rows start at inner.y + 2 (header + blank row)
    let week_row = if y >= inner.y + 2 {
        (y - (inner.y + 2)) as usize
    } else {
        return None;
    };

    let weeks = grid.generate_cells(app.today, &app.holidays, &app.event_dates);
    let week = weeks.get(week_row)?;
    let cell = week.get(col)?;
    Some(cell.date)
}

/// Calendar grid widget showing a monthly view
pub struct CalendarView<'a> {
    pub year: i32,
    pub month: u32,
    pub today: NaiveDate,
    pub selected_date: NaiveDate,
    pub holidays: &'a HashMap<NaiveDate, String>,
    pub event_dates: &'a HashSet<NaiveDate>,
    pub config: &'a Config,
    pub focused: bool,
}

impl<'a> CalendarView<'a> {
    pub fn new(
        year: i32,
        month: u32,
        today: NaiveDate,
        selected_date: NaiveDate,
        holidays: &'a HashMap<NaiveDate, String>,
        event_dates: &'a HashSet<NaiveDate>,
        config: &'a Config,
    ) -> Self {
        Self { year, month, today, selected_date, holidays, event_dates, config, focused: false }
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for CalendarView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let grid = CalendarGrid::new(self.year, self.month, self.config.first_day_of_week);
        let weeks = grid.generate_cells(self.today, self.holidays, self.event_dates);
        let headers = grid.weekday_headers();

        let text_color = parse_color(&self.config.theme.text);
        let muted = parse_color(&self.config.theme.muted);
        let accent = parse_color(&self.config.theme.accent);
        let today_color = parse_color(&self.config.theme.today);
        let holiday_color = parse_color(&self.config.theme.holiday);
        let event_color = parse_color(&self.config.theme.event);
        let surface = parse_color(&self.config.theme.surface);

        let month_name = chrono::NaiveDate::from_ymd_opt(self.year, self.month, 1)
            .unwrap()
            .format("%B %Y")
            .to_string();

        let block = Block::default()
            .title(format!(" {} ", month_name))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { accent } else { surface }));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 20 {
            return;
        }

        // Render weekday headers
        let col_width = inner.width / 7;
        for (i, header) in headers.iter().enumerate() {
            let x = inner.x + (i as u16 * col_width);
            let style = Style::default().fg(muted).add_modifier(Modifier::BOLD);
            buf.set_string(x, inner.y, header, style);
        }

        // Render day cells
        for (week_idx, week) in weeks.iter().enumerate() {
            let y = inner.y + 2 + week_idx as u16;
            if y >= inner.y + inner.height {
                break;
            }

            for (day_idx, cell) in week.iter().enumerate() {
                let x = inner.x + (day_idx as u16 * col_width);
                if x + 3 > inner.x + inner.width {
                    break;
                }

                let day_str = format!("{:>2}", cell.date.day());

                let style = if cell.is_today {
                    Style::default()
                        .fg(today_color)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else if cell.date == self.selected_date {
                    Style::default()
                        .fg(accent)
                        .add_modifier(Modifier::BOLD)
                } else if !cell.is_current_month {
                    Style::default().fg(muted)
                } else if cell.is_holiday {
                    Style::default().fg(holiday_color)
                } else {
                    Style::default().fg(text_color)
                };

                buf.set_string(x, y, &day_str, style);

                // Event dot indicator
                if cell.has_events {
                    let dot_x = x + 3;
                    if dot_x < inner.x + inner.width {
                        buf.set_string(dot_x, y, "•", Style::default().fg(event_color));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use chrono::NaiveDate;

    fn make_test_app() -> App {
        let mut app = App::new(Config::default()).unwrap();
        app.view_year = 2026;
        app.view_month = 6;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.today = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app
    }

    #[test]
    fn test_hit_test_monthly_correct() {
        let app = make_test_app();
        // Default config: Sunday start. June 1 2026 is Monday → offset 1.
        // Rect: x=0, y=0, width=40, height=20
        // Block borders at edges, inner: x=1..38, y=1..18
        // Header at y=1, weeks start at y=3 (inner.y + 2)
        // col_width = 37 / 7 = 5
        // Day 1 (June 1): col=1, row=0 → x=6, y=3
        let rect = Rect::new(0, 0, 40, 20);
        let date = hit_test_monthly(6, 3, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));

        // Day 15: col = ((15-1) + 1) % 7 = 1, row = 2 → x=6, y=5
        let date = hit_test_monthly(6, 5, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
    }

    #[test]
    fn test_hit_test_monthly_outside_returns_none() {
        let app = make_test_app();
        let rect = Rect::new(0, 0, 40, 20);
        assert_eq!(hit_test_monthly(0, 0, rect, &app), None); // border
        assert_eq!(hit_test_monthly(100, 100, rect, &app), None); // far outside
    }
}

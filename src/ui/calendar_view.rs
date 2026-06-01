use chrono::{Datelike, NaiveDate};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::HashMap;

use crate::app::App;
use crate::calendar::grid::CalendarGrid;
use crate::config::Config;

/// Height of each calendar cell in rows (2 lines: day number + event count)
pub(crate) const CELL_HEIGHT: u16 = 2;

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

    // Week rows start at inner.y + 2 (header + blank row), each CELL_HEIGHT rows
    let week_row = if y >= inner.y + 2 {
        ((y - (inner.y + 2)) / CELL_HEIGHT) as usize
    } else {
        return None;
    };

    let weeks = grid.generate_cells(app.today, &app.holidays, &app.event_counts);
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
    pub event_counts: &'a HashMap<NaiveDate, usize>,
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
        event_counts: &'a HashMap<NaiveDate, usize>,
        config: &'a Config,
    ) -> Self {
        Self { year, month, today, selected_date, holidays, event_counts, config, focused: false }
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl<'a> Widget for CalendarView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let grid = CalendarGrid::new(self.year, self.month, self.config.first_day_of_week);
        let weeks = grid.generate_cells(self.today, self.holidays, self.event_counts);
        let headers = grid.weekday_headers();

        let block = Block::default()
            .title(" [1]-Calendar ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.focused { Color::Magenta } else { Color::DarkGray }));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 20 {
            return;
        }

        // Render weekday headers
        let col_width = inner.width / 7;
        for (i, header) in headers.iter().enumerate() {
            let x = inner.x + (i as u16 * col_width);
            let style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD);
            buf.set_string(x, inner.y, header, style);
        }

        // Render day cells (2-line: day number + event count)
        for (week_idx, week) in weeks.iter().enumerate() {
            let week_y = inner.y + 2 + (week_idx as u16 * CELL_HEIGHT);
            if week_y >= inner.y + inner.height {
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
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else if cell.date == self.selected_date {
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD)
                } else if !cell.is_current_month {
                    Style::default().fg(Color::DarkGray)
                } else if cell.is_holiday {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };

                // Line 1: day number
                buf.set_string(x, week_y, &day_str, style);

                // Line 2: event count (green) when col_width >= 2 and count > 0
                if col_width >= 2 && cell.event_count > 0 {
                    let line2_y = week_y + 1;
                    if line2_y < inner.y + inner.height {
                        let count_str = format!("{}", cell.event_count);
                        buf.set_string(x, line2_y, &count_str, Style::default().fg(Color::Green));
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
        // Header at y=1, weeks start at y=3 (inner.y + 2), CELL_HEIGHT=2
        // col_width = 37 / 7 = 5
        // Day 1 (June 1): col=1, row=0 → x=6, line1_y=3
        let rect = Rect::new(0, 0, 40, 20);
        let date = hit_test_monthly(6, 3, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));

        // Day 1 line 2 (same column, y=4) should return same date
        let date2 = hit_test_monthly(6, 4, rect, &app);
        assert_eq!(date2, Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));

        // Day 15: col=1, row=2 → x=6, week_y = 3 + 2*2 = 7
        let date = hit_test_monthly(6, 7, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));

        // Day 15 line 2 (y=8)
        let date = hit_test_monthly(6, 8, rect, &app);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
    }

    #[test]
    fn test_hit_test_monthly_outside_returns_none() {
        let app = make_test_app();
        let rect = Rect::new(0, 0, 40, 20);
        assert_eq!(hit_test_monthly(0, 0, rect, &app), None); // border
        assert_eq!(hit_test_monthly(100, 100, rect, &app), None); // far outside
    }

    // ── rendering tests (tasks 4.4, 4.6) ──

    fn make_test_event_counts() -> HashMap<NaiveDate, usize> {
        let mut counts = HashMap::new();
        counts.insert(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), 3);
        counts.insert(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(), 12);
        counts
    }

    #[test]
    fn test_render_shows_event_count_in_green() {
        let config = Config::default();
        let holidays = HashMap::new();
        let event_counts = make_test_event_counts();

        let view = CalendarView::new(
            2026, 6,
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            &holidays,
            &event_counts,
            &config,
        );

        // Wide enough for 2-line cells (col_width = 37/7 = 5 >= 2)
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // Verify count "3" appears on line 2 of June 1 cell
        // June 1 (Monday, col=1 with Sunday start), header at y=1, week 0 starts at y=3
        // line1_y=3, line2_y=4, x=inner.x + 1*5 = 1+5=6
        let cell_content_3 = buf.cell((6, 4)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_content_3, "3", "June 1 line 2 should show '3'");

        // Verify count "12" appears on line 2 of June 15 cell
        // June 15 (Monday, col=1 with Sunday start), week 2 starts at y=3+4=7
        // line1_y=7, line2_y=8, x=6
        let cell_content_12 = buf.cell((6, 8)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_content_12, "1", "June 15 line 2 should show '12' (first char)");
        // "2" should be at x=7
        let cell_12_digit2 = buf.cell((7, 8)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_12_digit2, "2", "June 15 line 2 second char should be '2'");
    }

    #[test]
    fn test_render_day_without_events_is_blank_on_line2() {
        let config = Config::default();
        let holidays = HashMap::new();
        let event_counts = make_test_event_counts(); // only Jun 1 and Jun 15 have events

        let view = CalendarView::new(
            2026, 6,
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            &holidays,
            &event_counts,
            &config,
        );

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // June 8 (Monday, col=1 with Sunday start), week 1 starts at y=3+2=5
        // line1_y=5, line2_y=6, x=6  — should be blank (0 events)
        let cell_line2 = buf.cell((6, 6)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_line2, " ", "June 8 line 2 should be blank (0 events)");
    }

    #[test]
    fn test_render_skips_count_line_when_narrow() {
        let config = Config::default();
        let holidays = HashMap::new();
        let event_counts = make_test_event_counts();

        let view = CalendarView::new(
            2026, 6,
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            &holidays,
            &event_counts,
            &config,
        );

        // Narrow area: width=10, inner width=8, col_width=1 (< 2)
        let area = Rect::new(0, 0, 10, 20);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // col_width < 2, so count line should never render
        // Scan all cells in the buffer for any non-space content on line 2 positions
        // (week rows y >= inner.y+2, and (y - (inner.y+2)) % 2 == 1 means line 2)
        let inner = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .inner(area);
        let has_count_on_line2 = (inner.y + 2..inner.y + inner.height).any(|y| {
            if (y - (inner.y + 2)) % CELL_HEIGHT != 1 {
                return false; // line 1, not line 2
            }
            (inner.x..inner.x + inner.width).any(|x| {
                buf.cell((x, y)).map(|c| c.symbol() != " ").unwrap_or(false)
            })
        });
        assert!(!has_count_on_line2, "Narrow column should skip all line 2 content");
    }
}

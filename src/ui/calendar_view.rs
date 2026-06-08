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

/// Height of each calendar cell in rows (2 lines: day number + optional indicators)
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

                // Line 1: day number + optional event indicator on the same line
                buf.set_string(x, week_y, &day_str, style);

                let ind_x = x + day_str.len() as u16;

                // Event indicator (green dot)
                if cell.event_count > 0 && ind_x < inner.x + inner.width {
                    buf.set_string(ind_x, week_y, "\u{2022}", Style::default().fg(Color::Green));
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

    fn make_test_holidays() -> HashMap<NaiveDate, String> {
        let mut holidays = HashMap::new();
        holidays.insert(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), "Some Holiday".into());
        holidays.insert(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(), "Another Holiday".into());
        holidays
    }

    #[test]
    fn test_render_holiday_day_is_red() {
        let config = Config::default();
        let holidays = make_test_holidays();
        let event_counts = HashMap::new();

        // Use June 2 as selected/today so June 15 is not "today" (today gets Yellow priority)
        let view = CalendarView::new(
            2026, 6,
            NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            &holidays,
            &event_counts,
            &config,
        );

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // June 15 is a holiday (and not today) — verify the day number is rendered in red
        // June 15 (Monday, col=1 with Sunday start), week 2 starts at y=3+4=7
        // day_str = "15" (2 chars), x=6
        let cell_fg_15 = buf.cell((7, 7)).map(|c| c.fg);
        assert_eq!(cell_fg_15, Some(Color::Red), "June 15 day number should be red for holiday");

        // Verify no extra indicator after the day number
        let after_day = buf.cell((8, 7)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(after_day, " ", "Holiday should not show extra indicator, just red color");
    }

    #[test]
    fn test_render_shows_event_indicator() {
        let config = Config::default();
        let holidays = HashMap::new();
        let mut event_counts = HashMap::new();
        event_counts.insert(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), 3);

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

        // June 1: day_str = " 1" (2 chars), x=6, indicator at x=8, y=3
        let cell_content = buf.cell((8, 3)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_content, "\u{2022}", "June 1 should show event indicator after day number");
    }

    #[test]
    fn test_render_holiday_and_event_shows_red_day_with_green_dot() {
        let config = Config::default();
        let holidays = make_test_holidays(); // Jun 15 is a holiday
        let mut event_counts = HashMap::new();
        event_counts.insert(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(), 3);

        // Use June 2 as selected/today so June 15 is not "today" (today gets Yellow priority)
        let view = CalendarView::new(
            2026, 6,
            NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            &holidays,
            &event_counts,
            &config,
        );

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        // June 15: day_str = "15" (2 chars), x=6
        // Day number should be in red (holiday, and not today)
        let day_fg = buf.cell((7, 7)).map(|c| c.fg);
        assert_eq!(day_fg, Some(Color::Red), "June 15 day number should be red for holiday");

        // Only one indicator: green dot for event at x=8
        let event_indicator = buf.cell((8, 7)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(event_indicator, "\u{2022}", "June 15 should show green event indicator");

        // x=9 should be blank (no second indicator)
        let after_indicator = buf.cell((9, 7)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(after_indicator, " ", "No second indicator after event dot");
    }

    #[test]
    fn test_render_line2_is_always_blank() {
        let config = Config::default();
        let holidays = make_test_holidays(); // only Jun 1 and Jun 15 are holidays
        let event_counts = HashMap::new();

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
        // line2_y=6, x=6  — line 2 should always be blank now
        let cell_line2 = buf.cell((6, 6)).map(|c| c.symbol().to_string()).unwrap_or_default();
        assert_eq!(cell_line2, " ", "Line 2 should be blank since indicators moved to line 1");
    }

    #[test]
    fn test_render_line2_is_blank_when_narrow() {
        let config = Config::default();
        let holidays = make_test_holidays();
        let event_counts = HashMap::new();

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

        // Scan all cells in the buffer for any non-space content on line 2 positions
        // (week rows y >= inner.y+2, and (y - (inner.y+2)) % 2 == 1 means line 2)
        let inner = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .inner(area);
        let has_content_on_line2 = (inner.y + 2..inner.y + inner.height).any(|y| {
            if (y - (inner.y + 2)) % CELL_HEIGHT != 1 {
                return false; // line 1, not line 2
            }
            (inner.x..inner.x + inner.width).any(|x| {
                buf.cell((x, y)).map(|c| c.symbol() != " ").unwrap_or(false)
            })
        });
        assert!(!has_content_on_line2, "Line 2 should be blank even when narrow");
    }
}

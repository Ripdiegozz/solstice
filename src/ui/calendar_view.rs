use chrono::NaiveDate;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::{HashMap, HashSet};

use crate::calendar::grid::CalendarGrid;
use crate::config::Config;
use crate::ui::clock::parse_color;

/// Calendar grid widget showing a monthly view
pub struct CalendarView<'a> {
    pub year: i32,
    pub month: u32,
    pub today: NaiveDate,
    pub selected_date: NaiveDate,
    pub holidays: &'a HashMap<NaiveDate, String>,
    pub event_dates: &'a HashSet<NaiveDate>,
    pub config: &'a Config,
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
        Self { year, month, today, selected_date, holidays, event_dates, config }
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
            .border_style(Style::default().fg(surface));

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

use chrono::Datelike;

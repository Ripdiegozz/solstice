use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use crate::config::FirstDayOfWeek;

/// Represents a single cell in the calendar grid
#[derive(Debug, Clone)]
pub struct DayCell {
    pub date: NaiveDate,
    pub is_current_month: bool,
    pub is_today: bool,
    pub is_holiday: bool,
    pub holiday_name: Option<String>,
    pub event_count: usize,
}

/// Calendar grid logic for a monthly view
pub struct CalendarGrid {
    pub year: i32,
    pub month: u32,
    pub first_day: FirstDayOfWeek,
}

impl CalendarGrid {
    pub fn new(year: i32, month: u32, first_day: FirstDayOfWeek) -> Self {
        Self { year, month, first_day }
    }

    /// Get the weekday offset for the first day of the month
    fn start_offset(&self) -> usize {
        let first = NaiveDate::from_ymd_opt(self.year, self.month, 1).unwrap();
        let weekday_num = match self.first_day {
            FirstDayOfWeek::Sunday => first.weekday().num_days_from_sunday(),
            FirstDayOfWeek::Monday => first.weekday().num_days_from_monday(),
        };
        weekday_num as usize
    }

    /// Get weekday headers in order
    pub fn weekday_headers(&self) -> Vec<&'static str> {
        match self.first_day {
            FirstDayOfWeek::Sunday => {
                vec!["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
            }
            FirstDayOfWeek::Monday => {
                vec!["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
            }
        }
    }

    /// Generate all day cells for the monthly grid (including padding days)
    pub fn generate_cells(
        &self,
        today: NaiveDate,
        holidays: &HashMap<NaiveDate, String>,
        event_counts: &HashMap<NaiveDate, usize>,
    ) -> Vec<Vec<DayCell>> {
        let num_days = days_in_month(self.year, self.month);
        let offset = self.start_offset();

        // Previous month padding
        let prev_month = if self.month == 1 { 12 } else { self.month - 1 };
        let prev_year = if self.month == 1 { self.year - 1 } else { self.year };
        let prev_days = days_in_month(prev_year, prev_month);

        let mut all_days: Vec<DayCell> = Vec::new();

        // Fill in previous month's trailing days
        for i in 0..offset {
            let day = prev_days - offset + i + 1;
            let date = NaiveDate::from_ymd_opt(prev_year, prev_month, day as u32).unwrap();
            all_days.push(DayCell {
                date,
                is_current_month: false,
                is_today: false,
                is_holiday: false,
                holiday_name: None,
                event_count: 0,
            });
        }

        // Current month days
        for day in 1..=num_days {
            let date = NaiveDate::from_ymd_opt(self.year, self.month, day as u32).unwrap();
            let is_today = date == today;
            let holiday_name = holidays.get(&date).cloned();
            let is_holiday = holiday_name.is_some();
            let event_count = event_counts.get(&date).copied().unwrap_or(0);

            all_days.push(DayCell {
                date,
                is_current_month: true,
                is_today,
                is_holiday,
                holiday_name,
                event_count,
            });
        }

        // Next month padding to fill complete weeks
        let next_month = if self.month == 12 { 1 } else { self.month + 1 };
        let next_year = if self.month == 12 { self.year + 1 } else { self.year };
        let remaining = 7 - (all_days.len() % 7);
        if remaining < 7 {
            for day in 1..=remaining {
                let date = NaiveDate::from_ymd_opt(next_year, next_month, day as u32).unwrap();
                all_days.push(DayCell {
                    date,
                    is_current_month: false,
                    is_today: false,
                    is_holiday: false,
                    holiday_name: None,
                    event_count: 0,
                });
            }
        }

        // Chunk into weeks (rows of 7)
        all_days.chunks(7).map(|chunk| chunk.to_vec()).collect()
    }
}

/// Returns the start date of the week containing `date`,
/// respecting `first_day` configuration (Sunday or Monday).
pub fn week_start_date(date: NaiveDate, first_day: FirstDayOfWeek) -> NaiveDate {
    let weekday_num = match first_day {
        FirstDayOfWeek::Sunday => date.weekday().num_days_from_sunday(),
        FirstDayOfWeek::Monday => date.weekday().num_days_from_monday(),
    };
    date - chrono::Duration::days(weekday_num as i64)
}

/// Returns the number of days in a given month/year
fn days_in_month(year: i32, month: u32) -> usize {
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first_of_month = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (next_month.unwrap() - first_of_month).num_days() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_in_month() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29); // leap year
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn test_weekday_headers_sunday() {
        let grid = CalendarGrid::new(2026, 5, FirstDayOfWeek::Sunday);
        let headers = grid.weekday_headers();
        assert_eq!(headers[0], "Su");
        assert_eq!(headers[6], "Sa");
    }

    #[test]
    fn test_weekday_headers_monday() {
        let grid = CalendarGrid::new(2026, 5, FirstDayOfWeek::Monday);
        let headers = grid.weekday_headers();
        assert_eq!(headers[0], "Mo");
        assert_eq!(headers[6], "Su");
    }

    #[test]
    fn test_week_start_date_sunday_start() {
        // Wednesday Jun 3 2026 → Sunday May 31 2026 (Sunday start)
        let date = NaiveDate::from_ymd_opt(2026, 6, 3).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Sunday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 5, 31).unwrap());
    }

    #[test]
    fn test_week_start_date_monday_start() {
        // Wednesday Jun 3 2026 → Monday Jun 1 2026 (Monday start)
        let date = NaiveDate::from_ymd_opt(2026, 6, 3).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Monday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());
    }

    #[test]
    fn test_week_start_date_monday_start_on_sunday() {
        // Sunday Jun 21 2026 → Monday Jun 15 2026 (Monday start)
        let date = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Monday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
    }

    #[test]
    fn test_week_start_date_already_week_start() {
        // Already a Sunday (Sunday start)
        let date = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Sunday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 5, 31).unwrap());
    }

    #[test]
    fn test_week_start_date_crosses_month() {
        // Jul 1 Wed → Jun 29 Mon (Monday start)
        let date = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Monday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 6, 29).unwrap());
    }

    #[test]
    fn test_week_start_date_crosses_year() {
        // Jan 1 Thu → Dec 29 Mon (Monday start, year boundary)
        let date = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        let start = week_start_date(date, FirstDayOfWeek::Monday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 12, 28).unwrap());
    }

    #[test]
    fn test_generate_cells_has_correct_structure() {
        let grid = CalendarGrid::new(2026, 5, FirstDayOfWeek::Sunday);
        let today = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let holidays = HashMap::new();
        let event_counts = HashMap::new();

        let weeks = grid.generate_cells(today, &holidays, &event_counts);

        // Should have 5-6 weeks
        assert!(weeks.len() >= 4 && weeks.len() <= 6);
        // Each week should have 7 days
        for week in &weeks {
            assert_eq!(week.len(), 7);
        }
        // Today should be marked
        let has_today = weeks.iter().flatten().any(|c| c.is_today);
        assert!(has_today);
    }

    #[test]
    fn test_generate_cells_with_event_counts() {
        let grid = CalendarGrid::new(2026, 6, FirstDayOfWeek::Sunday);
        let today = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let holidays = HashMap::new();

        let mut event_counts = HashMap::new();
        event_counts.insert(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), 3);
        event_counts.insert(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(), 1);

        let weeks = grid.generate_cells(today, &holidays, &event_counts);

        // Find cells with events
        let cells: Vec<&DayCell> = weeks.iter().flatten().collect();
        let jun1 = cells.iter().find(|c| c.date == NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()).unwrap();
        let jun15 = cells.iter().find(|c| c.date == NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()).unwrap();
        let jun10 = cells.iter().find(|c| c.date == NaiveDate::from_ymd_opt(2026, 6, 10).unwrap()).unwrap();

        assert_eq!(jun1.event_count, 3);
        assert_eq!(jun15.event_count, 1);
        assert_eq!(jun10.event_count, 0);
    }
}

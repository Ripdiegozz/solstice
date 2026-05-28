use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, params};
use std::path::PathBuf;

use crate::gcal::GCalEvent;

/// Recurrence frequency for an event
#[derive(Debug, Clone, PartialEq)]
pub enum Recurrence {
    None,
    Daily,
    Weekly,
    Monthly,
}

impl Recurrence {
    /// Convert to the string stored in SQLite
    pub fn to_str(&self) -> &'static str {
        match self {
            Recurrence::None => "none",
            Recurrence::Daily => "daily",
            Recurrence::Weekly => "weekly",
            Recurrence::Monthly => "monthly",
        }
    }

    /// Parse from the string stored in SQLite
    pub fn from_stored(s: &str) -> Self {
        match s {
            "daily" => Recurrence::Daily,
            "weekly" => Recurrence::Weekly,
            "monthly" => Recurrence::Monthly,
            _ => Recurrence::None,
        }
    }
}

/// A local calendar event
#[derive(Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub date: NaiveDate,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub source: EventSource,
    pub recurrence: Recurrence,
}

/// Where the event came from
#[derive(Debug, Clone, PartialEq)]
pub enum EventSource {
    Local,
    GoogleCalendar,
}

impl std::fmt::Display for EventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSource::Local => write!(f, "local"),
            EventSource::GoogleCalendar => write!(f, "gcal"),
        }
    }
}

/// SQLite-backed event store
pub struct EventStore {
    conn: Connection,
}

impl EventStore {
    /// Open or create the events database at ~/.local/share/solstice/events.db
    pub fn open() -> Result<Self> {
        let db_path = Self::db_path();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create data directory {:?}", parent))?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;

        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn db_path() -> PathBuf {
        crate::config::data_dir().join("events.db")
    }

    /// Create tables if they don't exist
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                description TEXT,
                date TEXT NOT NULL,
                start_time TEXT,
                end_time TEXT,
                source TEXT NOT NULL DEFAULT 'local',
                gcal_id TEXT UNIQUE
            );

            CREATE INDEX IF NOT EXISTS idx_events_date ON events(date);
            CREATE INDEX IF NOT EXISTS idx_events_source ON events(source);"
        ).context("Failed to initialize database schema")?;

        // Migration: add recurrence column (idempotent)
        let result = self.conn.execute_batch(
            "ALTER TABLE events ADD COLUMN recurrence TEXT DEFAULT 'none';"
        );
        // Ignore "duplicate column" errors
        if let Err(e) = result {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e).context("Failed to add recurrence column");
            }
        }

        Ok(())
    }

    /// Get all events for a specific date (including recurring event expansion)
    pub fn events_for_date(&self, date: NaiveDate) -> Result<Vec<Event>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, title, description, date, start_time, end_time, source, recurrence
             FROM events WHERE date <= ?1 ORDER BY start_time"
        )?;

        let events = stmt.query_map(params![date_str], |row| {
            let source_str: String = row.get(6)?;
            let source = match source_str.as_str() {
                "gcal" => EventSource::GoogleCalendar,
                _ => EventSource::Local,
            };
            let recurrence_str: Option<String> = row.get(7)?;
            let recurrence = recurrence_str
                .as_deref()
                .map(Recurrence::from_stored)
                .unwrap_or(Recurrence::None);
            Ok(Event {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(3)?, "%Y-%m-%d")
                    .unwrap_or(date),
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                source,
                recurrence,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        // Expand recurring events
        let mut result = Vec::new();
        for event in events {
            if event.date == date {
                // Direct match — include as-is
                result.push(event);
            } else if event.recurrence != Recurrence::None {
                // Check if this recurring event generates an instance on the query date
                if Self::recurrence_matches(&event, date) {
                    let mut virtual_event = event.clone();
                    virtual_event.date = date;
                    result.push(virtual_event);
                }
            }
        }

        Ok(result)
    }

    /// Check if a recurring event generates an instance on the given date.
    /// Returns false if the date is beyond the 1-year expansion cap.
    fn recurrence_matches(event: &Event, query_date: NaiveDate) -> bool {
        if event.recurrence == Recurrence::None {
            return false;
        }
        if query_date <= event.date {
            return false;
        }
        // 1-year cap
        let cap = event.date + chrono::Duration::days(365);
        if query_date > cap {
            return false;
        }

        match event.recurrence {
            Recurrence::Daily => true,
            Recurrence::Weekly => event.date.weekday() == query_date.weekday(),
            Recurrence::Monthly => {
                // Monthly: same day-of-month, but skip months that don't have that day
                event.date.day() == query_date.day()
            }
            Recurrence::None => false,
        }
    }

    /// Get dates that have events (for showing dots on calendar)
    /// Includes recurring event expansion within the month window.
    pub fn dates_with_events(&self, year: i32, month: u32) -> Result<std::collections::HashSet<NaiveDate>> {
        let _start = format!("{:04}-{:02}-01", year, month);
        let end_month = if month == 12 { 1 } else { month + 1 };
        let end_year = if month == 12 { year + 1 } else { year };
        let end = format!("{:04}-{:02}-01", end_year, end_month);

        // Fetch all events with date < end (includes events before the month for recurrence expansion)
        let mut stmt = self.conn.prepare(
            "SELECT id, title, description, date, start_time, end_time, source, recurrence
             FROM events WHERE date < ?1"
        )?;

        let all_events: Vec<Event> = stmt.query_map(params![end], |row| {
            let source_str: String = row.get(6)?;
            let source = match source_str.as_str() {
                "gcal" => EventSource::GoogleCalendar,
                _ => EventSource::Local,
            };
            let recurrence_str: Option<String> = row.get(7)?;
            let recurrence = recurrence_str
                .as_deref()
                .map(Recurrence::from_stored)
                .unwrap_or(Recurrence::None);
            Ok(Event {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(3)?, "%Y-%m-%d")
                    .unwrap_or_else(|_| NaiveDate::from_ymd_opt(year, month, 1).unwrap()),
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                source,
                recurrence,
            })
        })?.filter_map(|r| r.ok()).collect();

        let month_start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let month_end = NaiveDate::from_ymd_opt(end_year, end_month, 1).unwrap();

        let mut dates = std::collections::HashSet::new();

        for event in &all_events {
            // Direct match: event falls within the month
            if event.date >= month_start && event.date < month_end {
                dates.insert(event.date);
            }
            // Recurrence expansion: check each day in the month
            if event.recurrence != Recurrence::None {
                let mut d = month_start;
                while d < month_end {
                    if Self::recurrence_matches(event, d) {
                        dates.insert(d);
                    }
                    d = d.succ_opt().unwrap_or(d);
                    if d == month_end { break; }
                }
            }
        }

        Ok(dates)
    }

    /// Get upcoming events from today
    pub fn upcoming_events(&self, from_date: NaiveDate, limit: usize) -> Result<Vec<Event>> {
        let date_str = from_date.format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, title, description, date, start_time, end_time, source, recurrence
             FROM events WHERE date >= ?1 ORDER BY date, start_time LIMIT ?2"
        )?;

        let events = stmt.query_map(params![date_str, limit as i64], |row| {
            let source_str: String = row.get(6)?;
            let source = match source_str.as_str() {
                "gcal" => EventSource::GoogleCalendar,
                _ => EventSource::Local,
            };
            let recurrence_str: Option<String> = row.get(7)?;
            let recurrence = recurrence_str
                .as_deref()
                .map(Recurrence::from_stored)
                .unwrap_or(Recurrence::None);
            Ok(Event {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(3)?, "%Y-%m-%d")
                    .unwrap_or(from_date),
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                source,
                recurrence,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }

    /// Add a new local event
    pub fn add_event(&self, title: &str, date: NaiveDate, start_time: Option<&str>, end_time: Option<&str>) -> Result<i64> {
        self.add_event_with_recurrence(title, date, start_time, end_time, None, Recurrence::None)
    }

    /// Add a new local event with description and recurrence
    pub fn add_event_with_recurrence(
        &self,
        title: &str,
        date: NaiveDate,
        start_time: Option<&str>,
        end_time: Option<&str>,
        description: Option<&str>,
        recurrence: Recurrence,
    ) -> Result<i64> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let recurrence_str = recurrence.to_str();
        self.conn.execute(
            "INSERT INTO events (title, date, start_time, end_time, description, source, recurrence) VALUES (?1, ?2, ?3, ?4, ?5, 'local', ?6)",
            params![title, date_str, start_time, end_time, description, recurrence_str],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert or replace a Google Calendar event, keyed on `gcal_id`.
    /// Uses INSERT OR REPLACE for atomic idempotent upserts.
    /// Sets source='gcal' to distinguish from local events.
    pub fn upsert_gcal_event(&self, event: &GCalEvent) -> Result<()> {
        let date_str = event.date.format("%Y-%m-%d").to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO events (title, description, date, start_time, end_time, source, gcal_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'gcal', ?6)",
            params![
                event.title,
                event.description,
                date_str,
                event.start_time,
                event.end_time,
                event.gcal_id,
            ],
        ).context("Failed to upsert gcal event")?;
        Ok(())
    }

    /// Delete all events where source='gcal'. Returns the number of deleted rows.
    pub fn clear_gcal_events(&self) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM events WHERE source = 'gcal'",
            [],
        ).context("Failed to clear gcal events")?;
        Ok(deleted)
    }

    /// Update an existing local event. Returns error if ID doesn't exist or event is not local.
    pub fn update_event(
        &self,
        id: i64,
        title: &str,
        date: NaiveDate,
        start_time: Option<&str>,
        end_time: Option<&str>,
        description: Option<&str>,
        recurrence: Recurrence,
    ) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let recurrence_str = recurrence.to_str();
        let rows = self.conn.execute(
            "UPDATE events SET title = ?1, date = ?2, start_time = ?3, end_time = ?4, description = ?5, recurrence = ?6
             WHERE id = ?7 AND source = 'local'",
            params![title, date_str, start_time, end_time, description, recurrence_str, id],
        )?;
        if rows == 0 {
            anyhow::bail!("No local event found with id {}", id);
        }
        Ok(())
    }

    /// Delete a local event by ID. Returns the number of deleted rows (0 if not found or not local).
    pub fn delete_event(&self, id: i64) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM events WHERE id = ?1 AND source = 'local'",
            params![id],
        )?;
        Ok(deleted)
    }

    /// Open an in-memory EventStore for testing
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to open in-memory database")?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Task 1.1: Recurrence enum roundtrip tests
    #[test]
    fn test_recurrence_none_roundtrip() {
        let r = Recurrence::None;
        assert_eq!(r.to_str(), "none");
        assert_eq!(Recurrence::from_stored("none"), Recurrence::None);
    }

    #[test]
    fn test_recurrence_daily_roundtrip() {
        let r = Recurrence::Daily;
        assert_eq!(r.to_str(), "daily");
        assert_eq!(Recurrence::from_stored("daily"), Recurrence::Daily);
    }

    #[test]
    fn test_recurrence_weekly_roundtrip() {
        let r = Recurrence::Weekly;
        assert_eq!(r.to_str(), "weekly");
        assert_eq!(Recurrence::from_stored("weekly"), Recurrence::Weekly);
    }

    #[test]
    fn test_recurrence_monthly_roundtrip() {
        let r = Recurrence::Monthly;
        assert_eq!(r.to_str(), "monthly");
        assert_eq!(Recurrence::from_stored("monthly"), Recurrence::Monthly);
    }

    #[test]
    fn test_recurrence_from_str_unknown_defaults_to_none() {
        assert_eq!(Recurrence::from_stored("bogus"), Recurrence::None);
        assert_eq!(Recurrence::from_stored(""), Recurrence::None);
    }

    // Task 1.2: Event struct has recurrence field, schema migration
    #[test]
    fn test_event_has_recurrence_field() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event("Test", date, None, None).unwrap();
        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].recurrence, Recurrence::None);
    }

    #[test]
    fn test_add_event_with_recurrence() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Weekly Standup", date, Some("09:00"), Some("09:15"), None, Recurrence::Weekly).unwrap();
        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].recurrence, Recurrence::Weekly);
        assert_eq!(events[0].title, "Weekly Standup");
    }

    // Task 1.3: update_event
    #[test]
    fn test_update_event() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let id = store.add_event("Meeting", date, Some("10:00"), Some("11:00")).unwrap();

        let new_date = NaiveDate::from_ymd_opt(2026, 6, 2).unwrap();
        store.update_event(id, "Standup", new_date, Some("09:00"), Some("09:15"), None, Recurrence::Weekly).unwrap();

        let events = store.events_for_date(new_date).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Standup");
        assert_eq!(events[0].start_time.as_deref(), Some("09:00"));
        assert_eq!(events[0].end_time.as_deref(), Some("09:15"));
        assert_eq!(events[0].recurrence, Recurrence::Weekly);

        // Old date should have no events
        let old_events = store.events_for_date(date).unwrap();
        assert_eq!(old_events.len(), 0);
    }

    #[test]
    fn test_update_nonexistent_event() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let result = store.update_event(999, "Ghost", date, None, None, None, Recurrence::None);
        assert!(result.is_err());
    }

    // Task 1.4: delete_event
    #[test]
    fn test_delete_event() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let id = store.add_event("Doomed", date, None, None).unwrap();

        let deleted = store.delete_event(id).unwrap();
        assert_eq!(deleted, 1);

        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_delete_nonexistent_event() {
        let store = EventStore::open_in_memory().unwrap();
        let deleted = store.delete_event(999).unwrap();
        assert_eq!(deleted, 0);
    }

    // Task 1.5: Recurrence expansion in events_for_date
    #[test]
    fn test_daily_recurrence_expansion() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Daily Standup", start, Some("09:00"), Some("09:15"), None, Recurrence::Daily).unwrap();

        // Should appear on a future date
        let query = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Daily Standup");
        assert_eq!(events[0].date, query);
    }

    #[test]
    fn test_weekly_recurrence_match() {
        let store = EventStore::open_in_memory().unwrap();
        // Monday June 1, 2026
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Weekly Review", start, Some("14:00"), Some("15:00"), None, Recurrence::Weekly).unwrap();

        // Monday June 8, 2026 (same weekday)
        let query = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Weekly Review");
    }

    #[test]
    fn test_weekly_recurrence_non_match() {
        let store = EventStore::open_in_memory().unwrap();
        // Monday June 1, 2026
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Weekly Review", start, Some("14:00"), Some("15:00"), None, Recurrence::Weekly).unwrap();

        // Tuesday June 2, 2026 (different weekday)
        let query = NaiveDate::from_ymd_opt(2026, 6, 2).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_monthly_recurrence_match() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        store.add_event_with_recurrence("Monthly Report", start, None, None, None, Recurrence::Monthly).unwrap();

        // July 15
        let query = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Monthly Report");
    }

    #[test]
    fn test_monthly_recurrence_non_match() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        store.add_event_with_recurrence("Monthly Report", start, None, None, None, Recurrence::Monthly).unwrap();

        // July 1 (different day)
        let query = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_recurrence_one_year_cap() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Daily Thing", start, None, None, None, Recurrence::Daily).unwrap();

        // 366 days later — beyond 1-year cap
        let query = start + chrono::Duration::days(366);
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 0);

        // 364 days later — within cap
        let query2 = start + chrono::Duration::days(364);
        let events2 = store.events_for_date(query2).unwrap();
        assert_eq!(events2.len(), 1);
    }

    #[test]
    fn test_monthly_recurrence_skips_day_31() {
        let store = EventStore::open_in_memory().unwrap();
        // Event on Jan 31
        let start = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        store.add_event_with_recurrence("End of Month", start, None, None, None, Recurrence::Monthly).unwrap();

        // Feb has no 31st — should not expand
        let query = NaiveDate::from_ymd_opt(2026, 2, 28).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 0);

        // March 31 — should expand
        let query2 = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        let events2 = store.events_for_date(query2).unwrap();
        assert_eq!(events2.len(), 1);
    }

    #[test]
    fn test_no_recurrence_does_not_expand() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event("One-time", start, None, None).unwrap();

        // Should NOT appear on a future date
        let query = NaiveDate::from_ymd_opt(2026, 6, 2).unwrap();
        let events = store.events_for_date(query).unwrap();
        assert_eq!(events.len(), 0);
    }

    // Task 1.6: dates_with_events includes recurring occurrences
    #[test]
    fn test_dates_with_events_includes_recurring() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Daily Standup", start, Some("09:00"), Some("09:15"), None, Recurrence::Daily).unwrap();

        let dates = store.dates_with_events(2026, 6).unwrap();
        // Daily recurrence from June 1 should include all dates from June 1 to June 30
        for day in 1..=30 {
            let d = NaiveDate::from_ymd_opt(2026, 6, day).unwrap();
            assert!(dates.contains(&d), "Expected June {} to have event dot", day);
        }
    }

    #[test]
    fn test_dates_with_events_weekly_only_matching_weekday() {
        let store = EventStore::open_in_memory().unwrap();
        // Monday June 1, 2026
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Weekly", start, None, None, None, Recurrence::Weekly).unwrap();

        let dates = store.dates_with_events(2026, 6).unwrap();
        // Should include June 1, 8, 15, 22, 29 (Mondays)
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 8).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 22).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 29).unwrap()));
        // Should NOT include June 2 (Tuesday)
        assert!(!dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 2).unwrap()));
    }

    fn make_gcal_event(id: &str, title: &str, date: NaiveDate) -> GCalEvent {
        GCalEvent {
            gcal_id: id.into(),
            title: title.into(),
            description: None,
            date,
            start_time: None,
            end_time: None,
        }
    }

    #[test]
    fn test_upsert_gcal_event_inserts_new() {
        let store = EventStore::open_in_memory().unwrap();
        let event = make_gcal_event("gcal-1", "Standup", NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());

        store.upsert_gcal_event(&event).unwrap();

        let events = store.events_for_date(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Standup");
        assert_eq!(events[0].source, EventSource::GoogleCalendar);
    }

    #[test]
    fn test_upsert_gcal_event_replaces_existing() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        let event_v1 = make_gcal_event("gcal-1", "Standup v1", date);
        store.upsert_gcal_event(&event_v1).unwrap();

        let event_v2 = GCalEvent {
            gcal_id: "gcal-1".into(),
            title: "Standup v2".into(),
            description: Some("Updated".into()),
            date,
            start_time: Some("09:00".into()),
            end_time: Some("09:15".into()),
        };
        store.upsert_gcal_event(&event_v2).unwrap();

        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 1, "Should have exactly 1 event after upsert");
        assert_eq!(events[0].title, "Standup v2");
        assert_eq!(events[0].description.as_deref(), Some("Updated"));
        assert_eq!(events[0].start_time.as_deref(), Some("09:00"));
    }

    #[test]
    fn test_upsert_gcal_event_does_not_touch_local_events() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        store.add_event("Local meeting", date, Some("10:00"), Some("11:00")).unwrap();

        let gcal = make_gcal_event("gcal-1", "GCal event", date);
        store.upsert_gcal_event(&gcal).unwrap();

        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 2);
        let local = events.iter().find(|e| e.source == EventSource::Local).unwrap();
        assert_eq!(local.title, "Local meeting");
    }

    #[test]
    fn test_clear_gcal_events_removes_only_gcal() {
        let store = EventStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        store.add_event("Local meeting", date, None, None).unwrap();
        store.add_event("Another local", date, None, None).unwrap();
        store.upsert_gcal_event(&make_gcal_event("gcal-1", "GCal 1", date)).unwrap();
        store.upsert_gcal_event(&make_gcal_event("gcal-2", "GCal 2", date)).unwrap();

        let deleted = store.clear_gcal_events().unwrap();
        assert_eq!(deleted, 2, "Should delete exactly 2 gcal events");

        let events = store.events_for_date(date).unwrap();
        assert_eq!(events.len(), 2, "Only local events should remain");
        assert!(events.iter().all(|e| e.source == EventSource::Local));
    }

    #[test]
    fn test_clear_gcal_events_empty_store() {
        let store = EventStore::open_in_memory().unwrap();
        let deleted = store.clear_gcal_events().unwrap();
        assert_eq!(deleted, 0, "Should return 0 when no gcal events exist");
    }
}

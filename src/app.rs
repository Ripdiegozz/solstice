use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

use crate::calendar::grid::week_start_date;
use crate::calendar::holidays::{self, HolidayProvider};
use crate::config::Config;
use crate::events::{Event, EventSource, EventStore, Recurrence};
use crate::ui::text_input::TextInput;

/// Modal mode: create or edit
#[derive(Debug, Clone, PartialEq)]
pub enum ModalMode {
    Create,
    Edit,
}

/// View mode for the calendar
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    Monthly,
    Weekly,
}

/// A single form field in the modal
#[derive(Debug, Clone)]
pub struct FormField {
    pub label: &'static str,
    pub input: TextInput,
}

/// State for the event creation/editing modal
#[derive(Debug, Clone)]
pub struct ModalState {
    pub mode: ModalMode,
    pub event_id: Option<i64>,
    pub fields: Vec<FormField>,
    pub focused_field: usize,
    pub error_message: Option<String>,
}

impl ModalState {
    /// Create a new modal for creating an event, with date pre-filled
    pub fn new_create(selected_date: NaiveDate) -> Self {
        let date_str = selected_date.format("%Y-%m-%d").to_string();
        let fields = vec![
            FormField { label: "Title", input: TextInput::new(200) },
            FormField { label: "Date (YYYY-MM-DD)", input: TextInput::with_value(&date_str, 10) },
            FormField { label: "Start Time (HH:MM)", input: TextInput::new(5) },
            FormField { label: "End Time (HH:MM)", input: TextInput::new(5) },
            FormField { label: "Description", input: TextInput::new(500) },
            FormField { label: "Recurrence (Space to cycle)", input: TextInput::with_value("none", 10) },
        ];
        Self {
            mode: ModalMode::Create,
            event_id: None,
            fields,
            focused_field: 0,
            error_message: None,
        }
    }

    /// Create a new modal for editing an event, pre-populated
    pub fn new_edit(event: &Event) -> Self {
        let date_str = event.date.format("%Y-%m-%d").to_string();
        let start_time = event.start_time.as_deref().unwrap_or("");
        let end_time = event.end_time.as_deref().unwrap_or("");
        let description = event.description.as_deref().unwrap_or("");
        let recurrence_str = event.recurrence.to_str();

        let fields = vec![
            FormField { label: "Title", input: TextInput::with_value(&event.title, 200) },
            FormField { label: "Date (YYYY-MM-DD)", input: TextInput::with_value(&date_str, 10) },
            FormField { label: "Start Time (HH:MM)", input: TextInput::with_value(start_time, 5) },
            FormField { label: "End Time (HH:MM)", input: TextInput::with_value(end_time, 5) },
            FormField { label: "Description", input: TextInput::with_value(description, 500) },
            FormField { label: "Recurrence (Space to cycle)", input: TextInput::with_value(recurrence_str, 10) },
        ];
        Self {
            mode: ModalMode::Edit,
            event_id: Some(event.id),
            fields,
            focused_field: 0,
            error_message: None,
        }
    }

    /// Validate all fields. Returns Ok(()) if valid, Err(message) if invalid.
    pub fn validate(&self) -> std::result::Result<(), String> {
        // Title required
        let title = self.fields[0].input.value().trim();
        if title.is_empty() {
            return Err("Title is required".to_string());
        }

        // Date must be YYYY-MM-DD
        let date_str = self.fields[1].input.value().trim();
        if NaiveDate::parse_from_str(date_str, "%Y-%m-%d").is_err() {
            return Err("Date must be YYYY-MM-DD".to_string());
        }

        // Start time: HH:MM when non-empty
        let start_str = self.fields[2].input.value().trim();
        let start_time = if !start_str.is_empty() {
            match parse_time(start_str) {
                Ok(t) => Some(t),
                Err(e) => return Err(e),
            }
        } else {
            None
        };

        // End time: HH:MM when non-empty
        let end_str = self.fields[3].input.value().trim();
        let end_time = if !end_str.is_empty() {
            match parse_time(end_str) {
                Ok(t) => Some(t),
                Err(e) => return Err(e),
            }
        } else {
            None
        };

        // End time must be >= start time when both provided
        if let (Some(start), Some(end)) = (start_time, end_time) {
            if end < start {
                return Err("End time must be after start time".to_string());
            }
        }

        Ok(())
    }

    /// Extract parsed event data from the form fields
    pub fn to_event_data(&self) -> Option<(String, NaiveDate, Option<String>, Option<String>, Option<String>, Recurrence)> {
        let title = self.fields[0].input.value().trim().to_string();
        let date_str = self.fields[1].input.value().trim();
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
        let start_time = {
            let s = self.fields[2].input.value().trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        };
        let end_time = {
            let s = self.fields[3].input.value().trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        };
        let description = {
            let s = self.fields[4].input.value().trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        };
        let recurrence = Recurrence::from_stored(self.fields[5].input.value().trim());

        Some((title, date, start_time, end_time, description, recurrence))
    }

    /// Cycle the recurrence field through the options
    pub fn cycle_recurrence(&mut self) {
        let current = self.fields[5].input.value().trim();
        let next = match current {
            "none" => "daily",
            "daily" => "weekly",
            "weekly" => "monthly",
            "monthly" => "none",
            _ => "none",
        };
        self.fields[5].input = TextInput::with_value(next, 10);
    }

    /// Move focus to the next field (wraps)
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % self.fields.len();
    }

    /// Move focus to the previous field (wraps)
    pub fn focus_prev(&mut self) {
        if self.focused_field == 0 {
            self.focused_field = self.fields.len() - 1;
        } else {
            self.focused_field -= 1;
        }
    }
}

/// Parse a time string in HH:MM format
fn parse_time(s: &str) -> std::result::Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("Time must be HH:MM".to_string());
    }
    let hours: u32 = parts[0].parse().map_err(|_| "Invalid time format".to_string())?;
    let minutes: u32 = parts[1].parse().map_err(|_| "Invalid time format".to_string())?;
    if hours > 23 || minutes > 59 {
        return Err("Time must be HH:MM (00:00-23:59)".to_string());
    }
    Ok((hours, minutes))
}

/// Application state
pub struct App {
    pub config: Config,
    pub running: bool,

    // Time
    pub now: chrono::DateTime<Tz>,
    pub today: NaiveDate,
    pub timezone: Tz,

    // Calendar view state
    pub view_year: i32,
    pub view_month: u32,
    pub selected_date: NaiveDate,

    // Data
    pub holidays: HashMap<NaiveDate, String>,
    pub event_dates: HashSet<NaiveDate>,
    pub selected_day_events: Vec<Event>,
    pub upcoming_events: Vec<Event>,
    pub view_mode: ViewMode,
    pub week_events: HashMap<NaiveDate, Vec<Event>>,

    // Modal state
    pub modal: Option<ModalState>,
    pub selected_event_index: Option<usize>,
    pub confirm_delete: bool,
    pub status_message: Option<String>,

    // Internal
    holiday_provider: Box<dyn HolidayProvider>,
    event_store: Option<EventStore>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let tz: Tz = config.timezone.parse()
            .unwrap_or(chrono_tz::UTC);

        let now = chrono::Utc::now().with_timezone(&tz);
        let today = now.date_naive();

        let holiday_provider = holidays::create_provider(
            config.calendarific_api_key.as_deref(),
        );
        let holidays = holiday_provider.load(&config.country_code, today.year())?;

        // Try to open event store (non-fatal if it fails)
        let event_store = EventStore::open().ok();
        let event_dates = event_store
            .as_ref()
            .and_then(|s| s.dates_with_events(today.year(), today.month()).ok())
            .unwrap_or_default();

        let selected_day_events = event_store
            .as_ref()
            .and_then(|s| s.events_for_date(today).ok())
            .unwrap_or_default();

        let upcoming_events = event_store
            .as_ref()
            .and_then(|s| s.upcoming_events(today, config.upcoming_event_count).ok())
            .unwrap_or_default();

        Ok(Self {
            config: config.clone(),
            running: true,
            now,
            today,
            timezone: tz,
            view_year: today.year(),
            view_month: today.month(),
            selected_date: today,
            holidays,
            event_dates,
            selected_day_events,
            upcoming_events,
            view_mode: ViewMode::Monthly,
            week_events: HashMap::new(),
            modal: None,
            selected_event_index: None,
            confirm_delete: false,
            status_message: None,
            holiday_provider,
            event_store,
        })
    }

    /// Update the current time
    pub fn tick(&mut self) {
        self.now = chrono::Utc::now().with_timezone(&self.timezone);
    }

    /// Navigate to previous month
    pub fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_month = 12;
            self.view_year -= 1;
        } else {
            self.view_month -= 1;
        }
        self.refresh_data();
    }

    /// Navigate to next month
    pub fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_month = 1;
            self.view_year += 1;
        } else {
            self.view_month += 1;
        }
        self.refresh_data();
    }

    /// Navigate to previous day
    pub fn prev_day(&mut self) {
        self.selected_date = self.selected_date.pred_opt().unwrap_or(self.selected_date);
        self.refresh_selected_day();

        // Auto-navigate month if selected date moves to previous month
        if self.selected_date.month() != self.view_month || self.selected_date.year() != self.view_year {
            self.view_year = self.selected_date.year();
            self.view_month = self.selected_date.month();
            self.refresh_data();
        }
    }

    /// Navigate to next day
    pub fn next_day(&mut self) {
        self.selected_date = self.selected_date.succ_opt().unwrap_or(self.selected_date);
        self.refresh_selected_day();

        // Auto-navigate month if selected date moves to next month
        if self.selected_date.month() != self.view_month || self.selected_date.year() != self.view_year {
            self.view_year = self.selected_date.year();
            self.view_month = self.selected_date.month();
            self.refresh_data();
        }
    }

    /// Jump to today
    pub fn go_to_today(&mut self) {
        self.selected_date = self.today;
        self.view_year = self.today.year();
        self.view_month = self.today.month();
        self.refresh_data();
    }

    /// Toggle between monthly and weekly view
    pub fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Monthly => ViewMode::Weekly,
            ViewMode::Weekly => ViewMode::Monthly,
        };
        self.refresh_data();
    }

    /// Navigate to previous week
    pub fn prev_week(&mut self) {
        self.selected_date -= chrono::Duration::days(7);
        self.view_year = self.selected_date.year();
        self.view_month = self.selected_date.month();
        self.refresh_data();
    }

    /// Navigate to next week
    pub fn next_week(&mut self) {
        self.selected_date += chrono::Duration::days(7);
        self.view_year = self.selected_date.year();
        self.view_month = self.selected_date.month();
        self.refresh_data();
    }

    /// Navigate backward based on current view mode
    pub fn navigate_back(&mut self) {
        match self.view_mode {
            ViewMode::Monthly => self.prev_month(),
            ViewMode::Weekly => self.prev_week(),
        }
    }

    /// Navigate forward based on current view mode
    pub fn navigate_forward(&mut self) {
        match self.view_mode {
            ViewMode::Monthly => self.next_month(),
            ViewMode::Weekly => self.next_week(),
        }
    }

    /// Refresh holidays and events for the current view month
    pub fn refresh_data(&mut self) {
        // Reload holidays for the viewed year
        self.holidays = self.holiday_provider
            .load(&self.config.country_code, self.view_year)
            .unwrap_or_default();

        // Reload event dates for the viewed month
        self.event_dates = self.event_store
            .as_ref()
            .and_then(|s| s.dates_with_events(self.view_year, self.view_month).ok())
            .unwrap_or_default();

        // Populate week_events when in weekly mode
        if self.view_mode == ViewMode::Weekly {
            let week_start = week_start_date(self.selected_date, self.config.first_day_of_week);
            let mut week_events = HashMap::new();
            for i in 0..7 {
                let date = week_start + chrono::Duration::days(i);
                let events = self.event_store
                    .as_ref()
                    .and_then(|s| s.events_for_date(date).ok())
                    .unwrap_or_default();
                week_events.insert(date, events);
            }
            self.week_events = week_events;
        }

        // Reload upcoming events
        self.upcoming_events = self.event_store
            .as_ref()
            .and_then(|s| s.upcoming_events(self.today, self.config.upcoming_event_count).ok())
            .unwrap_or_default();

        self.refresh_selected_day();
    }

    /// Refresh events for the selected day
    pub fn refresh_selected_day(&mut self) {
        self.selected_day_events = self.event_store
            .as_ref()
            .and_then(|s| s.events_for_date(self.selected_date).ok())
            .unwrap_or_default();

        // Clamp selected_event_index if it's out of bounds
        if let Some(idx) = self.selected_event_index {
            if idx >= self.selected_day_events.len() {
                self.selected_event_index = if self.selected_day_events.is_empty() {
                    None
                } else {
                    Some(self.selected_day_events.len() - 1)
                };
            }
        }
    }

    /// Open the create modal with the selected date pre-filled
    pub fn open_create_modal(&mut self) {
        self.modal = Some(ModalState::new_create(self.selected_date));
    }

    /// Open the edit modal for the selected event.
    /// Returns false if no event is selected or the event is from gcal.
    pub fn open_edit_modal(&mut self) -> bool {
        let idx = match self.selected_event_index {
            Some(i) if i < self.selected_day_events.len() => i,
            _ => return false,
        };

        let event = &self.selected_day_events[idx];
        if event.source == EventSource::GoogleCalendar {
            self.status_message = Some("Cannot edit Google Calendar events".to_string());
            return false;
        }

        self.modal = Some(ModalState::new_edit(event));
        true
    }

    /// Select the next event in the list
    pub fn select_next_event(&mut self) {
        if self.selected_day_events.is_empty() {
            self.selected_event_index = None;
            return;
        }
        self.selected_event_index = Some(match self.selected_event_index {
            Some(i) if i + 1 < self.selected_day_events.len() => i + 1,
            Some(i) => i, // already at end
            None => 0,
        });
    }

    /// Select the previous event in the list
    pub fn select_prev_event(&mut self) {
        if self.selected_day_events.is_empty() {
            self.selected_event_index = None;
            return;
        }
        self.selected_event_index = Some(match self.selected_event_index {
            Some(0) => 0, // already at start
            Some(i) => i - 1,
            None => 0,
        });
    }

    /// Delete the selected event (with gcal protection check).
    /// Returns true if deleted, false if blocked.
    pub fn delete_selected_event(&mut self) -> bool {
        let idx = match self.selected_event_index {
            Some(i) if i < self.selected_day_events.len() => i,
            _ => {
                self.status_message = Some("No event selected".to_string());
                return false;
            }
        };

        let event = &self.selected_day_events[idx];
        if event.source == EventSource::GoogleCalendar {
            self.status_message = Some("Cannot delete Google Calendar events".to_string());
            return false;
        }

        let event_id = event.id;
        if let Some(ref store) = self.event_store {
            match store.delete_event(event_id) {
                Ok(deleted) if deleted > 0 => {
                    // Reset selection
                    if self.selected_day_events.len() <= 1 {
                        self.selected_event_index = None;
                    } else if idx >= self.selected_day_events.len() - 1 {
                        self.selected_event_index = Some(self.selected_day_events.len() - 2);
                    }
                    self.refresh_data();
                    self.status_message = Some("Event deleted".to_string());
                    return true;
                }
                _ => {
                    self.status_message = Some("Failed to delete event".to_string());
                }
            }
        }
        false
    }

    /// Handle delete confirmation key press.
    /// Returns true if the key was handled.
    pub fn handle_delete_confirm(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        
        self.confirm_delete = false;
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.delete_selected_event();
                true
            }
            _ => {
                self.status_message = Some("Delete cancelled".to_string());
                true
            }
        }
    }

    /// Save the modal form data (create or update event)
    pub fn save_modal(&mut self) -> bool {
        let modal = match &self.modal {
            Some(m) => m.clone(),
            None => return false,
        };

        // Validate
        if let Err(msg) = modal.validate() {
            if let Some(ref mut m) = self.modal {
                m.error_message = Some(msg);
            }
            return false;
        }

        let (title, date, start_time, end_time, description, recurrence) = match modal.to_event_data() {
            Some(data) => data,
            None => {
                if let Some(ref mut m) = self.modal {
                    m.error_message = Some("Invalid form data".to_string());
                }
                return false;
            }
        };

        if let Some(ref store) = self.event_store {
            let result = match modal.mode {
                ModalMode::Create => {
                    store.add_event_with_recurrence(
                        &title,
                        date,
                        start_time.as_deref(),
                        end_time.as_deref(),
                        description.as_deref(),
                        recurrence,
                    ).map(|_| ())
                }
                ModalMode::Edit => {
                    if let Some(id) = modal.event_id {
                        store.update_event(
                            id,
                            &title,
                            date,
                            start_time.as_deref(),
                            end_time.as_deref(),
                            description.as_deref(),
                            recurrence,
                        )
                    } else {
                        Ok(())
                    }
                }
            };

            match result {
                Ok(()) => {
                    self.modal = None;
                    self.refresh_data();
                    self.status_message = Some(match modal.mode {
                        ModalMode::Create => "Event created".to_string(),
                        ModalMode::Edit => "Event updated".to_string(),
                    });
                    return true;
                }
                Err(e) => {
                    if let Some(ref mut m) = self.modal {
                        m.error_message = Some(format!("Save failed: {}", e));
                    }
                }
            }
        }
        false
    }

    /// Handle a key event when the modal is active.
    /// Returns true if the key was handled.
    pub fn handle_modal_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Check if we need to save (Enter on non-recurrence field)
        // This needs to be checked before we borrow modal mutably
        let should_save = key.code == KeyCode::Enter && matches!(&self.modal, Some(m) if m.focused_field != 5);

        if should_save {
            self.save_modal();
            return true;
        }

        let modal = match &mut self.modal {
            Some(m) => m,
            None => return false,
        };

        // Clear error on any key
        modal.error_message = None;

        match key.code {
            KeyCode::Esc => {
                self.modal = None;
                return true;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    modal.focus_prev();
                } else {
                    modal.focus_next();
                }
                return true;
            }
            KeyCode::Enter => {
                // If on recurrence field, cycle it
                modal.cycle_recurrence();
                return true;
            }
            KeyCode::Backspace => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.delete_backward();
                return true;
            }
            KeyCode::Left => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.move_cursor_left();
                return true;
            }
            KeyCode::Right => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.move_cursor_right();
                return true;
            }
            KeyCode::Home => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.move_cursor_home();
                return true;
            }
            KeyCode::End => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.move_cursor_end();
                return true;
            }
            KeyCode::Char(' ') if modal.focused_field == 5 => {
                modal.cycle_recurrence();
                return true;
            }
            KeyCode::Char(c) => {
                let field_idx = modal.focused_field;
                modal.fields[field_idx].input.insert_char(c);
                return true;
            }
            _ => {}
        }

        true // Modal absorbs all keys
    }

    /// Quit the application
    pub fn quit(&mut self) {
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Task 3.3: Validation tests
    #[test]
    fn test_validate_empty_title() {
        let modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        let result = modal.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Title is required");
    }

    #[test]
    fn test_validate_invalid_date() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.fields[0].input = TextInput::with_value("Meeting", 200);
        modal.fields[1].input = TextInput::with_value("15-06-2026", 10);
        let result = modal.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Date must be YYYY-MM-DD");
    }

    #[test]
    fn test_validate_end_before_start() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.fields[0].input = TextInput::with_value("Meeting", 200);
        modal.fields[2].input = TextInput::with_value("14:00", 5);
        modal.fields[3].input = TextInput::with_value("10:00", 5);
        let result = modal.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "End time must be after start time");
    }

    #[test]
    fn test_validate_success() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.fields[0].input = TextInput::with_value("Lunch", 200);
        modal.fields[2].input = TextInput::with_value("12:00", 5);
        modal.fields[3].input = TextInput::with_value("13:00", 5);
        let result = modal.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_time_format() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.fields[0].input = TextInput::with_value("Meeting", 200);
        modal.fields[2].input = TextInput::with_value("25:00", 5);
        let result = modal.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_times_allowed() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.fields[0].input = TextInput::with_value("All Day", 200);
        // Leave times empty
        let result = modal.validate();
        assert!(result.is_ok());
    }

    // Task 3.1/3.2: Modal creation tests
    #[test]
    fn test_new_create_modal_prefills_date() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let modal = ModalState::new_create(date);
        assert_eq!(modal.mode, ModalMode::Create);
        assert_eq!(modal.event_id, None);
        assert_eq!(modal.fields[1].input.value(), "2026-06-15");
        assert_eq!(modal.focused_field, 0);
    }

    #[test]
    fn test_new_edit_modal_prefills_event_data() {
        let event = Event {
            id: 42,
            title: "Meeting".to_string(),
            description: Some("Important".to_string()),
            date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            start_time: Some("09:00".to_string()),
            end_time: Some("10:00".to_string()),
            source: EventSource::Local,
            recurrence: Recurrence::Weekly,
        };
        let modal = ModalState::new_edit(&event);
        assert_eq!(modal.mode, ModalMode::Edit);
        assert_eq!(modal.event_id, Some(42));
        assert_eq!(modal.fields[0].input.value(), "Meeting");
        assert_eq!(modal.fields[1].input.value(), "2026-06-01");
        assert_eq!(modal.fields[2].input.value(), "09:00");
        assert_eq!(modal.fields[3].input.value(), "10:00");
        assert_eq!(modal.fields[4].input.value(), "Important");
        assert_eq!(modal.fields[5].input.value(), "weekly");
    }

    // Task 3.4: Event selection tests
    #[test]
    fn test_select_next_event() {
        let mut app = make_test_app();
        app.selected_day_events = vec![
            make_test_event(1, "A"),
            make_test_event(2, "B"),
            make_test_event(3, "C"),
        ];
        app.selected_event_index = None;
        app.select_next_event();
        assert_eq!(app.selected_event_index, Some(0));
        app.select_next_event();
        assert_eq!(app.selected_event_index, Some(1));
        app.select_next_event();
        assert_eq!(app.selected_event_index, Some(2));
        app.select_next_event(); // clamp at end
        assert_eq!(app.selected_event_index, Some(2));
    }

    #[test]
    fn test_select_prev_event() {
        let mut app = make_test_app();
        app.selected_day_events = vec![
            make_test_event(1, "A"),
            make_test_event(2, "B"),
        ];
        app.selected_event_index = Some(1);
        app.select_prev_event();
        assert_eq!(app.selected_event_index, Some(0));
        app.select_prev_event(); // clamp at start
        assert_eq!(app.selected_event_index, Some(0));
    }

    #[test]
    fn test_select_event_empty_list() {
        let mut app = make_test_app();
        app.selected_day_events = vec![];
        app.select_next_event();
        assert_eq!(app.selected_event_index, None);
    }

    // Tab navigation wrapping tests
    #[test]
    fn test_modal_tab_wraps() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(modal.focused_field, 0);
        for _ in 0..6 {
            modal.focus_next();
        }
        assert_eq!(modal.focused_field, 0, "Tab should wrap around");
    }

    #[test]
    fn test_modal_shift_tab_wraps() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        modal.focus_prev();
        assert_eq!(modal.focused_field, 5, "Shift-Tab from 0 should wrap to last");
    }

    // Recurrence cycling test
    #[test]
    fn test_cycle_recurrence() {
        let mut modal = ModalState::new_create(NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(modal.fields[5].input.value(), "none");
        modal.cycle_recurrence();
        assert_eq!(modal.fields[5].input.value(), "daily");
        modal.cycle_recurrence();
        assert_eq!(modal.fields[5].input.value(), "weekly");
        modal.cycle_recurrence();
        assert_eq!(modal.fields[5].input.value(), "monthly");
        modal.cycle_recurrence();
        assert_eq!(modal.fields[5].input.value(), "none");
    }

    // Weekly view tests
    #[test]
    fn test_view_mode_default_is_monthly() {
        let app = make_test_app();
        assert_eq!(app.view_mode, ViewMode::Monthly);
    }

    #[test]
    fn test_toggle_view_flips_mode_and_preserves_date() {
        let mut app = make_test_app();
        let original_date = app.selected_date;

        app.toggle_view();
        assert_eq!(app.view_mode, ViewMode::Weekly);
        assert_eq!(app.selected_date, original_date);

        app.toggle_view();
        assert_eq!(app.view_mode, ViewMode::Monthly);
        assert_eq!(app.selected_date, original_date);
    }

    #[test]
    fn test_prev_week_moves_back_seven_days() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

        app.prev_week();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 6, 10).unwrap());
        assert_eq!(app.view_year, 2026);
        assert_eq!(app.view_month, 6);
    }

    #[test]
    fn test_next_week_moves_forward_seven_days() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

        app.next_week();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 6, 24).unwrap());
        assert_eq!(app.view_year, 2026);
        assert_eq!(app.view_month, 6);
    }

    #[test]
    fn test_prev_week_crosses_month_boundary() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();

        app.prev_week();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 6, 24).unwrap());
        assert_eq!(app.view_year, 2026);
        assert_eq!(app.view_month, 6);
    }

    #[test]
    fn test_next_week_crosses_month_boundary() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();

        app.next_week();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 7, 6).unwrap());
        assert_eq!(app.view_year, 2026);
        assert_eq!(app.view_month, 7);
    }

    #[test]
    fn test_next_week_crosses_year_boundary() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 12, 28).unwrap();

        app.next_week();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2027, 1, 4).unwrap());
        assert_eq!(app.view_year, 2027);
        assert_eq!(app.view_month, 1);
    }

    #[test]
    fn test_navigate_back_dispatches_based_on_mode() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Monthly;
        let original_month = app.view_month;
        app.navigate_back();
        assert_eq!(app.view_month, original_month - 1);

        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        app.navigate_back();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 6, 10).unwrap());
    }

    #[test]
    fn test_navigate_forward_dispatches_based_on_mode() {
        let mut app = make_test_app();
        app.view_mode = ViewMode::Monthly;
        let original_month = app.view_month;
        app.navigate_forward();
        assert_eq!(app.view_month, original_month + 1);

        app.view_mode = ViewMode::Weekly;
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
        app.navigate_forward();
        assert_eq!(app.selected_date, NaiveDate::from_ymd_opt(2026, 6, 24).unwrap());
    }

    fn make_test_event(id: i64, title: &str) -> Event {
        Event {
            id,
            title: title.to_string(),
            description: None,
            date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            start_time: None,
            end_time: None,
            source: EventSource::Local,
            recurrence: Recurrence::None,
        }
    }

    fn make_test_app() -> App {
        App {
            config: Config::default(),
            running: true,
            now: chrono::Utc::now().with_timezone(&chrono_tz::UTC),
            today: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            timezone: chrono_tz::UTC,
            view_year: 2026,
            view_month: 6,
            selected_date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            holidays: HashMap::new(),
            event_dates: HashSet::new(),
            selected_day_events: vec![],
            upcoming_events: vec![],
            view_mode: ViewMode::Monthly,
            week_events: HashMap::new(),
            modal: None,
            selected_event_index: None,
            confirm_delete: false,
            status_message: None,
            holiday_provider: Box::new(crate::calendar::holidays::LocalFileProvider::new()),
            event_store: None,
        }
    }

    fn make_test_app_with_store() -> App {
        let store = EventStore::open_in_memory().unwrap();
        App {
            config: Config::default(),
            running: true,
            now: chrono::Utc::now().with_timezone(&chrono_tz::UTC),
            today: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            timezone: chrono_tz::UTC,
            view_year: 2026,
            view_month: 6,
            selected_date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            holidays: HashMap::new(),
            event_dates: HashSet::new(),
            selected_day_events: vec![],
            upcoming_events: vec![],
            view_mode: ViewMode::Monthly,
            week_events: HashMap::new(),
            modal: None,
            selected_event_index: None,
            confirm_delete: false,
            status_message: None,
            holiday_provider: Box::new(crate::calendar::holidays::LocalFileProvider::new()),
            event_store: Some(store),
        }
    }

    // Task 6.1: Integration test — full create flow
    #[test]
    fn test_full_create_flow() {
        let mut app = make_test_app_with_store();
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();

        // Open create modal
        app.open_create_modal();
        assert!(app.modal.is_some());

        // Fill in title
        let modal = app.modal.as_mut().unwrap();
        modal.fields[0].input = TextInput::with_value("Lunch", 200);
        modal.fields[2].input = TextInput::with_value("12:00", 5);
        modal.fields[3].input = TextInput::with_value("13:00", 5);

        // Save
        let saved = app.save_modal();
        assert!(saved, "Save should succeed");
        assert!(app.modal.is_none(), "Modal should close after save");

        // Verify event in DB
        app.selected_date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        app.refresh_selected_day();
        assert_eq!(app.selected_day_events.len(), 1);
        assert_eq!(app.selected_day_events[0].title, "Lunch");
        assert_eq!(app.selected_day_events[0].start_time.as_deref(), Some("12:00"));
    }

    // Task 6.2: Integration test — full edit flow
    #[test]
    fn test_full_edit_flow() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        // Create an event first
        let store = app.event_store.as_ref().unwrap();
        let id = store.add_event("Meeting", date, Some("10:00"), Some("11:00")).unwrap();
        app.refresh_selected_day();

        // Select the event
        app.selected_event_index = Some(0);

        // Open edit modal
        let opened = app.open_edit_modal();
        assert!(opened, "Edit modal should open for local event");

        // Verify pre-populated
        let modal = app.modal.as_ref().unwrap();
        assert_eq!(modal.mode, ModalMode::Edit);
        assert_eq!(modal.event_id, Some(id));
        assert_eq!(modal.fields[0].input.value(), "Meeting");

        // Modify title
        let modal = app.modal.as_mut().unwrap();
        modal.fields[0].input = TextInput::with_value("Standup", 200);

        // Save
        let saved = app.save_modal();
        assert!(saved);
        assert!(app.modal.is_none());

        // Verify update
        app.refresh_selected_day();
        assert_eq!(app.selected_day_events.len(), 1);
        assert_eq!(app.selected_day_events[0].title, "Standup");
    }

    // Task 6.3: Integration test — gcal edit/delete blocked
    #[test]
    fn test_gcal_edit_blocked() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        // Add a gcal event directly
        let store = app.event_store.as_ref().unwrap();
        let gcal_event = crate::gcal::GCalEvent {
            gcal_id: "gcal-1".into(),
            title: "GCal Event".into(),
            description: None,
            date,
            start_time: None,
            end_time: None,
        };
        store.upsert_gcal_event(&gcal_event).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);

        let opened = app.open_edit_modal();
        assert!(!opened, "Edit modal should NOT open for gcal event");
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().contains("Google Calendar"));
    }

    #[test]
    fn test_gcal_delete_blocked() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        let store = app.event_store.as_ref().unwrap();
        let gcal_event = crate::gcal::GCalEvent {
            gcal_id: "gcal-1".into(),
            title: "GCal Event".into(),
            description: None,
            date,
            start_time: None,
            end_time: None,
        };
        store.upsert_gcal_event(&gcal_event).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);

        let deleted = app.delete_selected_event();
        assert!(!deleted, "Delete should be blocked for gcal event");
        assert!(app.status_message.as_ref().unwrap().contains("Google Calendar"));
    }

    // Task 6.4: Integration test — recurring events show calendar dots
    #[test]
    fn test_recurring_events_calendar_dots() {
        let mut app = make_test_app_with_store();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        let store = app.event_store.as_ref().unwrap();
        store.add_event_with_recurrence("Daily", start, None, None, None, Recurrence::Daily).unwrap();

        app.refresh_data();

        // Check that event_dates includes dates throughout the month
        assert!(app.event_dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()));
        assert!(app.event_dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
        assert!(app.event_dates.contains(&NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()));
    }

    // Task 6.5: Edge case tests
    #[test]
    fn test_delete_last_event_resets_selection() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        let store = app.event_store.as_ref().unwrap();
        store.add_event("Only Event", date, None, None).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);

        let deleted = app.delete_selected_event();
        assert!(deleted);
        assert_eq!(app.selected_event_index, None, "Selection should reset after deleting last event");
    }

    #[test]
    fn test_monthly_recurrence_skips_feb_31() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        store.add_event_with_recurrence("End of Month", start, None, None, None, Recurrence::Monthly).unwrap();

        // Feb 28 should NOT have the event
        let feb_events = store.events_for_date(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()).unwrap();
        assert_eq!(feb_events.len(), 0);

        // March 31 should have it
        let mar_events = store.events_for_date(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap()).unwrap();
        assert_eq!(mar_events.len(), 1);
    }

    #[test]
    fn test_one_year_expansion_cap() {
        let store = EventStore::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        store.add_event_with_recurrence("Daily", start, None, None, None, Recurrence::Daily).unwrap();

        // 365 days later — within cap
        let within = start + chrono::Duration::days(365);
        let events = store.events_for_date(within).unwrap();
        assert_eq!(events.len(), 1);

        // 366 days later — beyond cap
        let beyond = start + chrono::Duration::days(366);
        let events = store.events_for_date(beyond).unwrap();
        assert_eq!(events.len(), 0);
    }

    // Modal key handling integration test
    #[test]
    fn test_modal_key_esc_closes() {
        let mut app = make_test_app();
        app.open_create_modal();
        assert!(app.modal.is_some());

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_modal_key(esc);
        assert!(app.modal.is_none(), "Esc should close modal");
    }

    #[test]
    fn test_modal_key_char_inserts() {
        let mut app = make_test_app();
        app.open_create_modal();

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        app.handle_modal_key(key);

        let modal = app.modal.as_ref().unwrap();
        assert_eq!(modal.fields[0].input.value(), "A");
    }

    #[test]
    fn test_modal_key_tab_cycles_fields() {
        let mut app = make_test_app();
        app.open_create_modal();

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_modal_key(tab);

        let modal = app.modal.as_ref().unwrap();
        assert_eq!(modal.focused_field, 1);
    }

    #[test]
    fn test_modal_save_with_validation_error() {
        let mut app = make_test_app_with_store();
        app.open_create_modal();

        // Try to save without filling title
        let saved = app.save_modal();
        assert!(!saved, "Save should fail with empty title");
        assert!(app.modal.is_some(), "Modal should stay open on validation error");
        assert!(app.modal.as_ref().unwrap().error_message.is_some());
    }

    // Delete confirmation flow tests
    #[test]
    fn test_delete_confirm_with_y() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        let store = app.event_store.as_ref().unwrap();
        store.add_event("Test Event", date, None, None).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);
        app.confirm_delete = true;

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let y_key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let handled = app.handle_delete_confirm(y_key);

        assert!(handled, "Should handle 'y' key");
        assert!(!app.confirm_delete, "confirm_delete should be false after handling");
        assert_eq!(app.selected_day_events.len(), 0, "Event should be deleted");
    }

    #[test]
    fn test_delete_confirm_with_n() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        let store = app.event_store.as_ref().unwrap();
        store.add_event("Test Event", date, None, None).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);
        app.confirm_delete = true;

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let n_key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let handled = app.handle_delete_confirm(n_key);

        assert!(handled, "Should handle 'n' key");
        assert!(!app.confirm_delete, "confirm_delete should be false after handling");
        assert_eq!(app.selected_day_events.len(), 1, "Event should NOT be deleted");
        assert_eq!(app.status_message, Some("Delete cancelled".to_string()));
    }

    #[test]
    fn test_delete_confirm_with_any_key_cancels() {
        let mut app = make_test_app_with_store();
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        app.selected_date = date;

        let store = app.event_store.as_ref().unwrap();
        store.add_event("Test Event", date, None, None).unwrap();
        app.refresh_selected_day();
        app.selected_event_index = Some(0);
        app.confirm_delete = true;

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let handled = app.handle_delete_confirm(esc_key);

        assert!(handled, "Should handle Esc key");
        assert!(!app.confirm_delete, "confirm_delete should be false after handling");
        assert_eq!(app.selected_day_events.len(), 1, "Event should NOT be deleted");
        assert_eq!(app.status_message, Some("Delete cancelled".to_string()));
    }
}

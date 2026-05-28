use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, FocusedPanel, ViewMode};
use crate::calendar::grid::week_start_date;
use crate::ui::calendar_view::CalendarView;
use crate::ui::clock::{parse_color, ClockWidget};
use crate::ui::event_form;
use crate::ui::event_list::EventListWidget;
use crate::ui::upcoming_events::UpcomingEventsWidget;
use crate::ui::weekly_view::WeeklyView;

/// Computed layout rectangles for the main panels
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRects {
    pub calendar: Rect,
    pub event_list: Rect,
    pub upcoming: Rect,
}

/// Compute layout rectangles from the full terminal area
pub fn compute_layout(area: Rect) -> LayoutRects {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Top bar
            Constraint::Min(10),    // Content
            Constraint::Length(1),  // Bottom bar
        ])
        .split(area);

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(65),
            Constraint::Percentage(35),
        ])
        .split(main_chunks[1]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(content_chunks[1]);

    LayoutRects {
        calendar: content_chunks[0],
        event_list: right_chunks[0],
        upcoming: right_chunks[1],
    }
}

/// Main UI layout:
/// - Top bar: clock (time + date + timezone)
/// - Center: calendar grid (left) + event list (right)
/// - Bottom bar: keybindings hint
pub fn draw_ui(f: &mut Frame, app: &App) {
    let surface = parse_color(&app.config.theme.surface);
    let muted = parse_color(&app.config.theme.muted);

    // Main vertical layout: top bar | content | bottom bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Top bar (clock)
            Constraint::Min(10),    // Content area
            Constraint::Length(1),  // Bottom bar (keybindings)
        ])
        .split(f.area());

    // Top bar: clock widget
    let clock = ClockWidget::new(app.now, &app.config);
    f.render_widget(clock, main_chunks[0]);

    // Content area: calendar (left) + events (right)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(65),  // Calendar
            Constraint::Percentage(35),  // Events
        ])
        .split(main_chunks[1]);

    // Content area: conditional calendar view
    let calendar_focused = app.focused_panel == FocusedPanel::Calendar;
    match app.view_mode {
        ViewMode::Monthly => {
            let calendar = CalendarView::new(
                app.view_year,
                app.view_month,
                app.today,
                app.selected_date,
                &app.holidays,
                &app.event_dates,
                &app.config,
            ).with_focused(calendar_focused);
            f.render_widget(calendar, content_chunks[0]);
        }
        ViewMode::Weekly => {
            let ws = week_start_date(app.selected_date, app.config.first_day_of_week);
            let weekly = WeeklyView::new(
                ws,
                app.selected_date,
                app.today,
                &app.holidays,
                &app.week_events,
                &app.config,
            ).with_focused(calendar_focused);
            f.render_widget(weekly, content_chunks[0]);
        }
    }

    // Right panel: split vertically into day events + upcoming events
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60),  // Day events
            Constraint::Percentage(40),  // Upcoming events
        ])
        .split(content_chunks[1]);

    // Day events (with selection highlight)
    let date_label = app.selected_date.format("%b %d, %Y").to_string();
    let event_list = EventListWidget::new(
        &app.selected_day_events,
        date_label,
        &app.config,
    )
    .with_selected_index(app.selected_event_index)
    .with_focused(app.focused_panel == FocusedPanel::EventList);
    f.render_widget(event_list, right_chunks[0]);

    // Upcoming events
    let upcoming = UpcomingEventsWidget::new(&app.upcoming_events, &app.config)
        .with_focused(app.focused_panel == FocusedPanel::Upcoming)
        .with_selected_index(app.selected_upcoming_index);
    f.render_widget(upcoming, right_chunks[1]);

    // Bottom bar: split into status (left) + keybindings (right)
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Status message
            Constraint::Percentage(60),  // Keybindings
        ])
        .split(main_chunks[2]);

    // Status message (left side)
    if let Some(ref msg) = app.status_message {
        let status_line = Line::from(Span::styled(
            format!(" {} ", msg),
            Style::default().fg(parse_color(&app.config.theme.accent)),
        ));
        f.render_widget(Paragraph::new(status_line), bottom_chunks[0]);
    }

    // Keybindings (right side)
    let nav_label = match app.view_mode {
        ViewMode::Monthly => " month ",
        ViewMode::Weekly => " week ",
    };
    let keybindings = Line::from(vec![
        Span::styled(" 1 ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" cal ", Style::default().fg(muted)),
        Span::styled(" 2 ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" events ", Style::default().fg(muted)),
        Span::styled(" 3 ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" upcoming ", Style::default().fg(muted)),
        Span::styled(" ← → ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(nav_label, Style::default().fg(muted)),
        Span::styled(" ↑ ↓ ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" nav ", Style::default().fg(muted)),
        Span::styled(" t ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" today ", Style::default().fg(muted)),
        Span::styled(" n ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" new ", Style::default().fg(muted)),
        Span::styled(" e ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" edit ", Style::default().fg(muted)),
        Span::styled(" d ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" del ", Style::default().fg(muted)),
        Span::styled(" v ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" view ", Style::default().fg(muted)),
        Span::styled(" q ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" quit ", Style::default().fg(muted)),
    ]);
    f.render_widget(
        Paragraph::new(keybindings),
        bottom_chunks[1],
    );

    // Modal overlay
    if let Some(ref modal) = app.modal {
        let area = f.area();
        f.render_widget(ratatui::widgets::Clear, area);
        // We need to render into the buffer directly since we're in a Frame context
        // Use a buffer-based approach
        let mut buf = ratatui::buffer::Buffer::empty(area);
        event_form::render_modal(modal, &app.config, area, &mut buf);
        // Merge the modal buffer into the frame
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell((x, y)) {
                    if let Some(c) = f.buffer_mut().cell_mut((x, y)) {
                        *c = cell.clone();
                    }
                }
            }
        }
    }

    // Delete confirmation overlay
    if app.confirm_delete {
        let area = f.area();
        let mut buf = ratatui::buffer::Buffer::empty(area);
        event_form::render_delete_confirm(area, &mut buf, &app.config);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell((x, y)) {
                    if let Some(c) = f.buffer_mut().cell_mut((x, y)) {
                        *c = cell.clone();
                    }
                }
            }
        }
    }
}

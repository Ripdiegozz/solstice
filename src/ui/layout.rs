use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, ViewMode};
use crate::calendar::grid::week_start_date;
use crate::ui::calendar_view::CalendarView;
use crate::ui::clock::{parse_color, ClockWidget};
use crate::ui::event_form;
use crate::ui::event_list::EventListWidget;
use crate::ui::upcoming_events::UpcomingEventsWidget;
use crate::ui::weekly_view::WeeklyView;

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
            );
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
            );
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
    ).with_selected_index(app.selected_event_index);
    f.render_widget(event_list, right_chunks[0]);

    // Upcoming events (purely informational)
    let upcoming = UpcomingEventsWidget::new(&app.upcoming_events, &app.config);
    f.render_widget(upcoming, right_chunks[1]);

    // Bottom bar: keybindings
    let nav_label = match app.view_mode {
        ViewMode::Monthly => " month ",
        ViewMode::Weekly => " week ",
    };
    let keybindings = Line::from(vec![
        Span::styled(" ← → ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(nav_label, Style::default().fg(muted)),
        Span::styled(" ↑ ↓ ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" day ", Style::default().fg(muted)),
        Span::styled(" t ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" today ", Style::default().fg(muted)),
        Span::styled(" n ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" new ", Style::default().fg(muted)),
        Span::styled(" e ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" edit ", Style::default().fg(muted)),
        Span::styled(" d ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" delete ", Style::default().fg(muted)),
        Span::styled(" v ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" view ", Style::default().fg(muted)),
        Span::styled(" q ", Style::default().fg(surface).bg(parse_color(&app.config.theme.accent))),
        Span::styled(" quit ", Style::default().fg(muted)),
    ]);
    f.render_widget(
        Paragraph::new(keybindings),
        main_chunks[2],
    );

    // Status message (if any)
    if let Some(ref msg) = app.status_message {
        let status_area = main_chunks[2];
        let status_line = Line::from(Span::styled(
            format!(" {} ", msg),
            Style::default().fg(parse_color(&app.config.theme.accent)),
        ));
        f.render_widget(Paragraph::new(status_line), status_area);
    }

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

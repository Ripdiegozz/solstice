use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Widget,
};

use crate::app::{App, FocusedPanel, ViewMode};
use crate::calendar::grid::week_start_date;
use crate::ui::calendar_view::CalendarView;
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

/// Render the single-line tabbed header (tabs left, compact clock right)
fn render_header(area: Rect, buf: &mut ratatui::buffer::Buffer, app: &App) {
    use ratatui::{
        style::{Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };

    let accent = crate::ui::clock::parse_color(&app.config.theme.accent);
    let muted = crate::ui::clock::parse_color(&app.config.theme.muted);
    let text_color = crate::ui::clock::parse_color(&app.config.theme.text);

    // Build tab labels with active tab highlighted
    let tabs = [
        ("Calendar", FocusedPanel::Calendar),
        ("Events", FocusedPanel::EventList),
        ("Upcoming", FocusedPanel::Upcoming),
    ];

    let mut spans: Vec<Span> = Vec::new();
    for (i, (label, panel)) in tabs.iter().enumerate() {
        let num = i + 1;
        let style = if app.focused_panel == *panel {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        spans.push(Span::styled(format!("[{}]", num), style));
        spans.push(Span::styled(format!("-{}", label), style));
    }

    // Compact clock on the right
    let time_str = app.now.format(&app.config.time_format).to_string();

    let line = Line::from(spans);

    // Render tabs left-aligned
    Paragraph::new(line).render(
        Rect::new(area.x, area.y, area.width.saturating_sub(time_str.len() as u16), area.height),
        buf,
    );

    // Render clock right-aligned via simple right-justification
    let right_x = area.x + area.width.saturating_sub(time_str.len() as u16);
    buf.set_string(right_x, area.y, &time_str, Style::default().fg(text_color));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect};
    use crate::app::App;
    use crate::config::Config;

    fn make_header_test_app() -> App {
        App::new(Config::default()).expect("should create test app")
    }

    #[test]
    fn test_header_renders_one_line_with_tabs() {
        let app = make_header_test_app();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        render_header(area, &mut buf, &app);

        // Calendar tab [1] should be visible
        let first_line = buf.content()
            .iter()
            .filter(|c| c.symbol() == "[")
            .count();
        assert!(first_line > 0, "tab brackets should be visible");
    }

    #[test]
    fn test_footer_renders_hint_on_right() {
        let app = make_header_test_app();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        render_footer(area, &mut buf, &app);

        // Should contain "ctrl+p" somewhere
        let content: String = buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("ctrl+p"), "footer should contain ctrl+p hint: '{}'", content);
    }

    // T13: Integration tests for layout rendering
    #[test]
    fn test_header_is_one_line() {
        let app = make_header_test_app();
        let area = Rect::new(0, 0, 80, 3); // 3-line area to verify only 1 line used
        let mut buf = Buffer::empty(area);

        render_header(Rect::new(0, 0, 80, 1), &mut buf, &app);

        // Line 0 should have content (tabs or clock)
        let line0_has_content = (0..80).any(|x| buf.cell((x, 0)).map_or(false, |c| c.symbol() != " "));
        // Lines 1 and 2 should be empty
        let lines_below_empty = (1..3).all(|y| {
            (0..80).all(|x| buf.cell((x, y)).map_or(true, |c| c.symbol() == " "))
        });

        assert!(line0_has_content, "header line 0 should have content");
        assert!(lines_below_empty, "header should be exactly 1 line, lines 1-2 empty");
    }

    #[test]
    fn test_footer_is_one_line() {
        let app = make_header_test_app();
        let area = Rect::new(0, 0, 80, 3);
        let mut buf = Buffer::empty(area);

        render_footer(Rect::new(0, 0, 80, 1), &mut buf, &app);

        // Line 0 should have the hint
        let line0_has_content = (0..80).any(|x| buf.cell((x, 0)).map_or(false, |c| c.symbol() != " "));
        let lines_below_empty = (1..3).all(|y| {
            (0..80).all(|x| buf.cell((x, y)).map_or(true, |c| c.symbol() == " "))
        });

        assert!(line0_has_content, "footer line 0 should have content");
        assert!(lines_below_empty, "footer should be exactly 1 line");
    }

    #[test]
    fn test_compute_layout_header_is_one_line() {
        let area = Rect::new(0, 0, 80, 24);
        let rects = compute_layout(area);

        // Calendar starts at y=1 (header is y=0, 1 line)
        assert_eq!(rects.calendar.y, 1, "calendar should start after 1-line header");
    }
}

/// Render the single-line footer (status left, hint right)
fn render_footer(area: Rect, buf: &mut ratatui::buffer::Buffer, app: &App) {
    use ratatui::{
        style::Style,
        text::{Line, Span},
        widgets::Paragraph,
    };

    let accent = crate::ui::clock::parse_color(&app.config.theme.accent);
    let muted = crate::ui::clock::parse_color(&app.config.theme.muted);

    // Left side: status message
    if let Some(ref msg) = app.status_message {
        let status_line = Line::from(Span::styled(
            format!(" {} ", msg),
            Style::default().fg(accent),
        ));
        let left_width = (area.width as f32 * 0.4) as u16;
        Paragraph::new(status_line).render(
            Rect::new(area.x, area.y, left_width, area.height),
            buf,
        );
    }

    // Right side: hint
    let hint = "ctrl+p: commands";
    let right_x = area.x + area.width.saturating_sub(hint.len() as u16);
    buf.set_string(right_x, area.y, hint, Style::default().fg(muted));
}

/// Compute layout rectangles from the full terminal area
pub fn compute_layout(area: Rect) -> LayoutRects {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Top bar (tabbed header)
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

    // Main vertical layout: top bar | content | bottom bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Top bar (tabbed header + compact clock)
            Constraint::Min(10),    // Content area
            Constraint::Length(1),  // Bottom bar (status + hint)
        ])
        .split(f.area());

    // Top bar: tabbed header with compact clock
    render_header(main_chunks[0], f.buffer_mut(), app);

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

    // Bottom bar: status (left) + hint (right)
    render_footer(main_chunks[2], f.buffer_mut(), app);

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

    // Command palette overlay
    if app.command_palette_open {
        let area = f.area();
        let mut buf = ratatui::buffer::Buffer::empty(area);
        event_form::render_command_palette(area, &mut buf, &app.config);
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

    // Event detail overlay
    if let Some(ref event) = app.event_detail {
        let area = f.area();
        let mut buf = ratatui::buffer::Buffer::empty(area);
        event_form::render_event_detail(event, area, &mut buf, &app.config);
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

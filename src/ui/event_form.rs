use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::app::ModalState;
use crate::ui::text_input::TextInputWidget;
use crate::config::Config;

/// Hit-test a modal: return the field index under (x, y), or None if outside
pub fn hit_test_modal(x: u16, y: u16, area: Rect, modal: &ModalState) -> Option<usize> {
    let width = (area.width as f32 * 0.6) as u16;
    let height = (area.height as f32 * 0.7) as u16;
    let modal_x = area.x + (area.width.saturating_sub(width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, width, height);

    if x < modal_area.x || x >= modal_area.x + modal_area.width
        || y < modal_area.y || y >= modal_area.y + modal_area.height
    {
        return None;
    }

    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(modal_area);

    if inner.height < 4 || inner.width < 10 {
        return None;
    }

    let field_height = 3u16;
    let footer_height = 3u16;
    let available = inner.height.saturating_sub(footer_height);

    for (i, _field) in modal.fields.iter().enumerate() {
        let field_y = inner.y + (i as u16 * field_height);
        if field_y + field_height > inner.y + available {
            break;
        }
        if y >= field_y && y < field_y + field_height {
            return Some(i);
        }
    }

    Some(modal.focused_field)
}

/// Render the modal overlay centered on the screen
pub fn render_modal(modal: &ModalState, _config: &Config, area: Rect, buf: &mut Buffer) {
    // Calculate centered overlay dimensions (60% width × 70% height)
    let width = (area.width as f32 * 0.6) as u16;
    let height = (area.height as f32 * 0.7) as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    // Clear the area behind the modal
    Clear.render(modal_area, buf);

    // Modal container
    let title = match modal.mode {
        crate::app::ModalMode::Create => " New Event ",
        crate::app::ModalMode::Edit => " Edit Event ",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(modal_area);
    block.render(modal_area, buf);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    // Render fields
    let field_count = modal.fields.len();
    let field_height = 3u16; // Each field: label + input + spacing
    let _total_field_height = field_count as u16 * field_height;

    // Reserve space for error message and footer
    let footer_height = 3u16;
    let available = inner.height.saturating_sub(footer_height);

    for (i, field) in modal.fields.iter().enumerate() {
        let field_y = inner.y + (i as u16 * field_height);
        if field_y + field_height > inner.y + available {
            break;
        }

        let focused = i == modal.focused_field;

        // Label
        let label_style = if focused {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        buf.set_string(inner.x, field_y, field.label, label_style);

        // Input field
        let input_area = Rect::new(inner.x, field_y + 1, inner.width, 1);
        let widget = TextInputWidget {
            input: &field.input,
            label: "",
            focused,
        };
        widget.render(input_area, buf);
    }

    // Error message
    if let Some(ref err) = modal.error_message {
        let err_y = inner.y + inner.height.saturating_sub(footer_height);
        let err_area = Rect::new(inner.x, err_y, inner.width, 1);
        Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )))
        .render(err_area, buf);
    }

    // Footer hints
    let footer_y = inner.y + inner.height.saturating_sub(1);
    let footer_area = Rect::new(inner.x, footer_y, inner.width, 1);
    let hints = Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(" navigate ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(" save ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(" cancel ", Style::default().fg(Color::DarkGray)),
    ]);
    Paragraph::new(hints).render(footer_area, buf);
}

/// Render delete confirmation prompt
pub fn render_delete_confirm(area: Rect, buf: &mut Buffer, _config: &Config) {
    let width = 40u16.min(area.width.saturating_sub(4));
    let height = 5u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let confirm_area = Rect::new(x, y, width, height);

    Clear.render(confirm_area, buf);

    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(confirm_area);
    block.render(confirm_area, buf);

    if inner.height >= 2 && inner.width >= 10 {
        let msg = Line::from(vec![
            Span::styled("Delete this event? ", Style::default().fg(Color::White)),
            Span::styled("[y]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled("es / ", Style::default().fg(Color::White)),
            Span::styled("[n]", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled("o", Style::default().fg(Color::White)),
        ]);
        Paragraph::new(msg).render(
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
            buf,
        );
    }
}

/// Render the command palette overlay (centered, 60%×40%)
pub fn render_command_palette(area: Rect, buf: &mut Buffer, _config: &Config) {
    let width = (area.width as f32 * 0.6) as u16;
    let height = (area.height as f32 * 0.4) as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let palette_area = Rect::new(x, y, width, height);

    Clear.render(palette_area, buf);

    let block = Block::default()
        .title(" Commands ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(palette_area);
    block.render(palette_area, buf);

    if inner.height < 2 || inner.width < 5 {
        return;
    }

    // List of commands
    let commands = [
        ("n", "New event"),
        ("e", "Edit event"),
        ("d", "Delete event"),
        ("v", "Toggle month/week view"),
        ("t", "Go to today"),
        ("1/2/3", "Switch panel"),
        ("Tab", "Cycle focus"),
        ("hjkl/arrows", "Navigate"),
        ("Ctrl+P", "Toggle palette"),
        ("q", "Quit"),
    ];

    let mut y_pos = inner.y;
    for (key, desc) in &commands {
        if y_pos >= inner.y + inner.height {
            break;
        }
        let line = Line::from(vec![
            Span::styled(format!(" {} ", key), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(*desc, Style::default().fg(Color::White)),
        ]);
        Paragraph::new(line).render(
            Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
            buf,
        );
        y_pos += 1;
    }

    // Footer hint
    if y_pos < inner.y + inner.height {
        let hint = Line::from(Span::styled(
            "Esc, Ctrl+P, or q to close",
            Style::default().fg(Color::DarkGray),
        ));
        Paragraph::new(hint).render(
            Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
            buf,
        );
    }
}

/// Render the event detail overlay (centered, 50% width, auto-height)
pub fn render_event_detail(event: &crate::events::Event, area: Rect, buf: &mut Buffer, _config: &Config) {
    let width = (area.width as f32 * 0.5) as u16;
    // Calculate height based on content: title + date + time + desc + recurrence + padding
    let has_desc = event.description.as_ref().is_some_and(|d| !d.is_empty());
    let desc_lines = if has_desc { 2 } else { 0 };
    let height = (7 + desc_lines).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let detail_area = Rect::new(x, y, width, height);

    Clear.render(detail_area, buf);

    let block = Block::default()
        .title(" Event Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(detail_area);
    block.render(detail_area, buf);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let mut y_pos = inner.y;

    // Title
    let title_line = Line::from(vec![
        Span::styled("Title: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&event.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    Paragraph::new(title_line).render(
        Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
        buf,
    );
    y_pos += 1;

    // Date
    let date_str = event.date.format("%A, %B %d, %Y").to_string();
    let date_line = Line::from(vec![
        Span::styled("Date: ", Style::default().fg(Color::DarkGray)),
        Span::styled(date_str, Style::default().fg(Color::White)),
    ]);
    Paragraph::new(date_line).render(
        Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
        buf,
    );
    y_pos += 1;

    // Time
    let time_str = match (&event.start_time, &event.end_time) {
        (Some(s), Some(e)) => format!("{} - {}", s, e),
        (Some(s), None) => s.clone(),
        (None, Some(e)) => format!("? - {}", e),
        (None, None) => "All day".to_string(),
    };
    let time_line = Line::from(vec![
        Span::styled("Time: ", Style::default().fg(Color::DarkGray)),
        Span::styled(time_str, Style::default().fg(Color::White)),
    ]);
    Paragraph::new(time_line).render(
        Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
        buf,
    );
    y_pos += 1;

    // Recurrence
    let rec_str = match event.recurrence {
        crate::events::Recurrence::None => "None".to_string(),
        crate::events::Recurrence::Daily => "Daily".to_string(),
        crate::events::Recurrence::Weekly => "Weekly".to_string(),
        crate::events::Recurrence::Monthly => "Monthly".to_string(),
    };
    let rec_line = Line::from(vec![
        Span::styled("Recurrence: ", Style::default().fg(Color::DarkGray)),
        Span::styled(rec_str, Style::default().fg(Color::White)),
    ]);
    Paragraph::new(rec_line).render(
        Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
        buf,
    );
    y_pos += 1;

    // Description (if present)
    if let Some(ref desc) = event.description {
        if !desc.is_empty() {
            let desc_label = Line::from(Span::styled("Description:", Style::default().fg(Color::DarkGray)));
            Paragraph::new(desc_label).render(
                Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
                buf,
            );
            y_pos += 1;

            let desc_text = Line::from(Span::styled(desc.as_str(), Style::default().fg(Color::White)));
            Paragraph::new(desc_text).render(
                Rect::new(inner.x + 2, y_pos, inner.width.saturating_sub(3), 1),
                buf,
            );
            y_pos += 1;
        }
    }

    y_pos += 1;
    // Footer hint
    if y_pos < inner.y + inner.height {
        let hint = Line::from(Span::styled(
            "Esc or Enter to close",
            Style::default().fg(Color::DarkGray),
        ));
        Paragraph::new(hint).render(
            Rect::new(inner.x + 1, y_pos, inner.width.saturating_sub(2), 1),
            buf,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect};
    use crate::config::Config;

    #[test]
    fn test_render_command_palette_has_border_and_text() {
        let config = Config::default();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        render_command_palette(area, &mut buf, &config);

        let content: String = buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("Commands"), "should have title 'Commands'");
    }

    #[test]
    fn test_render_command_palette_shows_commands() {
        let config = Config::default();
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);

        render_command_palette(area, &mut buf, &config);

        let content: String = buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("New event"), "should list 'New event' command");
        assert!(content.contains("Toggle palette"), "should list 'Toggle palette' command");
    }

    #[test]
    fn test_render_event_detail_shows_fields() {
        let config = Config::default();
        let event = crate::events::Event {
            id: 1,
            title: "Team Meeting".to_string(),
            description: Some("Weekly sync".to_string()),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            start_time: Some("10:00".to_string()),
            end_time: Some("11:00".to_string()),
            source: crate::events::EventSource::Local,
            recurrence: crate::events::Recurrence::Weekly,
        };
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        render_event_detail(&event, area, &mut buf, &config);

        let content: String = buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("Team Meeting"), "should show event title");
        assert!(content.contains("June"), "should show date month in rendered output");
    }

    #[test]
    fn test_render_event_detail_shows_time_and_recurrence() {
        let config = Config::default();
        let event = crate::events::Event {
            id: 2,
            title: "Lunch".to_string(),
            description: None,
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            start_time: Some("12:00".to_string()),
            end_time: Some("13:00".to_string()),
            source: crate::events::EventSource::Local,
            recurrence: crate::events::Recurrence::Daily,
        };
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        render_event_detail(&event, area, &mut buf, &config);

        let content: String = buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("12:00 - 13:00"), "should show time range");
        assert!(content.contains("Daily"), "should show recurrence");
    }
}

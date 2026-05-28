use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::app::ModalState;
use crate::ui::clock::parse_color;
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
pub fn render_modal(modal: &ModalState, config: &Config, area: Rect, buf: &mut Buffer) {
    let accent = parse_color(&config.theme.accent);
    let text_color = parse_color(&config.theme.text);
    let surface = parse_color(&config.theme.surface);
    let base = parse_color(&config.theme.base);
    let muted = parse_color(&config.theme.muted);

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
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(base));

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
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        buf.set_string(inner.x, field_y, field.label, label_style);

        // Input field
        let input_area = Rect::new(inner.x, field_y + 1, inner.width, 1);
        let widget = TextInputWidget {
            input: &field.input,
            label: "",
            focused,
            accent_color: accent,
            text_color,
            surface_color: surface,
        };
        widget.render(input_area, buf);
    }

    // Error message
    if let Some(ref err) = modal.error_message {
        let err_y = inner.y + inner.height.saturating_sub(footer_height);
        let err_area = Rect::new(inner.x, err_y, inner.width, 1);
        Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(parse_color("#f38ba8")),
        )))
        .render(err_area, buf);
    }

    // Footer hints
    let footer_y = inner.y + inner.height.saturating_sub(1);
    let footer_area = Rect::new(inner.x, footer_y, inner.width, 1);
    let hints = Line::from(vec![
        Span::styled("Tab", Style::default().fg(surface).bg(accent)),
        Span::styled(" navigate ", Style::default().fg(muted)),
        Span::styled("Enter", Style::default().fg(surface).bg(accent)),
        Span::styled(" save ", Style::default().fg(muted)),
        Span::styled("Esc", Style::default().fg(surface).bg(accent)),
        Span::styled(" cancel ", Style::default().fg(muted)),
    ]);
    Paragraph::new(hints).render(footer_area, buf);
}

/// Render delete confirmation prompt
pub fn render_delete_confirm(area: Rect, buf: &mut Buffer, config: &Config) {
    let accent = parse_color(&config.theme.accent);
    let text_color = parse_color(&config.theme.text);
    let base = parse_color(&config.theme.base);

    let width = 40u16.min(area.width.saturating_sub(4));
    let height = 5u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let confirm_area = Rect::new(x, y, width, height);

    Clear.render(confirm_area, buf);

    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(base));

    let inner = block.inner(confirm_area);
    block.render(confirm_area, buf);

    if inner.height >= 2 && inner.width >= 10 {
        let msg = Line::from(vec![
            Span::styled("Delete this event? ", Style::default().fg(text_color)),
            Span::styled("[y]", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled("es / ", Style::default().fg(text_color)),
            Span::styled("[n]", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled("o", Style::default().fg(text_color)),
        ]);
        Paragraph::new(msg).render(
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
            buf,
        );
    }
}

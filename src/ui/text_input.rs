use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// A minimal text input widget with cursor and character editing
#[derive(Debug, Clone)]
pub struct TextInput {
    pub buffer: String,
    pub cursor_pos: usize,
    pub max_length: usize,
}

impl TextInput {
    pub fn new(max_length: usize) -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
            max_length,
        }
    }

    pub fn with_value(value: &str, max_length: usize) -> Self {
        let len = value.len();
        Self {
            buffer: value.to_string(),
            cursor_pos: len,
            max_length,
        }
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        if self.buffer.len() >= self.max_length {
            return;
        }
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Delete the character before the cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        // Find the previous character boundary
        let prev = self.buffer[..self.cursor_pos]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.buffer.drain(prev..self.cursor_pos);
        self.cursor_pos = prev;
    }

    /// Move cursor one character left
    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let prev = self.buffer[..self.cursor_pos]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.cursor_pos = prev;
    }

    /// Move cursor one character right
    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos >= self.buffer.len() {
            return;
        }
        let next = self.buffer[self.cursor_pos..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor_pos + i)
            .unwrap_or(self.buffer.len());
        self.cursor_pos = next;
    }

    /// Move cursor to the beginning
    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to the end
    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.buffer.len();
    }

    /// Get the buffer content
    pub fn value(&self) -> &str {
        &self.buffer
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_pos = 0;
    }
}

/// Render a TextInput as a ratatui widget
pub struct TextInputWidget<'a> {
    pub input: &'a TextInput,
    pub label: &'a str,
    pub focused: bool,
    pub accent_color: ratatui::style::Color,
    pub text_color: ratatui::style::Color,
    pub surface_color: ratatui::style::Color,
}

impl<'a> Widget for TextInputWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(self.accent_color)
        } else {
            Style::default().fg(self.surface_color)
        };

        let block = Block::default()
            .title(format!(" {} ", self.label))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let text = &self.input.buffer;
        let display_text = if text.len() > inner.width as usize {
            // Truncate if too long
            &text[..inner.width as usize]
        } else {
            text
        };

        if self.focused {
            // Render with cursor indicator
            let before_cursor = &self.input.buffer[..self.input.cursor_pos.min(display_text.len())];
            let cursor_char = if self.input.cursor_pos < self.input.buffer.len() {
                self.input.buffer[self.input.cursor_pos..].chars().next().unwrap_or(' ')
            } else {
                ' '
            };
            let after_cursor = if self.input.cursor_pos < self.input.buffer.len() {
                let next = self.input.buffer[self.input.cursor_pos..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| self.input.cursor_pos + i)
                    .unwrap_or(self.input.buffer.len());
                &self.input.buffer[next..]
            } else {
                ""
            };

            let mut spans = vec![
                Span::styled(before_cursor, Style::default().fg(self.text_color)),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .fg(self.accent_color)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
                ),
                Span::styled(after_cursor, Style::default().fg(self.text_color)),
            ];

            // Pad remaining width
            let used = before_cursor.len() + 1 + after_cursor.len();
            if used < inner.width as usize {
                spans.push(Span::raw(" ".repeat(inner.width as usize - used)));
            }

            Paragraph::new(Line::from(spans)).render(inner, buf);
        } else {
            // Unfocused: just render the text
            Paragraph::new(Line::from(Span::styled(
                display_text,
                Style::default().fg(self.text_color),
            )))
            .render(inner, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Task 2.1: insert_char tests
    #[test]
    fn test_insert_at_end() {
        let mut input = TextInput::with_value("hello", 100);
        input.insert_char('!');
        assert_eq!(input.buffer, "hello!");
        assert_eq!(input.cursor_pos, 6);
    }

    #[test]
    fn test_insert_in_middle() {
        let mut input = TextInput::with_value("hllo", 100);
        input.cursor_pos = 1;
        input.insert_char('e');
        assert_eq!(input.buffer, "hello");
        assert_eq!(input.cursor_pos, 2);
    }

    #[test]
    fn test_insert_respects_max_length() {
        let mut input = TextInput::with_value("abc", 3);
        input.insert_char('d');
        assert_eq!(input.buffer, "abc");
        assert_eq!(input.cursor_pos, 3);
    }

    // Task 2.2: delete_backward tests
    #[test]
    fn test_backspace_at_end() {
        let mut input = TextInput::with_value("hello", 100);
        input.delete_backward();
        assert_eq!(input.buffer, "hell");
        assert_eq!(input.cursor_pos, 4);
    }

    #[test]
    fn test_backspace_at_start_noop() {
        let mut input = TextInput::with_value("hello", 100);
        input.cursor_pos = 0;
        input.delete_backward();
        assert_eq!(input.buffer, "hello");
        assert_eq!(input.cursor_pos, 0);
    }

    #[test]
    fn test_backspace_in_middle() {
        let mut input = TextInput::with_value("hello", 100);
        input.cursor_pos = 3; // after 'l'
        input.delete_backward();
        assert_eq!(input.buffer, "helo");
        assert_eq!(input.cursor_pos, 2);
    }

    // Task 2.3: cursor movement tests
    #[test]
    fn test_cursor_movement() {
        let mut input = TextInput::with_value("hello", 100);
        input.cursor_pos = 3;
        input.move_cursor_left();
        assert_eq!(input.cursor_pos, 2);
        input.move_cursor_left();
        assert_eq!(input.cursor_pos, 1);
        input.move_cursor_right();
        assert_eq!(input.cursor_pos, 2);
    }

    #[test]
    fn test_home_end() {
        let mut input = TextInput::with_value("hello", 100);
        input.cursor_pos = 3;
        input.move_cursor_home();
        assert_eq!(input.cursor_pos, 0);
        input.move_cursor_end();
        assert_eq!(input.cursor_pos, 5);
    }

    #[test]
    fn test_cursor_left_at_start_noop() {
        let mut input = TextInput::with_value("hello", 100);
        input.cursor_pos = 0;
        input.move_cursor_left();
        assert_eq!(input.cursor_pos, 0);
    }

    #[test]
    fn test_cursor_right_at_end_noop() {
        let mut input = TextInput::with_value("hello", 100);
        input.move_cursor_right();
        assert_eq!(input.cursor_pos, 5);
    }

    // Task 2.4: Widget rendering (style assertion)
    #[test]
    fn test_focused_widget_renders_cursor() {
        let input = TextInput::with_value("hello", 100);
        let widget = TextInputWidget {
            input: &input,
            label: "Test",
            focused: true,
            accent_color: ratatui::style::Color::Magenta,
            text_color: ratatui::style::Color::White,
            surface_color: ratatui::style::Color::DarkGray,
        };

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Verify the text is rendered
        let content: String = (0..20)
            .map(|x| buf.cell((x, 1)).map(|c| c.symbol()).unwrap_or(" ").to_string())
            .collect();
        assert!(content.contains("hello"), "Expected 'hello' in rendered content: '{}'", content);
    }

    #[test]
    fn test_unfocused_widget_renders_text() {
        let input = TextInput::with_value("hello", 100);
        let widget = TextInputWidget {
            input: &input,
            label: "Test",
            focused: false,
            accent_color: ratatui::style::Color::Magenta,
            text_color: ratatui::style::Color::White,
            surface_color: ratatui::style::Color::DarkGray,
        };

        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = (0..20)
            .map(|x| buf.cell((x, 1)).map(|c| c.symbol()).unwrap_or(" ").to_string())
            .collect();
        assert!(content.contains("hello"), "Expected 'hello' in rendered content: '{}'", content);
    }
}

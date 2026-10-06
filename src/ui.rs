use std::rc::Rc;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{App, app::Mode, fs::FileEntry};
impl App {
    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    pub fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Max(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

        let top = chunks[0];
        let main_area = chunks[1];
        let status_area = chunks[2];
        let status_chunks = Layout::horizontal([
            Constraint::Max(8),
            Constraint::Min(0),
            Constraint::Length(9),
        ])
        .split(status_area);
        let displayed = self.dir_view.displayed();

        let title = Paragraph::new(self.dir_view.cwd.to_string_lossy());

        let list_item: Vec<ListItem> = displayed
            .iter()
            .map(|p| ListItem::new(format!("{}", p)))
            .collect();

        let list = List::new(list_item).highlight_symbol("> ");

        let mut list_state = ListState::default().with_selected(Some(self.dir_view.get_selected()));

        frame.render_widget(title, top);
        frame.render_stateful_widget(list, main_area, &mut list_state);
        self.make_status_bar(status_chunks, frame)
    }

    fn make_status_bar(&self, area: Rc<[Rect]>, frame: &mut Frame<'_>) {
        let mode_ch = area[0];
        let action_ch = area[1];
        let permissions_ch = area[2];
        let mode = self
            .bot_right_mode()
            .style(Style::new().fg(Color::Black).bg(Color::Gray))
            .alignment(Alignment::Center);

        let curr_file = self.dir_view.get_curr_file();
        let hint = Paragraph::new(self.mode_hint(curr_file.unwrap()))
            .style(Style::new().fg(Color::Yellow).bg(Color::Red))
            .alignment(Alignment::Center);

        let permissions = match curr_file {
            Some(entry) => Paragraph::new(entry.permissions()),
            None => Paragraph::new(""),
        }
        .style(Style::new().fg(Color::White).bg(Color::Red))
        .alignment(Alignment::Right);
        frame.render_widget(mode, mode_ch);
        frame.render_widget(permissions, permissions_ch);
        frame.render_widget(hint, action_ch);
        if let (Mode::Rename { input }, Some(curr_file)) = (&self.mode, curr_file) {
            let prefix = format!("Renaming {}: ", curr_file.name);
            let text = format!("{}{}", prefix, input.text);

            let text_width = text.len() as u16;

            let left_padding = action_ch.width.saturating_sub(text_width) / 2;

            let cursor_x = action_ch.x + left_padding + prefix.len() as u16 + input.cursor as u16;

            frame.set_cursor_position((cursor_x, action_ch.y));
        }
    }

    fn bot_right_mode(&self) -> Paragraph<'_> {
        match self.mode {
            Mode::Browsing | Mode::Rename { .. } | Mode::ConfirmDelete { .. } => {
                Paragraph::new("NORMAL")
            }
            Mode::Visual => Paragraph::new("VISUAL"),
        }
    }
    fn mode_hint(&self, curr_file: &FileEntry) -> String {
        match &self.mode {
            Mode::Browsing => String::new(),
            Mode::ConfirmDelete { permanent } => {
                let first_part = if *permanent {
                    "PERMANENTLY".to_string()
                } else {
                    String::new()
                };
                format!("Delete {} {}: y/n", curr_file.name, first_part)
            }
            Mode::Rename { input: query } => {
                format!("Renaming {}: {}", curr_file.name, query.text)
            }
            Mode::Visual => String::new(),
        }
    }
}

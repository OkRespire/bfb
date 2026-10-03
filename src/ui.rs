use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::App;
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

        let mode_ch = status_chunks[0];
        let action_ch = status_chunks[1];
        let permissions_ch = status_chunks[2];
        let title = Paragraph::new(self.dir_view.cwd.to_string_lossy());

        let mode = Paragraph::new("NORMAL")
            .style(Style::new().fg(Color::Black).bg(Color::Gray))
            .alignment(Alignment::Center);

        let hint = Paragraph::new(self.mode_hint())
            .style(Style::new().fg(Color::White).bg(Color::Red))
            .alignment(Alignment::Center);

        let permissions = match displayed.get(self.dir_view.get_selected()) {
            Some(entry) => Paragraph::new(entry.permissions()),
            None => Paragraph::new(""),
        }
        .style(Style::new().fg(Color::White).bg(Color::Red))
        .alignment(Alignment::Right);
        let list_item: Vec<ListItem> = displayed
            .iter()
            .map(|p| ListItem::new(format!("{}", p)))
            .collect();

        let list = List::new(list_item).highlight_symbol("> ");

        let mut list_state = ListState::default().with_selected(Some(self.dir_view.get_selected()));

        frame.render_widget(title, top);
        frame.render_widget(mode, mode_ch);
        frame.render_widget(permissions, permissions_ch);
        frame.render_widget(hint, action_ch);
        frame.render_stateful_widget(list, main_area, &mut list_state)
    }

    pub(crate) fn mode_hint(&self) -> String {
        "DELETE File.txt y/n".to_string()
    }
}

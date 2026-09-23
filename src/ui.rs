use ratatui::{
    Frame,
    text::Line,
    widgets::{Block, List, ListItem, ListState},
};

use crate::App;
impl App {
    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    pub fn draw(&mut self, frame: &mut Frame) {
        let title = Line::from(self.dir_view.cwd.to_string_lossy().to_string());
        let list_item: Vec<ListItem> = if self.show_hidden {
            self.dir_view
                .displayed(self.show_hidden)
                .iter()
                .map(|p| ListItem::new(format!("{}", p)))
                .collect()
        } else {
            self.dir_view
                .visible
                .iter()
                .map(|p| -> ListItem<'_> { ListItem::new(format!("{}", p)) })
                .collect()
        };

        let list = List::new(list_item)
            .block(Block::bordered().title(title))
            .highlight_symbol("> ");
        let mut list_state = ListState::default().with_selected(Some(self.dir_view.get_selected()));
        frame.render_stateful_widget(list, frame.area(), &mut list_state)
    }
}

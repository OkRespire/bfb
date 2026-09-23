use color_eyre::Result;
use std::{env, path::PathBuf};

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt};
use ratatui::{
    DefaultTerminal, Frame,
    text::Line,
    widgets::{Block, List, ListItem, ListState},
};
use tokio::fs::read_dir;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    running: bool,
    show_hidden: bool,
    // Event stream.
    event_stream: EventStream,

    curr_dir: PathBuf,
    visible: Vec<FileEntry>,
    hidden: Vec<FileEntry>,
    len: usize,
    selected: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

impl std::fmt::Display for FileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_dir {
            write!(f, "\u{f07b} ")?
        } else {
            write!(f, "\u{f15b} ")?
        }

        writeln!(f, "{}", self.name)?;

        Ok(())
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.is_dir, other.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => self.name.cmp(&other.name),
        }
    }
}

async fn get_files(cwd: &PathBuf) -> Result<Vec<FileEntry>> {
    let mut entries = read_dir(cwd).await?;
    let mut files: Vec<FileEntry> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().await?.is_dir();
        let path = entry.path();
        let fe = FileEntry { name, is_dir, path };
        files.push(fe)
    }

    Ok(files)
}

fn part_files(mut files: Vec<FileEntry>) -> (Vec<FileEntry>, Vec<FileEntry>) {
    files.sort_by(|a, b| a.cmp(b));
    files.into_iter().partition(|x| x.name.starts_with('.'))
}

impl App {
    /// Construct a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let curr_dir = env::current_dir().unwrap();
        let files = get_files(&curr_dir).await?;
        let (hidden, visible) = part_files(files);
        Ok(Self {
            running: true,
            show_hidden: false,
            event_stream: EventStream::default(),
            curr_dir,
            len: hidden.len() + visible.len(),
            visible,
            hidden,
            selected: 0,
        })
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.running = true;
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_crossterm_events().await?;
        }
        Ok(())
    }
    fn displayed(&self) -> Vec<&FileEntry> {
        if self.show_hidden {
            self.hidden.iter().chain(self.visible.iter()).collect()
        } else {
            self.visible.iter().collect()
        }
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    fn draw(&mut self, frame: &mut Frame) {
        let title = Line::from(self.curr_dir.to_string_lossy().to_string());
        let list_item: Vec<ListItem> = if self.show_hidden {
            self.hidden
                .iter()
                .chain(self.visible.iter())
                .map(|p| ListItem::new(format!("{}", p)))
                .collect()
        } else {
            self.visible
                .iter()
                .map(|p| -> ListItem<'_> { ListItem::new(format!("{}", p)) })
                .collect()
        };

        let list: List = List::new(list_item)
            .block(Block::bordered().title(title))
            .highlight_symbol("> ");
        let mut list_state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(list, frame.area(), &mut list_state)
    }

    /// Reads the crossterm events and updates the state of [`App`].
    async fn handle_crossterm_events(&mut self) -> color_eyre::Result<()> {
        let event = self.event_stream.next().fuse().await;
        match event {
            Some(Ok(evt)) => match evt {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    self.on_key_event(key).await?
                }
                Event::Mouse(_) => {}
                Event::Resize(_, _) => {}
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn set_files(&mut self, files: Vec<FileEntry>) {
        (self.hidden, self.visible) = part_files(files);
    }

    /// Handles the key events and updates the state of [`App`].
    async fn on_key_event(&mut self, key: KeyEvent) -> Result<()> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            (_, KeyCode::Up | KeyCode::Char('k')) => {
                self.selected = self.selected.saturating_sub(1);
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                if self.show_hidden {
                    self.selected = (self.selected + 1).min(self.len - 1);
                } else {
                    self.selected = (self.selected + 1).min(self.visible.len() - 1);
                }
            }
            (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => {
                let idx = &self.selected;
                let files = self.displayed();
                let file = files[*idx].clone();
                if file.is_dir {
                    self.selected = 0;
                    let new_files = get_files(&file.path).await?;
                    self.set_files(new_files);
                    self.curr_dir = file.path;
                }
            }
            (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => {
                if let Some(parent) = &self.curr_dir.parent() {
                    self.selected = 0;
                    let files = get_files(&parent.to_path_buf()).await?;
                    self.curr_dir = parent.to_path_buf();
                    self.set_files(files);
                }
            }
            (_, KeyCode::Char('.')) => self.show_hidden = !self.show_hidden,

            _ => {}
        }
        Ok(())
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

use futures::{FutureExt, StreamExt};
use std::{env, path::PathBuf, process::Stdio};

use color_eyre::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::DefaultTerminal;

use crate::fs::{FileEntry, get_files, part_files};

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    pub show_hidden: bool,
    // Event stream.
    pub event_stream: EventStream,

    pub curr_dir: PathBuf,
    pub visible: Vec<FileEntry>,
    pub hidden: Vec<FileEntry>,
    pub selected: usize,
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
            self.handle_crossterm_events(&mut terminal).await?;
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    pub async fn on_key_event(
        &mut self,
        key: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            (_, KeyCode::Up | KeyCode::Char('k')) => {
                self.selected = self.selected.saturating_sub(1);
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                if self.displayed().is_empty() {
                    return Ok(());
                }
                self.selected = (self.selected + 1).min(self.displayed_len() - 1);
            }
            (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => {
                let idx = &self.selected;
                let files = self.displayed();
                if files.is_empty() {
                    return Ok(());
                }
                let file = files[*idx].clone();
                if file.is_dir {
                    self.selected = 0;
                    let new_files = get_files(&file.path).await?;
                    self.set_files(new_files);
                    self.curr_dir = file.path;
                } else if file.is_text_file() {
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    file.edit_file().await?;
                    enable_raw_mode()?;
                    execute!(terminal.backend_mut(), EnterAlternateScreen,)?;
                    terminal.clear()?;
                } else {
                    tokio::process::Command::new("xdg-open")
                        .arg(&file.path)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                }
            }
            (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => {
                if let Some(parent) = &self.curr_dir.parent() {
                    self.selected = 0;
                    let files = get_files(&parent.to_path_buf()).await?;
                    self.curr_dir = parent.to_path_buf();
                    self.set_files(files);
                } else {
                    return Ok(());
                }
            }
            (_, KeyCode::Char('.')) => self.show_hidden = !self.show_hidden,

            _ => {}
        }
        Ok(())
    }

    /// Reads the crossterm events and updates the state of [`App`].
    pub async fn handle_crossterm_events(
        &mut self,
        terminal: &mut DefaultTerminal,
    ) -> color_eyre::Result<()> {
        let event = self.event_stream.next().fuse().await;
        match event {
            Some(Ok(evt)) => match evt {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    self.on_key_event(key, terminal).await?
                }
                Event::Mouse(_) => {}
                Event::Resize(_, _) => {}
                _ => {}
            },
            _ => {}
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

    fn displayed_len(&self) -> usize {
        self.displayed().len()
    }

    fn set_files(&mut self, files: Vec<FileEntry>) {
        (self.hidden, self.visible) = part_files(files);
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

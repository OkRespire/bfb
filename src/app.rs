use futures::{FutureExt, StreamExt};
use std::{env, path::PathBuf, process::Stdio};

use color_eyre::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::DefaultTerminal;

use crate::fs::{EntryType, FileEntry, get_files, part_files};

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    pub show_hidden: bool,
    // Event stream.
    pub event_stream: EventStream,
    pub dir_view: DirView,
}

#[derive(Debug, Default)]
pub struct DirView {
    pub cwd: PathBuf,
    pub visible: Vec<FileEntry>,
    pub hidden: Vec<FileEntry>,
    pub selected: usize,
}
impl DirView {
    pub async fn new(cwd: PathBuf) -> Result<Self> {
        let files = get_files(&cwd).await?;
        let (hidden, visible) = part_files(files);
        Ok(Self {
            cwd,
            visible,
            hidden,
            selected: 0,
        })
    }

    pub fn increase_sel(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn decrease_sel(&mut self, is_hidden: bool) {
        if self.displayed(is_hidden).is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.displayed_len(is_hidden) - 1);
    }

    pub async fn get_entry_type(&mut self, is_hidden: bool) -> Option<EntryType> {
        let idx = &self.selected;
        let files = self.displayed(is_hidden);
        if files.is_empty() {
            return None;
        }
        let file = files[*idx].clone();
        Some(file.ent_type)
    }

    pub async fn open_dir(&mut self, is_hidden: bool) -> Result<()> {
        let idx = &self.selected;
        let file = self.displayed(is_hidden)[*idx].clone();
        self.selected = 0;
        let new_files = get_files(&file.path).await?;
        self.set_files(new_files);
        self.cwd = file.path;
        Ok(())
    }
    pub async fn open_file(&mut self, is_hidden: bool) -> Result<(FileEntry, bool)> {
        let idx = &self.selected;
        let file = self.displayed(is_hidden)[*idx].clone();
        if file.is_text_file() {
            Ok((file, true))
        } else {
            Ok((file, false))
        }
    }

    pub async fn go_parent(&mut self) -> Result<()> {
        if let Some(parent) = &self.cwd.parent() {
            self.selected = 0;
            let files = get_files(&parent.to_path_buf()).await?;
            self.cwd = parent.to_path_buf();
            self.set_files(files);
            Ok(())
        } else {
            return Ok(());
        }
    }

    fn set_files(&mut self, files: Vec<FileEntry>) {
        (self.hidden, self.visible) = part_files(files);
    }
    pub fn displayed(&self, is_hidden: bool) -> Vec<&FileEntry> {
        if is_hidden {
            self.hidden.iter().chain(self.visible.iter()).collect()
        } else {
            self.visible.iter().collect()
        }
    }

    pub fn get_selected(&self) -> usize {
        self.selected
    }

    fn displayed_len(&self, is_hidden: bool) -> usize {
        self.displayed(is_hidden).len()
    }
}

impl App {
    /// Construct a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let curr_dir = env::current_dir().unwrap();
        let dir_view = DirView::new(curr_dir).await?;
        Ok(Self {
            running: true,
            show_hidden: false,
            event_stream: EventStream::default(),
            dir_view,
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

    pub async fn handle_file(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let (f, is_txt) = self.dir_view.open_file(self.show_hidden).await?;
        if is_txt {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            f.edit_file().await?;
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen,)?;
            terminal.clear()?;
        } else {
            tokio::process::Command::new("xdg-open")
                .arg(&f.path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
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
                self.dir_view.increase_sel();
            }
            (_, KeyCode::Down | KeyCode::Char('j')) => {
                self.dir_view.decrease_sel(self.show_hidden);
            }
            (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => {
                if let Some(e) = self.dir_view.get_entry_type(self.show_hidden).await {
                    match e {
                        EntryType::Directory => {
                            self.dir_view.open_dir(self.show_hidden).await?;
                        }
                        EntryType::File => self.handle_file(terminal).await?,
                        EntryType::Symlink => todo!(),
                    }
                }
            }
            (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => {
                self.dir_view.go_parent().await?;
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

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

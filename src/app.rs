use futures::{FutureExt, StreamExt};
use std::{env, process::Stdio};

use color_eyre::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::DefaultTerminal;

use crate::{dirview::DirView, fs::EntryType};

pub enum Message {
    MoveUp,
    MoveDown,
    Open,
    GoParent,
    ToggleHidden,
    Quit,
}

pub enum Command {
    None,
    OpenSelected,
    GoParent,
}

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    pub show_hidden: bool,
    // Event stream.
    pub event_stream: EventStream,
    pub dir_view: DirView,
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
        let (f, is_txt) = self.dir_view.check_file(self.show_hidden).await?;
        if is_txt {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            let res = f.edit_file().await;
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen,)?;
            terminal.clear()?;
            // allows for the terminal to recover itself
            res?
        } else {
            tokio::process::Command::new("xdg-open")
                .arg(&f.path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        Ok(())
    }

    pub async fn handle_command(
        &mut self,
        command: Command,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match command {
            Command::None => {}
            Command::OpenSelected => {
                if let Some(e) = self.dir_view.get_entry_type(self.show_hidden).await {
                    match e {
                        EntryType::Directory => {
                            self.dir_view.open_dir(self.show_hidden).await?;
                        }
                        EntryType::File => self.handle_file(terminal).await?,
                        EntryType::Symlink { is_dir, .. } => match is_dir {
                            Some(a) => {
                                if a {
                                    self.dir_view.open_dir(self.show_hidden).await?;
                                } else {
                                    self.handle_file(terminal).await?;
                                }
                            }
                            None => return Ok(()),
                        },
                    }
                }
            }
            Command::GoParent => self.dir_view.go_parent().await?,
        }

        Ok(())
    }

    pub fn update(&mut self, action: Option<Message>) -> Command {
        if let Some(a) = action {
            match a {
                Message::MoveUp => {
                    self.dir_view.move_up();
                }
                Message::MoveDown => {
                    self.dir_view.move_down(self.show_hidden);
                }
                Message::Open => return Command::OpenSelected,

                Message::GoParent => return Command::GoParent,
                Message::ToggleHidden => {
                    self.show_hidden = !self.show_hidden;
                }
                Message::Quit => {
                    self.quit();
                }
            }
        }
        Command::None
    }

    /// Handles the key events and updates the state of [`App`].
    pub fn on_key_event(&mut self, key: KeyEvent) -> Option<Message> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                Some(Message::Quit)
            }
            (_, KeyCode::Up | KeyCode::Char('k')) => Some(Message::MoveUp),
            (_, KeyCode::Down | KeyCode::Char('j')) => Some(Message::MoveDown),
            (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => Some(Message::Open),
            (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => Some(Message::GoParent),
            (_, KeyCode::Char('.')) => Some(Message::ToggleHidden),

            _ => None,
        }
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
                    let action = self.on_key_event(key);
                    let command = self.update(action);
                    self.handle_command(command, terminal).await?;
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

use futures::{FutureExt, StreamExt};
use std::{env, process::Stdio};

use color_eyre::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::DefaultTerminal;

use crate::{
    config::Config,
    dirview::DirView,
    fs::{EntryType, FileEntry},
};

#[derive(Debug)]
pub enum Mode {
    Browsing,
    Delete { permanent: bool },
    Rename { input: Input },
    Add { input: Input },
    Visual,
}

#[derive(Debug)]
pub struct Input {
    pub text: String,
    pub cursor: usize,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            text: Default::default(),
            cursor: Default::default(),
        }
    }
}
impl Input {
    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;

            while !self.text.is_char_boundary(self.cursor) {
                self.cursor -= 1;
            }
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;

            while !self.text.is_char_boundary(self.cursor) {
                self.cursor += 1;
            }
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::Browsing
    }
}
pub enum Message {
    MoveUp,
    MoveDown,
    Open,
    GoParent,
    ToggleHidden,
    Quit,
    CancelConfirm,
    RequestDelete { permanent: bool },
    ConfirmDelete,
    RequestRename { curr_file: FileEntry },
    Rename,
    ToggleVisual,
    Add,
    RequestAdd,
}

pub enum Command {
    None,
    OpenSelected,
    GoParent,
    Delete { permanent: bool },
    Rename { input: Input },
    Add { input: Input },
}

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    // Event stream.
    pub event_stream: EventStream,
    pub dir_view: DirView,
    pub config: Config,
    pub mode: Mode,
}

impl App {
    /// Construct a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let curr_dir = env::current_dir().unwrap();
        let dir_view = DirView::new(curr_dir).await?;
        Ok(Self {
            running: true,
            event_stream: EventStream::default(),
            config: Config::default(),
            dir_view,
            mode: Mode::default(),
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
        let (f, is_txt) = self.dir_view.check_file().await?;
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
                if let Some(e) = self.dir_view.get_entry_type().await {
                    match e {
                        EntryType::Directory => {
                            self.dir_view.open_dir().await?;
                        }
                        EntryType::File => self.handle_file(terminal).await?,
                        EntryType::Symlink { is_dir, .. } => match is_dir {
                            Some(a) => {
                                if a {
                                    self.dir_view.open_dir().await?;
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
            Command::Delete { permanent: normal } => {
                self.dir_view.delete(normal).await?;
            }
            Command::Rename { input } => self.dir_view.rename(input.text).await?,
            Command::Add { input } => self.dir_view.add(input.text).await?,
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
                    self.dir_view.move_down();
                }
                Message::Open => return Command::OpenSelected,

                Message::GoParent => return Command::GoParent,
                Message::ToggleHidden => {
                    self.dir_view.is_hidden = !self.dir_view.is_hidden;
                }
                Message::Quit => {
                    self.quit();
                }
                Message::ConfirmDelete => {
                    let Mode::Delete { permanent } =
                        std::mem::replace(&mut self.mode, Mode::Browsing)
                    else {
                        return Command::None;
                    };
                    return Command::Delete { permanent };
                }
                Message::RequestDelete { permanent } => self.mode = Mode::Delete { permanent },
                Message::CancelConfirm => self.mode = Mode::Browsing,
                Message::RequestRename { curr_file } => {
                    let input = Input {
                        text: curr_file.name.clone(),
                        cursor: curr_file.name.len(),
                    };
                    self.mode = Mode::Rename { input }
                }
                Message::ToggleVisual => self.mode = Mode::Visual,
                Message::Rename => {
                    let Mode::Rename { input } = std::mem::replace(&mut self.mode, Mode::Browsing)
                    else {
                        return Command::None;
                    };
                    return Command::Rename { input };
                }
                Message::Add => {
                    let Mode::Add { input } = std::mem::replace(&mut self.mode, Mode::Browsing)
                    else {
                        return Command::None;
                    };
                    return Command::Add { input };
                }
                Message::RequestAdd => {
                    self.mode = Mode::Add {
                        input: Input::default(),
                    }
                }
            }
        }
        Command::None
    }

    /// Handles the key events and updates the state of [`App`].
    pub fn on_key_event(&mut self, key: KeyEvent) -> Option<Message> {
        match &mut self.mode {
            Mode::Browsing => match (key.modifiers, key.code) {
                (_, KeyCode::Char('q'))
                | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                    Some(Message::Quit)
                }
                (_, KeyCode::Up | KeyCode::Char('k')) => Some(Message::MoveUp),
                (_, KeyCode::Down | KeyCode::Char('j')) => Some(Message::MoveDown),
                (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => Some(Message::Open),
                (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => {
                    Some(Message::GoParent)
                }
                (_, KeyCode::Char('d')) => Some(Message::RequestDelete { permanent: false }),
                (_, KeyCode::Char('D')) => Some(Message::RequestDelete { permanent: true }),
                (_, KeyCode::Char('r')) => {
                    let curr_file = self.dir_view.get_curr_file().unwrap();
                    Some(Message::RequestRename {
                        curr_file: curr_file.clone(),
                    })
                }
                (_, KeyCode::Char('.')) => Some(Message::ToggleHidden),
                (_, KeyCode::Char('a')) => Some(Message::RequestAdd),
                (_, KeyCode::Char('v')) => Some(Message::ToggleVisual),

                _ => None,
            },
            Mode::Delete { .. } => match (key.modifiers, key.code) {
                (_, KeyCode::Char('y')) => Some(Message::ConfirmDelete),
                (_, KeyCode::Char('n') | KeyCode::Esc) => Some(Message::CancelConfirm),
                _ => None,
            },
            Mode::Rename { input } => match key.code {
                KeyCode::Char('/') => None,
                KeyCode::Left => {
                    input.move_left();
                    None
                }
                KeyCode::Right => {
                    input.move_right();
                    None
                }
                KeyCode::Char(c) => {
                    input.text.insert(input.cursor, c);
                    input.cursor += c.len_utf8();
                    None
                }
                KeyCode::Backspace => {
                    if input.cursor > 0 {
                        input.move_left();
                        input.text.remove(input.cursor);
                    }
                    None
                }
                KeyCode::Enter => {
                    // fire the actual rename, same two-step
                    // confirm-pattern as delete: Action::ConfirmRename
                    Some(Message::Rename)
                }
                KeyCode::Esc => Some(Message::CancelConfirm),
                _ => None,
            },
            Mode::Visual => match key.code {
                KeyCode::Esc => Some(Message::CancelConfirm),
                _ => None,
            },
            Mode::Add { input } => match key.code {
                KeyCode::Left => {
                    input.move_left();
                    None
                }
                KeyCode::Right => {
                    input.move_right();
                    None
                }
                KeyCode::Char(c) => {
                    input.text.insert(input.cursor, c);
                    input.cursor += c.len_utf8();
                    None
                }
                KeyCode::Backspace => {
                    if input.cursor > 0 {
                        input.move_left();
                        input.text.remove(input.cursor);
                    }
                    None
                }
                KeyCode::Enter => {
                    // fire the actual rename, same two-step
                    // confirm-pattern as delete: Action::ConfirmRename
                    Some(Message::Add)
                }
                KeyCode::Esc => Some(Message::CancelConfirm),
                _ => None,
            },
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

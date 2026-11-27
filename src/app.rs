use futures::{FutureExt, StreamExt};
use std::{env, process::Stdio};

use color_eyre::Result;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal,
    layout::{Position, Rect},
    widgets::{ListState, TableState},
};

use crate::{
    config::Config,
    dirview::DirView,
    error::AppError,
    fs::{EntryType, FileEntry},
};

#[derive(Debug)]
pub enum HitTarget {
    Row(usize),
    OutOfBounds,
    HeaderClick,
    Empty,
}

#[derive(Debug, Default)]
pub struct ContextMenuOptions {
    pub open: bool,
    pub rename: bool,
    pub delete: bool,
    pub p_delete: bool,
    pub copy: bool,
    pub paste: bool,
    pub add: bool,
}
#[derive(Debug, Clone)]
pub enum ContextAction {
    Open,
    Rename,
    Delete,
    PermaDelete,
    Copy,
    Paste,
    Add,
}

impl ContextMenuOptions {
    pub fn visible_options(&self) -> Vec<(ContextAction, &'static str)> {
        let mut options = Vec::new();

        if self.open {
            options.push((ContextAction::Open, "Open"));
        }
        if self.rename {
            options.push((ContextAction::Rename, "Rename"));
        }
        if self.delete {
            options.push((ContextAction::Delete, "Move to Trash"));
        }
        if self.p_delete {
            options.push((ContextAction::PermaDelete, "Delete"));
        }
        if self.copy {
            options.push((ContextAction::Copy, "Copy"));
        }
        if self.paste {
            options.push((ContextAction::Paste, "Paste"));
        }
        if self.add {
            options.push((ContextAction::Add, "New File"));
        }

        options
    }

    fn len(&self) -> usize {
        self.visible_options().len()
    }

    fn is_empty(&self) -> bool {
        self.visible_options().is_empty()
    }
}

#[derive(Debug)]
pub struct MouseTarget {
    pub target: HitTarget,
    pub pos: Position,
}

#[derive(Debug)]
pub struct ContextMenuView {
    pub mt: MouseTarget,
    pub selected: usize,
    pub hover: Option<usize>,
    pub options: ContextMenuOptions,
    pub list_state: ListState,
    pub list_area: Rect,
}

impl ContextMenuView {
    pub fn new(mt: MouseTarget, yanked: Option<FileEntry>) -> Self {
        let paste = yanked.is_some();
        let options = match mt.target {
            HitTarget::Row(_) => ContextMenuOptions {
                open: true,
                rename: true,
                delete: true,
                p_delete: true,
                copy: true,
                add: true,
                paste,
                ..Default::default()
            },
            HitTarget::Empty => ContextMenuOptions {
                paste,
                add: true,
                ..Default::default()
            },
            HitTarget::HeaderClick | HitTarget::OutOfBounds => ContextMenuOptions::default(),
        };

        Self {
            mt,
            selected: 0,
            hover: None,
            options,
            list_state: ListState::default(),
            list_area: Rect::default(),
        }
    }
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);

        tracing::debug!(?self.selected, "selected");
    }

    pub fn move_down(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.options.len() - 1);

        tracing::debug!(?self.selected, "selected");
    }
    pub fn selected_action(&self, curr_file: Option<FileEntry>) -> Option<Message> {
        self.options
            .visible_options()
            .get(self.selected)
            .map(|(action, _)| match action {
                ContextAction::Open => Message::Open,
                ContextAction::Rename => curr_file
                    .map(|file| Message::RequestRename { curr_file: file })
                    .unwrap(),
                ContextAction::Delete => Message::RequestDelete { permanent: false },
                ContextAction::PermaDelete => Message::RequestDelete { permanent: true },
                ContextAction::Copy => Message::Yank,
                ContextAction::Paste => Message::Paste,
                ContextAction::Add => Message::RequestAdd,
            })
    }
}

#[derive(Debug, Default)]
pub enum Mode {
    #[default]
    Browsing,
    Delete {
        permanent: bool,
    },
    Rename {
        input: Input,
    },
    Add {
        input: Input,
    },
    Visual,
    ContextMenu(ContextMenuView),
}

#[derive(Debug, Default)]
pub struct Input {
    pub text: String,
    pub cursor: usize,
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

pub enum Message {
    MoveUp,
    MoveDown,
    Open,
    GoParent,
    ToggleHidden,
    Quit,
    DefaultMode,
    RequestDelete { permanent: bool },
    ConfirmDelete,
    RequestRename { curr_file: FileEntry },
    Rename,
    ToggleVisual,
    Add,
    RequestAdd,
    Yank,
    Paste,
    SelectRow(usize),
    ContextMenu(MouseTarget),
    ChangeHover(HitTarget),
}

pub enum Command {
    None,
    OpenSelected,
    GoParent,
    Delete { permanent: bool },
    Rename { input: Input },
    Add { input: Input },
    Yank,
    Paste,
}

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    // Event stream.
    pub event_stream: EventStream,
    pub dir_view: DirView,
    pub error: Option<AppError>,
    pub config: Config,
    pub mode: Mode,
    pub table_state: TableState,
    pub table_area: Rect,
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
            error: None,
            mode: Mode::default(),
            table_state: TableState::default(),
            table_area: Rect::default(),
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
            let _ = execute!(terminal.backend_mut(), DisableMouseCapture);
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            let res = f.edit_file().await;
            enable_raw_mode()?;
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;

            execute!(terminal.backend_mut(), EnableMouseCapture)?;
            terminal.clear()?;
            // terminal is restored, so now it's safe to report the editor's failure
            if let Err(e) = res {
                self.error = Some(AppError::from_report(e, &f.path));
            }
        } else if let Err(e) = tokio::process::Command::new("xdg-open")
            .arg(&f.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            self.error = Some(AppError::from_io(e, &f.path));
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
                            if let Err(e) = self.dir_view.open_dir().await {
                                self.error = Some(e);
                            };
                        }
                        EntryType::File => {
                            self.handle_file(terminal).await?;
                        }
                        EntryType::Symlink { is_dir, .. } => match is_dir {
                            Some(a) => {
                                if a {
                                    if let Err(e) = self.dir_view.open_dir().await {
                                        self.error = Some(e);
                                    };
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
                if let Err(e) = self.dir_view.delete(normal).await {
                    self.error = Some(e)
                }
            }
            Command::Rename { input } => self.dir_view.rename(input.text).await?,
            Command::Add { input } => {
                if let Err(e) = self.dir_view.add(input.text).await {
                    self.error = Some(e);
                }
            }
            Command::Yank => self.dir_view.copy(),
            Command::Paste => {
                if let Err(e) = self.dir_view.paste().await {
                    self.error = Some(e)
                }
            }
        }

        Ok(())
    }

    pub fn update(&mut self, action: Option<Message>) -> Command {
        if let Some(a) = action {
            match a {
                Message::MoveUp => {
                    if let Mode::ContextMenu(cmv) = &mut self.mode {
                        cmv.move_up();
                    } else {
                        self.dir_view.move_up();
                    }
                }
                Message::MoveDown => {
                    if let Mode::ContextMenu(cmv) = &mut self.mode {
                        cmv.move_down();
                    } else {
                        self.dir_view.move_down();
                    }
                }
                Message::Open => {
                    self.mode = Mode::Browsing;

                    return Command::OpenSelected;
                }

                Message::GoParent => {
                    self.mode = Mode::Browsing;
                    return Command::GoParent;
                }
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
                Message::DefaultMode => self.mode = Mode::Browsing,
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
                Message::Yank => {
                    self.mode = Mode::Browsing;
                    return Command::Yank;
                }
                Message::Paste => {
                    self.mode = Mode::Browsing;
                    return Command::Paste;
                }
                Message::SelectRow(i) => {
                    if let Mode::ContextMenu(cmv) = &mut self.mode {
                        cmv.selected = i
                    } else {
                        self.dir_view.select(i);
                    }
                }
                Message::ContextMenu(mt) => {
                    match mt.target {
                        HitTarget::Row(idx) => {
                            self.dir_view.select(idx);
                        }
                        _ => {}
                    }
                    let cmv = ContextMenuView::new(mt, self.dir_view.yanked.clone());
                    self.mode = Mode::ContextMenu(cmv);
                }
                Message::ChangeHover(ht) => match ht {
                    HitTarget::Row(idx) => {
                        if let Mode::ContextMenu(cmv) = &mut self.mode {
                            cmv.hover = Some(idx);
                        } else {
                            self.dir_view.hovered = Some(idx);
                        }
                    }
                    _ => {
                        if let Mode::ContextMenu(cmv) = &mut self.mode {
                            cmv.hover = None;
                        } else {
                            self.dir_view.hovered = None;
                        }
                    }
                },
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
                    let curr_file = match self.dir_view.get_curr_file() {
                        Some(f) => f,
                        None => return None,
                    };
                    Some(Message::RequestRename {
                        curr_file: curr_file.clone(),
                    })
                }
                (_, KeyCode::Char('.')) => Some(Message::ToggleHidden),
                (_, KeyCode::Char('a')) => Some(Message::RequestAdd),
                (_, KeyCode::Char('v')) => Some(Message::ToggleVisual),
                (_, KeyCode::Char('y')) => Some(Message::Yank),
                (_, KeyCode::Char('p')) => Some(Message::Paste),

                _ => None,
            },
            Mode::Delete { .. } => match (key.modifiers, key.code) {
                (_, KeyCode::Char('y')) => Some(Message::ConfirmDelete),
                (_, KeyCode::Char('n') | KeyCode::Esc) => Some(Message::DefaultMode),
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
                KeyCode::Esc => Some(Message::DefaultMode),
                _ => None,
            },
            Mode::Visual => match key.code {
                KeyCode::Esc => Some(Message::DefaultMode),
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
                KeyCode::Esc => Some(Message::DefaultMode),
                _ => None,
            },
            Mode::ContextMenu(cmv) => match cmv.mt.target {
                HitTarget::Row(_) => match (key.modifiers, key.code) {
                    (_, KeyCode::Up | KeyCode::Char('k')) => Some(Message::MoveUp),
                    (_, KeyCode::Down | KeyCode::Char('j')) => Some(Message::MoveDown),
                    (_, KeyCode::Esc) => Some(Message::DefaultMode),
                    (_, KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right) => {
                        let curr_file = self.dir_view.get_curr_file();
                        cmv.selected_action(curr_file.cloned())
                    }
                    (_, KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left) => {
                        Some(Message::DefaultMode)
                    }
                    _ => None,
                },
                HitTarget::OutOfBounds => todo!(),
                HitTarget::HeaderClick => todo!(),
                HitTarget::Empty => todo!(),
            },
        }
    }

    pub fn on_mouse_event(&self, mouse: MouseEvent) -> Option<Message> {
        match &self.mode {
            Mode::Browsing => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => match self.cursor_loc(mouse).target {
                    HitTarget::Row(idx) => {
                        if idx == self.dir_view.get_selected() {
                            return Some(Message::Open);
                        } else {
                            return Some(Message::SelectRow(idx));
                        }
                    }
                    HitTarget::OutOfBounds => return Some(Message::DefaultMode),
                    HitTarget::HeaderClick => return Some(Message::DefaultMode),
                    HitTarget::Empty => return Some(Message::DefaultMode),
                },
                MouseEventKind::Moved => {
                    let ht = self.cursor_loc(mouse).target;
                    return Some(Message::ChangeHover(ht));
                }

                MouseEventKind::Down(MouseButton::Right) => {
                    let ht = self.cursor_loc(mouse);
                    return Some(Message::ContextMenu(ht));
                }
                MouseEventKind::ScrollUp => return Some(Message::MoveUp),
                MouseEventKind::ScrollDown => return Some(Message::MoveDown),
                _ => return None,
            },
            Mode::ContextMenu(cmv) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => match self.cursor_loc(mouse).target {
                    HitTarget::Row(idx) => {
                        if idx == cmv.selected {
                            let curr_file = self.dir_view.get_curr_file();
                            return cmv.selected_action(curr_file.cloned());
                        } else {
                            return Some(Message::SelectRow(idx));
                        }
                    }
                    HitTarget::OutOfBounds => return Some(Message::DefaultMode),
                    HitTarget::HeaderClick => return Some(Message::DefaultMode),
                    HitTarget::Empty => return Some(Message::DefaultMode),
                },

                MouseEventKind::Down(MouseButton::Right) => {
                    let ht = self.cursor_loc(mouse);
                    return Some(Message::ContextMenu(ht));
                }
                MouseEventKind::Moved => {
                    let ht = self.cursor_loc(mouse).target;
                    return Some(Message::ChangeHover(ht));
                }
                MouseEventKind::ScrollUp => return Some(Message::MoveUp),
                MouseEventKind::ScrollDown => return Some(Message::MoveDown),
                _ => return None,
            },
            _ => return None,
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
                    self.error = None;
                    let message = self.on_key_event(key);
                    let command = self.update(message);
                    self.handle_command(command, terminal).await?;
                }
                Event::Mouse(m) => {
                    let message = self.on_mouse_event(m);
                    let command = self.update(message);
                    self.handle_command(command, terminal).await?;
                }
                Event::Resize(_, _) => {}
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn cursor_loc(&self, mouse: MouseEvent) -> MouseTarget {
        let pos = Position::new(mouse.column, mouse.row);

        if let Mode::ContextMenu(cmv) = &self.mode {
            if !cmv.list_area.contains(pos) {
                return MouseTarget {
                    target: HitTarget::OutOfBounds,
                    pos,
                };
            }

            let row = match (mouse.row - cmv.list_area.y).checked_sub(1) {
                Some(row) => row,
                None => {
                    return MouseTarget {
                        target: HitTarget::Empty,
                        pos,
                    };
                }
            };

            let idx = row as usize + cmv.list_state.offset();
            let len = cmv.options.len();

            if idx >= len || row >= cmv.list_area.height.saturating_sub(1) {
                return MouseTarget {
                    target: HitTarget::Empty,
                    pos,
                };
            }

            return MouseTarget {
                target: HitTarget::Row(idx),
                pos,
            };
        }
        if !self.table_area.contains(pos) {
            return MouseTarget {
                target: HitTarget::OutOfBounds,
                pos,
            };
        }

        let row_in_table = match (mouse.row - self.table_area.y).checked_sub(1) {
            Some(e) => e,
            None => {
                return MouseTarget {
                    target: HitTarget::HeaderClick,
                    pos,
                };
            }
        };
        let idx = row_in_table as usize + self.table_state.offset();

        let len = self.dir_view.displayed().len();

        if idx >= len {
            return MouseTarget {
                target: HitTarget::Empty,
                pos,
            };
        }

        MouseTarget {
            target: HitTarget::Row(idx),
            pos,
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

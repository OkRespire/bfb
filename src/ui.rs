use std::{rc::Rc, time::SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
};

const MIN: u64 = 60;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

use crate::{
    App,
    app::{ContextMenuView, Mode},
    fs::FileEntry,
};
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
        self.table_area = main_area;

        let title = Paragraph::new(self.dir_view.cwd.to_string_lossy());
        let rows: Vec<Row> = displayed
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let row = Row::new([
                    Cell::from(e.to_string()),
                    Cell::from(fmt_size(e.size)),
                    Cell::from(fmt_time(e.modified, SystemTime::now())),
                ]);

                if self.dir_view.hovered == Some(i) {
                    row.style(Style::new().bg(Color::Gray).fg(Color::Black))
                } else {
                    row.style(Style::new().bg(Color::Reset).fg(Color::White))
                }
            })
            .collect();

        let widths = [
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(16),
        ];

        if displayed.is_empty() {
            self.table_state.select(None);
            let msg =
                Paragraph::new("Your files are in another folder").alignment(Alignment::Center);
            frame.render_widget(msg, main_area);
        } else {
            let table = Table::new(rows, widths)
                .header(Row::new(["Name", "Size", "Modified"]).style(Style::new().bold()))
                .highlight_symbol("> ")
                .row_highlight_style(Style::new().bg(Color::LightBlue).fg(Color::White).bold());
            self.table_state.select(Some(self.dir_view.get_selected()));

            frame.render_stateful_widget(table, main_area, &mut self.table_state);
        }

        frame.render_widget(title, top);
        self.make_status_bar(status_chunks, frame);
        if let Mode::ContextMenu(cmv) = &mut self.mode {
            let area = Self::context_menu_area(cmv.mt.pos, frame.area(), 20, 9);
            cmv.list_area = area;
            frame.render_widget(Clear, area);
            Self::make_context_menu(frame, cmv);
        }
    }

    fn make_context_menu(frame: &mut Frame<'_>, cmv: &mut ContextMenuView) {
        let options = cmv.options.visible_options();

        let items: Vec<ListItem<'_>> = options
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let item = ListItem::new(s.1);

                if cmv.hover == Some(i) {
                    item.style(Style::new().bg(Color::Gray).fg(Color::Black))
                } else {
                    item.style(Style::new().bg(Color::Reset).fg(Color::White))
                }
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered())
            .bg(Color::Black)
            .highlight_symbol("> ")
            .highlight_style(Style::new().bg(Color::LightBlue).fg(Color::White).bold());
        cmv.list_state.select(Some(cmv.selected));
        frame.render_stateful_widget(list, cmv.list_area, &mut cmv.list_state);
    }

    fn make_status_bar(&self, area: Rc<[Rect]>, frame: &mut Frame<'_>) {
        let mode_ch = area[0];
        let action_ch = area[1];
        let permissions_ch = area[2];
        let mode = self
            .bot_right_mode()
            .style(Style::new().fg(Color::Black).bg(Color::Gray))
            .alignment(Alignment::Center);

        let curr_file = self.dir_view.get_curr_file().map(|f| f);
        let hint = Paragraph::new(self.mode_hint(curr_file))
            .style(Style::new().fg(Color::Gray).bg(Color::Black))
            .alignment(Alignment::Center);

        let permissions = match curr_file {
            Some(entry) => Paragraph::new(entry.permissions()),
            None => Paragraph::new(""),
        }
        .style(Style::new().fg(Color::Black).bg(Color::Gray))
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

    fn context_menu_area(pos: Position, term: Rect, w: u16, h: u16) -> Rect {
        let x = pos.x.min(term.width.saturating_sub(w));
        let y = pos.y.min(term.height.saturating_sub(h));

        Rect::new(x, y, w, h)
    }

    fn bot_right_mode(&self) -> Paragraph<'_> {
        match self.mode {
            Mode::Browsing
            | Mode::ContextMenu(_)
            | Mode::Rename { .. }
            | Mode::Delete { .. }
            | Mode::Add { .. } => Paragraph::new("NORMAL"),
            Mode::Visual => Paragraph::new("VISUAL"),
        }
    }
    fn mode_hint(&self, cf: Option<&FileEntry>) -> String {
        match &self.error {
            Some(e) => return e.to_string(),

            None => {}
        }
        let curr_file = match cf {
            Some(e) => e,
            None => match &self.mode {
                Mode::Add { input } => {
                    return format!("Creating: {}", input.text);
                }
                _ => return String::new(),
            },
        };
        match &self.mode {
            Mode::Browsing | Mode::ContextMenu(_) => String::new(),
            Mode::Delete { permanent } => {
                let first_part = if *permanent {
                    "PERMANENTLY".to_string()
                } else {
                    String::new()
                };
                format!("Delete {} {}: y/n", curr_file.name, first_part)
            }
            Mode::Rename { input } => {
                format!("Renaming {}: {}", curr_file.name, input.text)
            }
            Mode::Add { input } => {
                format!("Creating: {}", input.text)
            }
            Mode::Visual => String::new(),
        }
    }
}

fn fmt_time(modified: Option<SystemTime>, now: SystemTime) -> String {
    let Some(modified) = modified else {
        return "-".into();
    };
    let Ok(age) = now.duration_since(modified) else {
        return "future".into();
    };
    let secs = age.as_secs();

    if secs < MIN {
        "just now".into()
    } else if secs < HOUR {
        format!("{}m ago", secs / MIN)
    } else if secs < DAY {
        format!("{}h ago", secs / HOUR)
    } else if secs < 30 * DAY {
        format!("{}d ago", secs / DAY)
    } else if secs < 365 * DAY {
        format!("{}mo ago", secs / (30 * DAY))
    } else {
        format!("{}y ago", secs / (365 * DAY))
    }
}

fn fmt_size(size: Option<u64>) -> String {
    let Some(n) = size else {
        return "-".into();
    };

    const UNIT: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut count = 0;
    let mut curr_num = n as f64;

    while curr_num >= 1024.0 && count < UNIT.len() - 1 {
        curr_num /= 1024.0;
        count += 1
    }

    let unit = UNIT[count];
    if unit == "B" {
        return format!("{}{}", curr_num, unit);
    }
    format!("{:.1}{}", curr_num, unit)
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn none_is_a_dash() {
        assert_eq!(fmt_size(None), "-");
    }

    #[test]
    fn plain_bytes_have_no_decimal() {
        assert_eq!(fmt_size(Some(0)), "0B");
        assert_eq!(fmt_size(Some(512)), "512B");
        assert_eq!(fmt_size(Some(1023)), "1023B");
    }

    #[test]
    fn unit_boundaries() {
        assert_eq!(fmt_size(Some(1024)), "1.0KB");
        assert_eq!(fmt_size(Some(1_048_576)), "1.0MB");
        assert_eq!(fmt_size(Some(1_073_741_824)), "1.0GB");
        assert_eq!(fmt_size(Some(1 << 40)), "1.0TB");
    }

    #[test]
    fn keeps_one_decimal() {
        assert_eq!(fmt_size(Some(1536)), "1.5KB");
        assert_eq!(fmt_size(Some(1100)), "1.1KB");
    }

    #[test]
    fn huge_values_stay_in_tb() {
        assert_eq!(fmt_size(Some(u64::MAX)), "16777216.0TB");
    }
}
#[cfg(test)]
mod time_tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;

    // A fixed "now" about 30 years after the epoch, so subtracting
    // a few years from it can never underflow.
    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(30 * 365 * DAY)
    }

    fn ago(secs: u64) -> Option<SystemTime> {
        Some(now() - Duration::from_secs(secs))
    }

    #[test]
    fn none_is_a_dash() {
        assert_eq!(fmt_time(None, now()), "-");
    }

    #[test]
    fn future_timestamps_are_labelled() {
        let later = Some(now() + Duration::from_secs(10));
        assert_eq!(fmt_time(later, now()), "future");
    }

    #[test]
    fn under_a_minute_is_just_now() {
        assert_eq!(fmt_time(ago(0), now()), "just now");
        assert_eq!(fmt_time(ago(59), now()), "just now");
    }

    #[test]
    fn minutes() {
        assert_eq!(fmt_time(ago(MIN), now()), "1m ago");
        assert_eq!(fmt_time(ago(90), now()), "1m ago"); // rounds down
        assert_eq!(fmt_time(ago(HOUR - 1), now()), "59m ago");
    }

    #[test]
    fn hours() {
        assert_eq!(fmt_time(ago(HOUR), now()), "1h ago");
        assert_eq!(fmt_time(ago(DAY - 1), now()), "23h ago");
    }

    #[test]
    fn days() {
        assert_eq!(fmt_time(ago(DAY), now()), "1d ago");
        assert_eq!(fmt_time(ago(29 * DAY), now()), "29d ago");
    }

    #[test]
    fn months() {
        assert_eq!(fmt_time(ago(30 * DAY), now()), "1mo ago");
        assert_eq!(fmt_time(ago(364 * DAY), now()), "12mo ago");
    }

    #[test]
    fn years() {
        assert_eq!(fmt_time(ago(365 * DAY), now()), "1y ago");
        assert_eq!(fmt_time(ago(730 * DAY), now()), "2y ago");
    }
}

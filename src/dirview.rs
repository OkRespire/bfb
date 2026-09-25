use color_eyre::Result;
use std::path::PathBuf;

use crate::fs::{EntryType, FileEntry, get_files, part_files};

#[derive(Debug, Default)]
pub struct DirView {
    pub cwd: PathBuf,
    pub history: Vec<PathBuf>,
    pub visible: Vec<FileEntry>,
    pub hidden: Vec<FileEntry>,
    pub selected: usize,
}
impl DirView {
    pub async fn new(cwd: PathBuf) -> Result<Self> {
        let files = get_files(&cwd).await?;
        let (hidden, visible) = part_files(files);
        Ok(Self {
            history: Vec::new(),
            visible,
            hidden,
            selected: 0,
            cwd,
        })
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self, is_hidden: bool) {
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
        let path = match file.ent_type {
            EntryType::Directory => file.path,
            EntryType::File => unreachable!(),
            EntryType::Symlink { path, .. } => path,
        };
        self.selected = 0;
        let new_files = get_files(&path).await?;
        self.set_files(new_files);
        self.history.push(self.cwd.clone());
        self.cwd = path;
        Ok(())
    }

    pub async fn check_file(&mut self, is_hidden: bool) -> Result<(FileEntry, bool)> {
        let idx = &self.selected;
        let file = self.displayed(is_hidden)[*idx].clone();
        if file.is_text_file().await? {
            Ok((file, true))
        } else {
            Ok((file, false))
        }
    }

    pub async fn go_parent(&mut self) -> Result<()> {
        let path = match self.history.pop() {
            Some(p) => p,
            None => match self.cwd.parent() {
                Some(pt) => pt.to_path_buf(),
                None => return Ok(()),
            },
        };

        self.selected = 0;
        let files = get_files(&path).await?;
        self.cwd = path;
        self.set_files(files);
        Ok(())
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

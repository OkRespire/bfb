use color_eyre::Result;
use std::{fs, path::PathBuf};

use crate::fs::{EntryType, FileEntry, get_files, part_files};

#[derive(Debug, Default)]
pub struct DirView {
    pub cwd: PathBuf,
    pub history: Vec<PathBuf>,
    pub visible: Vec<FileEntry>,
    pub hidden: Vec<FileEntry>,
    pub is_hidden: bool,
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
            is_hidden: false,
            selected: 0,
            cwd,
        })
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.displayed().is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.displayed_len() - 1);
    }

    pub async fn get_entry_type(&mut self) -> Option<EntryType> {
        let idx = &self.selected;
        let files = self.displayed();
        if files.is_empty() {
            return None;
        }
        let file = files[*idx].clone();
        Some(file.ent_type)
    }

    pub async fn open_dir(&mut self) -> Result<()> {
        let idx = &self.selected;
        let file = self.displayed()[*idx].clone();
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

    pub async fn check_file(&mut self) -> Result<(FileEntry, bool)> {
        let idx = &self.selected;
        let file = self.displayed()[*idx].clone();
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

    pub async fn rename(&mut self, name: String) -> Result<()> {
        let displayed = self.displayed();
        let x = if let Some(f) = displayed.get(self.selected) {
            f
        } else {
            return Ok(());
        };
        x.rename(name).await?;
        Ok(())
    }

    pub async fn delete(&mut self, remove: bool) -> Result<()> {
        let displayed = self.displayed();
        let x = if let Some(f) = displayed.get(self.selected) {
            f
        } else {
            return Ok(());
        };
        x.delete(remove).await?;
        self.refresh().await?;

        // TODO: Change this so it is doing it dynamically
        // at bounds its 0 and new_len - 1 and normal it is
        // one item down, for now just put it to 0.
        self.selected = 0;
        Ok(())
    }

    pub async fn refresh(&mut self) -> Result<()> {
        let new_files = get_files(&self.cwd).await?;
        self.set_files(new_files);
        Ok(())
    }

    fn set_files(&mut self, files: Vec<FileEntry>) {
        (self.hidden, self.visible) = part_files(files);
    }

    pub fn displayed(&self) -> Vec<&FileEntry> {
        if self.is_hidden {
            self.hidden.iter().chain(self.visible.iter()).collect()
        } else {
            self.visible.iter().collect()
        }
    }

    pub fn get_selected(&self) -> usize {
        self.selected
    }

    fn displayed_len(&self) -> usize {
        self.displayed().len()
    }
}

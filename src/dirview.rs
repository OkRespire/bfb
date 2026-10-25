use color_eyre::Result;
use std::{
    fs::File,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tokio::fs;

use crate::fs::{EntryType, FileEntry, get_files, part_files};

#[derive(Debug, Default)]
pub struct DirView {
    pub cwd: PathBuf,
    pub history: Vec<PathBuf>,
    pub visible: Vec<FileEntry>,
    pub hidden: Vec<FileEntry>,
    pub is_hidden: bool,
    pub selected: usize,
    pub yanked: Option<FileEntry>,
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
            yanked: None,
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
        self.refresh().await?;
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

    pub fn copy(&mut self) -> Result<()> {
        let displayed = self.displayed();
        let yanked = if let Some(f) = displayed.get(self.selected) {
            (*f).clone()
        } else {
            return Ok(());
        };

        self.yanked = Some(yanked);
        tracing::debug!(?self.yanked, "yanked file");

        Ok(())
    }

    pub async fn paste(&mut self) -> Result<()> {
        let f = match &self.yanked {
            Some(f) => f,
            None => return Ok(()),
        };

        match f.ent_type {
            EntryType::File | EntryType::Symlink { .. } => {
                let mut candidate = self.cwd.join(&f.name);
                tracing::debug!(?candidate, "file to be pasted");
                let mut n = 1;
                while candidate.exists() {
                    let stem = f.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let ext = f.path.extension().and_then(|s| s.to_str());

                    let new_name = match ext {
                        Some(ext) => format!("{}_{}.{}", stem, n, ext),
                        None => format!("{}_{}", stem, n),
                    };
                    candidate = self.cwd.join(new_name);
                    n += 1
                }
                fs::copy(&f.path, candidate).await?;
            }
            EntryType::Directory => {
                tracing::debug!(?f.name, "folder to be pasted");
                let mut candidate = self.cwd.join(&f.name);
                tracing::debug!(?candidate, "file to be pasted");
                let mut n = 1;
                while candidate.exists() {
                    let stem = f.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let ext = f.path.extension().and_then(|s| s.to_str());

                    let new_name = match ext {
                        Some(ext) => format!("{}_{}.{}", stem, n, ext),
                        None => format!("{}_{}", stem, n),
                    };
                    candidate = self.cwd.join(new_name);
                    n += 1
                }
                let src = f.path.clone();
                let dest = self.cwd.join(candidate);
                self.paste_dir(&src, &dest).await?
            }
        }
        self.refresh().await?;
        self.yanked = None;
        Ok(())
    }

    async fn paste_dir(&self, src: &Path, dest: &Path) -> Result<()> {
        fs::create_dir(dest).await?;
        let mut entries = fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            let ft = entry.file_type().await?;

            if ft.is_dir() {
                Box::pin(self.paste_dir(&src_path, &dest_path)).await?;
            } else {
                fs::copy(&src_path, &dest_path).await?;
            }
        }
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

    pub fn get_curr_file(&self) -> Option<&FileEntry> {
        match self.displayed().get(self.get_selected()) {
            Some(entry) => Some(*entry),
            None => None,
        }
    }

    pub async fn add(&mut self, text: String) -> Result<()> {
        let text = text.trim();
        let ends_in_slash = text.ends_with('/');
        let parts: Vec<&str> = text.split('/').filter(|s| !s.is_empty()).collect();
        if text.is_empty() {
            return Ok(());
        }
        let mut current = self.cwd.clone();

        // TODO: Add custom error so the UI can display
        for (i, part) in parts.iter().enumerate() {
            current.push(part);

            if parts.len() - 1 == i && !ends_in_slash {
                match fs::File::create_new(&current).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                };
            } else {
                match fs::create_dir(&current).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        self.refresh().await?;
        Ok(())
    }
}

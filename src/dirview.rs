use color_eyre::Result;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tokio::fs;

use crate::{
    error::UiError,
    fs::{EntryType, FileEntry, get_files, part_files},
};

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

    pub async fn open_dir(&mut self) -> Result<(), UiError> {
        let idx = &self.selected;
        let file = self.displayed()[*idx].clone();
        let path = match file.ent_type {
            EntryType::Directory => file.path,
            EntryType::File => unreachable!(),
            EntryType::Symlink { path, .. } => path,
        };
        self.selected = 0;
        let new_files = match get_files(&path).await {
            Ok(f) => f,
            Err(e) => return Err(UiError::from_report(e, &path)),
        };
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

    pub async fn delete(&mut self, remove: bool) -> Result<(), UiError> {
        let displayed = self.displayed();
        let x = if let Some(f) = displayed.get(self.selected) {
            f
        } else {
            return Ok(());
        };
        x.delete(remove)
            .await
            .map_err(|e| UiError::from_report(e, &x.path))?;
        if displayed.len() == 1 {
            self.selected = 0;
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
        match self.refresh().await {
            Ok(_) => {}
            Err(e) => {
                return Err(UiError::Refresh {
                    path: self.cwd.clone(),
                    message: format!("{e:#}"),
                });
            }
        };

        Ok(())
    }

    pub fn copy(&mut self) {
        let displayed = self.displayed();
        let yanked = if let Some(f) = displayed.get(self.selected) {
            (*f).clone()
        } else {
            return;
        };

        self.yanked = Some(yanked);
        tracing::debug!(?self.yanked, "yanked file");
    }

    pub async fn paste(&mut self) -> Result<(), UiError> {
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
                match fs::copy(&f.path, candidate).await {
                    Ok(_) => {}
                    Err(e) => return Err(UiError::from_io(e, &f.path)),
                };
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
        self.yanked = None;
        match self.refresh().await {
            Ok(_) => {}
            Err(e) => {
                return Err(UiError::Refresh {
                    path: self.cwd.clone(),
                    message: format!("{e:#}"),
                });
            }
        };
        Ok(())
    }

    async fn paste_dir(&self, src: &Path, dest: &Path) -> Result<(), UiError> {
        match fs::create_dir(dest).await {
            Ok(_) => {}
            Err(e) => return Err(UiError::from_io(e, &dest)),
        };

        let mut entries = match fs::read_dir(src).await {
            Ok(r) => r,
            Err(e) => return Err(UiError::from_io(e, &src)),
        };

        while let Some(entry) = match entries.next_entry().await {
            Ok(e) => e,
            Err(e) => return Err(UiError::from_io(e, &src)),
        } {
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => return Err(UiError::from_io(e, &entry.path())),
            };

            if ft.is_dir() {
                Box::pin(self.paste_dir(&src_path, &dest_path)).await?;
            } else {
                match fs::copy(&src_path, &dest_path).await {
                    Ok(_) => {}
                    Err(e) => return Err(UiError::from_io(e, &src_path)),
                };
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

    pub async fn add(&mut self, text: String) -> Result<(), UiError> {
        let text = text.trim();
        let ends_in_slash = text.ends_with('/');
        let parts: Vec<&str> = text.split('/').filter(|s| !s.is_empty()).collect();
        if text.is_empty() {
            return Ok(());
        }
        let mut current = self.cwd.clone();

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            current.push(part);

            if parts.len() - 1 == i && !ends_in_slash {
                match fs::File::create_new(&current).await {
                    Ok(_) => {}
                    Err(e) => return Err(UiError::from_io(e, &current)),
                };
            } else {
                match fs::create_dir(&current).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::AlreadyExists && !is_last => {
                        let meta = fs::metadata(&current)
                            .await
                            .map_err(|e| UiError::from_io(e, &current))?;
                        if !meta.is_dir() {
                            return Err(UiError::AlreadyExists(current));
                        }
                    }
                    Err(e) => return Err(UiError::from_io(e, &current)),
                }
            }
        }
        match self.refresh().await {
            Ok(_) => {}
            Err(e) => {
                return Err(UiError::Refresh {
                    path: self.cwd.clone(),
                    message: format!("{e:#}"),
                });
            }
        };
        Ok(())
    }
}

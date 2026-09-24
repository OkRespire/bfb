use std::{env, path::PathBuf, str::from_utf8};

use color_eyre::Result;
use tokio::{
    fs::{File, read_dir},
    io::AsyncReadExt,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryType {
    Directory,
    File,
    Symlink,
}

impl PartialOrd for EntryType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntryType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            // Directories come before everything else
            (EntryType::Directory, EntryType::Directory) => std::cmp::Ordering::Equal,
            (EntryType::Directory, _) => std::cmp::Ordering::Less,
            (_, EntryType::Directory) => std::cmp::Ordering::Greater,

            // Files and symlinks have no hierarchy yet
            (EntryType::File, EntryType::File) => std::cmp::Ordering::Equal,
            (EntryType::Symlink, EntryType::Symlink) => std::cmp::Ordering::Equal,
            (EntryType::File, EntryType::Symlink) => std::cmp::Ordering::Equal,
            (EntryType::Symlink, EntryType::File) => std::cmp::Ordering::Equal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub ent_type: EntryType,
}

impl std::fmt::Display for FileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ent_type {
            EntryType::Directory => write!(f, "\u{f07b} ")?,
            EntryType::File => write!(f, "\u{f15b} ")?,
            EntryType::Symlink => write!(f, "\u{f504}")?,
        }

        write!(f, "{}", self.name)?;

        Ok(())
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ent_type
            .cmp(&other.ent_type)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.path.cmp(&other.path))
    }
}
impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl FileEntry {
    pub async fn edit_file(&self) -> Result<()> {
        let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        tokio::process::Command::new(editor)
            .arg(&self.path)
            .status()
            .await?;
        Ok(())
    }

    /// Checks if the file is a text file by inspecting the first 4KB of the file.
    /// [`true`] => is text
    ///
    /// [`false`] => is binary
    ///
    /// Returns [`Ok(true)`] if the file is empty, is a valid UTF-8 string, or the bytes are a
    /// valid UTF-8 string except that they end part-way through a character (like a 4-byte emoji).
    ///
    /// Returns [`Ok(false)`] if the file contains a NUL (0x00) in the first 4096 bytes of the file or if the bytes contain an
    /// invalid UTF-8 byte.
    ///
    /// # Error
    /// - If the file cannot be opened or read, it will return an [`Err`]
    ///
    /// # Limitation
    /// - Since the reading of the file is only 4096 bytes, a case can arise where the NUL character
    /// is beyond the taken bytes and return [`Ok(true)`]
    /// - **NOTE:** This has not been rigorously tested, so there may be more limitations than meets
    /// the eye
    pub async fn is_text_file(&self) -> Result<bool> {
        let mut buf = Vec::new();
        let f = File::open(&self.path).await?;
        let mut take = AsyncReadExt::take(f, 4096);
        take.read_to_end(&mut buf).await?;

        if buf.is_empty() {
            return Ok(true);
        }

        if buf.contains(&0b0) {
            return Ok(false);
        }
        match from_utf8(&buf) {
            Ok(_) => return Ok(true),
            Err(e) => match e.error_len() {
                Some(_) => return Ok(false),
                None => return Ok(true),
            },
        }
    }
}

pub async fn get_files(cwd: &PathBuf) -> Result<Vec<FileEntry>> {
    let mut entries = read_dir(cwd).await?;
    let mut files: Vec<FileEntry> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_ft = entry.file_type().await?;
        let ent_type = if entry_ft.is_dir() {
            EntryType::Directory
        } else if entry_ft.is_file() {
            EntryType::File
        } else {
            EntryType::Symlink
        };
        let path = entry.path();
        let fe = FileEntry {
            name,
            ent_type,
            path,
        };
        files.push(fe)
    }

    Ok(files)
}

pub fn part_files(mut files: Vec<FileEntry>) -> (Vec<FileEntry>, Vec<FileEntry>) {
    files.sort();
    files.into_iter().partition(|x| x.name.starts_with('.'))
}

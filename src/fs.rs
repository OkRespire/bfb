use std::{env, fs::File, path::PathBuf};

use color_eyre::Result;
use tokio::fs::read_dir;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

impl std::fmt::Display for FileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_dir {
            write!(f, "\u{f07b} ")?
        } else {
            write!(f, "\u{f15b} ")?
        }

        writeln!(f, "{}", self.name)?;

        Ok(())
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .is_dir
            .cmp(&self.is_dir)
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

    pub fn is_text_file(&self) -> bool {
        matches!(
            self.path.extension().and_then(|e| e.to_str()),
            Some(
                "rs" | "cpp"
                    | "c"
                    | "h"
                    | "hpp"
                    | "py"
                    | "js"
                    | "ts"
                    | "toml"
                    | "md"
                    | "txt"
                    | "nix"
                    | "sh"
                    | "json"
                    | "yaml"
                    | "yml"
            )
        )
    }
}

pub async fn get_files(cwd: &PathBuf) -> Result<Vec<FileEntry>> {
    let mut entries = read_dir(cwd).await?;
    let mut files: Vec<FileEntry> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().await?.is_dir();
        let path = entry.path();
        let fe = FileEntry { name, is_dir, path };
        files.push(fe)
    }

    Ok(files)
}

pub fn part_files(mut files: Vec<FileEntry>) -> (Vec<FileEntry>, Vec<FileEntry>) {
    files.sort();
    files.into_iter().partition(|x| x.name.starts_with('.'))
}

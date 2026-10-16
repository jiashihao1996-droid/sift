// ============================================================
// Sift - Test Trash
// A `Trash` for tests: moves files into a bin folder of its own temp dir
// (so they leave the photo folder, as with the real trash) and records them.
// ============================================================

use super::delete_batch::Trash;
use crate::models::organize::{OrganizeError, OrganizeErrorKind};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct BinTrash {
    bin: TempDir,
    pub trashed: Vec<PathBuf>,
    /// Refuse every file, as a volume without a trash does unconfirmed
    pub is_refusing: bool,
}

impl BinTrash {
    pub fn new() -> Self {
        Self {
            bin: TempDir::new().unwrap(),
            trashed: Vec::new(),
            is_refusing: false,
        }
    }

    pub fn refusing() -> Self {
        Self {
            is_refusing: true,
            ..Self::new()
        }
    }
}

impl Trash for BinTrash {
    fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError> {
        paths.iter().try_for_each(|path| self.trash_one(path))
    }

    fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError> {
        if self.is_refusing {
            let detail = format!("{} has no trash", path.display());
            return Err(OrganizeError::new(
                OrganizeErrorKind::TrashUnsupported,
                Some(path),
                detail,
            ));
        }
        let dest = self.bin.path().join(self.trashed.len().to_string());
        fs::rename(path, dest).unwrap();
        self.trashed.push(path.to_path_buf());
        Ok(())
    }
}

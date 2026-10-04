// ============================================================
// Sift - Deleting Where There Is No Trash
// Wraps the system trash for volumes without one (network shares, removable
// FAT/exFAT cards on Windows). There a file is removed for good, but only
// when the user confirmed permanent deletion for this archive; without that
// confirmation it fails with `TrashUnsupported` and stays where it is.
// Every other file goes to the trash as usual: a volume that has a trash is
// never deleted from permanently, whatever the frontend asked for.
// ============================================================

use super::delete_batch::Trash;
use crate::models::organize::{NoTrashReason, OrganizeError, OrganizeErrorKind};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct TrashOrRemove<T, F> {
    trash: T,
    /// The user confirmed, for this archive, that files without a trash are
    /// deleted for good
    is_permanent_confirmed: bool,
    no_trash_reason: F,
    /// Per folder: a folder's files all live on the same volume
    reasons: HashMap<PathBuf, Option<NoTrashReason>>,
}

impl<T, F> TrashOrRemove<T, F>
where
    T: Trash,
    F: FnMut(&Path) -> Option<NoTrashReason>,
{
    /// `no_trash_reason` is `os::trash_support::no_trash_reason` in production.
    pub fn new(trash: T, is_permanent_confirmed: bool, no_trash_reason: F) -> Self {
        Self {
            trash,
            is_permanent_confirmed,
            no_trash_reason,
            reasons: HashMap::new(),
        }
    }

    fn has_no_trash(&mut self, path: &Path) -> bool {
        let folder = path.parent().unwrap_or(path).to_path_buf();
        let lookup = &mut self.no_trash_reason;
        self.reasons
            .entry(folder)
            .or_insert_with_key(|folder| lookup(folder))
            .is_some()
    }

    fn remove(&self, path: &Path) -> Result<(), OrganizeError> {
        if !self.is_permanent_confirmed {
            return Err(OrganizeError::new(
                OrganizeErrorKind::TrashUnsupported,
                Some(path),
                format!(
                    "{} is on a volume without a trash and permanent deletion was not confirmed",
                    path.display()
                ),
            ));
        }
        fs::remove_file(path).map_err(|e| OrganizeError::io(&e, path, "Failed to delete"))
    }
}

impl<T, F> Trash for TrashOrRemove<T, F>
where
    T: Trash,
    F: FnMut(&Path) -> Option<NoTrashReason>,
{
    fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError> {
        let (no_trash, with_trash): (Vec<&Path>, Vec<&Path>) =
            paths.iter().partition(|path| self.has_no_trash(path));
        for path in no_trash {
            self.remove(path)?;
        }
        match with_trash.is_empty() {
            true => Ok(()),
            false => self.trash.trash_all(&with_trash),
        }
    }

    fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError> {
        if self.has_no_trash(path) {
            self.remove(path)
        } else {
            self.trash.trash_one(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Records what reached the system trash, without moving anything
    #[derive(Default)]
    struct RecordingTrash {
        trashed: Vec<PathBuf>,
    }

    impl Trash for RecordingTrash {
        fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError> {
            self.trashed.extend(paths.iter().map(|p| p.to_path_buf()));
            Ok(())
        }

        fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError> {
            self.trashed.push(path.to_path_buf());
            Ok(())
        }
    }

    /// `<tmp>/nas` has no trash, `<tmp>/local` has one
    struct Fixture {
        tmp: TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                tmp: TempDir::new().unwrap(),
            }
        }

        fn file(&self, rel: &str) -> PathBuf {
            let path = self.tmp.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "photo").unwrap();
            path
        }

        fn trash(
            &self,
            is_permanent_confirmed: bool,
        ) -> TrashOrRemove<RecordingTrash, impl FnMut(&Path) -> Option<NoTrashReason> + '_>
        {
            let nas = self.tmp.path().join("nas");
            TrashOrRemove::new(
                RecordingTrash::default(),
                is_permanent_confirmed,
                move |folder: &Path| {
                    folder
                        .starts_with(&nas)
                        .then_some(NoTrashReason::NetworkVolume)
                },
            )
        }
    }

    #[test]
    fn deletes_for_good_only_where_there_is_no_trash_and_it_was_confirmed() {
        let fx = Fixture::new();
        let on_nas = fx.file("nas/IMG_1.ARW");
        let local = fx.file("local/IMG_2.ARW");
        let mut trash = fx.trash(true);

        trash.trash_all(&[&on_nas, &local]).unwrap();

        assert!(!on_nas.exists());
        assert!(
            local.exists(),
            "a volume with a trash is never deleted from"
        );
        assert_eq!(trash.trash.trashed, [local]);
    }

    #[test]
    fn refuses_without_confirmation_and_keeps_the_file() {
        let fx = Fixture::new();
        let on_nas = fx.file("nas/IMG_1.ARW");
        let mut trash = fx.trash(false);

        let err = trash.trash_one(&on_nas).unwrap_err();

        assert_eq!(err.kind, OrganizeErrorKind::TrashUnsupported);
        assert!(on_nas.exists());
        assert!(
            trash.trash.trashed.is_empty(),
            "never handed to the trash either"
        );
    }

    #[test]
    fn a_volume_with_a_trash_uses_it_even_when_permanent_deletion_was_confirmed() {
        let fx = Fixture::new();
        let local = fx.file("local/IMG_2.ARW");
        let mut trash = fx.trash(true);

        trash.trash_one(&local).unwrap();

        assert!(local.exists());
        assert_eq!(trash.trash.trashed, [local]);
    }

    #[test]
    fn checks_each_folder_once() {
        let fx = Fixture::new();
        let files: Vec<PathBuf> = (0..3)
            .map(|n| fx.file(&format!("local/IMG_{}.ARW", n)))
            .collect();
        let mut lookups = 0;
        let mut trash = TrashOrRemove::new(RecordingTrash::default(), false, |_: &Path| {
            lookups += 1;
            None
        });

        let paths: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        trash.trash_all(&paths).unwrap();
        drop(trash);

        assert_eq!(lookups, 1);
    }
}

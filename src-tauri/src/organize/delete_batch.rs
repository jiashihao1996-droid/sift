// ============================================================
// Sift - Batch Delete
// Deletes many pairs with few trash calls: pairs go to the trash in chunks
// (one `trash::delete_all`, i.e. one Finder call on macOS, per chunk instead
// of one per file). A chunk that fails is retried pair by pair, file by file,
// so a bad file only fails its own pair and the rest carry on. The result
// lists every pair as trashed or failed.
// ============================================================

use super::delete::{execute_delete, plan_delete, remove_preview, DeletePlan};
use super::file_name;
use crate::models::organize::{
    DeleteFailure, DeletePairInput, DeleteResult, OrganizeError, OrganizeErrorKind,
};
use crate::models::photo::{ArchiveProgress, PhotoSource};
use std::path::Path;

/// Pairs per `trash_all` call: a few hundred paths keep one Finder command
/// well within argument limits
const CHUNK_PAIRS: usize = 50;

/// The system trash, injected so tests can run without it.
pub trait Trash {
    /// Trash every path in one go. On error some may have moved already.
    fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError>;
    fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError>;
}

/// The real trash (`trash` crate; Finder on macOS)
pub struct SystemTrash;

impl Trash for SystemTrash {
    fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError> {
        trash::delete_all(paths).map_err(|e| OrganizeError::trash(&e, paths[0]))
    }

    fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError> {
        trash::delete(path).map_err(|e| OrganizeError::trash(&e, path))
    }
}

type Planned<'a> = (&'a DeletePairInput, DeletePlan);

/// Delete every pair, carrying on past failures. `on_progress` is called
/// after each chunk with the number of pairs handled so far.
pub fn delete_pairs(
    pairs: &[DeletePairInput],
    preview_cache: &Path,
    trash: &mut impl Trash,
    on_progress: &mut dyn FnMut(ArchiveProgress),
) -> DeleteResult {
    let mut result = DeleteResult::default();
    let mut planned: Vec<Planned> = Vec::new();
    for pair in pairs {
        let plan = plan_delete(
            &pair.jpg_path,
            pair.raw_path.as_deref(),
            &pair.source,
            &pair.xmp_paths,
            preview_cache,
        );
        match plan {
            Ok(plan) => planned.push((pair, plan)),
            Err(error) => fail(&mut result, pair, error),
        }
    }

    let mut handled = result.failed.len();
    let mut chunks = planned.chunks(CHUNK_PAIRS);
    while let Some(chunk) = chunks.next() {
        let systemic = delete_chunk(chunk, trash, &mut result);
        handled += chunk.len();
        on_progress(progress(handled, pairs.len(), chunk));
        if let Some(error) = systemic {
            // Every later pair would fail the same way: say so without trying
            for (pair, _) in chunks.flatten() {
                fail(&mut result, pair, error.clone().with_completed(0));
            }
            break;
        }
    }
    result
}

/// Trash one chunk; returns an error that will fail every remaining pair too.
fn delete_chunk(
    chunk: &[Planned],
    trash: &mut impl Trash,
    result: &mut DeleteResult,
) -> Option<OrganizeError> {
    let paths: Vec<&Path> = chunk
        .iter()
        .flat_map(|(_, plan)| plan.trash.iter().map(|item| item.path.as_path()))
        .collect();
    let all_trashed = match paths.is_empty() {
        true => true,
        false => trash
            .trash_all(&paths)
            .inspect_err(|e| eprintln!("[Sift] Batch trash failed, retrying one by one: {:?}", e))
            .is_ok(),
    };

    for (pair, plan) in chunk {
        if all_trashed {
            remove_preview(plan);
            result.trashed.push(pair.id.clone());
            continue;
        }
        match execute_delete(plan, |path| trash.trash_one(path)) {
            Ok(()) => result.trashed.push(pair.id.clone()),
            Err(error) if is_systemic(&error) => {
                let systemic = error.clone();
                fail(result, pair, error);
                fail_rest(chunk, pair, result, &systemic);
                return Some(systemic);
            }
            Err(error) => fail(result, pair, error),
        }
    }
    None
}

/// Fail the pairs of `chunk` after `failed` without trying them.
fn fail_rest(
    chunk: &[Planned],
    failed: &DeletePairInput,
    result: &mut DeleteResult,
    error: &OrganizeError,
) {
    let rest = chunk.iter().skip_while(|(p, _)| p.id != failed.id).skip(1);
    for (pair, _) in rest {
        fail(result, pair, error.clone().with_completed(0));
    }
}

/// An error no other pair can escape, such as Finder refusing Sift.
fn is_systemic(error: &OrganizeError) -> bool {
    error.kind == OrganizeErrorKind::AutomationDenied
}

fn fail(result: &mut DeleteResult, pair: &DeletePairInput, error: OrganizeError) {
    result.failed.push(DeleteFailure {
        id: pair.id.clone(),
        error,
    });
}

fn progress(current: usize, total: usize, chunk: &[Planned]) -> ArchiveProgress {
    let current_file = chunk
        .last()
        .map(|(pair, _)| match (&pair.source, &pair.raw_path) {
            (PhotoSource::RawPreview, Some(raw)) => file_name(Path::new(raw)),
            _ => file_name(Path::new(&pair.jpg_path)),
        })
        .unwrap_or_default();
    ArchiveProgress {
        current,
        total,
        current_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Moves files into `bin`; paths in `broken` fail, as does any batch
    /// containing one. Counts the calls made.
    struct FakeTrash {
        bin: PathBuf,
        broken: Vec<PathBuf>,
        kind: OrganizeErrorKind,
        batch_calls: usize,
        single_calls: usize,
    }

    impl FakeTrash {
        fn new(tmp: &TempDir) -> Self {
            let bin = tmp.path().join("bin");
            fs::create_dir_all(&bin).unwrap();
            Self {
                bin,
                broken: Vec::new(),
                kind: OrganizeErrorKind::PermissionDenied,
                batch_calls: 0,
                single_calls: 0,
            }
        }

        fn refuse(&self, path: &Path) -> OrganizeError {
            OrganizeError::new(self.kind, Some(path), "refused")
        }

        fn bin(&self, path: &Path) {
            fs::rename(path, self.bin.join(path.file_name().unwrap())).unwrap();
        }
    }

    impl Trash for FakeTrash {
        fn trash_all(&mut self, paths: &[&Path]) -> Result<(), OrganizeError> {
            self.batch_calls += 1;
            if let Some(bad) = paths
                .iter()
                .find(|p| self.broken.contains(&p.to_path_buf()))
            {
                return Err(self.refuse(bad));
            }
            paths.iter().for_each(|p| self.bin(p));
            Ok(())
        }

        fn trash_one(&mut self, path: &Path) -> Result<(), OrganizeError> {
            self.single_calls += 1;
            if self.broken.contains(&path.to_path_buf()) {
                return Err(self.refuse(path));
            }
            self.bin(path);
            Ok(())
        }
    }

    fn touch(path: &Path) -> String {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
        path.to_string_lossy().into_owned()
    }

    /// A JPG + RAW pair `IMG_<n>` under `<tmp>/photos`
    fn pair(tmp: &TempDir, n: usize) -> DeletePairInput {
        let dir = tmp.path().join("photos");
        DeletePairInput {
            id: format!("id-{}", n),
            jpg_path: touch(&dir.join(format!("IMG_{}.jpg", n))),
            raw_path: Some(touch(&dir.join(format!("RAW/IMG_{}.ARW", n)))),
            source: PhotoSource::Jpg,
            xmp_paths: vec![],
        }
    }

    fn run(tmp: &TempDir, pairs: &[DeletePairInput], trash: &mut FakeTrash) -> DeleteResult {
        delete_pairs(pairs, &tmp.path().join("cache"), trash, &mut |_| {})
    }

    fn failed_ids(result: &DeleteResult) -> Vec<&str> {
        result.failed.iter().map(|f| f.id.as_str()).collect()
    }

    #[test]
    fn trashes_a_chunk_of_pairs_in_one_call() {
        let tmp = TempDir::new().unwrap();
        let pairs: Vec<_> = (0..3).map(|n| pair(&tmp, n)).collect();
        let mut trash = FakeTrash::new(&tmp);

        let result = run(&tmp, &pairs, &mut trash);

        assert_eq!(result.trashed, ["id-0", "id-1", "id-2"]);
        assert!(result.failed.is_empty());
        assert_eq!((trash.batch_calls, trash.single_calls), (1, 0));
        assert!(trash.bin.join("IMG_1.ARW").exists());
    }

    #[test]
    fn splits_large_batches_into_chunks_and_reports_progress_after_each() {
        let tmp = TempDir::new().unwrap();
        let pairs: Vec<_> = (0..CHUNK_PAIRS + 1).map(|n| pair(&tmp, n)).collect();
        let mut trash = FakeTrash::new(&tmp);
        let mut events = Vec::new();

        let result = delete_pairs(&pairs, &tmp.path().join("cache"), &mut trash, &mut |p| {
            events.push((p.current, p.total))
        });

        assert_eq!(result.trashed.len(), CHUNK_PAIRS + 1);
        assert_eq!(trash.batch_calls, 2);
        assert_eq!(
            events,
            [
                (CHUNK_PAIRS, CHUNK_PAIRS + 1),
                (CHUNK_PAIRS + 1, CHUNK_PAIRS + 1)
            ]
        );
    }

    #[test]
    fn a_file_that_fails_only_fails_its_own_pair() {
        let tmp = TempDir::new().unwrap();
        let pairs: Vec<_> = (0..3).map(|n| pair(&tmp, n)).collect();
        let mut trash = FakeTrash::new(&tmp);
        let bad_raw = PathBuf::from(pairs[1].raw_path.clone().unwrap());
        trash.broken.push(bad_raw.clone());

        let result = run(&tmp, &pairs, &mut trash);

        assert_eq!(result.trashed, ["id-0", "id-2"]);
        assert_eq!(failed_ids(&result), ["id-1"]);
        let error = &result.failed[0].error;
        assert_eq!(error.kind, OrganizeErrorKind::PermissionDenied);
        assert_eq!(error.completed, 1, "its JPG is already in the trash");
        assert!(bad_raw.exists());
    }

    #[test]
    fn deleting_again_after_a_failure_finishes_the_pair() {
        let tmp = TempDir::new().unwrap();
        let pairs = [pair(&tmp, 0)];
        let mut trash = FakeTrash::new(&tmp);
        trash
            .broken
            .push(PathBuf::from(pairs[0].raw_path.clone().unwrap()));
        run(&tmp, &pairs, &mut trash);
        trash.broken.clear();

        // The JPG is gone now; the RAW still has to go
        let result = run(&tmp, &pairs, &mut trash);

        assert_eq!(result.trashed, ["id-0"]);
        assert!(trash.bin.join("IMG_0.ARW").exists());
    }

    #[test]
    fn a_pair_that_is_already_gone_counts_as_trashed_without_touching_the_trash() {
        let tmp = TempDir::new().unwrap();
        let mut gone = pair(&tmp, 0);
        fs::remove_file(&gone.jpg_path).unwrap();
        fs::remove_file(gone.raw_path.take().unwrap()).unwrap();
        let mut trash = FakeTrash::new(&tmp);

        let result = run(&tmp, &[gone], &mut trash);

        assert_eq!(result.trashed, ["id-0"]);
        assert_eq!(trash.batch_calls + trash.single_calls, 0);
    }

    #[test]
    fn a_denied_finder_permission_fails_the_rest_without_asking_finder_again() {
        let tmp = TempDir::new().unwrap();
        let pairs: Vec<_> = (0..CHUNK_PAIRS * 2).map(|n| pair(&tmp, n)).collect();
        let mut trash = FakeTrash::new(&tmp);
        trash.kind = OrganizeErrorKind::AutomationDenied;
        trash.broken = pairs.iter().map(|p| PathBuf::from(&p.jpg_path)).collect();

        let result = run(&tmp, &pairs, &mut trash);

        assert!(result.trashed.is_empty());
        assert_eq!(
            result.failed.len(),
            pairs.len(),
            "every pair is accounted for"
        );
        assert_eq!((trash.batch_calls, trash.single_calls), (1, 1));
        assert!(result
            .failed
            .iter()
            .all(|f| f.error.kind == OrganizeErrorKind::AutomationDenied));
    }

    #[test]
    fn a_preview_outside_the_cache_fails_only_that_pair() {
        let tmp = TempDir::new().unwrap();
        let mut sneaky = pair(&tmp, 0);
        sneaky.source = PhotoSource::RawPreview;
        fs::create_dir_all(tmp.path().join("cache")).unwrap();
        let honest = pair(&tmp, 1);
        let mut trash = FakeTrash::new(&tmp);

        let result = run(&tmp, &[sneaky.clone(), honest], &mut trash);

        assert_eq!(result.trashed, ["id-1"]);
        assert_eq!(failed_ids(&result), ["id-0"]);
        assert!(Path::new(&sneaky.jpg_path).exists(), "user JPG untouched");
        assert!(Path::new(sneaky.raw_path.as_deref().unwrap()).exists());
    }

    #[test]
    fn removes_the_cached_preview_of_a_trashed_raw_only_pair() {
        let tmp = TempDir::new().unwrap();
        let preview = touch(&tmp.path().join("cache/abc.jpg"));
        let raw_only = DeletePairInput {
            id: "raw".into(),
            jpg_path: preview.clone(),
            raw_path: Some(touch(&tmp.path().join("photos/IMG_9.CR3"))),
            source: PhotoSource::RawPreview,
            xmp_paths: vec![],
        };
        let mut trash = FakeTrash::new(&tmp);

        let result = run(&tmp, &[raw_only], &mut trash);

        assert_eq!(result.trashed, ["raw"]);
        assert!(!Path::new(&preview).exists());
        assert!(trash.bin.join("IMG_9.CR3").exists());
        assert!(
            !trash.bin.join("abc.jpg").exists(),
            "the preview never goes to the trash"
        );
    }
}

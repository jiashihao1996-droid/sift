// ============================================================
// Sift - Group Transfer
// Moves one group of files (a photo's JPG, RAW and XMP sidecars) as a unit:
// either every file reaches its destination, or the files that already
// moved are put back where they were. Either way the placeholders the group
// claimed and left empty are removed, so a rescan never finds a split pair
// or an empty file standing in for a real one.
// ============================================================

use crate::paths;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

/// Moves a file onto an empty placeholder we claimed. Production passes
/// `move_file`; tests inject failures.
pub type MoveFn<'a> = dyn FnMut(&Path, &Path) -> Result<(), String> + 'a;

/// How a file reaches its destination.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Transfer {
    /// A user file: moved (or renamed in place).
    Move,
    /// Our cached RAW preview: copied. The caller removes the cache file
    /// once the whole group is done.
    CopyPreview,
}

/// One file of a group and the placeholder it goes to.
pub struct Step<'a> {
    pub src: &'a Path,
    pub dest: &'a Path,
    pub transfer: Transfer,
}

/// Move a file onto `dest`, an empty placeholder claimed with `create_new`,
/// falling back to copy + remove when rename fails (e.g. across volumes).
///
/// The fallback removes `src` only after `fs::copy` has returned Ok. When it
/// fails, `src` stays where it was and `dest` is emptied again, so neither a
/// partial copy nor a full copy whose source could not be removed is left as
/// a second version of the photo; the caller removes the empty placeholder.
/// `dest` is only emptied while `src` still exists.
pub fn move_file(src: &Path, dest: &Path) -> Result<(), String> {
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    copy_then_remove(src, dest)
}

/// `move_file`'s fallback, separate so tests can reach it without having to
/// make `rename` fail (which differs per platform).
fn copy_then_remove(src: &Path, dest: &Path) -> Result<(), String> {
    if let Err(e) = fs::copy(src, dest) {
        discard_copy(src, dest);
        return Err(format!(
            "Failed to copy {} -> {}: {}",
            src.display(),
            dest.display(),
            e
        ));
    }
    match fs::remove_file(src) {
        Ok(()) => Ok(()),
        // The remove reported an error, yet the source is gone: the file moved
        Err(_) if !src.exists() => Ok(()),
        Err(e) => {
            discard_copy(src, dest);
            Err(format!(
                "Failed to remove {} after copying it: {}",
                src.display(),
                e
            ))
        }
    }
}

/// Empty our placeholder `dest` after a failed copy into it. Skipped when
/// `src` is gone, since `dest` may then hold the only copy.
fn discard_copy(src: &Path, dest: &Path) {
    if !src.exists() {
        return;
    }
    if let Err(e) = OpenOptions::new().write(true).truncate(true).open(dest) {
        eprintln!(
            "[Sift] Failed to empty placeholder {}: {}",
            dest.display(),
            e
        );
    }
}

/// Run `steps` as a unit. `placeholders` are all the files the group's claim
/// created (see `paths::claim_first_free`); those left unused and still empty
/// are removed, whether the group succeeds or not.
///
/// Preview copies run after every move, so a copy is never rolled back: once
/// it is done, nothing is left to fail.
pub fn move_group(
    steps: &[Step],
    mut placeholders: Vec<PathBuf>,
    move_file: &mut MoveFn,
) -> Result<(), String> {
    let moves = steps.iter().filter(|s| s.transfer == Transfer::Move);
    let copies = steps.iter().filter(|s| s.transfer == Transfer::CopyPreview);
    let mut done: Vec<&Step> = Vec::new();

    for step in moves.chain(copies) {
        if let Err(cause) = run_step(step, move_file) {
            if went_through_anyway(step) {
                done.push(step);
            }
            let rollback = put_back(&done, &mut placeholders, move_file);
            remove_empty_placeholders(&placeholders, &rollback.occupied);
            return Err(rollback.describe(&cause));
        }
        done.push(step);
    }

    let occupied: Vec<&Path> = done.iter().map(|s| s.dest).collect();
    remove_empty_placeholders(&placeholders, &occupied);
    Ok(())
}

/// A move that reported failure, yet its source is gone and the destination
/// holds data: the file did arrive, so it must be put back like the others.
/// A source that vanished on its own leaves only our empty placeholder,
/// which is not moved anywhere.
fn went_through_anyway(step: &Step) -> bool {
    step.transfer == Transfer::Move
        && !step.src.exists()
        && fs::symlink_metadata(step.dest).is_ok_and(|m| m.len() > 0)
}

fn run_step(step: &Step, move_file: &mut MoveFn) -> Result<(), String> {
    match step.transfer {
        Transfer::Move => move_file(step.src, step.dest),
        Transfer::CopyPreview => fs::copy(step.src, step.dest).map(drop).map_err(|e| {
            discard_copy(step.src, step.dest);
            format!("Failed to copy preview JPG: {}", e)
        }),
    }
}

/// What putting a failed group back achieved.
#[derive(Default)]
struct Rollback<'a> {
    put_back: usize,
    /// Files that could not go back, described for the error message
    stuck: Vec<String>,
    /// Paths now holding the group's files, which may themselves be empty
    occupied: Vec<&'a Path>,
}

impl Rollback<'_> {
    fn describe(&self, cause: &str) -> String {
        let mut message = cause.to_string();
        if self.put_back > 0 {
            message += &format!("; put {} moved file(s) back", self.put_back);
        }
        if !self.stuck.is_empty() {
            message += &format!("; could not put back {}", self.stuck.join(", "));
        }
        message
    }
}

/// Move completed steps back, newest first. Each original location is
/// claimed with `create_new` first, so a file that appeared there in the
/// meantime is never overwritten; that step's file stays at its destination.
fn put_back<'a>(
    done: &[&'a Step<'a>],
    placeholders: &mut Vec<PathBuf>,
    move_file: &mut MoveFn,
) -> Rollback<'a> {
    let mut rollback = Rollback::default();
    for step in done.iter().rev() {
        match put_back_one(step, placeholders, move_file) {
            Ok(()) => {
                rollback.put_back += 1;
                rollback.occupied.push(step.src);
            }
            Err(e) => {
                rollback.occupied.push(step.dest);
                rollback.stuck.push(format!(
                    "{} (belongs at {}): {}",
                    step.dest.display(),
                    step.src.display(),
                    e
                ));
            }
        }
    }
    rollback
}

fn put_back_one(
    step: &Step,
    placeholders: &mut Vec<PathBuf>,
    move_file: &mut MoveFn,
) -> Result<(), String> {
    paths::claim_one(step.src)
        .map_err(|e| format!("cannot reclaim the original location: {}", e))?;
    placeholders.push(step.src.to_path_buf());
    move_file(step.dest, step.src)
}

/// Remove the placeholders in `created` that are still empty files. Paths in
/// `occupied` hold the group's own files, which may be empty photos, and are
/// never touched. Failures are logged: a leftover empty file is untidy, not
/// a reason to fail an archive that otherwise succeeded.
pub fn remove_empty_placeholders(created: &[PathBuf], occupied: &[&Path]) {
    for path in created.iter().filter(|p| !occupied.contains(&p.as_path())) {
        if !is_empty_file(path) {
            continue;
        }
        if let Err(e) = fs::remove_file(path) {
            eprintln!(
                "[Sift] Failed to remove empty placeholder {}: {}",
                path.display(),
                e
            );
        }
    }
}

/// A regular file of zero bytes; a symlink never counts.
fn is_empty_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.is_file() && m.len() == 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, contents: &str) -> PathBuf {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        path.to_path_buf()
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// Claim `dests` as the archive would, returning the placeholders.
    fn claim(dests: &[&Path]) -> Vec<PathBuf> {
        for dest in dests {
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            paths::claim_one(dest).unwrap();
        }
        dests.iter().map(|d| d.to_path_buf()).collect()
    }

    fn step<'a>(src: &'a Path, dest: &'a Path) -> Step<'a> {
        Step {
            src,
            dest,
            transfer: Transfer::Move,
        }
    }

    /// The real move, except that moving `failing` fails.
    fn failing_on(failing: &Path) -> impl FnMut(&Path, &Path) -> Result<(), String> + '_ {
        move |src, dest| {
            if src == failing {
                return Err("disk full".into());
            }
            move_file(src, dest)
        }
    }

    #[test]
    fn move_file_moves_contents_and_removes_the_source() {
        let tmp = TempDir::new().unwrap();
        let src = write(&tmp.path().join("a.jpg"), "data");
        let dest = tmp.path().join("sub/a.jpg");
        claim(&[&dest]);

        move_file(&src, &dest).unwrap();

        assert!(!src.exists());
        assert_eq!(read(&dest), "data");
    }

    #[test]
    fn a_failed_copy_empties_the_placeholder_and_keeps_the_source() {
        let tmp = TempDir::new().unwrap();
        // A directory can't be copied as a file, on any platform
        let src = tmp.path().join("a.jpg");
        fs::create_dir(&src).unwrap();
        let dest = write(&tmp.path().join("sub/a.jpg"), "partial copy");

        let err = copy_then_remove(&src, &dest).unwrap_err();

        assert!(err.contains("Failed to copy"), "{}", err);
        assert!(src.is_dir());
        assert_eq!(read(&dest), "");
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_whose_source_cannot_be_removed_is_emptied() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let card = tmp.path().join("card");
        let src = write(&card.join("a.jpg"), "data");
        let dest = tmp.path().join("sub/a.jpg");
        claim(&[&dest]);
        // Read-only source folder: rename fails, the copy works, the remove fails
        fs::set_permissions(&card, fs::Permissions::from_mode(0o555)).unwrap();

        let result = move_file(&src, &dest);

        fs::set_permissions(&card, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.unwrap_err().contains("Failed to remove"));
        assert_eq!(read(&src), "data");
        assert_eq!(read(&dest), "", "no second version of the photo");
    }

    #[test]
    fn a_group_that_moves_keeps_its_files_and_drops_abandoned_placeholders() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "jpg");
        let empty_xmp = write(&tmp.path().join("IMG_1.xmp"), "");
        let (jpg_dest, xmp_dest) = (
            tmp.path().join("精选/IMG_1_1.jpg"),
            tmp.path().join("精选/IMG_1_1.xmp"),
        );
        let abandoned = tmp.path().join("精选/IMG_1.jpg");
        let placeholders = claim(&[&abandoned, &jpg_dest, &xmp_dest]);

        move_group(
            &[step(&jpg, &jpg_dest), step(&empty_xmp, &xmp_dest)],
            placeholders,
            &mut move_file,
        )
        .unwrap();

        assert_eq!(read(&jpg_dest), "jpg");
        assert!(xmp_dest.exists(), "an empty file of the group is kept");
        assert!(!abandoned.exists());
    }

    #[test]
    fn a_failed_move_puts_the_group_back_and_removes_its_placeholders() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "jpg");
        let raw = write(&tmp.path().join("IMG_1.ARW"), "raw");
        let (jpg_dest, raw_dest) = (
            tmp.path().join("精选/IMG_1.jpg"),
            tmp.path().join("精选/IMG_1.ARW"),
        );
        let placeholders = claim(&[&jpg_dest, &raw_dest]);

        let err = move_group(
            &[step(&jpg, &jpg_dest), step(&raw, &raw_dest)],
            placeholders,
            &mut failing_on(&raw),
        )
        .unwrap_err();

        assert!(err.contains("disk full"), "{}", err);
        assert!(err.contains("put 1 moved file(s) back"), "{}", err);
        assert_eq!(read(&jpg), "jpg");
        assert_eq!(read(&raw), "raw");
        assert!(!jpg_dest.exists() && !raw_dest.exists());
    }

    #[test]
    fn an_empty_photo_that_goes_back_is_not_mistaken_for_a_placeholder() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "");
        let raw = write(&tmp.path().join("IMG_1.ARW"), "raw");
        let (jpg_dest, raw_dest) = (
            tmp.path().join("精选/IMG_1.jpg"),
            tmp.path().join("精选/IMG_1.ARW"),
        );
        let placeholders = claim(&[&jpg_dest, &raw_dest]);

        move_group(
            &[step(&jpg, &jpg_dest), step(&raw, &raw_dest)],
            placeholders,
            &mut failing_on(&raw),
        )
        .unwrap_err();

        assert!(jpg.exists(), "the user's empty JPG is back, not removed");
    }

    #[test]
    fn a_file_that_appeared_at_the_original_location_is_never_overwritten() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "jpg");
        let raw = write(&tmp.path().join("IMG_1.ARW"), "raw");
        let (jpg_dest, raw_dest) = (
            tmp.path().join("精选/IMG_1.jpg"),
            tmp.path().join("精选/IMG_1.ARW"),
        );
        let placeholders = claim(&[&jpg_dest, &raw_dest]);
        let jpg_path = jpg.clone();

        let err = move_group(
            &[step(&jpg, &jpg_dest), step(&raw, &raw_dest)],
            placeholders,
            &mut |src, dest| {
                if src == raw {
                    // A sync client drops a new IMG_1.jpg where ours used to be
                    fs::write(&jpg_path, "other process").unwrap();
                    return Err("disk full".into());
                }
                move_file(src, dest)
            },
        )
        .unwrap_err();

        assert!(err.contains("could not put back"), "{}", err);
        assert_eq!(read(&jpg), "other process");
        assert_eq!(read(&jpg_dest), "jpg", "ours stays where it was moved");
        assert!(!raw_dest.exists());
    }

    #[test]
    fn a_file_that_cannot_be_moved_back_stays_at_its_destination() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "jpg");
        let raw = write(&tmp.path().join("IMG_1.ARW"), "raw");
        let (jpg_dest, raw_dest) = (
            tmp.path().join("精选/IMG_1.jpg"),
            tmp.path().join("精选/IMG_1.ARW"),
        );
        let placeholders = claim(&[&jpg_dest, &raw_dest]);

        let err = move_group(
            &[step(&jpg, &jpg_dest), step(&raw, &raw_dest)],
            placeholders,
            &mut |src, dest| {
                if src == raw || src == jpg_dest {
                    return Err("volume went away".into());
                }
                move_file(src, dest)
            },
        )
        .unwrap_err();

        assert!(err.contains("could not put back"), "{}", err);
        assert_eq!(read(&jpg_dest), "jpg");
        assert!(
            !jpg.exists(),
            "the reclaimed original location is removed again"
        );
        assert_eq!(read(&raw), "raw");
    }

    #[test]
    fn the_preview_copy_waits_for_every_move() {
        let tmp = TempDir::new().unwrap();
        let preview = write(&tmp.path().join("cache/abc.jpg"), "preview");
        let raw = write(&tmp.path().join("IMG_2.CR3"), "raw");
        let (preview_dest, raw_dest) = (
            tmp.path().join("精选/IMG_2.jpg"),
            tmp.path().join("精选/IMG_2.CR3"),
        );
        let placeholders = claim(&[&preview_dest, &raw_dest]);
        let copy = Step {
            src: &preview,
            dest: &preview_dest,
            transfer: Transfer::CopyPreview,
        };

        move_group(
            &[copy, step(&raw, &raw_dest)],
            placeholders,
            &mut failing_on(&raw),
        )
        .unwrap_err();

        assert!(!preview_dest.exists(), "never copied, placeholder removed");
        assert_eq!(read(&preview), "preview");
        assert_eq!(read(&raw), "raw");
    }

    #[test]
    fn a_source_that_vanished_never_gets_an_empty_file_in_its_place() {
        let tmp = TempDir::new().unwrap();
        let jpg = write(&tmp.path().join("IMG_1.jpg"), "jpg");
        let raw = tmp.path().join("IMG_1.ARW");
        let (jpg_dest, raw_dest) = (
            tmp.path().join("精选/IMG_1.jpg"),
            tmp.path().join("精选/IMG_1.ARW"),
        );
        let placeholders = claim(&[&jpg_dest, &raw_dest]);

        // Something else removed the RAW after the archive planned it
        move_group(
            &[step(&jpg, &jpg_dest), step(&raw, &raw_dest)],
            placeholders,
            &mut move_file,
        )
        .unwrap_err();

        assert!(!raw.exists(), "no empty RAW appears where the real one was");
        assert!(!raw_dest.exists());
        assert_eq!(read(&jpg), "jpg");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_never_removed_as_a_placeholder() {
        let tmp = TempDir::new().unwrap();
        let target = write(&tmp.path().join("empty.jpg"), "");
        let link = tmp.path().join("link.jpg");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        remove_empty_placeholders(std::slice::from_ref(&link), &[]);

        assert!(link.exists());
    }
}

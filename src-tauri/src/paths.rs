// ============================================================
// Sift - Path Utilities
// Collision-free destinations and containment checks for file operations
// ============================================================

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

/// Collision-free destinations for a group of files that pair by `stem`
/// (a RAW, its JPG and XMP sidecars). `candidates` are the preferred paths and
/// may sit in different folders (archive puts the JPG in `JPG/`, the RAW in
/// `RAW/`); none of the returned paths exist yet.
///
/// Every name gets the same `_N` suffix right after `stem`, using the smallest N
/// that is free for all of them, so `DSC_0001.ARW` + `DSC_0001.ARW.xmp` become
/// `DSC_0001_1.ARW` + `DSC_0001_1.ARW.xmp` and still pair up when rescanned.
pub fn unique_paths_sharing_stem(candidates: &[PathBuf], stem: &str) -> Vec<PathBuf> {
    first_free_suffix(candidates, stem, |_, path| !path.exists())
}

/// `unique_paths_sharing_stem` for a group where some files already sit at
/// their candidate path (`sources[i] == candidates[i]`), such as a JPG that
/// archive leaves in place while its RAW moves. Such a file keeps its name
/// while the group needs no suffix, and is renamed in place to the shared
/// `_N` once it does, so the group still pairs up.
pub fn unique_paths_sharing_stem_from(
    sources: &[&Path],
    candidates: &[PathBuf],
    stem: &str,
) -> Vec<PathBuf> {
    first_free_suffix(candidates, stem, |i, path| {
        path == sources[i] || !path.exists()
    })
}

/// Candidates with the smallest `_N` suffix after `stem` for which every
/// path passes `is_free(index, path)`; no suffix when they all pass as is.
fn first_free_suffix(
    candidates: &[PathBuf],
    stem: &str,
    is_free: impl Fn(usize, &Path) -> bool,
) -> Vec<PathBuf> {
    (0u32..)
        .map(|n| {
            let suffix = if n == 0 {
                String::new()
            } else {
                format!("_{}", n)
            };
            candidates
                .iter()
                .map(|path| with_suffix(path, stem, &suffix))
                .collect::<Vec<_>>()
        })
        .find(|paths| paths.iter().enumerate().all(|(i, p)| is_free(i, p)))
        .expect("some suffix is always free")
}

/// How many suffixes `claim_first_free` tries before giving up. Each retry
/// means another process created one of our names in the instant between
/// checking and claiming, so hitting this is practically impossible.
const MAX_CLAIM_ATTEMPTS: usize = 100;

/// Reserve the destinations returned by `choose` (usually
/// `unique_paths_sharing_stem`) by creating each as an empty file with
/// `create_new`. That is atomic: if a file appears at a name after it was
/// checked, the claim fails instead of the caller's copy overwriting it.
/// Callers then write over their own placeholders (`fs::copy` keeps the
/// source's metadata, unlike writing through the handle).
///
/// When a name turns out to be taken, `choose` is asked again, and the whole
/// group moves on to the next free suffix so it keeps sharing one stem.
/// Every placeholder this call creates is pushed onto `created`, including
/// those of an abandoned suffix and those created before an error, so the
/// caller can clean up after itself (`organize::transfer`).
pub(crate) fn claim_first_free(
    mut choose: impl FnMut() -> Vec<PathBuf>,
    created: &mut Vec<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    let mut last_err = None;
    for _ in 0..MAX_CLAIM_ATTEMPTS {
        let paths = choose();
        let claimed = paths.iter().try_for_each(|p| -> io::Result<()> {
            create_new(p)?;
            created.push(p.clone());
            Ok(())
        });
        match claimed {
            Ok(()) => return Ok(paths),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("MAX_CLAIM_ATTEMPTS is not zero"))
}

/// Claim `path` alone with `create_new`, as `claim_first_free` does for a group.
pub(crate) fn claim_one(path: &Path) -> io::Result<()> {
    create_new(path)
}

fn create_new(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
}

fn with_suffix(path: &Path, stem: &str, suffix: &str) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(insert_suffix(&name, stem, suffix))
}

/// Insert `suffix` after `stem` when `name` starts with it (`IMG.CR3.xmp` → `IMG_1.CR3.xmp`),
/// otherwise before the last extension. The stem matches ignoring ASCII case, as
/// pairing does (`img.cr3.xmp` → `img_1.cr3.xmp`); `name` keeps its own spelling.
fn insert_suffix(name: &str, stem: &str, suffix: &str) -> String {
    let after_stem = name
        .get(..stem.len())
        .filter(|head| head.eq_ignore_ascii_case(stem))
        .map(|_| &name[stem.len()..]);
    let split = match after_stem {
        Some(rest) if !stem.is_empty() && (rest.is_empty() || rest.starts_with('.')) => stem.len(),
        _ => name.rfind('.').filter(|&i| i > 0).unwrap_or(name.len()),
    };
    format!("{}{}{}", &name[..split], suffix, &name[split..])
}

/// True only if `path` exists and, with symlinks and `..` resolved, lies
/// strictly inside `dir`. Gate for every `remove_file` on a cache path.
pub fn is_inside_dir(path: &Path, dir: &Path) -> bool {
    match (path.canonicalize(), dir.canonicalize()) {
        (Ok(path), Ok(dir)) => path != dir && path.starts_with(&dir),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn in_dir(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| dir.join(n)).collect()
    }

    #[test]
    fn keeps_original_names_when_all_are_free() {
        let dir = tempdir().unwrap();
        let group = in_dir(dir.path(), &["DSC_0001.JPG", "DSC_0001.ARW"]);

        let got = unique_paths_sharing_stem(&group, "DSC_0001");

        assert_eq!(
            got,
            vec![
                dir.path().join("DSC_0001.JPG"),
                dir.path().join("DSC_0001.ARW")
            ]
        );
    }

    #[test]
    fn uses_smallest_counter_free_for_the_whole_group() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("DSC_0001.ARW"), "a").unwrap();
        fs::write(dir.path().join("DSC_0001_1.JPG"), "b").unwrap();
        let group = in_dir(dir.path(), &["DSC_0001.JPG", "DSC_0001.ARW"]);

        let got = unique_paths_sharing_stem(&group, "DSC_0001");

        assert_eq!(
            got,
            vec![
                dir.path().join("DSC_0001_2.JPG"),
                dir.path().join("DSC_0001_2.ARW")
            ]
        );
    }

    #[test]
    fn renames_whole_group_when_one_member_collides() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("DSC_0001.xmp"), "existing").unwrap();
        let group = in_dir(
            dir.path(),
            &["DSC_0001.ARW", "DSC_0001.xmp", "DSC_0001.ARW.xmp"],
        );

        let got = unique_paths_sharing_stem(&group, "DSC_0001");

        let expected: Vec<PathBuf> = ["DSC_0001_1.ARW", "DSC_0001_1.xmp", "DSC_0001_1.ARW.xmp"]
            .iter()
            .map(|n| dir.path().join(n))
            .collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn a_collision_in_one_folder_renames_the_group_in_every_folder() {
        let dir = tempdir().unwrap();
        let jpg_dir = dir.path().join("JPG");
        let raw_dir = dir.path().join("RAW");
        fs::create_dir_all(&jpg_dir).unwrap();
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(jpg_dir.join("IMG_1.jpg"), "existing").unwrap();
        let group = vec![jpg_dir.join("IMG_1.jpg"), raw_dir.join("IMG_1.ARW")];

        let got = unique_paths_sharing_stem(&group, "IMG_1");

        assert_eq!(
            got,
            vec![jpg_dir.join("IMG_1_1.jpg"), raw_dir.join("IMG_1_1.ARW")]
        );
    }

    #[test]
    fn a_file_already_at_its_candidate_keeps_its_name_while_the_group_is_free() {
        let dir = tempdir().unwrap();
        let jpg = dir.path().join("IMG_1.jpg");
        fs::write(&jpg, "stays").unwrap();
        let raw_dir = dir.path().join("RAW");
        let group = vec![jpg.clone(), raw_dir.join("IMG_1.ARW")];

        let got =
            unique_paths_sharing_stem_from(&[&jpg, &dir.path().join("IMG_1.ARW")], &group, "IMG_1");

        assert_eq!(got, group);
    }

    #[test]
    fn a_file_already_at_its_candidate_is_renamed_with_the_group() {
        let dir = tempdir().unwrap();
        let jpg = dir.path().join("IMG_1.jpg");
        fs::write(&jpg, "stays").unwrap();
        let raw_dir = dir.path().join("RAW");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("IMG_1.ARW"), "other card").unwrap();
        fs::write(dir.path().join("IMG_1_1.jpg"), "another photo").unwrap();
        let group = vec![jpg.clone(), raw_dir.join("IMG_1.ARW")];

        let got =
            unique_paths_sharing_stem_from(&[&jpg, &dir.path().join("IMG_1.ARW")], &group, "IMG_1");

        assert_eq!(
            got,
            vec![dir.path().join("IMG_1_2.jpg"), raw_dir.join("IMG_1_2.ARW")]
        );
    }

    #[test]
    fn insert_suffix_falls_back_to_last_extension_when_stem_differs() {
        assert_eq!(insert_suffix("other.xmp", "DSC_0001", "_1"), "other_1.xmp");
        assert_eq!(
            insert_suffix("DSC_00010.ARW", "DSC_0001", "_1"),
            "DSC_00010_1.ARW"
        );
        assert_eq!(insert_suffix("README", "", "_1"), "README_1");
        assert_eq!(insert_suffix(".hidden", "", "_1"), ".hidden_1");
    }

    #[test]
    fn insert_suffix_matches_stem_ignoring_ascii_case_and_keeps_the_name_case() {
        assert_eq!(
            insert_suffix("img_1.cr3.xmp", "IMG_1", "_1"),
            "img_1_1.cr3.xmp"
        );
        assert_eq!(insert_suffix("img_1.JPG", "IMG_1", "_2"), "img_1_2.JPG");
        assert_eq!(insert_suffix("img_10.jpg", "IMG_1", "_1"), "img_10_1.jpg");
        assert_eq!(insert_suffix("Über.xmp", "über", "_1"), "Über_1.xmp");
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn accepts_files_inside_the_dir_at_any_depth() {
        let tmp = tempdir().unwrap();
        let cache = tmp.path().join("cache");
        touch(&cache.join("a.jpg"));
        touch(&cache.join("sub/b.jpg"));

        assert!(is_inside_dir(&cache.join("a.jpg"), &cache));
        assert!(is_inside_dir(&cache.join("sub/b.jpg"), &cache));
    }

    #[test]
    fn rejects_the_dir_itself_siblings_and_missing_files() {
        let tmp = tempdir().unwrap();
        let cache = tmp.path().join("cache");
        touch(&cache.join("a.jpg"));
        touch(&tmp.path().join("cache2/a.jpg"));
        touch(&tmp.path().join("photo.jpg"));

        assert!(!is_inside_dir(&cache, &cache));
        assert!(!is_inside_dir(&tmp.path().join("cache2/a.jpg"), &cache));
        assert!(!is_inside_dir(&tmp.path().join("photo.jpg"), &cache));
        assert!(!is_inside_dir(&cache.join("missing.jpg"), &cache));
        assert!(!is_inside_dir(
            &cache.join("a.jpg"),
            &tmp.path().join("no-cache")
        ));
    }

    #[test]
    fn rejects_dot_dot_escapes() {
        let tmp = tempdir().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        touch(&tmp.path().join("photo.jpg"));

        assert!(!is_inside_dir(&cache.join("../photo.jpg"), &cache));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_point_outside() {
        let tmp = tempdir().unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let photo = tmp.path().join("photo.jpg");
        touch(&photo);
        std::os::unix::fs::symlink(&photo, cache.join("link.jpg")).unwrap();

        assert!(!is_inside_dir(&cache.join("link.jpg"), &cache));
    }

    fn is_empty_file(path: &Path) -> bool {
        fs::metadata(path)
            .map(|m| m.is_file() && m.len() == 0)
            .unwrap_or(false)
    }

    #[test]
    fn claim_reserves_every_destination_with_an_empty_file() {
        let dir = tempdir().unwrap();
        let group = in_dir(dir.path(), &["DSC_0001.JPG", "DSC_0001.ARW"]);

        let got = claim_first_free(
            || unique_paths_sharing_stem(&group, "DSC_0001"),
            &mut Vec::new(),
        )
        .unwrap();

        assert_eq!(got, group);
        assert!(got.iter().all(|p| is_empty_file(p)));
    }

    #[test]
    fn claim_moves_the_group_on_when_a_name_is_taken_after_it_was_chosen() {
        let dir = tempdir().unwrap();
        let group = in_dir(dir.path(), &["DSC_0001.ARW", "DSC_0001.xmp"]);
        let mut is_first_try = true;

        // Another process creates DSC_0001.xmp between the existence check
        // and the claim: exactly the window `create_new` has to close.
        let mut created = Vec::new();
        let got = claim_first_free(
            || {
                let chosen = unique_paths_sharing_stem(&group, "DSC_0001");
                if std::mem::take(&mut is_first_try) {
                    fs::write(&chosen[1], "other process").unwrap();
                }
                chosen
            },
            &mut created,
        )
        .unwrap();

        assert_eq!(
            got,
            in_dir(dir.path(), &["DSC_0001_1.ARW", "DSC_0001_1.xmp"])
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("DSC_0001.xmp")).unwrap(),
            "other process"
        );
        assert!(
            is_empty_file(&dir.path().join("DSC_0001.ARW")),
            "the abandoned placeholder is left for the caller"
        );
        assert_eq!(
            created,
            in_dir(
                dir.path(),
                &["DSC_0001.ARW", "DSC_0001_1.ARW", "DSC_0001_1.xmp"]
            ),
            "every placeholder created is reported, abandoned ones included"
        );
    }

    #[test]
    fn claim_gives_up_when_names_keep_getting_taken() {
        let dir = tempdir().unwrap();
        let group = in_dir(dir.path(), &["DSC_0001.JPG"]);

        let err = claim_first_free(
            || {
                let chosen = unique_paths_sharing_stem(&group, "DSC_0001");
                fs::write(&chosen[0], "someone else").unwrap();
                chosen
            },
            &mut Vec::new(),
        )
        .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn claim_reports_placeholders_created_before_an_error() {
        let dir = tempdir().unwrap();
        let mut group = in_dir(dir.path(), &["DSC_0001.JPG"]);
        group.extend(in_dir(&dir.path().join("missing"), &["DSC_0001.ARW"]));
        let mut created = Vec::new();

        let err = claim_first_free(|| group.clone(), &mut created).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(created, group[..1]);
    }

    #[test]
    fn claim_reports_other_io_errors() {
        let dir = tempdir().unwrap();
        let group = in_dir(&dir.path().join("missing"), &["DSC_0001.JPG"]);

        let err = claim_first_free(
            || unique_paths_sharing_stem(&group, "DSC_0001"),
            &mut Vec::new(),
        )
        .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}

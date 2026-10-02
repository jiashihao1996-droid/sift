// ============================================================
// Sift - Path Utilities
// Collision-free destinations and containment checks for file operations
// ============================================================

use std::path::{Path, PathBuf};

/// Destinations in `dir` for a group of files that pair by `stem`
/// (a RAW, its JPG and XMP sidecars), none of which exist yet.
///
/// Every name gets the same `_N` suffix right after `stem`, using the smallest N
/// that is free for all of them, so `DSC_0001.ARW` + `DSC_0001.ARW.xmp` become
/// `DSC_0001_1.ARW` + `DSC_0001_1.ARW.xmp` and still pair up when rescanned.
pub fn unique_paths_sharing_stem(dir: &Path, names: &[String], stem: &str) -> Vec<PathBuf> {
    (0u32..)
        .map(|n| {
            let suffix = if n == 0 {
                String::new()
            } else {
                format!("_{}", n)
            };
            names
                .iter()
                .map(|name| dir.join(insert_suffix(name, stem, &suffix)))
                .collect::<Vec<_>>()
        })
        .find(|candidates| candidates.iter().all(|p| !p.exists()))
        .expect("some suffix is always free")
}

/// Insert `suffix` after `stem` when `name` starts with it (`IMG.CR3.xmp` → `IMG_1.CR3.xmp`),
/// otherwise before the last extension.
fn insert_suffix(name: &str, stem: &str, suffix: &str) -> String {
    let split = match name.strip_prefix(stem) {
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

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keeps_original_names_when_all_are_free() {
        let dir = tempdir().unwrap();
        let group = names(&["DSC_0001.JPG", "DSC_0001.ARW"]);

        let got = unique_paths_sharing_stem(dir.path(), &group, "DSC_0001");

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
        let group = names(&["DSC_0001.JPG", "DSC_0001.ARW"]);

        let got = unique_paths_sharing_stem(dir.path(), &group, "DSC_0001");

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
        let group = names(&["DSC_0001.ARW", "DSC_0001.xmp", "DSC_0001.ARW.xmp"]);

        let got = unique_paths_sharing_stem(dir.path(), &group, "DSC_0001");

        let expected: Vec<PathBuf> = ["DSC_0001_1.ARW", "DSC_0001_1.xmp", "DSC_0001_1.ARW.xmp"]
            .iter()
            .map(|n| dir.path().join(n))
            .collect();
        assert_eq!(got, expected);
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
}

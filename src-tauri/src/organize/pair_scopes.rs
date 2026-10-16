// ============================================================
// Sift - Pairing Scopes During an Archive
// Scan pairs files by stem within two scopes: 精选/ on its own, and the
// folder together with RAW/ and JPG/. A file archive moves into the other
// scope must not arrive next to a different photo of the same stem, or a
// rescan would pair the two (and deleting one would trash both). So a name
// counts as taken when the scope it enters already holds a photo with that
// stem, even in another of its folders or with another extension. The
// stems are listed once per archive and kept up to date as groups move.
// ============================================================

use super::archive::is_already_in_dir;
use crate::catalog::file_types::{is_jpg, is_raw};
use crate::paths;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Scope {
    Picks,
    Loose,
}

pub(super) struct PairScopes {
    picks_dir: PathBuf,
    /// (scope, case-folded stem) -> photos with that stem
    stems: HashMap<(Scope, String), usize>,
}

impl PairScopes {
    /// List the photos of `picks_dir` and of `loose_dirs` (the folder, RAW/, JPG/)
    pub(super) fn read(picks_dir: &Path, loose_dirs: &[&Path]) -> Self {
        let mut scopes = Self {
            picks_dir: picks_dir.to_path_buf(),
            stems: HashMap::new(),
        };
        let dirs = std::iter::once((Scope::Picks, picks_dir))
            .chain(loose_dirs.iter().map(|dir| (Scope::Loose, *dir)));
        for (scope, dir) in dirs {
            for photo in photos_in(dir) {
                if let Some(key) = stem_key(&photo) {
                    *scopes.stems.entry((scope, key)).or_default() += 1;
                }
            }
        }
        scopes
    }

    fn scope_of(&self, path: &Path) -> Scope {
        match is_already_in_dir(path, &self.picks_dir) {
            true => Scope::Picks,
            false => Scope::Loose,
        }
    }

    /// Whether moving `src` to `dest` would put it next to another photo of
    /// the same stem in a scope it was not paired in before. The group's own
    /// photos (`group`) don't count: a RAW-only pick in 精选/ getting its
    /// preview back is not meeting anyone.
    fn meets_another_photo(&self, src: &Path, dest: &Path, group: &[&Path]) -> bool {
        let Some(key) = stem_key(dest) else {
            return false;
        };
        let scope = self.scope_of(dest);
        if scope == self.scope_of(src) {
            return false;
        }
        let own = group
            .iter()
            .filter(|path| stem_key(path).as_ref() == Some(&key) && self.scope_of(path) == scope)
            .count();
        self.stems.get(&(scope, key)).is_some_and(|&n| n > own)
    }

    /// `paths::unique_paths_sharing_stem_from`, also skipping names whose
    /// stem is taken in the scope a file enters
    pub(super) fn unique_paths(
        &self,
        sources: &[&Path],
        candidates: &[PathBuf],
        stem: &str,
    ) -> Vec<PathBuf> {
        paths::unique_paths_sharing_stem_from(sources, candidates, stem, |src, dest| {
            self.meets_another_photo(src, dest, sources)
        })
    }

    /// Account for a group that moved: each photo leaves its old scope and
    /// joins its new one. Call with what `unique_paths` chose.
    pub(super) fn record_moves(&mut self, sources: &[&Path], dests: &[PathBuf]) {
        for (src, dest) in sources.iter().zip(dests) {
            let (Some(from), Some(to)) = (stem_key(src), stem_key(dest)) else {
                continue;
            };
            if let Some(n) = self.stems.get_mut(&(self.scope_of(src), from)) {
                *n = n.saturating_sub(1);
            }
            *self.stems.entry((self.scope_of(dest), to)).or_default() += 1;
        }
    }
}

/// The pairing key of a JPG or RAW, ASCII case folded as in
/// `catalog::pairing`; None for anything else (XMP sidecars go with a photo)
fn stem_key(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if !is_jpg(&ext) && !is_raw(&ext) {
        return None;
    }
    Some(path.file_stem()?.to_str()?.to_ascii_lowercase())
}

/// Photo files directly in `dir`; a folder that is missing or unreadable has none
fn photos_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| !e.file_name().as_encoded_bytes().starts_with(b"."))
        .map(|e| e.path())
        .filter(|path| path.is_file() && stem_key(path).is_some())
        .collect()
}

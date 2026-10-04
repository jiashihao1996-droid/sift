// ============================================================
// Sift - Organize Error Classification
// Turns io and trash errors into an `OrganizeError` the UI can explain.
// The original message is always kept in `detail`.
// ============================================================

use crate::models::organize::{OrganizeError, OrganizeErrorKind};
use std::io;
use std::path::Path;

/// AppleScript error "not authorized to send Apple events", which Finder-based
/// trashing hits when the user denied Sift the Automation permission
const APPLE_EVENTS_NOT_PERMITTED: &str = "-1743";

impl OrganizeError {
    pub fn new(kind: OrganizeErrorKind, path: Option<&Path>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            detail: detail.into(),
            completed: 0,
        }
    }

    pub fn other(detail: impl Into<String>) -> Self {
        Self::new(OrganizeErrorKind::Other, None, detail)
    }

    /// An io error while handling `path`; `action` says what was attempted.
    pub fn io(error: &io::Error, path: &Path, action: &str) -> Self {
        let detail = format!("{} {}: {}", action, path.display(), error);
        Self::new(classify_io(error), Some(path), detail)
    }

    /// A failure to move `path` to the trash.
    pub fn trash(error: &trash::Error, path: &Path) -> Self {
        let detail = format!("Failed to trash {}: {}", path.display(), error);
        Self::new(classify_trash(error, path), Some(path), detail)
    }

    /// Prefix the detail with what the whole operation was doing.
    pub fn context(mut self, context: impl std::fmt::Display) -> Self {
        self.detail = format!("{}: {}", context, self.detail);
        self
    }

    pub fn with_completed(mut self, completed: usize) -> Self {
        self.completed = completed;
        self
    }
}

pub fn classify_io(error: &io::Error) -> OrganizeErrorKind {
    use io::ErrorKind as K;
    match error.kind() {
        K::PermissionDenied => OrganizeErrorKind::PermissionDenied,
        K::NotFound => OrganizeErrorKind::NotFound,
        K::StorageFull | K::QuotaExceeded => OrganizeErrorKind::DiskFull,
        K::ReadOnlyFilesystem => OrganizeErrorKind::ReadOnly,
        _ if is_write_protected(error) => OrganizeErrorKind::ReadOnly,
        _ => OrganizeErrorKind::Other,
    }
}

/// Windows reports a locked SD card as ERROR_WRITE_PROTECT (19), which std
/// does not map to `ReadOnlyFilesystem`.
fn is_write_protected(error: &io::Error) -> bool {
    cfg!(windows) && error.raw_os_error() == Some(19)
}

fn classify_trash(error: &trash::Error, path: &Path) -> OrganizeErrorKind {
    match error {
        trash::Error::Os { description, .. }
            if description.contains(APPLE_EVENTS_NOT_PERMITTED) =>
        {
            OrganizeErrorKind::AutomationDenied
        }
        trash::Error::CouldNotAccess { .. } if !path.exists() => OrganizeErrorKind::NotFound,
        trash::Error::CouldNotAccess { .. } => OrganizeErrorKind::PermissionDenied,
        _ => OrganizeErrorKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(kind: io::ErrorKind) -> OrganizeErrorKind {
        classify_io(&io::Error::from(kind))
    }

    #[test]
    fn classifies_the_io_errors_users_can_act_on() {
        assert_eq!(
            kind_of(io::ErrorKind::PermissionDenied),
            OrganizeErrorKind::PermissionDenied
        );
        assert_eq!(
            kind_of(io::ErrorKind::NotFound),
            OrganizeErrorKind::NotFound
        );
        assert_eq!(
            kind_of(io::ErrorKind::StorageFull),
            OrganizeErrorKind::DiskFull
        );
        assert_eq!(
            kind_of(io::ErrorKind::ReadOnlyFilesystem),
            OrganizeErrorKind::ReadOnly
        );
        assert_eq!(
            kind_of(io::ErrorKind::Interrupted),
            OrganizeErrorKind::Other
        );
    }

    #[cfg(unix)]
    #[test]
    fn classifies_raw_unix_errors_through_std() {
        // ENOSPC and EROFS
        assert_eq!(
            classify_io(&io::Error::from_raw_os_error(28)),
            OrganizeErrorKind::DiskFull
        );
        assert_eq!(
            classify_io(&io::Error::from_raw_os_error(30)),
            OrganizeErrorKind::ReadOnly
        );
    }

    #[test]
    fn keeps_the_path_and_original_message() {
        let error = io::Error::from(io::ErrorKind::PermissionDenied);

        let err = OrganizeError::io(&error, Path::new("/card/IMG_1.ARW"), "Failed to move")
            .context("Failed to archive IMG_1");

        assert_eq!(err.path.as_deref(), Some("/card/IMG_1.ARW"));
        assert!(err
            .detail
            .starts_with("Failed to archive IMG_1: Failed to move /card/IMG_1.ARW"));
    }

    #[test]
    fn recognizes_a_denied_finder_automation_permission() {
        let error = trash::Error::Os {
            code: 1,
            description: "execution error: Not authorized to send Apple events to Finder. (-1743)"
                .into(),
        };

        let err = OrganizeError::trash(&error, Path::new("/photos/IMG_1.jpg"));

        assert_eq!(err.kind, OrganizeErrorKind::AutomationDenied);
    }

    #[test]
    fn a_trash_target_that_is_gone_is_not_found() {
        let error = trash::Error::CouldNotAccess {
            target: "/photos/gone.jpg".into(),
        };

        let err = OrganizeError::trash(&error, Path::new("/photos/gone.jpg"));

        assert_eq!(err.kind, OrganizeErrorKind::NotFound);
    }
}

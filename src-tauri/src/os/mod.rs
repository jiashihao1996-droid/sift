//! Hand a user file to the operating system: reveal it in the file manager,
//! copy it to the clipboard; and ask it whether a volume has a trash.
//!
//! Paths come from the frontend, so they never become part of a script's
//! source text: they go to the child process as a separate argument or an
//! environment variable, where no quoting can break out of them.

pub mod clipboard;
pub mod reveal;
pub mod trash_support;

use std::path::Path;

/// Paths must be absolute so none of them can be read as a command-line
/// option (`-e`, `/select`, …) by the tools we hand them to.
fn ensure_absolute(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!("Not an absolute path: {}", path.display()))
    }
}

//! Reveal a file in the system file manager.

use std::path::Path;
use std::process::Command;

pub fn reveal(path: &Path) -> Result<(), String> {
    super::ensure_absolute(path)?;
    run(path)
}

#[cfg(target_os = "macos")]
fn run(path: &Path) -> Result<(), String> {
    let status = Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| format!("Failed to open Finder: {}", e))?;
    if !status.success() {
        return Err(format!("Finder could not show {}", path.display()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn run(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    // Explorer exits with 1 even when it succeeds, so only a failure to
    // start it is an error.
    Command::new("explorer")
        .raw_arg(explorer_select_arg(path)?)
        .status()
        .map_err(|e| format!("Failed to open Explorer: {}", e))?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let status = Command::new("xdg-open")
        .arg(parent)
        .status()
        .map_err(|e| format!("Failed to open file manager: {}", e))?;
    if !status.success() {
        return Err(format!("File manager could not show {}", path.display()));
    }
    Ok(())
}

/// Explorer's `/select,"<path>"` argument. Explorer does not parse its command
/// line like other programs, so this is passed verbatim (`raw_arg`): the
/// standard quoting would wrap a path with spaces as `"/select,\"…\""`, which
/// Explorer does not understand. Windows paths cannot contain `"`, so a path
/// that does is rejected rather than allowed to end the quoted string.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn explorer_select_arg(path: &Path) -> Result<String, String> {
    let path = path.to_string_lossy();
    if path.contains('"') {
        return Err(format!("Invalid path: {}", path));
    }
    Ok(format!("/select,\"{}\"", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_arg_quotes_paths_with_spaces_and_unicode() {
        let arg = explorer_select_arg(Path::new(r"C:\My Photos\中文 it’s & $(x).jpg")).unwrap();
        assert_eq!(arg, r#"/select,"C:\My Photos\中文 it’s & $(x).jpg""#);
    }

    #[test]
    fn explorer_arg_rejects_a_quote_that_would_end_the_path() {
        assert!(explorer_select_arg(Path::new(r#"C:\a" /root,"C:\b.jpg"#)).is_err());
    }

    #[test]
    fn rejects_relative_paths() {
        assert!(reveal(Path::new("-a")).is_err());
    }
}

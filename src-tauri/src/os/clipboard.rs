//! Copy an image file to the system clipboard.

use std::path::Path;
use std::process::Command;

pub fn copy_image(path: &Path) -> Result<(), String> {
    super::ensure_absolute(path)?;
    let output = copy_image_command(path)
        .output()
        .map_err(|e| format!("Failed to start clipboard helper: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Copy to clipboard failed: {}", stderr.trim()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_image_command(path: &Path) -> Command {
    macos_command(path)
}

#[cfg(target_os = "windows")]
fn copy_image_command(path: &Path) -> Command {
    windows_command(path)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn copy_image_command(path: &Path) -> Command {
    let mut cmd = Command::new("xclip");
    cmd.args(["-selection", "clipboard", "-t", "image/png", "-i"])
        .arg(path);
    cmd
}

/// AppleScript class for the pasteboard type matching the file extension.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_clipboard_class(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "«class PNGf»",
        "tiff" | "tif" => "«class TIFF»",
        _ => "«class JPEG»",
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_command(path: &Path) -> Command {
    let set_clipboard = format!(
        "set the clipboard to (read (POSIX file (item 1 of argv)) as {})",
        macos_clipboard_class(path)
    );
    osascript_with_path(&["on run argv", &set_clipboard, "end run"], path)
}

/// osascript running `statements`, with `path` as the run handler's only
/// argument (`item 1 of argv`). `--` ends osascript's own options.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn osascript_with_path(statements: &[&str], path: &Path) -> Command {
    let mut cmd = Command::new("osascript");
    for statement in statements {
        cmd.arg("-e").arg(statement);
    }
    cmd.arg("--").arg(path);
    cmd
}

/// Environment variable carrying the image path to the PowerShell script.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const WINDOWS_PATH_ENV: &str = "SIFT_CLIPBOARD_IMAGE";

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_command(path: &Path) -> Command {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; \
         [System.Windows.Forms.Clipboard]::SetImage(\
         [System.Drawing.Image]::FromFile($env:{WINDOWS_PATH_ENV}))"
    );
    powershell_with_path(&script, path)
}

/// PowerShell running the fixed `script`, with `path` in `WINDOWS_PATH_ENV`.
/// The script is passed UTF-16LE Base64-encoded so the command line needs
/// no quoting.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn powershell_with_path(script: &str, path: &Path) -> Command {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let utf16le: Vec<u8> = script
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(STANDARD.encode(utf16le))
        .env(WINDOWS_PATH_ENV, path);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// Names a user could give a photo, each breaking a naively quoted
    /// AppleScript or PowerShell string in a different way.
    const HOSTILE_NAMES: &[&str] = &[
        "a\" & (do shell script \"echo INJECTED\") & \".jpg",
        "it's.jpg",
        "it\u{2019}s ‘quoted’ ‛x‚.jpg",
        "a & b.jpg",
        "$(echo INJECTED).jpg",
        "`echo INJECTED`.jpg",
        "with space.jpg",
        "line\nbreak.jpg",
        "中文 照片.jpg",
    ];

    fn hostile_paths() -> Vec<std::path::PathBuf> {
        HOSTILE_NAMES
            .iter()
            .map(|name| Path::new("/Users/me/Photos").join(name))
            .collect()
    }

    fn args(cmd: &Command) -> Vec<&OsStr> {
        cmd.get_args().collect()
    }

    /// Everything the child process parses as code, i.e. every argument
    /// except the path itself.
    fn code_args<'a>(cmd: &'a Command, path: &Path) -> Vec<&'a OsStr> {
        args(cmd)
            .into_iter()
            .filter(|a| *a != path.as_os_str())
            .collect()
    }

    #[test]
    fn macos_script_never_contains_the_path() {
        for path in hostile_paths() {
            let cmd = macos_command(&path);
            let name = path.file_name().unwrap().to_str().unwrap();
            for arg in code_args(&cmd, &path) {
                assert!(
                    !arg.to_string_lossy().contains(name),
                    "path spliced into osascript code: {arg:?}"
                );
            }
        }
    }

    #[test]
    fn macos_passes_the_path_as_the_last_argument_after_double_dash() {
        for path in hostile_paths() {
            let cmd = macos_command(&path);
            let all = args(&cmd);
            assert_eq!(all[all.len() - 2], OsStr::new("--"));
            assert_eq!(all[all.len() - 1], path.as_os_str());
        }
    }

    #[test]
    fn macos_picks_the_pasteboard_class_from_the_extension() {
        assert_eq!(macos_clipboard_class(Path::new("/a/b.PNG")), "«class PNGf»");
        assert_eq!(macos_clipboard_class(Path::new("/a/b.tif")), "«class TIFF»");
        assert_eq!(
            macos_clipboard_class(Path::new("/a/b.jpeg")),
            "«class JPEG»"
        );
        assert_eq!(macos_clipboard_class(Path::new("/a/b")), "«class JPEG»");
    }

    #[test]
    fn windows_script_never_contains_the_path() {
        for path in hostile_paths() {
            let cmd = windows_command(&path);
            for arg in args(&cmd) {
                assert_ne!(arg, path.as_os_str());
            }
            // The script is the same constant for every path.
            assert_eq!(args(&cmd), args(&windows_command(Path::new("/x.jpg"))));
        }
    }

    #[test]
    fn windows_passes_the_path_in_the_environment() {
        for path in hostile_paths() {
            let cmd = windows_command(&path);
            let env: Vec<_> = cmd.get_envs().collect();
            assert_eq!(
                env,
                [(OsStr::new(WINDOWS_PATH_ENV), Some(path.as_os_str()))]
            );
        }
    }

    #[test]
    fn rejects_relative_paths() {
        assert!(copy_image(Path::new("-e.jpg")).is_err());
    }

    /// The real osascript hands every hostile path back unchanged and runs
    /// none of it.
    #[cfg(target_os = "macos")]
    #[test]
    fn osascript_receives_hostile_paths_verbatim() {
        for path in hostile_paths() {
            let output =
                osascript_with_path(&["on run argv", "return item 1 of argv", "end run"], &path)
                    .output()
                    .expect("run osascript");
            assert!(output.status.success(), "{output:?}");
            let returned = String::from_utf8(output.stdout).unwrap();
            assert_eq!(returned.strip_suffix('\n').unwrap(), path.to_str().unwrap());
        }
    }

    /// The real PowerShell reads every hostile path back unchanged from the
    /// environment (Base64 of UTF-8 so console encoding plays no part).
    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_receives_hostile_paths_verbatim() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let script = format!(
            "[Console]::Out.Write([Convert]::ToBase64String(\
             [Text.Encoding]::UTF8.GetBytes($env:{WINDOWS_PATH_ENV})))"
        );
        for path in hostile_paths() {
            let output = powershell_with_path(&script, &path)
                .output()
                .expect("run powershell");
            assert!(output.status.success(), "{output:?}");
            let returned = STANDARD.decode(output.stdout.trim_ascii()).unwrap();
            assert_eq!(String::from_utf8(returned).unwrap(), path.to_str().unwrap());
        }
    }
}

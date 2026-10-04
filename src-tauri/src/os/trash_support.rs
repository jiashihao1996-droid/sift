//! Whether files on a volume can go to the system trash at all. Network
//! shares (SMB, NAS, NFS) have none on either platform, and Windows keeps no
//! Recycle Bin on removable FAT/exFAT media such as SD cards. Deleting there
//! means deleting for good, which the user has to confirm first.
//!
//! Detection that fails reports "has a trash": the trash is then tried as
//! usual, so a failed check can never lead to a permanent delete.

use crate::models::organize::NoTrashReason;
use std::path::Path;

/// What decides whether a volume has a trash
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub is_network: bool,
    /// Removable media; only reported on Windows, where it matters
    pub is_removable: bool,
    /// File system name as the OS reports it, e.g. "exFAT", "NTFS", "apfs"
    pub filesystem: String,
}

/// Why `volume` has no trash, or `None` when it has one.
pub fn classify(volume: &Volume) -> Option<NoTrashReason> {
    if volume.is_network {
        return Some(NoTrashReason::NetworkVolume);
    }
    let is_fat = ["FAT", "FAT12", "FAT16", "FAT32", "EXFAT"]
        .iter()
        .any(|fs| volume.filesystem.eq_ignore_ascii_case(fs));
    (volume.is_removable && is_fat).then_some(NoTrashReason::RemovableFat)
}

/// Why the volume holding `path` has no trash, or `None` when it has one or
/// the check fails.
pub fn no_trash_reason(path: &Path) -> Option<NoTrashReason> {
    match platform::volume(path) {
        Ok(volume) => classify(&volume),
        Err(e) => {
            eprintln!(
                "[Sift] Could not tell whether {} has a trash: {}",
                path.display(),
                e
            );
            None
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Volume;
    use std::ffi::{CStr, CString};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    /// `statfs`: a volume without MNT_LOCAL is a network mount. Removable
    /// FAT/exFAT cards keep a `.Trashes` folder on macOS, so they count as
    /// having a trash.
    pub fn volume(path: &Path) -> io::Result<Volume> {
        let c_path = CString::new(path.as_os_str().as_bytes())?;
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::zeroed();
        // SAFETY: `c_path` is a valid NUL-terminated string and `stat` points
        // to writable memory the size of `statfs`, which the call fills in.
        let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: statfs returned 0, so it initialized the whole struct.
        let stat = unsafe { stat.assume_init() };
        // SAFETY: the kernel NUL-terminates f_fstypename.
        let filesystem = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) };
        Ok(Volume {
            is_network: stat.f_flags & (libc::MNT_LOCAL as u32) == 0,
            is_removable: false,
            filesystem: filesystem.to_string_lossy().into_owned(),
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::Volume;
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    use windows_sys::Win32::System::WindowsProgramming::{DRIVE_REMOTE, DRIVE_REMOVABLE};

    /// Generous for any volume root, `\\?\Volume{GUID}\` included
    const ROOT_LEN: usize = 1024;
    /// File system names are short ("NTFS", "exFAT")
    const FS_NAME_LEN: usize = 64;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    /// The drive type of the volume root (`GetDriveTypeW`) tells network
    /// shares and removable media apart; `GetVolumeInformationW` names the
    /// file system.
    pub fn volume(path: &Path) -> io::Result<Volume> {
        let path = wide(path.as_os_str());
        let mut root = vec![0u16; ROOT_LEN];
        // SAFETY: `path` is NUL-terminated; `root` holds ROOT_LEN u16s.
        if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), ROOT_LEN as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `root` was NUL-terminated by GetVolumePathNameW.
        let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
        let mut fs_name = vec![0u16; FS_NAME_LEN];
        // SAFETY: every out-pointer is either null (not requested) or a
        // buffer of the size passed alongside it.
        let has_info = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                FS_NAME_LEN as u32,
            )
        };
        // A network share may not report its file system: the drive type decides
        let filesystem = if has_info != 0 {
            from_wide(&fs_name)
        } else {
            String::new()
        };
        Ok(Volume {
            is_network: drive_type == DRIVE_REMOTE,
            is_removable: drive_type == DRIVE_REMOVABLE,
            filesystem,
        })
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::Volume;
    use std::io;
    use std::path::Path;

    /// Not a release target: assume a trash, so nothing is deleted for good.
    pub fn volume(_path: &Path) -> io::Result<Volume> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "trash detection is only implemented for macOS and Windows",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(is_network: bool, is_removable: bool, filesystem: &str) -> Volume {
        Volume {
            is_network,
            is_removable,
            filesystem: filesystem.into(),
        }
    }

    #[test]
    fn network_shares_have_no_trash() {
        assert_eq!(
            classify(&volume(true, false, "smbfs")),
            Some(NoTrashReason::NetworkVolume)
        );
        assert_eq!(
            classify(&volume(true, false, "")),
            Some(NoTrashReason::NetworkVolume)
        );
    }

    #[test]
    fn removable_fat_and_exfat_cards_have_no_recycle_bin() {
        for fs in ["FAT", "FAT32", "exFAT"] {
            assert_eq!(
                classify(&volume(false, true, fs)),
                Some(NoTrashReason::RemovableFat),
                "{}",
                fs
            );
        }
    }

    #[test]
    fn local_disks_and_removable_ntfs_have_a_trash() {
        assert_eq!(classify(&volume(false, false, "apfs")), None);
        assert_eq!(
            classify(&volume(false, false, "exFAT")),
            None,
            "fixed FAT disk"
        );
        assert_eq!(classify(&volume(false, true, "NTFS")), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_local_temp_folder_has_a_trash() {
        let dir = tempfile::tempdir().unwrap();
        let volume = platform::volume(dir.path()).unwrap();
        assert!(!volume.is_network, "{:?}", volume);
        assert_eq!(no_trash_reason(dir.path()), None);
    }

    #[test]
    fn a_missing_path_is_assumed_to_have_a_trash() {
        assert_eq!(no_trash_reason(Path::new("/no/such/sift/folder")), None);
    }
}

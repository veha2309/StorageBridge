//! A source-backed upload holds a read lease, rather than duplicating content in cache.
use crate::streaming::ContentSource;
use anyhow::{Context, Result, ensure};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stamp {
    pub identity: String,
    pub size: u64,
    pub modified: u128,
}
pub struct Source {
    pub path: PathBuf,
    pub stamp: Stamp,
    file: Mutex<File>,
}
fn stamp(file: &File) -> Result<Stamp> {
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "Select a regular local file");
    #[cfg(windows)]
    let identity = {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) }
                != 0,
            "Cannot identify the source file"
        );
        let info = unsafe { info.assume_init() };
        format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        )
    };
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.dev(), metadata.ino())
    };
    Ok(Stamp {
        identity,
        size: metadata.len(),
        modified: metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
    })
}
impl Source {
    pub fn open(path: &Path, move_source: bool) -> Result<Self> {
        ensure!(
            !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
            "Choose the original file, rather than a symbolic link"
        );
        let path = std::fs::canonicalize(path).context("Source file is missing or unavailable")?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Deny all writers and rename/delete access for the duration of verification.
            options.share_mode(1);
            if move_source {
                options.access_mode(0x80000000 | 0x00010000);
            } // GENERIC_READ | DELETE
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
            let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut volume = vec![0u16; 32768];
            ensure!(
                unsafe {
                    GetVolumePathNameW(name.as_ptr(), volume.as_mut_ptr(), volume.len() as u32)
                } != 0,
                "Cannot inspect source volume"
            );
            ensure!(
                matches!(unsafe { GetDriveTypeW(volume.as_ptr()) }, 2 | 3 | 6),
                "Direct transfers require a local source, not a mounted cloud/network file"
            );
        }
        #[cfg(not(windows))]
        ensure!(
            !move_source,
            "Automatic source removal is Windows-only in this preview; use Direct upload to retain the source on macOS"
        );
        let file = options
            .open(&path)
            .context("Close the file in other applications before transferring it")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                file.metadata()?.file_attributes() & 0x400 == 0,
                "Choose a fully local regular file, not a reparse-point/cloud placeholder"
            );
        }
        let stamp = stamp(&file)?;
        let source = Self {
            path,
            stamp,
            file: Mutex::new(file),
        };
        source.check_sqlite_snapshot()?;
        Ok(source)
    }
    pub fn unchanged(&self) -> Result<()> {
        ensure!(
            stamp(&self.file.lock())? == self.stamp,
            "Source file changed; original retained"
        );
        Ok(())
    }
    fn check_sqlite_snapshot(&self) -> Result<()> {
        if self.stamp.size < 20 {
            return Ok(());
        }
        let mut header = [0; 20];
        self.read_exact_at(0, &mut header)?;
        if &header[..16] == b"SQLite format 3\0" {
            ensure!(
                header[18] == 1 && header[19] == 1,
                "Export a standalone SQLite backup in DELETE journal mode before uploading; WAL snapshots are not accepted"
            );
            for suffix in ["-wal", "-shm", "-journal"] {
                let mut sidecar = self.path.as_os_str().to_os_string();
                sidecar.push(suffix);
                ensure!(
                    !Path::new(&sidecar).exists(),
                    "Close SQLite and create a clean standalone backup first; journal sidecars are present"
                );
            }
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 1024 * 1024];
        let mut offset = 0;
        while offset < self.stamp.size {
            let length = (self.stamp.size - offset).min(buffer.len() as u64) as usize;
            self.read_exact_at(offset, &mut buffer[..length])?;
            hash.update(&buffer[..length]);
            offset += length as u64;
        }
        self.unchanged()?;
        Ok(format!("{:x}", hash.finalize()))
    }
    pub fn remove_verified_original(&self) -> Result<()> {
        self.unchanged()?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
            };
            let info = FILE_DISPOSITION_INFO { DeleteFile: 1 };
            ensure!(
                unsafe {
                    SetFileInformationByHandle(
                        self.file.lock().as_raw_handle().cast(),
                        FileDispositionInfo,
                        (&info as *const FILE_DISPOSITION_INFO).cast(),
                        std::mem::size_of_val(&info) as u32,
                    )
                } != 0,
                "Upload verified, but Windows could not remove the original; original retained"
            );
            Ok(())
        }
        #[cfg(not(windows))]
        anyhow::bail!("Automatic source removal is not enabled on this platform")
    }
}
impl ContentSource for Source {
    fn read_exact_at(&self, offset: u64, bytes: &mut [u8]) -> Result<()> {
        self.unchanged()?;
        let mut file = self.file.lock();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(bytes)?;
        ensure!(
            stamp(&file)? == self.stamp,
            "Source file changed during upload; original retained"
        );
        Ok(())
    }
    fn fingerprint(&self) -> Result<u128> {
        let hash = Sha256::digest(serde_json::to_vec(&self.stamp)?);
        Ok(u128::from_le_bytes(hash[..16].try_into()?))
    }
}
